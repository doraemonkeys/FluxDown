/// 组件管理状态（v1 仅 ffmpeg；后续新增组件可复用同一模式扩展）。
///
/// ffmpeg 是可选的外部工具，由官方源（BtbN/FFmpeg-Builds）按需下载，
/// 不随安装包分发；用于合并音视频轨（DASH/轨对任务）。
library;

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:rinf/rinf.dart';

import '../bindings/bindings.dart';
import '../services/log_service.dart';

/// config 键：手动指定的 ffmpeg 路径。须与 Rust 端
/// `fluxdown_engine::components::CONFIG_FFMPEG_PATH` 保持一致。
const kFfmpegManualPathConfigKey = 'component.ffmpeg.path';

/// ffmpeg 组件状态管理。
///
/// 复刻 [PluginProvider]（见 `plugin_provider.dart`）的 ChangeNotifier +
/// rinf 信号订阅模式：构造时建立信号订阅，`requestStatus()`/
/// `requestVersions()` 主动拉取，写操作（install/uninstall/
/// saveManualPath）均为单向 `.sendSignalToRust()`，结果经
/// [FfmpegStatusReport]/[FfmpegVersionList]/[FfmpegInstallResult] 信号
/// 异步回流。手动路径的当前值借道全局 [ConfigLoaded] 信号读取（由应用
/// 启动时既有的 `RequestConfig` 触发），不重复发起整表配置拉取。
class ComponentsProvider extends ChangeNotifier {
  FfmpegStatusReport? _status;
  bool _statusLoading = false;

  String _manualPath = '';

  List<String> _versions = [];
  String _latestStable = '';
  bool _versionsLoading = false;
  String _versionsError = '';

  bool _installing = false;
  int _downloadedBytes = 0;
  int _totalBytes = 0;

  FfmpegInstallResult? _lastInstallResult;
  int _installResultSeq = 0;

  bool _disposed = false;

  StreamSubscription<RustSignalPack<FfmpegStatusReport>>? _statusSub;
  StreamSubscription<RustSignalPack<FfmpegVersionList>>? _versionsSub;
  StreamSubscription<RustSignalPack<FfmpegInstallProgress>>? _progressSub;
  StreamSubscription<RustSignalPack<FfmpegInstallResult>>? _resultSub;
  StreamSubscription<RustSignalPack<ConfigLoaded>>? _configSub;

  ComponentsProvider() {
    logInfo('Components', 'constructor');
    _startListening();
    _loadManualPathFromCache();
  }

  @override
  void dispose() {
    logInfo('Components', 'dispose');
    _disposed = true;
    _statusSub?.cancel();
    _versionsSub?.cancel();
    _progressSub?.cancel();
    _resultSub?.cancel();
    _configSub?.cancel();
    super.dispose();
  }

  /// 防止信号在 Provider 已释放后回调触发 "used after being disposed" 异常。
  void _safeNotifyListeners() {
    if (!_disposed) notifyListeners();
  }

  // ---------------------------------------------------------------------------
  // Getters
  // ---------------------------------------------------------------------------

  FfmpegStatusReport? get status => _status;
  bool get statusLoading => _statusLoading;

  /// 用户当前保存的手动路径（空 = 未设置）。与 [status] 的生效路径独立——
  /// 手动路径失效（文件不存在）时生效来源会回退，但此值仍展示用户的原始输入。
  String get manualPath => _manualPath;

  List<String> get versions => List.unmodifiable(_versions);
  String get latestStable => _latestStable;
  bool get versionsLoading => _versionsLoading;
  String get versionsError => _versionsError;

  bool get installing => _installing;
  int get downloadedBytes => _downloadedBytes;
  int get totalBytes => _totalBytes;

  /// 最近一次安装/卸载操作的结果。
  FfmpegInstallResult? get lastInstallResult => _lastInstallResult;

  /// 随每次 [FfmpegInstallResult] 信号单调递增，供调用方判断"是否是新结果"。
  int get installResultSeq => _installResultSeq;

  // ---------------------------------------------------------------------------
  // 信号订阅
  // ---------------------------------------------------------------------------

