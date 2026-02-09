import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import '../../main.dart';
import '../models/settings_provider.dart';
import '../theme/app_colors.dart';
import '../theme/theme_provider.dart';
import '../widgets/title_drag_area.dart';

class SettingsPage extends StatelessWidget {
  final VoidCallback onBack;
  final SettingsProvider settingsProvider;

  const SettingsPage({
    super.key,
    required this.onBack,
    required this.settingsProvider,
  });

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return Column(
      children: [
        // 顶部标题栏
        TitleDragArea(
          child: Container(
            height: 42,
            padding: const EdgeInsets.only(left: 12, right: 257),
            decoration: BoxDecoration(
              color: c.surface1,
              border: Border(bottom: BorderSide(color: c.border, width: 1)),
            ),
            child: Row(
              children: [
                // 返回按钮
                ShadButton.ghost(
                  onPressed: onBack,
                  size: ShadButtonSize.sm,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        LucideIcons.arrowLeft,
                        size: 14,
                        color: c.textSecondary,
                      ),
                      const SizedBox(width: 6),
                      Text(
                        '返回',
                        style: TextStyle(fontSize: 13, color: c.textSecondary),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                Text(
                  '设置',
                  style: TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                    color: c.textPrimary,
                  ),
                ),
              ],
            ),
          ),
        ),
        // 内容区域
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.symmetric(horizontal: 32, vertical: 24),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 640),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _GeneralSection(settingsProvider: settingsProvider),
                    const SizedBox(height: 32),
                    const _AppearanceSection(),
                    const SizedBox(height: 32),
                    _DownloadSection(settingsProvider: settingsProvider),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 通用设置区块
// ─────────────────────────────────────────────

class _GeneralSection extends StatelessWidget {
  final SettingsProvider settingsProvider;

  const _GeneralSection({required this.settingsProvider});

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return ListenableBuilder(
      listenable: settingsProvider,
      builder: (context, _) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // 区块标题
            Row(
              children: [
                Icon(LucideIcons.settings2, size: 16, color: c.textPrimary),
                const SizedBox(width: 8),
                Text(
                  '通用',
                  style: TextStyle(
                    fontSize: 15,
                    fontWeight: FontWeight.w600,
                    color: c.textPrimary,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              '应用的基本行为设置',
              style: TextStyle(fontSize: 12, color: c.textMuted),
            ),
            const SizedBox(height: 20),

            // 开机启动
            _SettingRow(
              label: '开机自启动',
              description: '系统启动时自动运行 FluxDown',
              child: ShadSwitch(
                value: settingsProvider.autoStartup,
                onChanged: (v) async {
                  final ok = await settingsProvider.setAutoStartup(v);
                  if (!ok && context.mounted) {
                    showShadDialog(
                      context: context,
                      builder: (ctx) => ShadDialog.alert(
                        title: const Text('设置失败'),
                        description: const Text('无法修改开机自启动设置，请检查系统权限。'),
                        actions: [
                          ShadButton(
                            child: const Text('确定'),
                            onPressed: () => Navigator.of(ctx).pop(),
                          ),
                        ],
                      ),
                    );
                  }
                },
              ),
            ),
            const SizedBox(height: 20),

            // 关闭到托盘
            _SettingRow(
              label: '关闭时最小化到托盘',
              description: '点击关闭按钮时隐藏到系统托盘，而非退出应用',
              child: ShadSwitch(
                value: settingsProvider.closeToTray,
                onChanged: (v) => settingsProvider.setCloseToTray(v),
              ),
            ),
          ],
        );
      },
    );
  }
}

// ─────────────────────────────────────────────
// 外观设置区块
// ─────────────────────────────────────────────

class _AppearanceSection extends StatelessWidget {
  const _AppearanceSection();

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // 区块标题
        Row(
          children: [
            Icon(LucideIcons.palette, size: 16, color: c.textPrimary),
            const SizedBox(width: 8),
            Text(
              '外观',
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w600,
                color: c.textPrimary,
              ),
            ),
          ],
        ),
        const SizedBox(height: 4),
        Text('自定义应用的外观主题', style: TextStyle(fontSize: 12, color: c.textMuted)),
        const SizedBox(height: 20),

        // 主题模式
        _SettingRow(
          label: '主题',
          description: '选择亮色、暗色或跟随系统',
          child: const _ThemeModeSelector(),
        ),
        const SizedBox(height: 20),

        // 主题色
        _SettingRow(
          label: '主题色',
          description: '选择应用的主色调',
          child: const _ColorSchemeSelector(),
        ),
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 下载设置区块
// ─────────────────────────────────────────────

class _DownloadSection extends StatelessWidget {
  final SettingsProvider settingsProvider;

