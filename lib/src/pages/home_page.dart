import 'package:flutter/material.dart';
import '../models/download_controller.dart';
import '../models/settings_provider.dart';
import '../theme/app_colors.dart';
import '../widgets/sidebar.dart';
import '../widgets/header_bar.dart';
import '../widgets/task_tab_bar.dart';
import '../widgets/task_list.dart';
import '../widgets/detail_panel.dart';
import '../widgets/status_bar.dart';
import '../widgets/new_download_dialog.dart';
import '../widgets/title_drag_area.dart';
import 'settings_page.dart';

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  final _controller = DownloadController();
  final _settingsProvider = SettingsProvider();

  // 页面切换
  bool _showSettings = false;

  // Sidebar
  double _sidebarWidth = 224;
  static const double _sidebarMinWidth = 180;
  static const double _sidebarMaxWidth = 320;

  // Detail panel
  bool _isDetailOpen = false;
  double _detailWidth = 280;
  static const double _detailMinWidth = 240;
  static const double _detailMaxWidth = 420;

  // 主内容区最小宽度，保证 HeaderBar 不溢出
  static const double _mainMinWidth = 400;

  @override
  void initState() {
    super.initState();
    // 请求 Rust 端加载下载配置
    _settingsProvider.requestConfig();
  }

  @override
  void dispose() {
    _controller.dispose();
    _settingsProvider.dispose();
    super.dispose();
  }

  void _openDetail() {
    if (!_isDetailOpen) {
      setState(() => _isDetailOpen = true);
    }
  }

  void _closeDetail() {
    setState(() => _isDetailOpen = false);
  }

  /// 根据总宽度计算 sidebar 的实际最大值
  double _sidebarMax(double totalWidth) {
    final reserved = _mainMinWidth + (_isDetailOpen ? _detailWidth : 0);
    return (totalWidth - reserved).clamp(_sidebarMinWidth, _sidebarMaxWidth);
  }

  /// 根据总宽度计算 detail 的实际最大值
  double _detailMax(double totalWidth) {
    final reserved = _mainMinWidth + _sidebarWidth;
    return (totalWidth - reserved).clamp(_detailMinWidth, _detailMaxWidth);
  }

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);

    // 设置页面
    if (_showSettings) {
      return Stack(
        children: [
          // 全宽顶部拖拽区域
          Positioned(
            top: 0,
            left: 0,
            right: 0,
            height: 42,
            child: TitleDragArea(child: ColoredBox(color: c.surface1)),
          ),
          ColoredBox(
            color: c.bg,
            child: SettingsPage(
              onBack: () => setState(() => _showSettings = false),
              settingsProvider: _settingsProvider,
            ),
          ),
          // 窗口控制按钮 — 始终固定在窗口右上角
          Positioned(
            top: 0,
            right: 0,
            child: WindowControls(
              controller: _controller,
              onSettings: () => setState(() => _showSettings = false),
              isSettingsActive: true,
            ),
          ),
        ],
      );
    }

    // 主页面
    return LayoutBuilder(
      builder: (context, constraints) {
        final totalWidth = constraints.maxWidth;
        // 自动收缩面板宽度以适应窗口
        _sidebarWidth = _sidebarWidth.clamp(
          _sidebarMinWidth,
          _sidebarMax(totalWidth),
        );
        if (_isDetailOpen) {
          _detailWidth = _detailWidth.clamp(
            _detailMinWidth,
            _detailMax(totalWidth),
          );
        }
        return Stack(
          children: [
            // 全宽顶部拖拽区域（在所有内容之下）
            Positioned(
              top: 0,
              left: 0,
              right: 0,
              height: 42,
              child: TitleDragArea(child: ColoredBox(color: c.surface1)),
            ),
            // 主布局
            ColoredBox(
              color: c.bg,
              child: Row(
                children: [
                  // Sidebar
                  SizedBox(
                    width: _sidebarWidth,
                    child: Sidebar(controller: _controller),
                  ),
                  _ResizeHandle(
                    color: c.border,
                    onDrag: (dx) {
                      setState(() {
                        _sidebarWidth = (_sidebarWidth + dx).clamp(
                          _sidebarMinWidth,
                          _sidebarMax(totalWidth),
                        );
                      });
                    },
                  ),
                  // Main content
                  Expanded(
                    child: Column(
                      children: [
                        HeaderBar(
                          onNewDownload: () => showNewDownloadDialog(
                            context,
                            _controller,
                            _settingsProvider,
                          ),
                        ),
                        TaskTabBar(controller: _controller),
                        Expanded(
                          child: TaskList(
                            controller: _controller,
                            onTaskTap: (_) => _openDetail(),
                            onNewDownload: () => showNewDownloadDialog(
                              context,
                              _controller,
                              _settingsProvider,
                            ),
                          ),
                        ),
                        StatusBar(controller: _controller),
                      ],
                    ),
                  ),
                  // Detail panel (conditional)
                  if (_isDetailOpen) ...[
                    _ResizeHandle(
                      color: c.border,
                      onDrag: (dx) {
                        setState(() {
                          _detailWidth = (_detailWidth - dx).clamp(
                            _detailMinWidth,
                            _detailMax(totalWidth),
                          );
                        });
                      },
                    ),
                    SizedBox(
                      width: _detailWidth,
                      child: DetailPanel(
                        controller: _controller,
                        onClose: _closeDetail,
                      ),
                    ),
                  ],
                ],
              ),
            ),
            // 窗口控制按钮 — 始终固定在窗口右上角
            Positioned(
              top: 0,
              right: 0,
              child: WindowControls(
                controller: _controller,
                onSettings: () => setState(() => _showSettings = true),
              ),
            ),
          ],
        );
      },
    );
  }
}

/// 可拖拽的分隔线
class _ResizeHandle extends StatefulWidget {
  final Color color;
  final ValueChanged<double> onDrag;

  const _ResizeHandle({required this.color, required this.onDrag});

  @override
  State<_ResizeHandle> createState() => _ResizeHandleState();
}

class _ResizeHandleState extends State<_ResizeHandle> {
  bool _isHovered = false;
  bool _isDragging = false;

  @override
  Widget build(BuildContext context) {
    final c = AppColors.of(context);
    final isActive = _isHovered || _isDragging;
    return MouseRegion(
      cursor: SystemMouseCursors.resizeColumn,
      onEnter: (_) => setState(() => _isHovered = true),
      onExit: (_) => setState(() => _isHovered = false),
      child: GestureDetector(
        onHorizontalDragStart: (_) => setState(() => _isDragging = true),
        onHorizontalDragUpdate: (details) => widget.onDrag(details.delta.dx),
        onHorizontalDragEnd: (_) => setState(() => _isDragging = false),
        child: Container(
          width: isActive ? 3 : 1,
          color: isActive ? c.accent.withValues(alpha: 0.6) : widget.color,
        ),
      ),
    );
  }
}
