import 'dart:io';

import 'package:url_launcher/url_launcher.dart';

/// 在系统默认文件管理器中打开文件所在目录。
/// 兼容 Windows / macOS / Linux，尊重用户注册的默认文件管理器。
Future<void> openFolder(String filePath) async {
  final dir = File(filePath).parent.path;
  await launchUrl(Uri.file(dir));
}

/// 用系统默认程序打开文件。
/// 兼容 Windows / macOS / Linux。
Future<void> openFile(String filePath) async {
  await launchUrl(Uri.file(filePath));
}
