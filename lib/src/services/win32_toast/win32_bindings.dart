// ignore_for_file: camel_case_types, non_constant_identifier_names, constant_identifier_names
import 'dart:ffi';

import 'package:ffi/ffi.dart';

// =============================================================================
// 基本 Win32 类型别名
// =============================================================================

typedef HWND = IntPtr;
typedef HDC = IntPtr;
typedef HBITMAP = IntPtr;
typedef HGDIOBJ = IntPtr;
typedef HINSTANCE = IntPtr;
typedef HCURSOR = IntPtr;
typedef WPARAM = IntPtr;
typedef LPARAM = IntPtr;
typedef LRESULT = IntPtr;
typedef COLORREF = Uint32;

// WndProc 函数指针类型
typedef WNDPROC_Native = LRESULT Function(
  IntPtr hwnd,
  Uint32 uMsg,
  WPARAM wParam,
  LPARAM lParam,
);

// =============================================================================
// 结构体定义
// =============================================================================

/// POINT 结构体
final class POINT extends Struct {
  @Int32()
  external int x;
  @Int32()
  external int y;
}

/// RECT 结构体
final class RECT extends Struct {
  @Int32()
  external int left;
  @Int32()
  external int top;
  @Int32()
  external int right;
  @Int32()
  external int bottom;
}

/// SIZE 结构体
final class SIZE extends Struct {
  @Int32()
  external int cx;
  @Int32()
  external int cy;
}

/// BLENDFUNCTION 结构体
final class BLENDFUNCTION extends Struct {
  @Uint8()
  external int BlendOp;
  @Uint8()
  external int BlendFlags;
  @Uint8()
  external int SourceConstantAlpha;
  @Uint8()
  external int AlphaFormat;
}

/// BITMAPINFOHEADER 结构体
final class BITMAPINFOHEADER extends Struct {
  @Uint32()
  external int biSize;
  @Int32()
  external int biWidth;
  @Int32()
  external int biHeight;
  @Uint16()
  external int biPlanes;
  @Uint16()
  external int biBitCount;
  @Uint32()
  external int biCompression;
  @Uint32()
  external int biSizeImage;
  @Int32()
  external int biXPelsPerMeter;
  @Int32()
  external int biYPelsPerMeter;
  @Uint32()
  external int biClrUsed;
  @Uint32()
  external int biClrImportant;
}

/// WNDCLASSEXW 结构体
final class WNDCLASSEXW extends Struct {
  @Uint32()
  external int cbSize;
  @Uint32()
  external int style;
  external Pointer<NativeFunction<WNDPROC_Native>> lpfnWndProc;
  @Int32()
  external int cbClsExtra;
  @Int32()
  external int cbWndExtra;
  @IntPtr()
  external int hInstance;
  @IntPtr()
  external int hIcon;
  @IntPtr()
  external int hCursor;
  @IntPtr()
  external int hbrBackground;
  external Pointer<Utf16> lpszMenuName;
  external Pointer<Utf16> lpszClassName;
  @IntPtr()
  external int hIconSm;
}

// =============================================================================
// Win32 常量
// =============================================================================

// Window styles
const int WS_POPUP = 0x80000000;

// Extended window styles
const int WS_EX_TOPMOST = 0x00000008;
const int WS_EX_TOOLWINDOW = 0x00000080;
const int WS_EX_NOACTIVATE = 0x08000000;
const int WS_EX_LAYERED = 0x00080000;

// ShowWindow commands
const int SW_SHOWNOACTIVATE = 4;

// SystemParametersInfoW
const int SPI_GETWORKAREA = 0x0030;

// Virtual key codes
const int VK_LBUTTON = 0x01;

// BLENDFUNCTION 常量
const int AC_SRC_OVER = 0x00;
const int AC_SRC_ALPHA = 0x01;

// UpdateLayeredWindow flags
const int ULW_ALPHA = 0x00000002;

// BITMAPINFOHEADER 常量
const int BI_RGB = 0;
const int DIB_RGB_COLORS = 0;

// =============================================================================
// DLL 句柄
// =============================================================================

final _user32 = DynamicLibrary.open('user32.dll');
final _gdi32 = DynamicLibrary.open('gdi32.dll');
final _kernel32 = DynamicLibrary.open('kernel32.dll');

// =============================================================================
// kernel32.dll
// =============================================================================

