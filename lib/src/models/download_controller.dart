import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:rinf/rinf.dart';

import '../bindings/bindings.dart';
import 'download_task.dart';

/// 顶部 Tab 状态筛选
enum StatusTab { all, downloading, completed, paused, error }

/// 核心状态管理器 — 桥接 Rust 信号和 Flutter UI
class DownloadController extends ChangeNotifier {
  final List<DownloadTask> _tasks = [];
  String? _selectedTaskId;
  FileCategory _categoryFilter = FileCategory.all;
  StatusTab _statusTab = StatusTab.all;

  StreamSubscription<RustSignalPack<TaskProgress>>? _progressSub;
  StreamSubscription<RustSignalPack<AllTasks>>? _allTasksSub;
  StreamSubscription<RustSignalPack<SegmentProgress>>? _segmentSub;

  DownloadController() {
    _startListening();
    // 启动时请求所有持久化任务
    const RequestAllTasks().sendSignalToRust();
  }

  @override
  void dispose() {
    _progressSub?.cancel();
    _allTasksSub?.cancel();
    _segmentSub?.cancel();
    super.dispose();
  }

  // ---------------------------------------------------------------------------
  // Public getters
  // ---------------------------------------------------------------------------

  List<DownloadTask> get tasks => _tasks;

  FileCategory get categoryFilter => _categoryFilter;
  StatusTab get statusTab => _statusTab;

  /// 按文件类型过滤（侧边栏维度）
  List<DownloadTask> get _categoryFiltered {
    if (_categoryFilter == FileCategory.all) return _tasks;
    return _tasks.where((t) => t.fileCategory == _categoryFilter).toList();
  }

  /// 双维度组合过滤后的任务列表（侧边栏文件类型 + 顶部状态 Tab）
  List<DownloadTask> get filteredTasks {
    final byCategory = _categoryFiltered;
    return switch (_statusTab) {
      StatusTab.all => byCategory,
      StatusTab.downloading =>
        byCategory
            .where(
              (t) =>
                  t.status == TaskStatus.downloading ||
                  t.status == TaskStatus.pending ||
                  t.status == TaskStatus.resuming,
            )
            .toList(),
      StatusTab.completed =>
        byCategory.where((t) => t.status == TaskStatus.completed).toList(),
      StatusTab.paused =>
        byCategory.where((t) => t.status == TaskStatus.paused).toList(),
      StatusTab.error =>
        byCategory.where((t) => t.status == TaskStatus.error).toList(),
    };
  }

  /// 在当前文件类型筛选下，各状态的任务数量（用于 Tab 显示计数）
  int filteredCountForStatus(StatusTab tab) {
    final byCategory = _categoryFiltered;
    return switch (tab) {
      StatusTab.all => byCategory.length,
      StatusTab.downloading =>
        byCategory
            .where(
              (t) =>
                  t.status == TaskStatus.downloading ||
                  t.status == TaskStatus.pending ||
                  t.status == TaskStatus.resuming,
            )
            .length,
      StatusTab.completed =>
        byCategory.where((t) => t.status == TaskStatus.completed).length,
      StatusTab.paused =>
        byCategory.where((t) => t.status == TaskStatus.paused).length,
      StatusTab.error =>
        byCategory.where((t) => t.status == TaskStatus.error).length,
    };
  }

  /// 各文件类型分类的任务数量（用于侧边栏显示计数）
  int countForCategory(FileCategory category) {
    if (category == FileCategory.all) return _tasks.length;
    return _tasks.where((t) => t.fileCategory == category).length;
  }

  String? get selectedTaskId => _selectedTaskId;

  DownloadTask? get selectedTask {
    if (_selectedTaskId == null) return null;
    final idx = _tasks.indexWhere((t) => t.id == _selectedTaskId);
    return idx >= 0 ? _tasks[idx] : null;
  }

  /// 统计数据
  int get downloadingCount =>
      _tasks.where((t) => t.status == TaskStatus.downloading).length;
  int get completedCount =>
      _tasks.where((t) => t.status == TaskStatus.completed).length;
  int get pausedCount =>
      _tasks.where((t) => t.status == TaskStatus.paused).length;
  int get errorCount =>
      _tasks.where((t) => t.status == TaskStatus.error).length;
  int get pendingCount =>
      _tasks.where((t) => t.status == TaskStatus.pending).length;
  int get resumingCount =>
      _tasks.where((t) => t.status == TaskStatus.resuming).length;
  int get activeCount => downloadingCount + pendingCount + resumingCount;

  /// 全局下载速度
  int get totalDownloadSpeed {
    int sum = 0;
    for (final t in _tasks) {
      if (t.status == TaskStatus.downloading) sum += t.speed;
    }
    return sum;
  }

  // ---------------------------------------------------------------------------
  // Actions — 发送信号到 Rust
  // ---------------------------------------------------------------------------

  void createTask({
    required String url,
    required String saveDir,
    String fileName = '',
    int segments = 0,
  }) {
    CreateTask(
      url: url,
      saveDir: saveDir,
      fileName: fileName,
      segments: segments,
    ).sendSignalToRust();
  }

