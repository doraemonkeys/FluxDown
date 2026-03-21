import 'dart:io';

import 'package:url_launcher/url_launcher.dart';

/// 在系统默认文件管理器中打开文件所在目录。
/// 兼容 Windows / macOS / Linux，尊重用户注册的默认文件管理器。
///
/// [filePath] 可以是文件路径，也可以是目录路径：
/// - 文件路径 → 打开其所在目录
/// - 目录路径 → 直接打开该目录
Future<void> openFolder(String filePath) async {
  // 判断传入的路径是文件还是目录，避免对目录路径错误地取 parent。
  // 当通知回调中 payload 丢失而 fallback 到 _resolveDefaultDir() 时，
  // 传入的是目录路径；若仍对其取 .parent，就会向上跳一级（如打开用户目录）。
  final type = await FileSystemEntity.type(filePath);
  final String dir;
  switch (type) {
    case FileSystemEntityType.file:
      dir = File(filePath).parent.path;
    case FileSystemEntityType.directory:
      dir = filePath;
    default:
      // 路径不存在 — 通过最后一段是否含扩展名来推测：
      // 有扩展名视为文件路径取 parent；否则视为目录路径直接使用。
      final leaf = filePath.split(Platform.pathSeparator).last;
      dir = leaf.contains('.') ? File(filePath).parent.path : filePath;
  }

  if (Platform.isWindows) {
    // url_launcher 的 file:// URI 在 Windows 上会被硬编码路由给 explorer.exe，
    // 绕过用户注册的第三方文件管理器（如 OneCommander）。
    // 改用 cmd /c start，走 ShellExecute "open" 操作，
    // 会正确查找 HKCR\Directory\shell\open\command 中注册的默认文件管理器。
    await Process.run('cmd', ['/c', 'start', '', dir]);
  } else {
    await launchUrl(Uri.file(dir));
  }
}

/// 用系统默认程序打开文件。
/// 兼容 Windows / macOS / Linux。
Future<void> openFile(String filePath) async {
  // 所有平台统一使用 url_launcher 的 file:// URI。
  // 在 Windows 上 launchUrl(file://) 最终调用 ShellExecuteW("open", ...)，
  // 通过完整的注册表查找链（HKCR → UserChoice → OpenWithProgids）解析关联应用，
  // 比 cmd /c start 更可靠——后者对 .zip/.7z/.docx 等通过现代 Windows 设置
  // 注册的文件类型偶尔无法正确识别关联程序。
  await launchUrl(Uri.file(filePath));
}
