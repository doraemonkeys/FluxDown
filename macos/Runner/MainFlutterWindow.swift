import Cocoa
import FlutterMacOS
import LaunchAtLogin

class MainFlutterWindow: NSWindow {
    override func awakeFromNib() {
        let flutterViewController = FlutterViewController()
        let windowFrame = self.frame
        self.contentViewController = flutterViewController
        self.setFrame(windowFrame, display: true)

        // launch_at_startup plugin requires platform channel bridging on macOS.
        // See: https://pub.dev/packages/launch_at_startup#macos-support
        FlutterMethodChannel(
            name: "launch_at_startup",
            binaryMessenger: flutterViewController.engine.binaryMessenger
        ).setMethodCallHandler { (_ call: FlutterMethodCall, result: @escaping FlutterResult) in
            switch call.method {
            case "launchAtStartupIsEnabled":
                result(LaunchAtLogin.isEnabled)
            case "launchAtStartupSetEnabled":
                if let arguments = call.arguments as? [String: Any],
                    let setEnabledValue = arguments["setEnabledValue"] as? Bool
                {
                    LaunchAtLogin.isEnabled = setEnabledValue
                }
                result(nil)
            default:
                result(FlutterMethodNotImplemented)
            }
        }

        // 悬浮球原生层（macOS）— MethodChannel `com.fluxdown/floating_ball`。
        // 详见 FloatingBallPanel.swift；协议参照 lib/src/services/floating_ball/floating_ball_service.dart。
        FloatingBallPanel.shared.register(with: flutterViewController.engine.binaryMessenger)

        // 外部唤起独立下载小窗（原生宿主，macOS）— MethodChannel `fluxdown/popup_host`。
        // 详见 PopupWindowHost.swift；协议参照跨端契约（外部唤起独立小窗 v1）。
        // 单例通过 static let 自持，弹窗窗口/引擎懒创建、常驻复用，不随本窗口生命周期回收。
        PopupWindowHost.shared.register(with: flutterViewController.engine.binaryMessenger)

        RegisterGeneratedPlugins(registry: flutterViewController)

        super.awakeFromNib()
    }
}
