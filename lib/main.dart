import 'dart:convert';
import 'dart:io';

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:flutter/material.dart';
import 'package:launch_at_startup/launch_at_startup.dart';
import 'package:rinf/rinf.dart';
import 'package:shadcn_ui/shadcn_ui.dart';
import 'package:window_manager/window_manager.dart';
import 'src/bindings/bindings.dart';
import 'src/models/settings_provider.dart';
import 'src/pages/home_page.dart';
import 'src/services/external_download_service.dart';
import 'src/services/tray_service.dart';
import 'src/theme/app_theme.dart';
import 'src/theme/theme_provider.dart';
import 'src/windows/quick_download_window.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();

  // desktop_multi_window 子窗口入口：
  // 当子窗口被创建时，同一个 main() 会再次调用，args 包含 ['multi_window', windowId, argument]
  if (args.firstOrNull == 'multi_window') {
    final windowId = args[1];
    final argument = args.length > 2 ? args[2] : '{}';

    final windowController = WindowController.fromWindowId(windowId);

    Map<String, dynamic> windowArgs;
    try {
      windowArgs = jsonDecode(argument) as Map<String, dynamic>;
    } catch (_) {
      windowArgs = {};
    }

    final windowType = windowArgs['windowType'] as String? ?? '';
    await windowManager.ensureInitialized();

    if (windowType == 'quick_download') {
      runApp(
        QuickDownloadApp(windowController: windowController, args: windowArgs),
      );
    }
    return;
  }

  // ===== 主窗口正常启动流程 =====

  // 在 runApp 之前恢复主题设置，避免启动时主题闪烁
  final themeProvider = ThemeProvider();
  await themeProvider.init();

  await windowManager.ensureInitialized();

  const windowOptions = WindowOptions(
    size: Size(1280, 720),
    minimumSize: Size(900, 500),
    center: true,
    titleBarStyle: TitleBarStyle.hidden,
    windowButtonVisibility: false,
  );

  windowManager.waitUntilReadyToShow(windowOptions, () async {
    await windowManager.show();
    await windowManager.focus();
  });

  // 初始化开机启动支持
  launchAtStartup.setup(
    appName: 'FluxDown',
    appPath: Platform.resolvedExecutable,
  );

  // 初始化系统托盘
  await TrayService.instance.init();

  await initializeRust(assignRustSignal);
  runApp(FluxDownApp(themeProvider: themeProvider));
}

class FluxDownApp extends StatefulWidget {
  final ThemeProvider themeProvider;

  const FluxDownApp({super.key, required this.themeProvider});

  /// 允许子组件通过 context 访问 ThemeProvider
  static ThemeProvider of(BuildContext context) {
    final state = context.findAncestorStateOfType<_FluxDownAppState>();
    return state!.themeProvider;
  }

  @override
  State<FluxDownApp> createState() => _FluxDownAppState();
}

class _FluxDownAppState extends State<FluxDownApp> with WindowListener {
  late final ThemeProvider themeProvider;
  final _navigatorKey = GlobalKey<NavigatorState>();
  final _settingsForExternal = SettingsProvider();

  @override
  void initState() {
    super.initState();
    themeProvider = widget.themeProvider;
    themeProvider.addListener(_onThemeChanged);
    windowManager.addListener(this);
    // 阻止默认关闭行为，由 onWindowClose 接管
    windowManager.setPreventClose(true);

    // 初始化外部下载服务 — 监听浏览器扩展的下载请求
    ExternalDownloadService.init(
      settingsProvider: _settingsForExternal,
      themeProvider: themeProvider,
    );
    // 请求加载配置，确保 settingsProvider 有默认保存目录等数据
    _settingsForExternal.requestConfig();
  }

  @override
  void dispose() {
    ExternalDownloadService.shutdown();
    _settingsForExternal.dispose();
    windowManager.removeListener(this);
    themeProvider.removeListener(_onThemeChanged);
    themeProvider.dispose();
    super.dispose();
  }

  void _onThemeChanged() => setState(() {});

  @override
  void onWindowClose() async {
    // 当用户设置了「关闭到托盘」时，隐藏窗口而非退出
    if (SettingsProvider.globalInstance?.closeToTray ?? true) {
      await TrayService.instance.hideToTray();
    } else {
      await TrayService.instance.destroy();
      await windowManager.destroy();
    }
  }

  ShadThemeData _resolveTheme(BuildContext context) {
    final mode = themeProvider.themeMode;
    final scheme = themeProvider.colorScheme;
    final platformBrightness = MediaQuery.platformBrightnessOf(context);
    final useDark =
        mode == ThemeMode.dark ||
        (mode == ThemeMode.system && platformBrightness == Brightness.dark);
    return useDark ? buildDarkTheme(scheme) : buildLightTheme(scheme);
  }

  @override
  Widget build(BuildContext context) {
    // 手动组合 ShadTheme + WidgetsApp，跳过 ShadApp 内部的：
    // - ShadAnimatedTheme（200ms 色彩 tween 插值）
    // - AnimatedTheme（200ms Material 主题动画）
    // - materialTheme() 每帧重建 ThemeData + applyGoogleFontToTextTheme
    final theme = _resolveTheme(context);
    return ShadTheme(
      data: theme,
      child: Directionality(
        textDirection: TextDirection.ltr,
        child: DefaultTextStyle(
          style: theme.textTheme.p.copyWith(
            color: theme.colorScheme.foreground,
          ),
          child: ShadToaster(
            child: ShadSonner(
              child: WidgetsApp(
                navigatorKey: _navigatorKey,
                color: theme.colorScheme.primary,
                debugShowCheckedModeBanner: false,
                home: const HomePage(),
                pageRouteBuilder:
                    <T>(RouteSettings settings, WidgetBuilder builder) {
                      return MaterialPageRoute<T>(
                        settings: settings,
                        builder: builder,
                      );
                    },
              ),
            ),
          ),
        ),
      ),
    );
  }
}
