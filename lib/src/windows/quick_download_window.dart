import 'dart:convert';

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import 'package:window_manager/window_manager.dart';

import '../theme/app_colors.dart';
import '../theme/app_theme.dart';
import '../theme/theme_provider.dart';

/// 独立快速下载确认窗口 — 浏览器扩展拦截下载时弹出
///
/// 作为独立子窗口运行，不影响主窗口。
/// 确认后通过 WindowController.invokeMethod 将数据回传主窗口。
class QuickDownloadWindow extends StatefulWidget {
  final WindowController windowController;
  final Map<String, dynamic> args;

  const QuickDownloadWindow({
    super.key,
    required this.windowController,
    required this.args,
  });

  @override
  State<QuickDownloadWindow> createState() => _QuickDownloadWindowState();
}

class _QuickDownloadWindowState extends State<QuickDownloadWindow> {
  final _saveDirController = TextEditingController();
  final _renameController = TextEditingController();
  String? selectedThreads;

  String get url => widget.args['url'] as String? ?? '';
  String get filename => widget.args['filename'] as String? ?? '';
  int get fileSize => widget.args['fileSize'] as int? ?? 0;
  String get mimeType => widget.args['mimeType'] as String? ?? '';
  String get defaultSaveDir => widget.args['defaultSaveDir'] as String? ?? '';
  String get mainWindowId => widget.args['mainWindowId'] as String? ?? '0';

  @override
  void initState() {
    super.initState();
    _saveDirController.text = defaultSaveDir;
    if (filename.isNotEmpty) {
      _renameController.text = filename;
    }
    _initWindow();
  }

  Future<void> _initWindow() async {
    await windowManager.setSize(const Size(520, 420));
    await windowManager.setMinimumSize(const Size(400, 350));
    await windowManager.center();
    await windowManager.setAlwaysOnTop(true);
    await windowManager.setTitle('FluxDown - 新建下载');
    await windowManager.show();
    await windowManager.focus();
  }

  @override
  void dispose() {
    _saveDirController.dispose();
    _renameController.dispose();
    super.dispose();
  }

  Future<void> _pickSaveDir() async {
    final result = await FilePicker.platform.getDirectoryPath(
      dialogTitle: '选择保存目录',
      initialDirectory: _saveDirController.text.trim().isNotEmpty
          ? _saveDirController.text.trim()
          : null,
    );
    if (result != null) {
      _saveDirController.text = result;
    }
  }

  Future<void> _startDownload() async {
    final saveDir = _saveDirController.text.trim();
    if (saveDir.isEmpty) return;

    final rename = _renameController.text.trim();
    final segments = switch (selectedThreads) {
      '自动' => 0,
      '4' => 4,
      '8' => 8,
      '16' => 16,
      '32' => 32,
      '64' => 64,
      _ => 0,
    };

    // 将确认数据发回主窗口
    try {
      final mainController = WindowController.fromWindowId(mainWindowId);
      await mainController.invokeMethod(
        'confirm_download',
        jsonEncode({
          'url': url,
          'saveDir': saveDir,
          'fileName': rename,
          'segments': segments,
        }),
      );
    } catch (e) {
      debugPrint('[QuickDownloadWindow] invokeMethod error: $e');
    }

    await windowManager.close();
  }

  Future<void> _cancel() async {
    await windowManager.close();
  }

  String _formatFileSize(int bytes) {
    if (bytes <= 0) return '未知大小';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    int unitIndex = 0;
    double size = bytes.toDouble();
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    return '${size.toStringAsFixed(unitIndex == 0 ? 0 : 1)} ${units[unitIndex]}';
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);

    return Scaffold(
      backgroundColor: c.bg,
      body: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // 标题行
            Row(
              children: [
                Icon(LucideIcons.download, size: 20, color: c.accent),
                const SizedBox(width: 8),
                Text(
                  '新建下载',
                  style: TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w600,
                    color: c.textPrimary,
                  ),
                ),
                const Spacer(),
                if (fileSize > 0)
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    decoration: BoxDecoration(
                      color: c.surface2,
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      _formatFileSize(fileSize),
                      style: TextStyle(fontSize: 11, color: c.textSecondary),
                    ),
                  ),
                if (mimeType.isNotEmpty) ...[
                  const SizedBox(width: 6),
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    decoration: BoxDecoration(
                      color: c.surface2,
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      mimeType,
                      style: TextStyle(fontSize: 11, color: c.textSecondary),
                    ),
                  ),
                ],
              ],
            ),
            const SizedBox(height: 16),

            // URL 显示
            _label('下载链接', c),
            const SizedBox(height: 4),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
              decoration: BoxDecoration(
                color: c.surface2,
                borderRadius: BorderRadius.circular(6),
                border: Border.all(color: c.border),
              ),
              child: SelectableText(
                url,
                style: TextStyle(
                  fontSize: 12,
                  color: c.textSecondary,
                  fontFamily: 'monospace',
                ),
                maxLines: 2,
              ),
            ),

            const SizedBox(height: 14),

            // 保存目录 + 线程数
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _label('保存目录', c),
                      const SizedBox(height: 4),
                      GestureDetector(
                        onTap: _pickSaveDir,
                        child: AbsorbPointer(
                          child: ShadInput(
                            controller: _saveDirController,
                            placeholder: const Text('保存目录'),
                            readOnly: true,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 10),
                SizedBox(
                  width: 100,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _label('线程数', c),
                      const SizedBox(height: 4),
                      ShadSelect<String>(
                        placeholder: const Text('自动'),
                        options: ['自动', '4', '8', '16', '32', '64']
                            .map((e) => ShadOption(value: e, child: Text(e)))
                            .toList(),
                        selectedOptionBuilder: (context, value) => Text(value),
                        onChanged: (v) => setState(() => selectedThreads = v),
                      ),
                    ],
                  ),
                ),
              ],
            ),

            const SizedBox(height: 14),

            // 文件名
            _label('文件名（留空自动识别）', c),
            const SizedBox(height: 4),
            ShadInput(
              controller: _renameController,
              placeholder: const Text('自动识别文件名'),
            ),

            const Spacer(),

            // 底部按钮
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                ShadButton.outline(onPressed: _cancel, child: const Text('取消')),
                const SizedBox(width: 8),
                ShadButton(
                  onPressed: _startDownload,
                  child: const Text('开始下载'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _label(String text, AppColors c) {
    return Text(
      text,
      style: TextStyle(
        fontSize: 11,
        fontWeight: FontWeight.w500,
        color: c.textSecondary,
      ),
    );
  }
}

/// 子窗口入口 App — 包装 shadcn_ui 主题
class QuickDownloadApp extends StatelessWidget {
  final WindowController windowController;
  final Map<String, dynamic> args;

  const QuickDownloadApp({
    super.key,
    required this.windowController,
    required this.args,
  });

  @override
  Widget build(BuildContext context) {
    // 从主窗口传来的主题配置
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
      home: QuickDownloadWindow(windowController: windowController, args: args),
    );
  }
}
