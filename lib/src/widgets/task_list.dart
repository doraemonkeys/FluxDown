import 'package:flutter/material.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import '../models/download_controller.dart';
import '../theme/app_colors.dart';
import 'context_menu.dart';
import 'task_list_item.dart';

class TaskList extends StatelessWidget {
  final DownloadController controller;
  final ValueChanged<String>? onTaskTap;
  final VoidCallback? onNewDownload;

  const TaskList({
    super.key,
    required this.controller,
    this.onTaskTap,
    this.onNewDownload,
  });

  void _showBlankAreaMenu(BuildContext context, TapDownDetails details) {
    final c = AppColors.of(context);
    final hasActive = controller.activeCount > 0;
    final hasPausedOrError = controller.pausedCount + controller.errorCount > 0;

    final items = <ContextMenuItem>[
      ContextMenuItem(
        icon: LucideIcons.plus,
        label: '新建下载',
        color: c.textPrimary,
        action: () => onNewDownload?.call(),
      ),
    ];

    if (hasActive || hasPausedOrError) {
      final dividers = <int>{0}; // 新建下载后加分隔线

      if (hasActive) {
        items.add(
          ContextMenuItem(
            icon: LucideIcons.pause,
            label: '全部暂停',
            color: c.textPrimary,
            action: () => controller.pauseAll(),
          ),
        );
      }
      if (hasPausedOrError) {
        items.add(
          ContextMenuItem(
            icon: LucideIcons.play,
            label: '全部开始',
            color: c.textPrimary,
            action: () => controller.resumeAll(),
          ),
        );
      }

      showContextMenu(
        context,
        details.globalPosition,
        items: items,
        dividerAfterIndices: dividers,
      );
    } else {
      showContextMenu(context, details.globalPosition, items: items);
    }
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final tasks = controller.filteredTasks;
        return Column(
          children: [
            _buildHeader(context),
            Expanded(
              child: tasks.isEmpty
                  ? _buildEmpty(context)
                  : _buildListWithBlankArea(context, tasks),
            ),
          ],
        );
      },
    );
  }

  /// 列表 + 列表下方空白区域均支持右键菜单
  Widget _buildListWithBlankArea(BuildContext context, List tasks) {
    return CustomScrollView(
      slivers: [
        SliverList(
          delegate: SliverChildBuilderDelegate((context, index) {
            final task = tasks[index];
            return TaskListItem(
              task: task,
              isSelected: task.id == controller.selectedTaskId,
              onTap: () {
                controller.selectTask(task.id);
                onTaskTap?.call(task.id);
              },
              onPause: () => controller.pauseTask(task.id),
              onResume: () => controller.resumeTask(task.id),
              onDelete: ({required bool deleteFiles}) =>
                  controller.deleteTask(task.id, deleteFiles: deleteFiles),
            );
          }, childCount: tasks.length),
        ),
        // 填满剩余空间的空白区域，仅此区域响应右键
        SliverFillRemaining(
          hasScrollBody: false,
          child: GestureDetector(
            onSecondaryTapDown: (details) =>
                _showBlankAreaMenu(context, details),
            behavior: HitTestBehavior.opaque,
            child: const SizedBox.expand(),
          ),
        ),
      ],
    );
  }

  Widget _buildEmpty(BuildContext context) {
    final c = AppColors.of(context);
    return GestureDetector(
      onSecondaryTapDown: (details) => _showBlankAreaMenu(context, details),
      behavior: HitTestBehavior.opaque,
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(LucideIcons.download, size: 48, color: c.textMuted),
            const SizedBox(height: 12),
            Text('暂无下载任务', style: TextStyle(fontSize: 14, color: c.textMuted)),
            const SizedBox(height: 4),
            Text(
              '点击「新建下载」或右键开始',
              style: TextStyle(fontSize: 12, color: c.textMuted),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildHeader(BuildContext context) {
    final c = AppColors.of(context);
    return Container(
      height: 36,
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: c.surface1,
        border: Border(bottom: BorderSide(color: c.border, width: 1)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              '文件名',
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w500,
                color: c.textMuted,
              ),
            ),
          ),
          SizedBox(
            width: 150,
            child: Center(
              child: Text(
                '进度',
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  color: c.textMuted,
                ),
              ),
            ),
          ),
          SizedBox(
            width: 100,
            child: Center(
              child: Text(
                '速度',
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  color: c.textMuted,
                ),
              ),
            ),
          ),
          SizedBox(
            width: 60,
            child: Text(
              '状态',
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w500,
                color: c.textMuted,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
