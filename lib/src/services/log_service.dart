import 'dart:async';
import 'dart:io';
import 'dart:isolate';

import 'package:flutter/foundation.dart';

/// 文件日志服务 — 将日志写入 exe 同级 logs/ 目录，按日期分文件。
///
/// 使用缓冲写入 + 定时刷盘，兼顾性能和崩溃前日志完整度。
/// 单例，应在 app 启动最早期调用 [init]。
class LogService {
  LogService._();
  static final LogService instance = LogService._();

  RandomAccessFile? _raf;
  String? _currentDateTag;
  Timer? _flushTimer;
  bool _initialized = false;

  /// 日志目录 — exe 同级 logs/
  late final Directory _logDir;

  /// 初始化日志服务。应在 main() 最开始调用。
  void init() {
    if (_initialized) return;
    _initialized = true;

    final exeDir = File(Platform.resolvedExecutable).parent.path;
    _logDir = Directory('$exeDir${Platform.pathSeparator}logs');
    if (!_logDir.existsSync()) {
      _logDir.createSync(recursive: true);
    }

    _rotateSink();

    // 每 2 秒刷盘一次，确保崩溃前有足够日志
    _flushTimer = Timer.periodic(const Duration(seconds: 2), (_) {
      try {
        _raf?.flushSync();
      } catch (_) {}
    });
  }

  /// 写一条日志。[tag] 是模块标签，[message] 是内容。
  void log(String tag, String message) {
    if (!_initialized) return;
    try {
      _rotateSink();
      final now = DateTime.now();
      final ts =
          '${_pad2(now.hour)}:${_pad2(now.minute)}:${_pad2(now.second)}.${_pad3(now.millisecond)}';
      final line = '$ts [$tag] $message\n';
      _raf?.writeStringSync(line);
      // 仅在 debug 模式下输出到控制台，避免 release 模式的字符串缓存开销
      if (kDebugMode) {
        // ignore: avoid_print
        print(line.trimRight());
      }
    } catch (e) {
      // 日志服务本身不应该抛异常影响业务
      // ignore: avoid_print
      print('[LogService] write error: $e');
    }
  }

  /// 记录错误（含堆栈）
  void error(String tag, String message, [Object? err, StackTrace? stack]) {
    log(tag, 'ERROR: $message');
    if (err != null) log(tag, '  exception: $err');
    if (stack != null) log(tag, '  stackTrace:\n$stack');
    // 错误立即刷盘
    try {
      _raf?.flushSync();
    } catch (_) {}
  }

  /// 关闭日志服务
  Future<void> dispose() async {
    _flushTimer?.cancel();
    _flushTimer = null;
    try {
      _raf?.flushSync();
      _raf?.closeSync();
    } catch (_) {}
    _raf = null;
    _initialized = false;
  }

  // ── 内部 ──

  /// 按日期切换日志文件（全同步，无 IOSink 异步问题）
  void _rotateSink() {
    final now = DateTime.now();
    final dateTag = '${now.year}-${_pad2(now.month)}-${_pad2(now.day)}';
    if (dateTag == _currentDateTag && _raf != null) return;

    // 关闭旧文件
    try {
      _raf?.flushSync();
      _raf?.closeSync();
    } catch (_) {}

    _currentDateTag = dateTag;
    final file = File(
      '${_logDir.path}${Platform.pathSeparator}fluxdown_$dateTag.log',
    );
    _raf = file.openSync(mode: FileMode.append);

    final header =
        '\n'
        '====== FluxDown log session started at $now ======\n'
        '  pid: $pid\n'
        '  exe: ${Platform.resolvedExecutable}\n'
        '  isolate: ${Isolate.current.debugName}\n'
        '\n';
    _raf!.writeStringSync(header);
  }

  static String _pad2(int n) => n.toString().padLeft(2, '0');
  static String _pad3(int n) => n.toString().padLeft(3, '0');
}

/// 全局快捷方法
void logInfo(String tag, String message) =>
    LogService.instance.log(tag, message);

void logError(String tag, String message, [Object? err, StackTrace? stack]) =>
    LogService.instance.error(tag, message, err, stack);
