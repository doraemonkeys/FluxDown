import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../models/download_task.dart';
import '../theme/theme_provider.dart';
import 'log_service.dart';

const _tag = 'NotifySvc';

/// 下载完成通知服务 — 通过 desktop_multi_window 创建独立的桌面通知窗口。
///
/// ## 防并发崩溃机制（迅雷模式）
///
/// `WindowController.create()` 内部创建 Dart isolate，
/// Dart VM 不允许同一线程上并发 `CreateIsolate`，
/// 否则触发 "CreateIsolate expects there to be no current isolate" 崩溃。
///
/// 迅雷的做法：多个下载同时完成时，合并为一条通知（"N 个任务下载完成"），
/// 而非为每个任务弹出独立窗口。
///
/// 本服务采用 **队列 + 防抖 + 单窗口守卫** 策略：
/// 1. 完成通知入队，启动 800ms 防抖定时器
/// 2. 防抖窗口内的多个完成事件合并为一批
/// 3. 同一时刻最多只有一个通知窗口在创建/显示中
/// 4. 当前窗口关闭后才处理队列中的下一批
class NotificationService {
  NotificationService._();
  static final instance = NotificationService._();

  ThemeProvider? _themeProvider;

  // ---------------------------------------------------------------------------
  // 队列 + 防抖 + 单窗口守卫
  // ---------------------------------------------------------------------------

  /// 等待通知的任务队列
  final List<DownloadTask> _queue = [];

  /// 防抖定时器 — 收集短时间内密集完成的任务
  Timer? _batchTimer;

  /// 单窗口守卫 — true 表示有通知窗口正在创建或显示中
  bool _windowActive = false;

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  /// 正在创建中的通知窗口计数（用于 waitForPending）
  int _pendingCount = 0;
  Completer<void>? _allDoneCompleter;

  // ---------------------------------------------------------------------------
  // 跨窗口关闭信号
  // ---------------------------------------------------------------------------

  /// 命名频道：通知子窗口 → 主窗口发送关闭信号（unidirectional 模式下所有引擎均可调用）
  final _closeChannel = WindowMethodChannel(
    'flux/notification_close',
    mode: ChannelMode.unidirectional,
  );

  /// 频道处理器是否已向平台注册
  bool _channelInitialized = false;

  /// 等待子窗口关闭信号的 Completer（非 null 表示当前正在等待某窗口关闭）
  Completer<void>? _windowCloseCompleter;

  /// 标记是否正在退出 — 退出过程中不再创建新的通知窗口
  bool _shuttingDown = false;

  /// 标记应用正在退出，停止接受新的通知请求
  void shutdown() {
    logInfo(_tag, 'shutdown called');
    _shuttingDown = true;
    _batchTimer?.cancel();
  }

  /// 设置主题提供者（在 FluxDownApp 初始化后调用）
  void setThemeProvider(ThemeProvider provider) {
    _themeProvider = provider;
  }

  /// 是否有正在创建中的通知窗口或队列中有待处理任务
  bool get hasPending => _pendingCount > 0 || _queue.isNotEmpty;

  /// 等待所有待处理的通知窗口创建完成（最多等 3 秒）。
  /// 在应用退出前调用，确保通知不会因进程销毁而丢失。
  /// 超时后直接返回，避免阻塞退出流程导致窗口卡死。
  Future<void> waitForPending() {
    logInfo(_tag, 'waitForPending: _pendingCount=$_pendingCount');
    if (_pendingCount == 0 && _queue.isEmpty) return Future.value();
    _allDoneCompleter ??= Completer<void>();
    return _allDoneCompleter!.future.timeout(
      const Duration(seconds: 3),
      onTimeout: () {
        logInfo(_tag, 'waitForPending timed out, proceeding with exit');
      },
    );
  }

  // ---------------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------------