typedef _GetModuleHandleW_Native = IntPtr Function(Pointer<Utf16> lpModuleName);
typedef _GetModuleHandleW_Dart = int Function(Pointer<Utf16> lpModuleName);

final getModuleHandleW = _kernel32
    .lookupFunction<_GetModuleHandleW_Native, _GetModuleHandleW_Dart>(
      'GetModuleHandleW',
    );

// =============================================================================
// user32.dll
// =============================================================================

// RegisterClassExW
typedef _RegisterClassExW_Native = Uint16 Function(
  Pointer<WNDCLASSEXW> lpwcx,
);
typedef _RegisterClassExW_Dart = int Function(Pointer<WNDCLASSEXW> lpwcx);
final registerClassExW = _user32
    .lookupFunction<_RegisterClassExW_Native, _RegisterClassExW_Dart>(
      'RegisterClassExW',
    );

// CreateWindowExW
typedef _CreateWindowExW_Native =
    IntPtr Function(
      Uint32 dwExStyle,
      Pointer<Utf16> lpClassName,
      Pointer<Utf16> lpWindowName,
      Uint32 dwStyle,
      Int32 x,
      Int32 y,
      Int32 nWidth,
      Int32 nHeight,
      IntPtr hWndParent,
      IntPtr hMenu,
      IntPtr hInstance,
      Pointer<Void> lpParam,
    );
typedef _CreateWindowExW_Dart =
    int Function(
      int dwExStyle,
      Pointer<Utf16> lpClassName,
      Pointer<Utf16> lpWindowName,
      int dwStyle,
      int x,
      int y,
      int nWidth,
      int nHeight,
      int hWndParent,
      int hMenu,
      int hInstance,
      Pointer<Void> lpParam,
    );
final createWindowExW = _user32
    .lookupFunction<_CreateWindowExW_Native, _CreateWindowExW_Dart>(
      'CreateWindowExW',
    );

// DestroyWindow
typedef _DestroyWindow_Native = Int32 Function(IntPtr hWnd);
typedef _DestroyWindow_Dart = int Function(int hWnd);
final destroyWindow = _user32
    .lookupFunction<_DestroyWindow_Native, _DestroyWindow_Dart>(
      'DestroyWindow',
    );

// ShowWindow
typedef _ShowWindow_Native = Int32 Function(IntPtr hWnd, Int32 nCmdShow);
typedef _ShowWindow_Dart = int Function(int hWnd, int nCmdShow);
final showWindow = _user32
    .lookupFunction<_ShowWindow_Native, _ShowWindow_Dart>('ShowWindow');

// DefWindowProcW — 直接获取原生函数指针，用作 WndProc（绕过 Dart isolate）
final defWindowProcWPtr = _user32
    .lookup<NativeFunction<WNDPROC_Native>>('DefWindowProcW');

// SystemParametersInfoW
typedef _SystemParametersInfoW_Native =
    Int32 Function(
      Uint32 uiAction,
      Uint32 uiParam,
      Pointer<Void> pvParam,
      Uint32 fWinIni,
    );
typedef _SystemParametersInfoW_Dart =
    int Function(
      int uiAction,
      int uiParam,
      Pointer<Void> pvParam,
      int fWinIni,
    );
final systemParametersInfoW = _user32
    .lookupFunction<
      _SystemParametersInfoW_Native,
      _SystemParametersInfoW_Dart
    >('SystemParametersInfoW');

// GetCursorPos
typedef _GetCursorPos_Native = Int32 Function(Pointer<POINT> lpPoint);
typedef _GetCursorPos_Dart = int Function(Pointer<POINT> lpPoint);
final getCursorPos = _user32
    .lookupFunction<_GetCursorPos_Native, _GetCursorPos_Dart>('GetCursorPos');

// ScreenToClient
typedef _ScreenToClient_Native = Int32 Function(
  IntPtr hWnd,
  Pointer<POINT> lpPoint,
);
typedef _ScreenToClient_Dart = int Function(int hWnd, Pointer<POINT> lpPoint);
final screenToClient = _user32
    .lookupFunction<_ScreenToClient_Native, _ScreenToClient_Dart>(
      'ScreenToClient',
    );