  const _DownloadSection({required this.settingsProvider});

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return ListenableBuilder(
      listenable: settingsProvider,
      builder: (context, _) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // 区块标题
            Row(
              children: [
                Icon(LucideIcons.download, size: 16, color: c.textPrimary),
                const SizedBox(width: 8),
                Text(
                  '下载',
                  style: TextStyle(
                    fontSize: 15,
                    fontWeight: FontWeight.w600,
                    color: c.textPrimary,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              '配置下载引擎的默认参数',
              style: TextStyle(fontSize: 12, color: c.textMuted),
            ),
            const SizedBox(height: 20),

            // 默认保存目录
            _SettingRow(
              label: '默认保存目录',
              description: '新建下载任务时的默认保存位置',
              child: _SaveDirPicker(settingsProvider: settingsProvider),
            ),
            const SizedBox(height: 20),

            // 默认线程数
            _SettingRow(
              label: '默认线程数',
              description: '每个下载任务的默认分片数量',
              child: _SegmentSelector(settingsProvider: settingsProvider),
            ),
            const SizedBox(height: 20),

            // 最大同时下载数
            _SettingRow(
              label: '最大同时下载数',
              description: '同时进行的最大下载任务数量',
              child: _ConcurrentSelector(settingsProvider: settingsProvider),
            ),
            const SizedBox(height: 20),

            // 速度限制
            _SettingRow(
              label: '速度限制',
              description: '限制全局下载速度（0 表示不限制）',
              child: _SpeedLimitInput(settingsProvider: settingsProvider),
            ),
          ],
        );
      },
    );
  }
}

// ─────────────────────────────────────────────
// 下载设置子组件
// ─────────────────────────────────────────────

class _SaveDirPicker extends StatelessWidget {
  final SettingsProvider settingsProvider;

  const _SaveDirPicker({required this.settingsProvider});

