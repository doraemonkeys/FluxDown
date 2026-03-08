import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../i18n/locale_provider.dart';
import 'flux_theme_tokens.dart';

// ═══════════════════════════════════════════════════════════
//  内置主题定义
// ═══════════════════════════════════════════════════════════

/// 内置主题 ID — 每个 ID 对应一套完整的 FluxThemeTokens
enum BuiltinThemeId {
  defaultDark,
  defaultLight,
  midnightBlue,
  nord,
  warmLight,
}

/// 内置主题注册表条目
class BuiltinThemeEntry {
  final BuiltinThemeId id;
  final Brightness appearance;

  /// 不带强调色的固定预览色（用于主题卡片中显示代表色）
  final Color previewBg;
  final Color previewAccent;

  /// 生成完整 token 的工厂（支持传入强调色覆盖）
  final FluxThemeTokens Function({Color accent}) _factory;

  const BuiltinThemeEntry._({
    required this.id,
    required this.appearance,
    required this.previewBg,
    required this.previewAccent,
    required FluxThemeTokens Function({Color accent}) factory,
  }) : _factory = factory;

  FluxThemeTokens build({Color? accent}) =>
      accent != null ? _factory(accent: accent) : _factory();
}

/// 所有内置主题（顺序即 UI 显示顺序）
final builtinThemes = <BuiltinThemeEntry>[
  BuiltinThemeEntry._(
    id: BuiltinThemeId.defaultDark,
    appearance: Brightness.dark,
    previewBg: const Color(0xFF1C1C1E),
    previewAccent: const Color(0xFF3B82F6),
    factory: FluxThemeTokens.defaultDark,
  ),
  BuiltinThemeEntry._(
    id: BuiltinThemeId.defaultLight,
    appearance: Brightness.light,
    previewBg: const Color(0xFFF8F9FA),
    previewAccent: const Color(0xFF3B82F6),
    factory: FluxThemeTokens.defaultLight,
  ),
  BuiltinThemeEntry._(
    id: BuiltinThemeId.midnightBlue,
    appearance: Brightness.dark,
    previewBg: const Color(0xFF0F172A),
    previewAccent: const Color(0xFF60A5FA),
    factory: FluxThemeTokens.midnightBlue,
  ),
  BuiltinThemeEntry._(
    id: BuiltinThemeId.nord,
    appearance: Brightness.dark,
    previewBg: const Color(0xFF2E3440),
    previewAccent: const Color(0xFF88C0D0),
    factory: FluxThemeTokens.nord,
  ),
  BuiltinThemeEntry._(
    id: BuiltinThemeId.warmLight,
    appearance: Brightness.light,
    previewBg: const Color(0xFFFFFBEB),
    previewAccent: const Color(0xFFE11D48),
    factory: FluxThemeTokens.warmLight,
  ),
];

// ═══════════════════════════════════════════════════════════
//  强调色方案（快速切换强调色的简化入口）
// ═══════════════════════════════════════════════════════════

enum AppColorScheme {
  blue(Color(0xFF3B82F6)),
  green(Color(0xFF22C55E)),
  violet(Color(0xFF8B5CF6)),
  rose(Color(0xFFF43F5E)),
  custom(Color(0xFF6366F1));

  final Color previewColor;
  const AppColorScheme(this.previewColor);
}

extension AppColorSchemeI18n on AppColorScheme {
  String get label {
    final s = currentS;
    return switch (this) {
      AppColorScheme.blue => s.colorBlue,
      AppColorScheme.green => s.colorGreen,
      AppColorScheme.violet => s.colorViolet,
      AppColorScheme.rose => s.colorRose,
      AppColorScheme.custom => s.colorCustom,
    };
  }
}

// ═══════════════════════════════════════════════════════════
//  i18n 工具
// ═══════════════════════════════════════════════════════════

extension BuiltinThemeI18n on BuiltinThemeId {
  String get label {
    final s = currentS;
    return switch (this) {
      BuiltinThemeId.defaultDark => s.themeDefaultDark,
      BuiltinThemeId.defaultLight => s.themeDefaultLight,
      BuiltinThemeId.midnightBlue => s.themeMidnightBlue,
      BuiltinThemeId.nord => s.themeNord,
      BuiltinThemeId.warmLight => s.themeWarmLight,
    };
  }
}

