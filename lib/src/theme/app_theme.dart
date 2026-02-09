import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import 'theme_provider.dart';

/// MiSans 字体族名（与 pubspec.yaml 中声明的 family 一致）
const _fontFamily = 'MiSans';

/// 构建紧凑的按钮尺寸主题（降低所有变体高度）
const _buttonSizes = ShadButtonSizesTheme(
  regular: ShadButtonSizeTheme(
    height: 32,
    padding: EdgeInsets.symmetric(horizontal: 12, vertical: 4),
  ),
  sm: ShadButtonSizeTheme(
    height: 28,
    padding: EdgeInsets.symmetric(horizontal: 10, vertical: 2),
  ),
  lg: ShadButtonSizeTheme(
    height: 36,
    padding: EdgeInsets.symmetric(horizontal: 20, vertical: 6),
  ),
  icon: ShadButtonSizeTheme(height: 32, width: 32, padding: EdgeInsets.zero),
);

/// 根据 AppColorScheme 获取 shadcn 颜色方案（亮色）
ShadColorScheme _lightColorScheme(AppColorScheme scheme) {
  return switch (scheme) {
    AppColorScheme.blue => const ShadBlueColorScheme.light(),
    AppColorScheme.green => const ShadGreenColorScheme.light(),
    AppColorScheme.violet => const ShadVioletColorScheme.light(),
    AppColorScheme.rose => const ShadRoseColorScheme.light(),
    AppColorScheme.orange => const ShadOrangeColorScheme.light(),
    AppColorScheme.red => const ShadRedColorScheme.light(),
    AppColorScheme.yellow => const ShadYellowColorScheme.light(),
    AppColorScheme.slate => const ShadSlateColorScheme.light(),
    AppColorScheme.zinc => const ShadZincColorScheme.light(),
    AppColorScheme.gray => const ShadGrayColorScheme.light(),
    AppColorScheme.neutral => const ShadNeutralColorScheme.light(),
    AppColorScheme.stone => const ShadStoneColorScheme.light(),
  };
}

/// 根据 AppColorScheme 获取 shadcn 颜色方案（暗色）
ShadColorScheme _darkColorScheme(AppColorScheme scheme) {
  return switch (scheme) {
    AppColorScheme.blue => const ShadBlueColorScheme.dark(),
    AppColorScheme.green => const ShadGreenColorScheme.dark(),
    AppColorScheme.violet => const ShadVioletColorScheme.dark(),
    AppColorScheme.rose => const ShadRoseColorScheme.dark(),
    AppColorScheme.orange => const ShadOrangeColorScheme.dark(),
    AppColorScheme.red => const ShadRedColorScheme.dark(),
    AppColorScheme.yellow => const ShadYellowColorScheme.dark(),
    AppColorScheme.slate => const ShadSlateColorScheme.dark(),
    AppColorScheme.zinc => const ShadZincColorScheme.dark(),
    AppColorScheme.gray => const ShadGrayColorScheme.dark(),
    AppColorScheme.neutral => const ShadNeutralColorScheme.dark(),
    AppColorScheme.stone => const ShadStoneColorScheme.dark(),
  };
}

/// 缓存当前颜色方案对应的主题数据
AppColorScheme? _cachedScheme;
ShadThemeData? _cachedLight;
ShadThemeData? _cachedDark;

/// 清除主题缓存（颜色方案变更时调用）
void invalidateThemeCache() {
  _cachedScheme = null;
  _cachedLight = null;
  _cachedDark = null;
}

void _ensureCache(AppColorScheme scheme) {
  if (_cachedScheme == scheme) return;
  _cachedScheme = scheme;
  _cachedLight = ShadThemeData(
    brightness: Brightness.light,
    colorScheme: _lightColorScheme(scheme),
    textTheme: ShadTextTheme(family: _fontFamily),
    buttonSizesTheme: _buttonSizes,
  );
  _cachedDark = ShadThemeData(
    brightness: Brightness.dark,
    colorScheme: _darkColorScheme(scheme),
    textTheme: ShadTextTheme(family: _fontFamily),
    buttonSizesTheme: _buttonSizes,
  );
}

ShadThemeData buildLightTheme([AppColorScheme scheme = AppColorScheme.blue]) {
  _ensureCache(scheme);
  return _cachedLight!;
}

ShadThemeData buildDarkTheme([AppColorScheme scheme = AppColorScheme.blue]) {
  _ensureCache(scheme);
  return _cachedDark!;
}
