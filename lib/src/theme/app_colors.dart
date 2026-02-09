import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

/// 主题感知色板 — 通过 AppColors.of(context) 获取
///
/// accent 系列颜色从 ShadTheme 的 colorScheme.primary 派生，
/// 跟随用户选择的主题色自动变化。
class AppColors {
  final Brightness _brightness;
  final ShadColorScheme _scheme;

  const AppColors._(this._brightness, this._scheme);

  factory AppColors.of(BuildContext context) {
    final theme = ShadTheme.of(context);
    return AppColors._(theme.brightness, theme.colorScheme);
  }

  bool get _isDark => _brightness == Brightness.dark;

  // Backgrounds
  Color get bg => _isDark ? const Color(0xFF0A0A0B) : const Color(0xFFF8F9FA);
  Color get surface1 =>
      _isDark ? const Color(0xFF111113) : const Color(0xFFFFFFFF);
  Color get surface2 =>
      _isDark ? const Color(0xFF1A1A1D) : const Color(0xFFF1F3F5);
  Color get surface3 =>
      _isDark ? const Color(0xFF232326) : const Color(0xFFE9ECEF);

  // Hover (subtle, non-flickering)
  Color get hoverBg =>
      _isDark ? const Color(0xFF1A1A1D) : const Color(0xFFF5F5F5);

  // Borders
  Color get border =>
      _isDark ? const Color(0xFF27272A) : const Color(0xFFE4E4E7);

  // Text
  Color get textPrimary =>
      _isDark ? const Color(0xFFFAFAFA) : const Color(0xFF09090B);
  Color get textSecondary =>
      _isDark ? const Color(0xFFA1A1AA) : const Color(0xFF71717A);
  Color get textMuted =>
      _isDark ? const Color(0xFF52525B) : const Color(0xFFA1A1AA);

  // Accent — 跟随主题色
  Color get accent => _scheme.primary;
  Color get accentHover {
    // 亮色模式下加亮，暗色模式下微调
    final hsl = HSLColor.fromColor(_scheme.primary);
    return _isDark
        ? hsl.withLightness((hsl.lightness + 0.08).clamp(0.0, 1.0)).toColor()
        : hsl.withLightness((hsl.lightness + 0.06).clamp(0.0, 1.0)).toColor();
  }

  Color get accentBg => _scheme.primary.withValues(alpha: 0.10);

  // Status (固定 — 语义色，不跟随主题)
  static const green = Color(0xFF22C55E);
  static const amber = Color(0xFFF59E0B);
  static const red = Color(0xFFEF4444);
}