// ═══════════════════════════════════════════════════════════
//  SharedPreferences 存储 key
// ═══════════════════════════════════════════════════════════

const _kThemeMode = 'theme_mode';
const _kSelectedTheme = 'selected_theme';
const _kColorScheme = 'color_scheme';
const _kCustomColor = 'custom_color';
const _kCustomThemeDark = 'custom_theme_dark_json';
const _kCustomThemeLight = 'custom_theme_light_json';

// ═══════════════════════════════════════════════════════════
//  ThemeProvider
// ═══════════════════════════════════════════════════════════

/// 全局主题管理器
///
/// 主题选择逻辑：
/// - [themeMode] = system 时，根据系统亮/暗自动选对应主题
///   - 暗色 → [selectedDarkTheme]
///   - 亮色 → [selectedLightTheme]
/// - [themeMode] = light/dark 时，强制使用对应主题
///
/// 主题来源优先级：
/// 1. 完整自定义主题 ([_customDarkTokens] / [_customLightTokens])
/// 2. 内置主题 + 强调色覆盖
class ThemeProvider extends ChangeNotifier {
  ThemeMode _themeMode = ThemeMode.system;

  /// 用户选择的暗色主题和亮色主题
  BuiltinThemeId _selectedDarkTheme = BuiltinThemeId.defaultDark;
  BuiltinThemeId _selectedLightTheme = BuiltinThemeId.defaultLight;

  /// 强调色
  AppColorScheme _colorScheme = AppColorScheme.blue;
  Color _customColor = const Color(0xFF6366F1);

  /// 完整自定义主题
  FluxThemeTokens? _customDarkTokens;
  FluxThemeTokens? _customLightTokens;

  /// 缓存
  FluxThemeTokens? _cachedTokens;
  bool _cachedIsDark = false;

  // ── Getters ──

  ThemeMode get themeMode => _themeMode;
  BuiltinThemeId get selectedDarkTheme => _selectedDarkTheme;
  BuiltinThemeId get selectedLightTheme => _selectedLightTheme;
  AppColorScheme get colorScheme => _colorScheme;
  Color get customColor => _customColor;

  FluxThemeTokens? get customDarkTokens => _customDarkTokens;
  FluxThemeTokens? get customLightTokens => _customLightTokens;

  bool get hasCustomTheme =>
      _customDarkTokens != null || _customLightTokens != null;

  String? get customThemeName =>
      _customDarkTokens?.name ?? _customLightTokens?.name;

  Color get activePreviewColor => _colorScheme == AppColorScheme.custom
      ? _customColor
      : _colorScheme.previewColor;

  /// 当前亮/暗模式下生效的内置主题 ID
  BuiltinThemeId activeBuiltinTheme(bool dark) =>
      dark ? _selectedDarkTheme : _selectedLightTheme;

  // ── 核心：计算当前 token ──

  FluxThemeTokens activeTokens(BuildContext context) {
    final dark = isDark(context);
    if (_cachedTokens != null && _cachedIsDark == dark) return _cachedTokens!;
    _cachedIsDark = dark;
    _cachedTokens = _computeTokens(dark);
    return _cachedTokens!;
  }

  FluxThemeTokens _computeTokens(bool dark) {
    // 优先级 1：完整自定义主题
    if (dark && _customDarkTokens != null) return _customDarkTokens!;
    if (!dark && _customLightTokens != null) return _customLightTokens!;

    // 优先级 2：内置主题 + 强调色覆盖
    final themeId = dark ? _selectedDarkTheme : _selectedLightTheme;
    final entry = builtinThemes.firstWhere((e) => e.id == themeId);
    final accent = _resolveAccentColor();
    return entry.build(accent: accent);
  }

  Color _resolveAccentColor() {
    return _colorScheme == AppColorScheme.custom
        ? _customColor
        : _colorScheme.previewColor;
  }

  // ═══════════════════════════════════════════════════════════
  //  初始化 & 持久化
  // ═══════════════════════════════════════════════════════════

