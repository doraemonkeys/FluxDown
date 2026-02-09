import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import 'package:window_manager/window_manager.dart';
import '../models/download_controller.dart';
import '../models/download_task.dart';
import '../theme/app_colors.dart';

class Sidebar extends StatelessWidget {
  final DownloadController controller;

  const Sidebar({super.key, required this.controller});

  /// 文件类型 → 图标映射
  static IconData _categoryIcon(FileCategory cat) {
    return switch (cat) {
      FileCategory.all => LucideIcons.layoutGrid,
      FileCategory.video => LucideIcons.film,
      FileCategory.audio => LucideIcons.music,
      FileCategory.document => LucideIcons.fileText,
      FileCategory.image => LucideIcons.image,
      FileCategory.archive => LucideIcons.archive,
      FileCategory.other => LucideIcons.file,
    };
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final ctrl = controller;
        final selected = ctrl.categoryFilter;
        return Container(
          width: 224,
          color: c.surface1,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _buildLogo(c),
              const SizedBox(height: 12),
              _buildSection('分类', c, [
                for (final cat in FileCategory.values)
                  _NavItem(
                    icon: _categoryIcon(cat),
                    label: cat.label,
                    count: ctrl.countForCategory(cat),
                    isSelected: selected == cat,
                    onTap: () => ctrl.setCategoryFilter(cat),
                  ),
              ]),
              const Spacer(),
              _buildFooter(c, ctrl),
            ],
          ),
        );
      },
    );
  }

  Widget _buildLogo(AppColors c) {
    return DragToMoveArea(
      child: Container(
        height: 42,
        padding: const EdgeInsets.symmetric(horizontal: 16),
        alignment: Alignment.centerLeft,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            ClipRRect(
              borderRadius: BorderRadius.circular(5),
              child: Image.asset(
                'assets/logo/fluxdown_logo.png',
                width: 22,
                height: 22,
                filterQuality: FilterQuality.medium,
              ),
            ),
            const SizedBox(width: 9),
            Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: 'Flux',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: c.accent,
                      letterSpacing: 0.3,
                    ),
                  ),
                  TextSpan(
                    text: 'Down',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w500,
                      color: c.textPrimary,
                      letterSpacing: 0.3,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSection(String title, AppColors c, List<Widget> items) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
          child: Text(
            title,
            style: TextStyle(
              fontSize: 10.5,
              fontWeight: FontWeight.w500,
              color: c.textMuted,
              letterSpacing: 0.5,
            ),
          ),
        ),
        const SizedBox(height: 4),
        ...items,
      ],
    );
  }

  Widget _buildFooter(AppColors c, DownloadController ctrl) {
    final dlSpeed = DownloadTask.formatBytes(ctrl.totalDownloadSpeed);
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: c.border, width: 1)),
      ),
      child: Row(
        children: [
          const Icon(LucideIcons.arrowDown, size: 11, color: AppColors.green),
          const SizedBox(width: 4),
          Text('下载', style: TextStyle(fontSize: 11, color: c.textSecondary)),
          const SizedBox(width: 6),
          Text(
            '$dlSpeed/s',
            style: TextStyle(
              fontSize: 11,
              color: AppColors.green,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}

class _NavItem extends StatefulWidget {
  final IconData icon;
  final String label;
  final int? count;
  final bool isSelected;
  final VoidCallback onTap;

  const _NavItem({
    required this.icon,
    required this.label,
    this.count,
    required this.isSelected,
    required this.onTap,
  });

  @override
  State<_NavItem> createState() => _NavItemState();
}

class _NavItemState extends State<_NavItem> {
  bool _isHovered = false;

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    final selected = widget.isSelected;

    return MouseRegion(
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() => _isHovered = false),
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        onTap: widget.onTap,
        child: Container(
          height: 32,
          margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
          padding: const EdgeInsets.symmetric(horizontal: 8),
          decoration: BoxDecoration(
            color: selected
                ? c.accentBg
                : _isHovered
                ? c.hoverBg
                : Colors.transparent,
            borderRadius: BorderRadius.circular(6),
          ),
          child: Row(
            children: [
              Icon(
                widget.icon,
                size: 14,
                color: selected ? c.accent : c.textSecondary,
              ),
              const SizedBox(width: 8),
              Text(
                widget.label,
                style: TextStyle(
                  fontSize: 12.5,
                  color: selected ? c.accent : c.textSecondary,
                  fontWeight: selected ? FontWeight.w500 : FontWeight.normal,
                ),
              ),
              if (widget.count != null) ...[
                const Spacer(),
                Text(
                  widget.count.toString(),
                  style: TextStyle(
                    fontSize: 11,
                    color: selected ? c.accent : c.textMuted,
                    fontFeatures: const [FontFeature.tabularFigures()],
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