  Future<void> _pickDir(BuildContext context) async {
    final result = await FilePicker.platform.getDirectoryPath(
      dialogTitle: '选择默认保存目录',
      initialDirectory: settingsProvider.defaultSaveDir,
    );
    if (result != null) {
      settingsProvider.setDefaultSaveDir(result);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return Row(
      children: [
        Expanded(
          child: Container(
            height: 36,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            decoration: BoxDecoration(
              color: c.surface1,
              borderRadius: BorderRadius.circular(6),
              border: Border.all(color: c.border, width: 1),
            ),
            alignment: Alignment.centerLeft,
            child: Text(
              settingsProvider.defaultSaveDir,
              style: TextStyle(fontSize: 13, color: c.textPrimary),
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ),
        const SizedBox(width: 8),
        ShadButton.outline(
          size: ShadButtonSize.sm,
          onPressed: () => _pickDir(context),
          child: const Text('浏览'),
        ),
      ],
    );
  }
}

class _SegmentSelector extends StatelessWidget {
  final SettingsProvider settingsProvider;

  const _SegmentSelector({required this.settingsProvider});

  // 0 = 自动（由 Rust segment_advisor 动态计算最优值）
  static const _options = [0, 4, 8, 16, 32, 64];

  static String _label(int n) => n == 0 ? '自动' : '$n 线程';

  @override
  Widget build(BuildContext context) {
    final current = settingsProvider.defaultSegments;
    return ShadSelect<int>(
      placeholder: const Text('自动'),
      initialValue: current,
      options: _options
          .map((n) => ShadOption(value: n, child: Text(_label(n))))
          .toList(),
      selectedOptionBuilder: (context, value) => Text(_label(value)),
      onChanged: (v) {
        if (v != null) settingsProvider.setDefaultSegments(v);
      },
    );
  }
}

class _ConcurrentSelector extends StatelessWidget {
  final SettingsProvider settingsProvider;

  const _ConcurrentSelector({required this.settingsProvider});

  static const _options = [1, 2, 3, 5, 8, 10];

  @override
  Widget build(BuildContext context) {
    final current = settingsProvider.maxConcurrentTasks;
    return ShadSelect<int>(
      placeholder: Text('$current'),
      initialValue: current,
      options: _options
          .map((n) => ShadOption(value: n, child: Text('$n')))
          .toList(),
      selectedOptionBuilder: (context, value) => Text('$value 个任务'),
      onChanged: (v) {
        if (v != null) settingsProvider.setMaxConcurrentTasks(v);
      },
    );
  }
}

class _SpeedLimitInput extends StatefulWidget {
  final SettingsProvider settingsProvider;

  const _SpeedLimitInput({required this.settingsProvider});

  @override
  State<_SpeedLimitInput> createState() => _SpeedLimitInputState();
}

class _SpeedLimitInputState extends State<_SpeedLimitInput> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    final kbps = widget.settingsProvider.speedLimitBytes ~/ 1024;
    _controller = TextEditingController(text: kbps == 0 ? '0' : '$kbps');
  }

  @override
  void didUpdateWidget(_SpeedLimitInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    final kbps = widget.settingsProvider.speedLimitBytes ~/ 1024;
    final current = int.tryParse(_controller.text) ?? 0;
    if (kbps != current) {
      _controller.text = kbps == 0 ? '0' : '$kbps';
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _onSubmit(String value) {
    final kbps = int.tryParse(value) ?? 0;
    widget.settingsProvider.setSpeedLimitBytes(kbps * 1024);
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return Row(
      children: [
        SizedBox(
          width: 120,
          child: ShadInput(
            controller: _controller,
            placeholder: const Text('0'),
            onSubmitted: _onSubmit,
            onChanged: _onSubmit,
          ),
        ),
        const SizedBox(width: 8),
        Text(
          'KB/s（0 = 不限制）',
          style: TextStyle(fontSize: 12, color: c.textMuted),
        ),
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 通用设置行
// ─────────────────────────────────────────────

class _SettingRow extends StatelessWidget {
  final String label;
  final String description;
  final Widget child;

  const _SettingRow({
    required this.label,
    required this.description,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 13,
            fontWeight: FontWeight.w500,
            color: c.textPrimary,
          ),
        ),
        const SizedBox(height: 2),
        Text(description, style: TextStyle(fontSize: 12, color: c.textMuted)),
        const SizedBox(height: 10),
        child,
      ],
    );
  }
}

// ─────────────────────────────────────────────
// 主题模式选择器（亮色 / 暗色 / 跟随系统）
// ─────────────────────────────────────────────

class _ThemeModeSelector extends StatelessWidget {
  const _ThemeModeSelector();

  static const _modes = [
    (mode: ThemeMode.system, label: '跟随系统', icon: LucideIcons.monitor),
    (mode: ThemeMode.light, label: '亮色', icon: LucideIcons.sun),
    (mode: ThemeMode.dark, label: '暗色', icon: LucideIcons.moon),
  ];

  @override
  Widget build(BuildContext context) {
    final provider = FluxDownApp.of(context);
    final current = provider.themeMode;
    final c = AppColors.of(context);

    return Row(
      children: [
        for (final item in _modes) ...[
          _ThemeModeCard(
            icon: item.icon,
            label: item.label,
            selected: current == item.mode,
            colors: c,
            onTap: () => provider.setThemeMode(item.mode),
          ),
          if (item != _modes.last) const SizedBox(width: 10),
        ],
      ],
    );
  }
}

class _ThemeModeCard extends StatelessWidget {
  final IconData icon;
  final String label;
  final bool selected;
  final AppColors colors;
  final VoidCallback onTap;

  const _ThemeModeCard({
    required this.icon,
    required this.label,
    required this.selected,
    required this.colors,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = ShadTheme.of(context);
    final borderColor = selected ? theme.colorScheme.primary : colors.border;
    final bgColor = selected
        ? theme.colorScheme.primary.withValues(alpha: 0.06)
        : colors.surface1;

    return GestureDetector(
      onTap: onTap,
      child: Container(
        width: 88,
        padding: const EdgeInsets.symmetric(vertical: 12),
        decoration: BoxDecoration(
          color: bgColor,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: borderColor, width: selected ? 1.5 : 1),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              icon,
              size: 20,
              color: selected
                  ? theme.colorScheme.primary
                  : colors.textSecondary,
            ),
            const SizedBox(height: 6),
            Text(
              label,
              style: TextStyle(
                fontSize: 12,
                fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                color: selected
                    ? theme.colorScheme.primary
                    : colors.textSecondary,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ─────────────────────────────────────────────
// 主题色选择器
// ─────────────────────────────────────────────

class _ColorSchemeSelector extends StatelessWidget {
  const _ColorSchemeSelector();

  @override
  Widget build(BuildContext context) {
    final provider = FluxDownApp.of(context);
    final current = provider.colorScheme;
    final c = AppColors.of(context);

    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        for (final scheme in AppColorScheme.values)
          _ColorDot(
            scheme: scheme,
            selected: current == scheme,
            colors: c,
            onTap: () => provider.setColorScheme(scheme),
          ),
      ],
    );
  }
}

class _ColorDot extends StatelessWidget {
  final AppColorScheme scheme;
  final bool selected;
  final AppColors colors;
  final VoidCallback onTap;

  const _ColorDot({
    required this.scheme,
    required this.selected,
    required this.colors,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return ShadTooltip(
      builder: (_) => Text(scheme.label),
      child: GestureDetector(
        onTap: onTap,
        child: Container(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            color: scheme.previewColor,
            shape: BoxShape.circle,
            border: Border.all(
              color: selected ? colors.textPrimary : scheme.previewColor,
              width: selected ? 2 : 0,
            ),
          ),
          child: selected
              ? const Icon(LucideIcons.check, size: 14, color: Colors.white)
              : null,
        ),
      ),
    );
  }
}
