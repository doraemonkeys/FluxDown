import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:flutter/material.dart';

import '../models/download_task.dart';
import '../theme/theme_provider.dart';
import 'log_service.dart';

const _tag = 'NotifySvc';

/// 下载完成通知服务 — 通过 desktop_multi_window 创建独立的桌面通知窗口。
///
/// 窗口定位在屏幕右下角，无边框、置顶、不占任务栏，8 秒自动关闭。
/// 类似迅雷的下载完成提示 UI。
class NotificationService {
  NotificationService._();
  static final instance = NotificationService._();

  ThemeProvider? _themeProvider;

  /// 正在创建中的通知窗口计数
  int _pendingCount = 0;
  Completer<void>? _allDoneCompleter;

  /// 标记是否正在退出 — 退出过程中不再创建新的通知窗口
  bool _shuttingDown = false;

  /// 标记应用正在退出，停止接受新的通知请求
  void shutdown() {
    logInfo(_tag, 'shutdown called');
    _shuttingDown = true;
  }

  /// 设置主题提供者（在 FluxDownApp 初始化后调用）
  void setThemeProvider(ThemeProvider provider) {
    _themeProvider = provider;
  }

  /// 是否有正在创建中的通知窗口
  bool get hasPending => _pendingCount > 0;

  /// 等待所有待处理的通知窗口创建完成（最多等 3 秒）。
  /// 在应用退出前调用，确保通知不会因进程销毁而丢失。
  /// 超时后直接返回，避免阻塞退出流程导致窗口卡死。
  Future<void> waitForPending() {
    logInfo(_tag, 'waitForPending: _pendingCount=$_pendingCount');
    if (_pendingCount == 0) return Future.value();
    _allDoneCompleter ??= Completer<void>();
    return _allDoneCompleter!.future.timeout(
      const Duration(seconds: 3),
      onTimeout: () {
        logInfo(_tag, 'waitForPending timed out, proceeding with exit');
      },
    );
  }

  /// 显示下载完成的桌面通知窗口
  void showDownloadComplete(DownloadTask task) {
    logInfo(
      _tag,
      'showDownloadComplete: file=${task.fileName}, shuttingDown=$_shuttingDown',
    );
    // 应用退出中不再创建通知窗口
    if (_shuttingDown) {
      logInfo(_tag, 'skipped (shuttingDown)');
      return;
    }

    _pendingCount++;
    logInfo(_tag, 'pendingCount++ => $_pendingCount');
    // 延迟到下一个微任务，避免在 Rust 信号流回调中同步创建窗口
    Future.microtask(() async {
      try {
        await _createNotifyWindow(task);
      } catch (e, stack) {
        logError(_tag, 'showDownloadComplete microtask error', e, stack);
      } finally {
        _pendingCount--;
        logInfo(_tag, 'pendingCount-- => $_pendingCount');
        if (_pendingCount == 0 && _allDoneCompleter != null) {
          _allDoneCompleter!.complete();
          _allDoneCompleter = null;
        }
      }
    });
  }

  Future<void> _createNotifyWindow(DownloadTask task) async {
    try {
      final filePath =
          '${task.saveDir}${Platform.pathSeparator}${task.fileName}';

      // 判断当前主题
      final isDark = _resolveIsDark();
      final schemeName = _themeProvider?.colorScheme.name ?? 'blue';

      logInfo(
        _tag,
        'creating notify window: file=${task.fileName}, isDark=$isDark',
      );
      await WindowController.create(
        WindowConfiguration(
          arguments: jsonEncode({
            'windowType': 'download_complete',
            'fileName': task.fileName,
            'fileSize': task.sizeText,
            'fileExt': task.fileExtension,
            'filePath': filePath,
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
