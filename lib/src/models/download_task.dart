import 'dart:math';

import '../bindings/bindings.dart';

/// 任务状态 — 与 Rust 端状态码对应
/// 0=pending, 1=downloading, 2=paused, 3=completed, 4=error
/// resuming 为纯 Dart 端状态，点击继续后立即切换，Rust 返回 status=1 后自动过渡到 downloading
enum TaskStatus { pending, downloading, paused, completed, error, resuming }

/// 文件类型分类 — 由扩展名推断
enum FileCategory {
  all,
  video,
  audio,
  document,
  image,
  archive,
  other;

  String get label => switch (this) {
    FileCategory.all => '全部文件',
    FileCategory.video => '视频',
    FileCategory.audio => '音频',
    FileCategory.document => '文档',
    FileCategory.image => '图片',
    FileCategory.archive => '压缩包',
    FileCategory.other => '其他',
  };

  static const _videoExts = {
    'mp4',
    'mkv',
    'avi',
    'mov',
    'wmv',
    'flv',
    'webm',
    'ts',
    'm4v',
    'rmvb',
    'rm',
    '3gp',
    'vob',
    'mpg',
    'mpeg',
  };
  static const _audioExts = {
    'mp3',
    'flac',
    'wav',
    'aac',
    'ogg',
    'wma',
    'm4a',
    'opus',
    'ape',
    'aiff',
  };
  static const _docExts = {
    'pdf',
    'doc',
    'docx',
    'xls',
    'xlsx',
    'ppt',
    'pptx',
    'txt',
    'csv',
    'rtf',
    'epub',
    'mobi',
    'md',
    'odt',
    'ods',
    'odp',
  };
  static const _imageExts = {
    'jpg',
    'jpeg',
    'png',
    'gif',
    'bmp',
    'webp',
    'svg',
    'ico',
    'tiff',
    'tif',
    'psd',
    'raw',
    'heic',
    'avif',
  };
  static const _archiveExts = {
    'zip',
    'rar',
    '7z',
    'tar',
    'gz',
    'bz2',
    'xz',
    'zst',
    'iso',
    'dmg',
    'cab',
    'lz',
    'lzma',
  };

  /// 根据文件扩展名推断分类
  static FileCategory fromExtension(String ext) {
    final e = ext.toLowerCase();
    if (_videoExts.contains(e)) return FileCategory.video;
    if (_audioExts.contains(e)) return FileCategory.audio;
    if (_docExts.contains(e)) return FileCategory.document;
    if (_imageExts.contains(e)) return FileCategory.image;
    if (_archiveExts.contains(e)) return FileCategory.archive;
    return FileCategory.other;
  }
}

TaskStatus taskStatusFromInt(int value) {
  return switch (value) {
    0 => TaskStatus.pending,
    1 => TaskStatus.downloading,
    2 => TaskStatus.paused,
    3 => TaskStatus.completed,
    4 => TaskStatus.error,
    _ => TaskStatus.error,
  };
}

/// Per-segment progress data for IDM-style visualization
class SegmentData {
  final int index;
  final int startByte;
  final int endByte;
  final int downloadedBytes;

  const SegmentData({
    required this.index,
    required this.startByte,
    required this.endByte,
    required this.downloadedBytes,
  });

  /// Segment size in bytes
  int get size => endByte - startByte + 1;

  /// Progress [0.0, 1.0]
  double get progress =>
      size > 0 ? (downloadedBytes / size).clamp(0.0, 1.0) : 0;
}

class DownloadTask {
  final String id;
  final String url;
  final String fileName;
  final String saveDir;
  final TaskStatus status;
  final int downloadedBytes;
  final int totalBytes;
  final int speed; // bytes per second
  final String errorMessage;
  final bool isSelected;

  /// Per-segment progress data (null if no segment info received yet)
  final List<SegmentData>? segments;

  const DownloadTask({
    required this.id,
    required this.url,
    required this.fileName,
    required this.saveDir,
    required this.status,
    required this.downloadedBytes,
    required this.totalBytes,
    this.speed = 0,
    this.errorMessage = '',
    this.isSelected = false,
    this.segments,
  });

  /// 从 AllTasks 信号中的 TaskInfo 构建
  factory DownloadTask.fromTaskInfo(TaskInfo info) {
    return DownloadTask(
      id: info.taskId,
      url: info.url,
      fileName: info.fileName.isEmpty ? '未知文件' : info.fileName,
      saveDir: info.saveDir,
      status: taskStatusFromInt(info.status),
      downloadedBytes: info.downloadedBytes,
      totalBytes: info.totalBytes,
      errorMessage: info.errorMessage,
    );
  }

