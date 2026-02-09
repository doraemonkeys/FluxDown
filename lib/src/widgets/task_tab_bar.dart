import 'package:flutter/material.dart';
import '../models/download_controller.dart';
import '../theme/app_colors.dart';

class TaskTabBar extends StatelessWidget {
  final DownloadController controller;

  const TaskTabBar({super.key, required this.controller});

  static const _tabs = [
    (StatusTab.all, '全部'),
    (StatusTab.downloading, '下载中'),
    (StatusTab.completed, '已完成'),
    (StatusTab.paused, '已暂停'),
    (StatusTab.error, '出错'),
  ];

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final ctrl = controller;
        final selected = ctrl.statusTab;
        return Container(
          height: 38,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          decoration: BoxDecoration(
            color: c.bg,
            border: Border(bottom: BorderSide(color: c.border, width: 1)),
          ),
          child: Row(
            children: [
              for (final (tab, label) in _tabs) ...[
                _Tab(
                  label: '$label (${ctrl.filteredCountForStatus(tab)})',
                  isSelected: selected == tab,
                  onTap: () => ctrl.setStatusTab(tab),
                ),
                const SizedBox(width: 4),
              ],
            ],
          ),
        );
      },
    );
  }
}

class _Tab extends StatefulWidget {
  final String label;
  final bool isSelected;
  final VoidCallback onTap;

  const _Tab({
    required this.label,
    required this.isSelected,
    required this.onTap,
  });

  @override
  State<_Tab> createState() => _TabState();
}

class _TabState extends State<_Tab> {
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
          padding: const EdgeInsets.symmetric(horizontal: 4),
          decoration: BoxDecoration(
            border: Border(
              bottom: BorderSide(
                color: selected ? c.accent : Colors.transparent,
                width: 2,
              ),
            ),
          ),
          child: Center(
            child: Text(
              widget.label,
              style: TextStyle(
                fontSize: 13,
                color: selected
                    ? c.textPrimary
                    : _isHovered
                    ? c.textSecondary
                    : c.textMuted,
                fontWeight: selected ? FontWeight.w500 : FontWeight.normal,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
