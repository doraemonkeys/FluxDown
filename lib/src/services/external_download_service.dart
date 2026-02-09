import 'dart:async';
import 'dart:convert';

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:flutter/material.dart';
import 'package:rinf/rinf.dart';

import '../bindings/bindings.dart';
import '../models/settings_provider.dart';
import '../theme/theme_provider.dart';

/// 监听来自浏览器扩展的外部下载请求，弹出独立的快速下载确认窗口。
///
/// 架构：
/// 1. Rust HTTP server 收到浏览器扩展的下载请求
/// 2. Rust 发送 ExternalDownloadRequest 信号到 Dart
/// 3. 本服务监听该信号，创建一个独立的子窗口（desktop_multi_window）
/// 4. 用户在子窗口中确认下载参数
/// 5. 子窗口通过 WindowMethodChannel 将确认数据回传主窗口
/// 6. 主窗口发送 ConfirmExternalDownload 信号到 Rust
class ExternalDownloadService {
  static ExternalDownloadService? _instance;

  final SettingsProvider settingsProvider;
  final ThemeProvider themeProvider;
  StreamSubscription<RustSignalPack<ExternalDownloadRequest>>? _sub;
  bool _windowOpen = false;

  ExternalDownloadService._({
    required this.settingsProvider,
    required this.themeProvider,
  });

  /// 初始化单例。应在 app 启动时调用一次。
  static void init({
    required SettingsProvider settingsProvider,
    required ThemeProvider themeProvider,
  }) {
    _instance?._teardown();
    _instance = ExternalDownloadService._(
      settingsProvider: settingsProvider,
      themeProvider: themeProvider,
    );
    _instance!._startListening();
    _instance!._registerMethodHandler();
  }

  static void shutdown() {
    _instance?._teardown();
    _instance = null;
  }

  void _teardown() {
    _sub?.cancel();
  }

  void _startListening() {
    _sub = ExternalDownloadRequest.rustSignalStream.listen(_onRequest);
  }

  /// 注册主窗口的方法处理器，接收子窗口回传的确认数据
  void _registerMethodHandler() {
    // 获取主窗口的 controller 并注册方法处理
    WindowController.fromCurrentEngine()
        .then((controller) {
          controller.setWindowMethodHandler((call) async {
            if (call.method == 'confirm_download') {
              _onConfirmDownload(call.arguments as String);
            }
          });
        })
        .catchError((e) {
          debugPrint(
            '[ExternalDownloadService] failed to register method handler: $e',
          );
        });
  }

  /// 处理子窗口回传的下载确认数据
  void _onConfirmDownload(String jsonStr) {
    try {
      final data = jsonDecode(jsonStr) as Map<String, dynamic>;
      final url = data['url'] as String? ?? '';
      final saveDir = data['saveDir'] as String? ?? '';
      final fileName = data['fileName'] as String? ?? '';
      final segments = data['segments'] as int? ?? 0;

      debugPrint(
        '[ExternalDownloadService] confirmed download: url=$url, dir=$saveDir',
      );

      // 发送确认信号到 Rust
      ConfirmExternalDownload(
        url: url,
        saveDir: saveDir,
        fileName: fileName,
        segments: segments,
      ).sendSignalToRust();
    } catch (e) {
      debugPrint('[ExternalDownloadService] failed to parse confirm data: $e');
    }
    _windowOpen = false;
  }

  void _onRequest(RustSignalPack<ExternalDownloadRequest> pack) async {
    final req = pack.message;
    debugPrint(
      '[ExternalDownloadService] received request: url=${req.url}, '
      'filename=${req.filename}, size=${req.fileSize}',
    );

    // 防止重复弹窗
    if (_windowOpen) {
      debugPrint(
        '[ExternalDownloadService] window already open, ignoring request',
      );
      return;
    }

    _windowOpen = true;

    try {
      // 获取主窗口 ID 用于子窗口回传
      final mainController = await WindowController.fromCurrentEngine();

      // 判断当前是否为暗色模式
      final isDark =
          themeProvider.themeMode == ThemeMode.dark ||
          (themeProvider.themeMode == ThemeMode.system &&
              WidgetsBinding.instance.platformDispatcher.platformBrightness ==
                  Brightness.dark);

      // 创建独立子窗口
      final windowController = await WindowController.create(
        WindowConfiguration(
          arguments: jsonEncode({
            'windowType': 'quick_download',
            'url': req.url,
            'filename': req.filename,
            'fileSize': req.fileSize,
            'mimeType': req.mimeType,
            'defaultSaveDir': settingsProvider.defaultSaveDir,
            'colorScheme': themeProvider.colorScheme.name,
            'isDark': isDark,
            'mainWindowId': mainController.windowId,
          }),
        ),
      );

      // 监听子窗口关闭以重置标志
      onWindowsChanged.listen((_) async {
        final windows = await WindowController.getAll();
        final stillExists = windows.any(
          (w) => w.windowId == windowController.windowId,
        );
        if (!stillExists) {
          _windowOpen = false;
        }
      });
    } catch (e) {
      debugPrint('[ExternalDownloadService] failed to create sub-window: $e');
      _windowOpen = false;
    }
  }
}