  /// 显示下载完成的桌面通知窗口。
  ///
  /// 不会立即创建窗口，而是入队后等待 800ms 防抖窗口，
  /// 以便合并短时间内密集完成的多个任务。
  void showDownloadComplete(DownloadTask task) {
    logInfo(
      _tag,
      'showDownloadComplete: file=${task.fileName}, shuttingDown=$_shuttingDown',
    );
    if (_shuttingDown) {
      logInfo(_tag, 'skipped (shuttingDown)');
      return;
    }

    _queue.add(task);
    logInfo(_tag, 'queued, queueSize=${_queue.length}');

    // 如果已有窗口在显示，仅入队，等窗口关闭后再处理
    if (_windowActive) {
      logInfo(_tag, 'window active, queued for later');
      return;
    }

    // 防抖：等待 800ms 收集短时间内密集完成的任务
    _batchTimer?.cancel();
    _batchTimer = Timer(const Duration(milliseconds: 800), _flushQueue);
  }

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  /// 冲刷队列 — 取出所有待处理任务，创建一个通知窗口。
  Future<void> _flushQueue() async {
    if (_windowActive || _queue.isEmpty || _shuttingDown) return;

    // 取出当前队列所有任务
    final batch = List<DownloadTask>.of(_queue);
    _queue.clear();

    _windowActive = true;
    _pendingCount++;
    logInfo(_tag, 'flushing ${batch.length} notifications');

    try {
      // 提前初始化频道并设置 Completer，确保即使窗口关闭极快也能捕获信号。
      await _ensureChannelInitialized();
      _windowCloseCompleter = Completer<void>();

      try {
        await _createNotifyWindow(batch.last, taskCount: batch.length);
      } catch (e, stack) {
        logError(_tag, 'notify window error', e, stack);
      }

      // 等待子窗口主动发送关闭信号（在 SubWindowUtils.close() 之前发出）。
      // 最多等待 60 秒兜底，防止用户无限悬停或信号丢失导致服务永久阻塞。
      await _windowCloseCompleter!.future.timeout(
        const Duration(seconds: 60),
        onTimeout: () {
          logInfo(_tag, 'window close signal timed out after 60s');
        },
      );
      _windowCloseCompleter = null;

      // 等待子窗口 WM_CLOSE 处理完毕、Flutter engine isolate 完全销毁，
      // 避免立即创建下一个窗口时触发并发 isolate 初始化崩溃。
      await Future.delayed(const Duration(milliseconds: 800));
    } finally {
      // try-finally 确保即使出现异常，守卫状态也能被正确重置，
      // 防止 _windowActive 永久卡在 true 导致后续通知全部被丢弃。
      _windowCloseCompleter = null; // 清理 completer，防止异常路径下泄漏
      _windowActive = false;
      _pendingCount--;
      logInfo(
        _tag,
        'window guard expired, pendingCount=$_pendingCount, queueSize=${_queue.length}',
      );

      if (_pendingCount == 0 &&
          _queue.isEmpty &&
          _allDoneCompleter != null &&
          !_allDoneCompleter!.isCompleted) {
        _allDoneCompleter!.complete();
        _allDoneCompleter = null;
      }
    }

    // 处理守卫期间新入队的任务
    if (_queue.isNotEmpty && !_shuttingDown) {
      await _flushQueue();
    }
  }

  /// 惰性注册频道处理器 — 首次调用时向平台注册，后续调用立即返回。
  Future<void> _ensureChannelInitialized() async {
    if (_channelInitialized) return;
    _channelInitialized = true;
    try {
      await _closeChannel.setMethodCallHandler(_onWindowCloseSignal);
      logInfo(_tag, 'notification close channel registered');
    } catch (e, stack) {
      logError(_tag, 'failed to register notification close channel', e, stack);
      _channelInitialized = false; // 允许重试
    }
  }

  /// 接收来自通知子窗口的关闭信号 — 释放单窗口守卫。
  Future<dynamic> _onWindowCloseSignal(MethodCall call) async {
    if (call.method == 'closed') {
      logInfo(_tag, 'notification window close signal received');
      final c = _windowCloseCompleter;
      if (c != null && !c.isCompleted) {
        c.complete();
      }
    }
  }

  Future<void> _createNotifyWindow(
    DownloadTask task, {
    int taskCount = 1,
  }) async {
    try {
      final filePath =
          '${task.saveDir}${Platform.pathSeparator}${task.fileName}';

      final isDark = _resolveIsDark();
      final schemeName = _themeProvider?.colorScheme.name ?? 'blue';

      logInfo(
        _tag,
        'creating notify window: file=${task.fileName}, taskCount=$taskCount, isDark=$isDark',
      );
      await WindowController.create(
        WindowConfiguration(
          arguments: jsonEncode({
            'windowType': 'download_complete',
            'fileName': task.fileName,
            'fileSize': task.sizeText,
            'fileExt': task.fileExtension,
            'filePath': filePath,
            'taskCount': taskCount,
            'colorScheme': schemeName,
            'isDark': isDark,
          }),
        ),
      );
      logInfo(_tag, 'notify window created');
    } catch (e, stack) {
      logError(_tag, 'failed to create notify window', e, stack);
    }
  }

  bool _resolveIsDark() {
    final provider = _themeProvider;
    if (provider == null) return true;
    return provider.themeMode == ThemeMode.dark ||
        (provider.themeMode == ThemeMode.system &&
            WidgetsBinding.instance.platformDispatcher.platformBrightness ==
                Brightness.dark);
  }
}