  Future<void> init() async {
    final prefs = await SharedPreferences.getInstance();

    // 主题模式
    final modeStr = prefs.getString(_kThemeMode);
    if (modeStr != null) {
      _themeMode = ThemeMode.values.firstWhere(
        (m) => m.name == modeStr,
        orElse: () => ThemeMode.system,
      );
    }

    // 选中的主题
    final themeStr = prefs.getString(_kSelectedTheme);
    if (themeStr != null) {
      _loadSelectedThemes(themeStr);
    }

    // 强调色方案
    final schemeStr = prefs.getString(_kColorScheme);
    if (schemeStr != null) {
      _colorScheme = AppColorScheme.values.firstWhere(
        (s) => s.name == schemeStr,
        orElse: () => AppColorScheme.blue,
      );
    }

    // 自定义颜色
    final customHex = prefs.getString(_kCustomColor);
    if (customHex != null) {
      final parsed = int.tryParse(customHex, radix: 16);
      if (parsed != null) _customColor = Color(parsed);
    }

    // 完整自定义主题 JSON
    _customDarkTokens = _loadTokensFromPrefs(prefs, _kCustomThemeDark);
    _customLightTokens = _loadTokensFromPrefs(prefs, _kCustomThemeLight);
  }

  // ── 主题模式 ──

  void setThemeMode(ThemeMode mode) {
    if (_themeMode == mode) return;
    _themeMode = mode;
    _invalidateCache();
    notifyListeners();
    _persist(_kThemeMode, mode.name);
  }

  // ── 主题选择 ──

  /// 选择暗色主题（从内置主题中选）
  void setDarkTheme(BuiltinThemeId id) {
    if (_selectedDarkTheme == id) return;
    _selectedDarkTheme = id;
    _customDarkTokens = null; // 清除自定义覆盖
    _persistRemove(_kCustomThemeDark);
    _invalidateCache();
    notifyListeners();
    _persistSelectedThemes();
  }

  /// 选择亮色主题（从内置主题中选）
  void setLightTheme(BuiltinThemeId id) {
    if (_selectedLightTheme == id) return;
    _selectedLightTheme = id;
    _customLightTokens = null;
    _persistRemove(_kCustomThemeLight);
    _invalidateCache();
    notifyListeners();
    _persistSelectedThemes();
  }

  // ── 强调色 ──

  void setColorScheme(AppColorScheme scheme) {
    if (_colorScheme == scheme) return;
    _colorScheme = scheme;
    _invalidateCache();
    notifyListeners();
    _persist(_kColorScheme, scheme.name);
  }

  void setCustomColor(Color color) {
    _customColor = color;
    if (_colorScheme != AppColorScheme.custom) {
      _colorScheme = AppColorScheme.custom;
      _persist(_kColorScheme, AppColorScheme.custom.name);
    }
    _invalidateCache();
    notifyListeners();
    _persist(
      _kCustomColor,
      color.toARGB32().toRadixString(16).padLeft(8, '0'),
    );
  }

  // ── 便捷操作 ──

  void toggleTheme(BuildContext context) {
    final brightness = MediaQuery.platformBrightnessOf(context);
    final currentDark =
        _themeMode == ThemeMode.dark ||
        (_themeMode == ThemeMode.system && brightness == Brightness.dark);
    setThemeMode(currentDark ? ThemeMode.light : ThemeMode.dark);
  }

  bool isDark(BuildContext context) {
    if (_themeMode == ThemeMode.system) {
      return MediaQuery.platformBrightnessOf(context) == Brightness.dark;
    }
    return _themeMode == ThemeMode.dark;
  }

  // ═══════════════════════════════════════════════════════════
  //  完整自定义主题管理
  // ═══════════════════════════════════════════════════════════

  /// 设置自定义主题。传入的参数会覆盖对应侧，null 值不会清除（用 clearCustomTheme）。
  void setCustomTheme({
    FluxThemeTokens? dark,
    FluxThemeTokens? light,
  }) {
    if (dark != null) {
      _customDarkTokens = dark;
      _persistTokens(_kCustomThemeDark, dark);
    }
    if (light != null) {
      _customLightTokens = light;
      _persistTokens(_kCustomThemeLight, light);
    }
    _invalidateCache();
    notifyListeners();
  }

