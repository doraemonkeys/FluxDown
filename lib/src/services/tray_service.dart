import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';

/// 系统托盘服务 — 管理托盘图标、菜单和事件
class TrayService with TrayListener {
  TrayService._();
  static final TrayService instance = TrayService._();

  bool _initialized = false;

  /// 初始化系统托盘图标和菜单
  Future<void> init() async {
    if (_initialized) return;
    _initialized = true;

    // 图标路径必须是相对于 exe 目录的路径或绝对路径
    // CMakeLists.txt 已配置将 app_icon.ico 复制到 exe 同级目录
    final exeDir = File(Platform.resolvedExecutable).parent.path;
    final iconPath = Platform.isWindows
        ? p.join(exeDir, 'app_icon.ico')
        : p.join(
            exeDir,
            'data',
            'flutter_assets',
            'assets',
            'logo',
            'fluxdown_logo.png',
          );

    await trayManager.setIcon(iconPath);
    await trayManager.setToolTip('FluxDown');

    final menu = Menu(
      items: [
        MenuItem(key: 'show_window', label: '显示主窗口'),
        MenuItem.separator(),
        MenuItem(key: 'exit_app', label: '退出'),
      ],
    );
    await trayManager.setContextMenu(menu);
    trayManager.addListener(this);
  }

  /// 销毁托盘图标
  Future<void> destroy() async {
    trayManager.removeListener(this);
    await trayManager.destroy();
    _initialized = false;
  }

  /// 显示窗口并聚焦
  Future<void> _showWindow() async {
    await windowManager.show();
    await windowManager.focus();
  }

  /// 隐藏窗口到托盘
  Future<void> hideToTray() async {
    await windowManager.hide();
  }

  // ─────────────────────────────────────────────
  // TrayListener 回调
  // ─────────────────────────────────────────────

  @override
  void onTrayIconMouseDown() {
    _showWindow();
  }

  @override
  void onTrayIconRightMouseDown() {
    trayManager.popUpContextMenu();
  }

  @override
  void onTrayIconRightMouseUp() {}

  @override
  void onTrayMenuItemClick(MenuItem menuItem) {
    switch (menuItem.key) {
      case 'show_window':
        _showWindow();
      case 'exit_app':
        // 真正退出应用
        windowManager.destroy();
    }
  }
}