  void _startListening() {
    _statusSub = FfmpegStatusReport.rustSignalStream.listen(_onStatus);
    _versionsSub = FfmpegVersionList.rustSignalStream.listen(_onVersions);
    _progressSub = FfmpegInstallProgress.rustSignalStream.listen(_onProgress);
    _resultSub = FfmpegInstallResult.rustSignalStream.listen(_onResult);
    _configSub = ConfigLoaded.rustSignalStream.listen(_onConfigLoaded);
  }

  void _loadManualPathFromCache() {
    final cached = ConfigLoaded.latestRustSignal?.message;
    if (cached != null) _applyManualPathFromEntries(cached.entries);
  }

  void _onConfigLoaded(RustSignalPack<ConfigLoaded> pack) {
    _applyManualPathFromEntries(pack.message.entries);
    _safeNotifyListeners();
  }

  void _applyManualPathFromEntries(List<ConfigEntry> entries) {
    for (final e in entries) {
      if (e.key == kFfmpegManualPathConfigKey) {
        _manualPath = e.value;
        return;
      }
    }
  }

  void _onStatus(RustSignalPack<FfmpegStatusReport> pack) {
    _status = pack.message;
    _statusLoading = false;
    logInfo(
      'Components',
      'ffmpeg status: source=${pack.message.source} '
          'version=${pack.message.version} path=${pack.message.path}',
    );
    _safeNotifyListeners();
  }

  void _onVersions(RustSignalPack<FfmpegVersionList> pack) {
    _versionsLoading = false;
    if (pack.message.ok) {
      _versions = pack.message.versions;
      _latestStable = pack.message.latestStable;
      _versionsError = '';
    } else {
      _versions = [];
      _latestStable = '';
      _versionsError = pack.message.message;
    }
    logInfo(
      'Components',
      'ffmpeg versions: ok=${pack.message.ok} '
          'count=${pack.message.versions.length}',
    );
    _safeNotifyListeners();
  }

  void _onProgress(RustSignalPack<FfmpegInstallProgress> pack) {
    _installing = true;
    _downloadedBytes = pack.message.downloadedBytes;
    _totalBytes = pack.message.totalBytes;
    _safeNotifyListeners();
  }

  void _onResult(RustSignalPack<FfmpegInstallResult> pack) {
    _installing = false;
    _downloadedBytes = 0;
    _totalBytes = 0;
    _lastInstallResult = pack.message;
    _installResultSeq++;
    logInfo(
      'Components',
      'ffmpeg install result: ok=${pack.message.ok} '
          'message=${pack.message.message}',
    );
    _safeNotifyListeners();
  }

  // ---------------------------------------------------------------------------
  // 写操作（均为单向信号，结果经上述信号异步回流）
  // ---------------------------------------------------------------------------

  /// 请求当前 ffmpeg 状态（进入组件设置分类时调用）。
  void requestStatus() {
    logInfo('Components', 'requestStatus');
    _statusLoading = true;
    _safeNotifyListeners();
    const RequestFfmpegStatus().sendSignalToRust();
  }

  /// 请求可安装版本列表（懒加载：首次展开安装区时调用）。
  void requestVersions() {
    logInfo('Components', 'requestVersions');
    _versionsLoading = true;
    _versionsError = '';
    _safeNotifyListeners();
    const RequestFfmpegVersions().sendSignalToRust();
  }

  /// 安装（或更新/重装）托管 ffmpeg。[version] 空 = 最新稳定版。
  void install(String version) {
    logInfo('Components', 'install: version=$version');
    _installing = true;
    _downloadedBytes = 0;
    _totalBytes = 0;
    _safeNotifyListeners();
    InstallFfmpeg(version: version).sendSignalToRust();
  }

  /// 卸载托管 ffmpeg（手动/系统路径不受影响）。
  void uninstall() {
    logInfo('Components', 'uninstall');
    const UninstallFfmpeg().sendSignalToRust();
  }

  /// 保存手动指定路径（空串 = 清除）；写入后重新探测状态。
  void saveManualPath(String path) {
    logInfo('Components', 'saveManualPath: $path');
    _manualPath = path;
    _safeNotifyListeners();
    SaveConfig(
      key: kFfmpegManualPathConfigKey,
      value: path,
    ).sendSignalToRust();
    requestStatus();
  }
}