// LoadCursorW
typedef _LoadCursorW_Native = HCURSOR Function(
  HINSTANCE hInstance,
  Pointer<Utf16> lpCursorName,
);
typedef _LoadCursorW_Dart = int Function(
  int hInstance,
  Pointer<Utf16> lpCursorName,
);
final loadCursorW = _user32
    .lookupFunction<_LoadCursorW_Native, _LoadCursorW_Dart>('LoadCursorW');

// GetDpiForWindow
typedef _GetDpiForWindow_Native = Uint32 Function(IntPtr hwnd);
typedef _GetDpiForWindow_Dart = int Function(int hwnd);
final getDpiForWindow = _user32
    .lookupFunction<_GetDpiForWindow_Native, _GetDpiForWindow_Dart>(
      'GetDpiForWindow',
    );

// GetAsyncKeyState — 查询键/鼠标按钮异步状态（返回 SHORT）
typedef _GetAsyncKeyState_Native = Int16 Function(Int32 vKey);
typedef _GetAsyncKeyState_Dart = int Function(int vKey);
final getAsyncKeyState = _user32
    .lookupFunction<_GetAsyncKeyState_Native, _GetAsyncKeyState_Dart>(
      'GetAsyncKeyState',
    );

// UpdateLayeredWindow — per-pixel alpha 分层窗口整图更新
typedef _UpdateLayeredWindow_Native =
    Int32 Function(
      IntPtr hWnd,
      HDC hdcDst,
      Pointer<POINT> pptDst,
      Pointer<SIZE> psize,
      HDC hdcSrc,
      Pointer<POINT> pptSrc,
      COLORREF crKey,
      Pointer<BLENDFUNCTION> pblend,
      Uint32 dwFlags,
    );
typedef _UpdateLayeredWindow_Dart =
    int Function(
      int hWnd,
      int hdcDst,
      Pointer<POINT> pptDst,
      Pointer<SIZE> psize,
      int hdcSrc,
      Pointer<POINT> pptSrc,
      int crKey,
      Pointer<BLENDFUNCTION> pblend,
      int dwFlags,
    );
final updateLayeredWindow = _user32
    .lookupFunction<_UpdateLayeredWindow_Native, _UpdateLayeredWindow_Dart>(
      'UpdateLayeredWindow',
    );

// =============================================================================
// gdi32.dll
// =============================================================================

// DeleteObject
typedef _DeleteObject_Native = Int32 Function(HGDIOBJ ho);
typedef _DeleteObject_Dart = int Function(int ho);
final deleteObject = _gdi32
    .lookupFunction<_DeleteObject_Native, _DeleteObject_Dart>('DeleteObject');

// SelectObject
typedef _SelectObject_Native = HGDIOBJ Function(HDC hdc, HGDIOBJ h);
typedef _SelectObject_Dart = int Function(int hdc, int h);
final selectObject = _gdi32
    .lookupFunction<_SelectObject_Native, _SelectObject_Dart>('SelectObject');

// CreateCompatibleDC
typedef _CreateCompatibleDC_Native = HDC Function(HDC hdc);
typedef _CreateCompatibleDC_Dart = int Function(int hdc);
final createCompatibleDC = _gdi32
    .lookupFunction<_CreateCompatibleDC_Native, _CreateCompatibleDC_Dart>(
      'CreateCompatibleDC',
    );

// DeleteDC
typedef _DeleteDC_Native = Int32 Function(HDC hdc);
typedef _DeleteDC_Dart = int Function(int hdc);
final deleteDC = _gdi32
    .lookupFunction<_DeleteDC_Native, _DeleteDC_Dart>('DeleteDC');

// CreateDIBSection — 创建可直接写像素的 32bpp DIB
typedef _CreateDIBSection_Native =
    HBITMAP Function(
      HDC hdc,
      Pointer<BITMAPINFOHEADER> pbmi,
      Uint32 usage,
      Pointer<Pointer<Void>> ppvBits,
      IntPtr hSection,
      Uint32 offset,
    );
typedef _CreateDIBSection_Dart =
    int Function(
      int hdc,
      Pointer<BITMAPINFOHEADER> pbmi,
      int usage,
      Pointer<Pointer<Void>> ppvBits,
      int hSection,
      int offset,
    );
final createDIBSection = _gdi32
    .lookupFunction<_CreateDIBSection_Native, _CreateDIBSection_Dart>(
      'CreateDIBSection',
    );
