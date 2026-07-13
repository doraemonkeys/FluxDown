#include "flutter_window.h"

#include <windowsx.h>

#include <algorithm>
#include <optional>
#include <string>
#include <vector>

#include "flutter/generated_plugin_registrant.h"
#include "utils.h"

// Must match kCopyDataId in main.cpp.
static const ULONG_PTR kCopyDataId = 0x464C5558; // "FLUX"

namespace {

// Property name storing the original WndProc of the Flutter view child
// window (set when subclassing for edge hit-test forwarding).
constexpr const wchar_t kChildOriginalProcProp[] = L"FluxDownChildProc";

// SDK 的 VersionHelpers 无 Win11 判定；经 RtlGetVersion 读真实
// build 号（不受兼容性清单影响），build >= 22000 即 Windows 11。
bool IsWindows11OrLater() {
  using RtlGetVersionFn = LONG(WINAPI*)(PRTL_OSVERSIONINFOW);
  static const bool is_win11 = [] {
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    if (!ntdll) {
      return false;
    }
    auto rtl_get_version = reinterpret_cast<RtlGetVersionFn>(
        GetProcAddress(ntdll, "RtlGetVersion"));
    if (!rtl_get_version) {
      return false;
    }
    RTL_OSVERSIONINFOW info{};
    info.dwOSVersionInfoSize = sizeof(info);
    if (rtl_get_version(&info) != 0) {
      return false;
    }
    return info.dwMajorVersion > 10 ||
           (info.dwMajorVersion == 10 && info.dwBuildNumber >= 22000);
  }();
  return is_win11;
}

// Returns true when the borderless (client-covers-whole-window) frame is in
// effect: the window keeps WS_THICKFRAME for snap/shadow but is neither
// maximized nor missing its resize frame (fullscreen strips it).
bool IsBorderlessNormalState(HWND hwnd) {
  const LONG_PTR style = GetWindowLongPtr(hwnd, GWL_STYLE);
  return (style & WS_THICKFRAME) != 0 && !IsZoomed(hwnd);
}

// Resize band thickness in physical pixels for |hwnd|'s DPI.
int ResizeInsetFor(HWND hwnd) {
  const UINT dpi = GetDpiForWindow(hwnd);
  return GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) +
         GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
}

// Hit-tests |screen_pt| against the resize band of top-level window |root|.
// Returns HTNOWHERE when the point is not on any edge.
LRESULT HitTestResizeEdge(HWND root, POINT screen_pt) {
  RECT rc;
  if (!GetWindowRect(root, &rc)) {
    return HTNOWHERE;
  }
  const int inset = ResizeInsetFor(root);
  const bool on_left = screen_pt.x < rc.left + inset;
  const bool on_right = screen_pt.x >= rc.right - inset;
  const bool on_top = screen_pt.y < rc.top + inset;
  const bool on_bottom = screen_pt.y >= rc.bottom - inset;
  if (on_top && on_left) return HTTOPLEFT;
  if (on_top && on_right) return HTTOPRIGHT;
  if (on_bottom && on_left) return HTBOTTOMLEFT;
  if (on_bottom && on_right) return HTBOTTOMRIGHT;
  if (on_left) return HTLEFT;
  if (on_right) return HTRIGHT;
  if (on_top) return HTTOP;
  if (on_bottom) return HTBOTTOM;
  return HTNOWHERE;
}