  DownloadTask copyWith({
    String? id,
    String? url,
    String? fileName,
    String? saveDir,
    TaskStatus? status,
    int? downloadedBytes,
    int? totalBytes,
    int? speed,
    String? errorMessage,
    bool? isSelected,
    List<SegmentData>? segments,
  }) {
    return DownloadTask(
      id: id ?? this.id,
      url: url ?? this.url,
      fileName: fileName ?? this.fileName,
      saveDir: saveDir ?? this.saveDir,
      status: status ?? this.status,
      downloadedBytes: downloadedBytes ?? this.downloadedBytes,
      totalBytes: totalBytes ?? this.totalBytes,
      speed: speed ?? this.speed,
      errorMessage: errorMessage ?? this.errorMessage,
      isSelected: isSelected ?? this.isSelected,
      segments: segments ?? this.segments,
    );
  }

  /// 根据 TaskProgress 信号增量更新
  DownloadTask applyProgress(TaskProgress p) {
    // Dart-side EMA smoothing for speed display (α = 0.3).
    // Rust already sends EMA-smoothed speed; this second pass further damps
    // any residual jitter from multi-segment reporting.
    final newStatus = taskStatusFromInt(p.status);
    final int smoothedSpeed;
    if (newStatus == TaskStatus.downloading && p.speed > 0) {
      if (speed > 0) {
        smoothedSpeed = (0.3 * p.speed + 0.7 * speed).round();
      } else {
        smoothedSpeed = p.speed; // first update — use raw value
      }
    } else {
      smoothedSpeed = p.speed;
    }

    return copyWith(
      status: newStatus,
      downloadedBytes: p.downloadedBytes,
      totalBytes: p.totalBytes > 0 ? p.totalBytes : null,
      speed: smoothedSpeed,
      fileName: p.fileName.isNotEmpty ? p.fileName : null,
      saveDir: p.saveDir.isNotEmpty ? p.saveDir : null,
      errorMessage: p.errorMessage,
    );
  }

  // ---------------------------------------------------------------------------
  // Computed properties
  // ---------------------------------------------------------------------------

  /// 下载进度 [0.0, 1.0]
  double get progress {
    if (totalBytes <= 0) return 0;
    return (downloadedBytes / totalBytes).clamp(0.0, 1.0);
  }

  /// 文件扩展名（用于图标显示）
  String get fileExtension {
    final dot = fileName.lastIndexOf('.');
    if (dot < 0 || dot == fileName.length - 1) return '?';
    return fileName.substring(dot + 1).toLowerCase();
  }

  /// 文件类型分类
  FileCategory get fileCategory => FileCategory.fromExtension(fileExtension);

  /// 格式化文件大小
  String get sizeText {
    if (totalBytes <= 0) return '未知大小';
    return formatBytes(totalBytes);
  }

  /// 格式化已下载
  String get downloadedText => formatBytes(downloadedBytes);

  /// 格式化速度
  String get speedText {
    if (speed <= 0) return '—';
    return '${formatBytes(speed)}/s';
  }

  /// 副标题信息
  String get subtitle {
    switch (status) {
      case TaskStatus.downloading:
        return 'HTTP · $sizeText · $speedText';
      case TaskStatus.paused:
        return 'HTTP · $sizeText · 已暂停';
      case TaskStatus.completed:
        return 'HTTP · $sizeText';
      case TaskStatus.error:
        return 'HTTP · $sizeText · ${errorMessage.isEmpty ? '出错' : errorMessage}';
      case TaskStatus.pending:
        return 'HTTP · 等待中...';
      case TaskStatus.resuming:
        return 'HTTP · $sizeText · 恢复中...';
    }
  }

  /// 状态文本
  String get statusText {
    return switch (status) {
      TaskStatus.pending => '等待中',
      TaskStatus.downloading => '下载中',
      TaskStatus.paused => '已暂停',
      TaskStatus.completed => '已完成',
      TaskStatus.error => '出错',
      TaskStatus.resuming => '恢复中',
    };
  }

  /// 剩余时间估算
  String get etaText {
    if (status != TaskStatus.downloading || speed <= 0 || totalBytes <= 0) {
      return '—';
    }
    final remaining = totalBytes - downloadedBytes;
    final seconds = remaining / speed;
    if (seconds < 60) return '${seconds.toInt()} 秒';
    if (seconds < 3600) return '${(seconds / 60).toInt()} 分钟';
    return '${(seconds / 3600).toStringAsFixed(1)} 小时';
  }

  // ---------------------------------------------------------------------------
  // Utility
  // ---------------------------------------------------------------------------

  static String formatBytes(int bytes) {
    if (bytes <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    final i = (log(bytes) / log(1024)).floor().clamp(0, units.length - 1);
    final value = bytes / pow(1024, i);
    return '${value.toStringAsFixed(value >= 100 ? 0 : 1)} ${units[i]}';
  }
}
