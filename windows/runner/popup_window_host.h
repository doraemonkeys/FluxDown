#ifndef RUNNER_POPUP_WINDOW_HOST_H_
#define RUNNER_POPUP_WINDOW_HOST_H_

#include <flutter/binary_messenger.h>
#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>
#include <flutter/method_channel.h>
#include <flutter/standard_method_codec.h>

#include <memory>
#include <optional>
#include <string>

#include "win32_window.h"

// 外部唤起独立快速下载小窗的原生宿主（契约见 popup-contract）。
//
// 原生 Win32 窗口承载**第二个 Flutter 引擎**（同一 Dart bundle，
// entrypoint 参数 --quick-popup），渲染快速下载确认表单：
// - 窗口 + 引擎懒创建、常驻复用：首次 show 创建，之后只 hide/show，
//   进程存续期间禁止销毁（规避历史 isolate 频繁建销崩溃）；
// - 弹窗引擎零插件注册、不初始化 Rust；
// - 两条 MethodChannel：主引擎 `fluxdown/popup_host`（show/close 入、
//   onResult/onClosed 出），弹窗引擎 `fluxdown/popup_child`
//   （ready/submit/cancel/pickFolder/startDrag/resize 入、setPayload 出）。
class PopupWindowHost : public Win32Window {
 public:
  // |host_messenger| 为主引擎 messenger，生命周期由 FlutterWindow 保证
  // 覆盖本对象（FlutterWindow 持有并先于主引擎销毁本对象）。
  explicit PopupWindowHost(flutter::BinaryMessenger* host_messenger);
  virtual ~PopupWindowHost();

 protected:
  // Win32Window:
  bool OnCreate() override;
  void OnDestroy() override;
  LRESULT MessageHandler(HWND window, UINT const message, WPARAM const wparam,
                         LPARAM const lparam) noexcept override;

 private:
  // ── 主引擎通道处理 ──
  void HandleHostShow(
      const std::string& payload,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

  // ── 弹窗引擎通道处理 ──
  void HandleChildCall(
      const flutter::MethodCall<flutter::EncodableValue>& call,
      std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

  // 懒创建弹窗窗口 + 第二引擎。已创建时直接返回 true。
  bool EnsureWindow();

  // 投递载荷到弹窗引擎（setPayload）。
  void DeliverPayload(const std::string& payload);

  // 重置为默认逻辑尺寸并居中于光标所在显示器工作区（隐藏状态下调整）。
  void ResetPlacement();

  // 显示并强制激活（获得键盘焦点）。
  void ShowPopup();

  // 隐藏窗口（不销毁）。
  void HidePopup();

  // 中继"用户取消/关闭"到主引擎。
  void NotifyClosed();

  // 应用无边框弹窗样式（WS_POPUP + WS_EX_TOOLWINDOW + Win11 圆角）。
  void ApplyPopupStyles();

  // 目录选择对话框（IFileDialog / FOS_PICKFOLDERS），取消返回空 optional。
  std::optional<std::wstring> PickFolder(const std::wstring& title,
                                         const std::wstring& initial_dir);

  // 主引擎通道（onResult / onClosed 出方向）
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      host_channel_;

  // 弹窗引擎通道（setPayload 出方向）
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      child_channel_;

  // 弹窗引擎的 DartProject（OnCreate 中构造 view controller 时使用）
  std::optional<flutter::DartProject> project_;

  // 弹窗窗口承载的第二 Flutter 引擎
  std::unique_ptr<flutter::FlutterViewController> flutter_controller_;

  // 弹窗 Dart 是否已 ready（决定载荷直接投递还是暂存）
  bool child_ready_ = false;

  // ready 之前暂存的载荷
  std::optional<std::string> pending_payload_;

  // SW_HIDE 时暂停弹窗引擎 vsync 的记账（与 FlutterWindow 同款处理）
  bool window_hidden_ = false;
};

#endif  // RUNNER_POPUP_WINDOW_HOST_H_
