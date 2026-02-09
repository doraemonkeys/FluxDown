import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import '../models/download_controller.dart';
import '../models/settings_provider.dart';
import '../theme/app_colors.dart';

void showNewDownloadDialog(
  BuildContext context,
  DownloadController controller,
  SettingsProvider settingsProvider,
) {
  showShadDialog(
    context: context,
    builder: (context) => _NewDownloadDialogContent(
      controller: controller,
      settingsProvider: settingsProvider,
    ),
  );
}

class _NewDownloadDialogContent extends StatefulWidget {
  final DownloadController controller;
  final SettingsProvider settingsProvider;

  const _NewDownloadDialogContent({
    required this.controller,
    required this.settingsProvider,
  });

  @override
  State<_NewDownloadDialogContent> createState() =>
      _NewDownloadDialogContentState();
}

class _NewDownloadDialogContentState extends State<_NewDownloadDialogContent> {
  final _urlController = TextEditingController();
  final _saveDirController = TextEditingController();
  final _renameController = TextEditingController();
  String? selectedThreads;

  @override
  void initState() {
    super.initState();
    // 使用 SettingsProvider 中的默认保存目录
    _saveDirController.text = widget.settingsProvider.defaultSaveDir;
    // 从剪切板自动识别 HTTP/HTTPS URL
    _pasteUrlFromClipboard();
  }

  /// 读取剪切板内容，如果包含 http/https/ftp 开头的 URL，自动填入下载地址
  Future<void> _pasteUrlFromClipboard() async {
    try {
      final data = await Clipboard.getData(Clipboard.kTextPlain);
      if (data == null || data.text == null) return;
      final text = data.text!.trim();
      // 匹配以 http:// / https:// / ftp:// 开头的 URL（取第一行，忽略多行内容）
      final firstLine = text.split('\n').first.trim();
      final urlPattern = RegExp(r'^(https?|ftp)://\S+', caseSensitive: false);
      final match = urlPattern.firstMatch(firstLine);
      if (match != null) {
        _urlController.text = match.group(0)!;
      }
    } catch (_) {
      // 剪切板访问失败时静默忽略（如权限不足等）
    }
  }

  @override
  void dispose() {
    _urlController.dispose();
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

  void _startDownload() {
    final url = _urlController.text.trim();
    if (url.isEmpty) return;

    final saveDir = _saveDirController.text.trim();
    if (saveDir.isEmpty) return;

    final rename = _renameController.text.trim();

    // 使用选择的线程数：「自动」= 0（由 Rust 动态计算），
    // 未选择则使用设置中的默认值
    final segments = switch (selectedThreads) {
      '自动' => 0,
      '4' => 4,
      '8' => 8,
      '16' => 16,
      '32' => 32,
      '64' => 64,
      _ => 0, // default to auto
    };

    widget.controller.createTask(
      url: url,
      saveDir: saveDir,
      fileName: rename,
      segments: segments,
    );

    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return ShadDialog(
      title: const Text('新建下载'),
      description: const Text('添加新的下载任务'),
      actions: [
        ShadButton.outline(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('取消'),
        ),
        ShadButton(onPressed: _startDownload, child: const Text('开始下载')),
      ],
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _label('下载链接', c),
            const SizedBox(height: 4),
            ShadInput(
              controller: _urlController,
              placeholder: const Text('HTTP / HTTPS / FTP 链接'),
            ),
            const SizedBox(height: 12),
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
            const SizedBox(height: 12),
            _label('重命名（可选，留空自动识别）', c),
            const SizedBox(height: 4),
            ShadInput(
              controller: _renameController,
              placeholder: const Text('可选'),
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