// Subclass proc for the Flutter view child window: makes the outer resize
// band mouse-transparent so WM_NCHITTEST reaches the top-level window,
// which then reports HTLEFT/HTTOP/... to enable native edge resizing.
LRESULT CALLBACK ChildEdgeForwardProc(HWND hwnd, UINT message, WPARAM wparam,
                                      LPARAM lparam) {
  auto original = reinterpret_cast<WNDPROC>(GetProp(hwnd, kChildOriginalProcProp));
  if (message == WM_NCHITTEST) {
    HWND root = GetAncestor(hwnd, GA_ROOT);
    if (root && IsBorderlessNormalState(root)) {
      const POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
      if (HitTestResizeEdge(root, pt) != HTNOWHERE) {
        return HTTRANSPARENT;
      }
    }
  }
  if (message == WM_NCDESTROY) {
    RemoveProp(hwnd, kChildOriginalProcProp);
  }
  return original ? CallWindowProc(original, hwnd, message, wparam, lparam)
                  : DefWindowProc(hwnd, message, wparam, lparam);
}

}  // namespace

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());

  // Create MethodChannel for forwarding second-instance args to Dart.
  single_instance_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "com.fluxdown/single_instance",
          &flutter::StandardMethodCodec::GetInstance());

  // Floating ball channel (plan A6): handles registerDropTarget /
  // unregisterDropTarget from Dart; forwards drop payloads back.
  floating_ball_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "com.fluxdown/floating_ball",
          &flutter::StandardMethodCodec::GetInstance());
  floating_ball_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        if (call.method_name() == "registerDropTarget") {
          const auto* args =
              std::get_if<flutter::EncodableMap>(call.arguments());
          if (!args) {
            result->Error("bad_args", "expected map with hwnd");
            return;
          }
          auto it = args->find(flutter::EncodableValue("hwnd"));
          if (it == args->end()) {
            result->Error("bad_args", "missing hwnd");
            return;
          }
          const int64_t hwnd_val =
              std::holds_alternative<int64_t>(it->second)
                  ? std::get<int64_t>(it->second)
                  : static_cast<int64_t>(std::get<int32_t>(it->second));
          if (!ball_drop_target_) {
            ball_drop_target_ =
                new FloatingBallDropTarget(floating_ball_channel_.get());
          }
          HRESULT hr = ball_drop_target_->RegisterOn(
              reinterpret_cast<HWND>(hwnd_val));
          if (SUCCEEDED(hr)) {
            result->Success();
          } else {
            result->Error("register_failed",
                          "RegisterDragDrop hr=" + std::to_string(hr));
          }
        } else if (call.method_name() == "unregisterDropTarget") {
          if (ball_drop_target_) {
            ball_drop_target_->Revoke();
          }
          result->Success();
        } else {
          result->NotImplemented();
        }
      });

  // 外部唤起独立快速下载小窗宿主 — 注册 fluxdown/popup_host 通道。
  // 弹窗窗口与第二引擎在首次 show 时才懒创建。
  popup_host_ = std::make_unique<PopupWindowHost>(
      flutter_controller_->engine()->messenger());

  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  // 无边框方案：子 Flutter 视图边缘 8px 命中带返回 HTTRANSPARENT，
  // 让 WM_NCHITTEST 冒泡到顶层窗口以恢复四边缩放（见 MessageHandler）。
  if (HWND view_hwnd = flutter_controller_->view()->GetNativeWindow()) {
    WNDPROC original = reinterpret_cast<WNDPROC>(SetWindowLongPtr(
        view_hwnd, GWLP_WNDPROC,
        reinterpret_cast<LONG_PTR>(ChildEdgeForwardProc)));
    SetProp(view_hwnd, kChildOriginalProcProp,
            reinterpret_cast<HANDLE>(original));
  }

  // Check --silentStart before the callback to avoid capturing by reference.
  const std::vector<std::string> cmd_args = GetCommandLineArguments();
  const bool is_silent_start =
      std::find(cmd_args.begin(), cmd_args.end(), "--silentStart") !=
      cmd_args.end();

  flutter_controller_->engine()->SetNextFrameCallback(
      [this, is_silent_start]() {
        // Skip showing the window on first frame if launched with --silentStart
        // (boot autostart silent mode).
        if (!is_silent_start) {
          this->Show();
        }
      });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  // UIPI hardening: allow a lower-integrity (Medium IL) second instance to
  // deliver its command-line args via WM_COPYDATA when this instance runs
  // elevated (High IL). Pairs with the single-instance mutex hardening in
  // main.cpp; without it the forwarded URL/torrent is silently dropped.
  if (HWND handle = GetHandle()) {
    ::ChangeWindowMessageFilterEx(handle, WM_COPYDATA, MSGFLT_ALLOW, nullptr);
  }

  return true;
}

