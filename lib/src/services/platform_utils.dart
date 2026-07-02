import 'dart:io';

/// Marker file name — a zero-byte file placed next to the exe by the portable
/// ZIP distribution.  Matches the Rust-side `PORTABLE_MARKER` constant.
const portableMarker = 'portable';

/// Whether the current Windows installation is portable mode.
///
/// Portable mode is detected by the presence of a `portable` marker file
/// next to the executable.  On non-Windows platforms this always returns false.
bool isPortableMode() {
  if (!Platform.isWindows) return false;
  try {
    final exeDir = File(Platform.resolvedExecutable).parent.path;
    return File('$exeDir${Platform.pathSeparator}$portableMarker').existsSync();
  } catch (_) {
    return false;
  }
}

/// Resolve the application data directory.
///
/// | Platform        | Mode      | Directory                                      |
/// |-----------------|-----------|-------------------------------------------------|
/// | Windows         | Portable  | `<exe_dir>/`  (data travels with the app)       |
/// | Windows         | Installed | `%LOCALAPPDATA%\FluxDown\`                      |
/// | Linux           | —         | `$XDG_DATA_HOME/fluxdown/`                      |
/// | macOS           | —         | `~/Library/Application Support/fluxdown/`        |
String resolveDataDir() {
  if (Platform.isLinux) {
    final xdgData = Platform.environment['XDG_DATA_HOME'] ??
        '${Platform.environment['HOME']}/.local/share';
    return '$xdgData/fluxdown';
  }
  if (Platform.isMacOS) {
    final home = Platform.environment['HOME'] ?? '';
    return '$home/Library/Application Support/fluxdown';
  }
  // Windows
  if (Platform.isWindows && isPortableMode()) {
    return File(Platform.resolvedExecutable).parent.path;
  }
  // Windows installed mode
  final localAppData = Platform.environment['LOCALAPPDATA'] ??
      Platform.environment['APPDATA'] ??
      File(Platform.resolvedExecutable).parent.path;
  return '$localAppData${Platform.pathSeparator}FluxDown';
}