  void pauseTask(String taskId) {
    // 乐观更新：立即切换到 paused 状态，防止用户快速重复点击
    final idx = _tasks.indexWhere((t) => t.id == taskId);
    if (idx >= 0) {
      final t = _tasks[idx];
      // 仅对活跃状态的任务执行暂停
      if (t.status == TaskStatus.downloading ||
          t.status == TaskStatus.resuming ||
          t.status == TaskStatus.pending) {
        _tasks[idx] = t.copyWith(status: TaskStatus.paused, speed: 0);
        notifyListeners();
      }
    }
    ControlTask(taskId: taskId, action: 0).sendSignalToRust();
  }

  void resumeTask(String taskId) {
    // 立即切换到 resuming 状态，让 UI 即时响应
    final idx = _tasks.indexWhere((t) => t.id == taskId);
    if (idx >= 0) {
      _tasks[idx] = _tasks[idx].copyWith(status: TaskStatus.resuming);
      notifyListeners();
    }
    ControlTask(taskId: taskId, action: 1).sendSignalToRust();
  }

  void cancelTask(String taskId) {
    ControlTask(taskId: taskId, action: 2).sendSignalToRust();
  }

  /// 删除任务。[deleteFiles] 为 true 时同时删除磁盘上的已下载文件。
  void deleteTask(String taskId, {bool deleteFiles = true}) {
    final action = deleteFiles ? 3 : 4;
    ControlTask(taskId: taskId, action: action).sendSignalToRust();
    _tasks.removeWhere((t) => t.id == taskId);
    if (_selectedTaskId == taskId) _selectedTaskId = null;
    notifyListeners();
  }

  void selectTask(String? taskId) {
    if (_selectedTaskId == taskId) return;
    _selectedTaskId = taskId;
    notifyListeners();
  }

  void setCategoryFilter(FileCategory category) {
    if (_categoryFilter == category) return;
    _categoryFilter = category;
    notifyListeners();
  }

  void setStatusTab(StatusTab tab) {
    if (_statusTab == tab) return;
    _statusTab = tab;
    notifyListeners();
  }

  void pauseAll() {
    for (final t in _tasks) {
      if (t.status == TaskStatus.downloading ||
          t.status == TaskStatus.resuming ||
          t.status == TaskStatus.pending) {
        pauseTask(t.id);
      }
    }
  }

  void resumeAll() {
    for (final t in _tasks) {
      if (t.status == TaskStatus.paused || t.status == TaskStatus.error) {
        resumeTask(t.id);
      }
    }
  }

  /// 默认下载目录
  static String get defaultSaveDir {
    // Windows: C:\Users\<user>\Downloads
    // macOS/Linux: ~/Downloads
    final home =
        Platform.environment['USERPROFILE'] ??
        Platform.environment['HOME'] ??
        '.';
    return '$home${Platform.pathSeparator}Downloads';
  }

  // ---------------------------------------------------------------------------
  // Signal listeners
  // ---------------------------------------------------------------------------

  void _startListening() {
    _allTasksSub = AllTasks.rustSignalStream.listen(_onAllTasks);
    _progressSub = TaskProgress.rustSignalStream.listen(_onProgress);
    _segmentSub = SegmentProgress.rustSignalStream.listen(_onSegmentProgress);
  }

  void _onAllTasks(RustSignalPack<AllTasks> pack) {
    final incoming = pack.message.tasks;
    _tasks.clear();
    for (final info in incoming) {
      _tasks.add(DownloadTask.fromTaskInfo(info));
    }
    notifyListeners();
  }

  void _onProgress(RustSignalPack<TaskProgress> pack) {
    final p = pack.message;
    final idx = _tasks.indexWhere((t) => t.id == p.taskId);
    if (idx >= 0) {
      _tasks[idx] = _tasks[idx].applyProgress(p);
    } else {
      // 新任务（刚刚创建的）
      _tasks.insert(
        0,
        DownloadTask(
          id: p.taskId,
          url: p.url,
          fileName: p.fileName.isEmpty ? '未知文件' : p.fileName,
          saveDir: p.saveDir,
          status: taskStatusFromInt(p.status),
          downloadedBytes: p.downloadedBytes,
          totalBytes: p.totalBytes,
          speed: p.speed,
          errorMessage: p.errorMessage,
        ),
      );
    }
    notifyListeners();
  }

  void _onSegmentProgress(RustSignalPack<SegmentProgress> pack) {
    final sp = pack.message;
    debugPrint(
      '[seg-vis] Dart received SegmentProgress: task=${sp.taskId}, '
      'segCount=${sp.segmentCount}, totalBytes=${sp.totalBytes}, '
      'segments=${sp.segments.length}',
    );
    final idx = _tasks.indexWhere((t) => t.id == sp.taskId);
    if (idx < 0) {
      debugPrint('[seg-vis] task not found in _tasks!');
      return;
    }

    final segments = sp.segments
        .map(
          (s) => SegmentData(
            index: s.index,
            startByte: s.startByte,
            endByte: s.endByte,
            downloadedBytes: s.downloadedBytes,
          ),
        )
        .toList();

    debugPrint(
      '[seg-vis] updating task[$idx] with ${segments.length} segments',
    );
    _tasks[idx] = _tasks[idx].copyWith(segments: segments);
    notifyListeners();
  }
}