void FlutterWindow::OnDestroy() {
  // 先销毁弹窗宿主：其主引擎通道引用 flutter_controller_ 的 messenger
  popup_host_ = nullptr;
  if (ball_drop_target_) {
    ball_drop_target_->Revoke();
    ball_drop_target_->Release();
    ball_drop_target_ = nullptr;
  }
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Handle WM_SHOWWINDOW to ensure Flutter's rendering engine pauses when the
  // window is hidden to the system tray and resumes when shown again.
  //
  // window_manager.hide() calls ShowWindow(SW_HIDE) which sends only
  // WM_SHOWWINDOW(FALSE) — NOT WM_SIZE(SIZE_MINIMIZED).  Without
  // SIZE_MINIMIZED, Flutter's compositor does not pause vsync and continues
  // rendering at the monitor refresh rate (~60 fps), wasting 3-4 % CPU even
  // when there is nothing to draw.
  //
  // We synthesize the missing WM_SIZE messages so the Flutter engine always
  // receives the signal it needs to suspend/resume the rasterizer.
  //
  // Guard: lParam == 0 means the visibility change was triggered by a direct
  // ShowWindow call (our case).  Non-zero lParam values indicate parent-window
  // state changes (SW_PARENTCLOSING, SW_PARENTOPENING) — we skip those because
  // a real WM_SIZE(SIZE_MINIMIZED) was already dispatched by the minimize path.
  if (message == WM_SHOWWINDOW && lparam == 0 && flutter_controller_) {
    if (wparam == FALSE) {
      // Window is being hidden.  Tell Flutter to pause vsync.
      window_hidden_ = true;
      ::PostMessage(hwnd, WM_SIZE, SIZE_MINIMIZED, 0);
    } else if (wparam == TRUE && window_hidden_) {
      // Window is being shown after a SW_HIDE.  Tell Flutter to resume vsync
      // at the actual client dimensions (unchanged since we never minimized).
      window_hidden_ = false;
      RECT rect = GetClientArea();
      ::PostMessage(hwnd, WM_SIZE, SIZE_RESTORED,
                    MAKELPARAM(rect.right - rect.left,
                               rect.bottom - rect.top));
    }
    // Fall through — let the base handler propagate WM_SHOWWINDOW normally.
  }

  // Handle WM_COPYDATA from a second instance before Flutter processes it.
  if (message == WM_COPYDATA) {
    auto* cds = reinterpret_cast<COPYDATASTRUCT*>(lparam);
    if (cds && cds->dwData == kCopyDataId && single_instance_channel_) {
      // Reconstruct the argument list (newline-separated UTF-8).
      // Guard against cbData=0 cross-process case where lpData may be null.
      std::string payload;
      if (cds->cbData > 0 && cds->lpData != nullptr) {
        payload = std::string(static_cast<const char*>(cds->lpData),
                              cds->cbData);
      }
      flutter::EncodableList args_list;
      size_t start = 0;
      while (start < payload.size()) {
        size_t end = payload.find('\n', start);
        if (end == std::string::npos) end = payload.size();
        args_list.push_back(flutter::EncodableValue(payload.substr(start, end - start)));
        start = end + 1;
      }
      single_instance_channel_->InvokeMethod(
          "onSecondInstance",
          std::make_unique<flutter::EncodableValue>(args_list));
    }
    return 0;
  }

  // 去除 window_manager TitleBarStyle.hidden 保留的左/右/下 8px 原生框架
  // （呈现为黑边）。必须先于插件的 HandleTopLevelWindowProc 处理：
  // 非最大化时让客户区铺满整个窗口；最大化/全屏仍交给插件调整
  // （否则内容会被屏幕边缘裁掉）。保留 WS_THICKFRAME → DWM 阴影、
  // Aero Snap、圆角均不受影响。
  if (message == WM_NCCALCSIZE && wparam == TRUE &&
      IsBorderlessNormalState(hwnd)) {
    auto* params = reinterpret_cast<NCCALCSIZE_PARAMS*>(lparam);
    // Windows 10 下客户区顶到 0 会出现 1px 白线（window_manager 同款
    // 规避，见插件源码注释）。
    if (!IsWindows11OrLater()) {
      params->rgrc[0].top += 1;
    }
    return 0;
  }

  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
    case WM_NCHITTEST: {
      // 子视图边缘带返回 HTTRANSPARENT 后由此处接管：报告缩放边缘。
      // 顶边此前因插件不保留 top 框架而无法缩放，此处一并修复。
      if (IsBorderlessNormalState(hwnd)) {
        const POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        const LRESULT hit = HitTestResizeEdge(hwnd, pt);
        if (hit != HTNOWHERE) {
          return hit;
        }
      }
      break;
    }
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