  /// 清除某侧的自定义主题（回到内置主题）
  void clearCustomTheme({required bool dark}) {
    if (dark) {
      _customDarkTokens = null;
      _persistRemove(_kCustomThemeDark);
    } else {
      _customLightTokens = null;
      _persistRemove(_kCustomThemeLight);
    }
    _invalidateCache();
    notifyListeners();
  }

  /// 激活自定义主题（当用户点击自定义主题卡片时）
  /// 确保自定义主题生效 — 实际上 _computeTokens 中自定义主题优先级最高，
  /// 只要存在就会生效，所以这里只需 invalidate 并通知。
  void activateCustomTheme({required bool dark}) {
    _invalidateCache();
    notifyListeners();
  }

  void updateToken({
    required bool dark,
    required FluxThemeTokens Function(FluxThemeTokens) updater,
  }) {
    final accent = _resolveAccentColor();
    if (dark) {
      final themeEntry = builtinThemes.firstWhere(
        (e) => e.id == _selectedDarkTheme,
      );
      _customDarkTokens = updater(
        _customDarkTokens ?? themeEntry.build(accent: accent),
      );
      _persistTokens(_kCustomThemeDark, _customDarkTokens);
    } else {
      final themeEntry = builtinThemes.firstWhere(
        (e) => e.id == _selectedLightTheme,
      );
      _customLightTokens = updater(
        _customLightTokens ?? themeEntry.build(accent: accent),
      );
      _persistTokens(_kCustomThemeLight, _customLightTokens);
    }
    _invalidateCache();
    notifyListeners();
  }

  void resetToDefault() {
    _customDarkTokens = null;
    _customLightTokens = null;
    _selectedDarkTheme = BuiltinThemeId.defaultDark;
    _selectedLightTheme = BuiltinThemeId.defaultLight;
    _colorScheme = AppColorScheme.blue;
    _invalidateCache();
    notifyListeners();
    _persist(_kColorScheme, AppColorScheme.blue.name);
    _persistRemove(_kCustomThemeDark);
    _persistRemove(_kCustomThemeLight);
    _persistSelectedThemes();
  }

  // ═══════════════════════════════════════════════════════════
  //  主题导入 / 导出
  // ═══════════════════════════════════════════════════════════

  String exportThemeJson(FluxThemeTokens tokens) {
    return const JsonEncoder.withIndent('  ').convert(tokens.toJson());
  }

  FluxThemeTokens importThemeJson(String jsonStr) {
    final json = jsonDecode(jsonStr) as Map<String, dynamic>;
    return FluxThemeTokens.fromJson(json);
  }

  FluxThemeTokens getExportableTokens(bool dark) {
    return _computeTokens(dark);
  }

  // ═══════════════════════════════════════════════════════════
  //  内部辅助
  // ═══════════════════════════════════════════════════════════

  void _invalidateCache() {
    _cachedTokens = null;
  }

  /// 持久化选中主题：格式 "darkId:lightId"
  void _persistSelectedThemes() {
    _persist(
      _kSelectedTheme,
      '${_selectedDarkTheme.name}:${_selectedLightTheme.name}',
    );
  }

  void _loadSelectedThemes(String str) {
    final parts = str.split(':');
    if (parts.length == 2) {
      _selectedDarkTheme = BuiltinThemeId.values.firstWhere(
        (e) => e.name == parts[0],
        orElse: () => BuiltinThemeId.defaultDark,
      );
      _selectedLightTheme = BuiltinThemeId.values.firstWhere(
        (e) => e.name == parts[1],
        orElse: () => BuiltinThemeId.defaultLight,
      );
    }
  }

  FluxThemeTokens? _loadTokensFromPrefs(SharedPreferences prefs, String key) {
    final jsonStr = prefs.getString(key);
    if (jsonStr == null) return null;
    try {
      final json = jsonDecode(jsonStr) as Map<String, dynamic>;
      return FluxThemeTokens.fromJson(json);
    } catch (_) {
      return null;
    }
  }

  Future<void> _persistTokens(String key, FluxThemeTokens? tokens) async {
    if (tokens == null) {
      _persistRemove(key);
      return;
    }
    _persist(key, jsonEncode(tokens.toJson()));
  }

  Future<void> _persist(String key, String value) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(key, value);
  }

  Future<void> _persistRemove(String key) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(key);
  }
}
