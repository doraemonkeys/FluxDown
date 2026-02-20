import 'dart:io';

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

import '../i18n/locale_provider.dart';
import '../theme/app_colors.dart';
import '../theme/app_theme.dart';
import '../theme/theme_provider.dart';
import 'sub_window_utils.dart';

/// 下载完成通知窗口 — 屏幕右下角弹出，8 秒后自动关闭。
class DownloadCompleteWindow extends StatefulWidget {
  final WindowController windowController;
  final Map<String, dynamic> args;

  const DownloadCompleteWindow({
    super.key,
    required this.windowController,
    required this.args,
  });

  @override
  State<DownloadCompleteWindow> createState() => _DownloadCompleteWindowState();
}

class _DownloadCompleteWindowState extends State<DownloadCompleteWindow>
    with SingleTickerProviderStateMixin {
  bool _isHovered = false;
  bool _closed = false;
  late final AnimationController _progressController;

  /// 关闭信号频道 — 关闭前通知主窗口释放单窗口守卫。
  final _closeChannel = WindowMethodChannel(
    'flux/notification_close',
    mode: ChannelMode.unidirectional,
  );

  String get fileName => widget.args['fileName'] as String? ?? '';
  String get fileSize => widget.args['fileSize'] as String? ?? '';
  String get fileExt => widget.args['fileExt'] as String? ?? '?';
  String get filePath => widget.args['filePath'] as String? ?? '';
  int get taskCount => widget.args['taskCount'] as int? ?? 1;
  bool get isBatch => taskCount > 1;

  static const _windowWidth = 340.0;
  static const _windowHeight = 158.0;
  static const _autoCloseDuration = Duration(seconds: 8);

  @override
  void initState() {
    super.initState();
    _progressController = AnimationController(
      vsync: this,
      duration: _autoCloseDuration,
    )..addStatusListener(_onAnimationStatus);
    _progressController.forward();
    WidgetsBinding.instance.addPostFrameCallback((_) => _initWindow());
  }

  @override
  void dispose() {
    _progressController.dispose();
    super.dispose();
  }

  void _initWindow() {
    SubWindowUtils.init();
    SubWindowUtils.removeCaption();
    SubWindowUtils.setSize(const Size(_windowWidth, _windowHeight));

    final display = WidgetsBinding.instance.platformDispatcher.displays.first;
    final screenSize = display.size / display.devicePixelRatio;
    final x = screenSize.width - _windowWidth - 16;
    final y = screenSize.height - _windowHeight - 60;
    SubWindowUtils.setPosition(Offset(x, y));

    SubWindowUtils.setAlwaysOnTop(true);
    SubWindowUtils.setSkipTaskbar(true);
    SubWindowUtils.show();
    SubWindowUtils.focus();
  }

  void _onAnimationStatus(AnimationStatus status) {
    if (status == AnimationStatus.completed && !_isHovered) {
      _close();
    }
  }

  Future<void> _close() async {
    if (_closed) return;
    _closed = true;
    // 先向主窗口发送关闭信号，释放单窗口守卫，再执行 WM_CLOSE。
    // 这使主窗口能立即弹出下一条通知，而非等待固定超时。
    try {
      await _closeChannel.invokeMethod<void>('closed');
    } catch (_) {
      // 主窗口可能已退出，忽略错误，直接关闭窗口
    }
    SubWindowUtils.close();
  }

  void _openFile() {
    if (Platform.isWindows) {
      Process.run('cmd', ['/c', 'start', '', filePath]);
    } else if (Platform.isMacOS) {
      Process.run('open', [filePath]);
    } else if (Platform.isLinux) {
      Process.run('xdg-open', [filePath]);
    }
    _close();
  }

  void _openFolder() {
    final file = File(filePath);
    final dir = file.parent.path;

    if (file.existsSync()) {
      if (Platform.isWindows) {
        Process.run('explorer', ['/select,', filePath]);
      } else if (Platform.isMacOS) {
        Process.run('open', ['-R', filePath]);
      } else if (Platform.isLinux) {
        Process.run('xdg-open', [dir]);
      }
    } else {
      if (Platform.isWindows) {
        Process.run('explorer', [dir]);
      } else if (Platform.isMacOS) {
        Process.run('open', [dir]);
      } else if (Platform.isLinux) {
        Process.run('xdg-open', [dir]);
      }
    }
    _close();
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);

    // 不使用 transparent 背景 — 原生 Win32 窗口不支持透明，
    // 会露出白色底色。直接用 dialogBg 填满整个窗口（暗色下使用 Apple 风格深灰）。
    return Scaffold(
      backgroundColor: c.dialogBg,
      body: MouseRegion(
        onEnter: (_) {
          _isHovered = true;
          _progressController.stop();
        },
        onExit: (_) {
          _isHovered = false;
          if (_progressController.isCompleted) {
            _close();
          } else {
            _progressController.forward();
          }
        },
        // 使用 Column（默认 mainAxisSize: max）填满窗口，
        // 中间区域用 Expanded 吸收多余空间，彻底杜绝溢出。
        child: Column(
          children: [
            // === 顶部自动关闭进度条（2px）===
            AnimatedBuilder(
              animation: _progressController,
              builder: (context, _) {
                return LinearProgressIndicator(
                  value: 1.0 - _progressController.value,
                  minHeight: 2,
                  backgroundColor: Colors.transparent,
                  valueColor: AlwaysStoppedAnimation(
                    c.accent.withValues(alpha: 0.4),
                  ),
                );
              },
            ),
            // === Header ===
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 8, 6, 0),
              child: Row(
                children: [
                  Container(
                    width: 18,
                    height: 18,
                    decoration: BoxDecoration(
                      color: AppColors.green.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(9),
                    ),
                    child: const Icon(
                      LucideIcons.check,
                      size: 11,
                      color: AppColors.green,
                    ),
                  ),
                  const SizedBox(width: 7),
                  Text(
                    isBatch
                        ? currentS.batchDownloadCompleted(taskCount)
                        : currentS.downloadCompleted,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: c.textPrimary,
                    ),
                  ),
                  const Spacer(),
                  _CloseButton(onTap: _close, colors: c),
                ],
              ),
            ),
            // === File info（Expanded 吸收多余空间）===
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(14, 6, 14, 6),
                child: Row(
                  children: [
                    Container(
                      width: 38,
                      height: 38,
                      decoration: BoxDecoration(
                        color: c.surface2,
                        borderRadius: BorderRadius.circular(8),
                        border: Border.all(
                          color: c.border.withValues(alpha: 0.5),
                        ),
                      ),
                      child: Center(
                        child: Text(
                          fileExt.toLowerCase(),
                          style: TextStyle(
                            fontSize: 10,
                            fontWeight: FontWeight.w600,
                            color: c.accent,
                            letterSpacing: 0.3,
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            fileName,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 12.5,
                              fontWeight: FontWeight.w500,
                              color: c.textPrimary,
                            ),
                          ),
                          const SizedBox(height: 2),
                          Text(
                            isBatch
                                ? currentS.andMoreFiles(taskCount - 1)
                                : fileSize,
                            style: TextStyle(fontSize: 11, color: c.textMuted),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
            // === Divider ===
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14),
              child: Divider(height: 1, color: c.border),
            ),
            // === Actions ===
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 8, 14, 10),
              child: Row(
                children: [
                  Expanded(
                    child: ShadButton.outline(
                      size: ShadButtonSize.sm,
                      onPressed: _openFolder,
                      child: Row(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          Icon(
                            LucideIcons.folderOpen,
                            size: 13,
                            color: c.textSecondary,
                          ),
                          const SizedBox(width: 6),
                          Text(
                            currentS.openFileFolder,
                            style: TextStyle(
                              fontSize: 12,
                              color: c.textPrimary,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: ShadButton(
                      size: ShadButtonSize.sm,
                      onPressed: _openFile,
                      child: Row(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          const Icon(
                            LucideIcons.externalLink,
                            size: 13,
                            color: Colors.white,
                          ),
                          const SizedBox(width: 6),
                          Text(
                            currentS.openFile,
                            style: const TextStyle(
                              fontSize: 12,
                              color: Colors.white,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 精致的关闭按钮
class _CloseButton extends StatefulWidget {
  final VoidCallback onTap;
  final AppColors colors;

  const _CloseButton({required this.onTap, required this.colors});

  @override
  State<_CloseButton> createState() => _CloseButtonState();
}

class _CloseButtonState extends State<_CloseButton> {
  bool _isHovered = false;

  @override
  Widget build(BuildContext context) {
    final c = widget.colors;
    return MouseRegion(
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() => _isHovered = false),
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: widget.onTap,
        child: Container(
          width: 26,
          height: 26,
          decoration: BoxDecoration(
            color: _isHovered ? c.surface3 : Colors.transparent,
            borderRadius: BorderRadius.circular(6),
          ),
          child: Icon(
            LucideIcons.x,
            size: 13,
            color: _isHovered ? c.textPrimary : c.textMuted,
          ),
        ),
      ),
    );
  }
}

/// 下载完成通知窗口入口 App
class DownloadCompleteApp extends StatelessWidget {
  final WindowController windowController;
  final Map<String, dynamic> args;

  const DownloadCompleteApp({
    super.key,
    required this.windowController,
    required this.args,
  });

  @override
  Widget build(BuildContext context) {
    final schemeName = args['colorScheme'] as String? ?? 'blue';
    final isDark = args['isDark'] as bool? ?? true;

    final scheme = AppColorScheme.values.firstWhere(
      (s) => s.name == schemeName,
      orElse: () => AppColorScheme.blue,
    );

    return ShadApp(
      debugShowCheckedModeBanner: false,
      themeMode: isDark ? ThemeMode.dark : ThemeMode.light,
      theme: buildLightTheme(scheme),
      darkTheme: buildDarkTheme(scheme),
      home: DownloadCompleteWindow(
        windowController: windowController,
        args: args,
      ),
    );
  }
}
