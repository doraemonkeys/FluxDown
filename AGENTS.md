# PROJECT KNOWLEDGE BASE

**Generated:** 2026-02-08
**Branch:** master (initial, no commits)

## OVERVIEW

类迅雷的多协议下载工具。Flutter 负责 GUI（任务管理、进度展示），Rust 负责高性能下载引擎（多线程分片、协议解析、断点续传），通过 [Rinf](https://rinf.cunarist.org) 框架实现 Dart↔Rust FFI 通信。

## STRUCTURE

```
x_down/
├── lib/
│   ├── main.dart                  # Flutter 入口 (MyApp widget)
│   └── src/bindings/              # ⚠️ 自动生成 — 勿手动编辑
│       ├── signals/               # Rust↔Dart 信号类型 (由 rinf gen 生成)
│       ├── serde/                 # 序列化工具
│       └── bincode/               # 二进制编码
├── native/hub/                    # Rust 后端 crate
│   └── src/
│       ├── lib.rs                 # Rust 入口 (tokio async main)
│       ├── signals/mod.rs         # 信号结构体定义 (DartSignal/RustSignal)
│       └── actors/                # Actor 模型：消息传递式并发
│           ├── mod.rs             # Actor 创建与编排
│           ├── first.rs           # FirstActor: 监听 Dart 信号 + 定时器
│           └── second.rs          # SecondActor: 跨 Actor 请求-响应
├── android/ ios/ macos/ linux/ windows/ web/  # 平台壳
├── pubspec.yaml                   # Flutter 依赖
├── Cargo.toml                     # Rust workspace
└── test/widget_test.dart          # 唯一测试文件
```

## WHERE TO LOOK

| 任务 | 位置 | 说明 |
|------|------|------|
| 添加新页面/Widget | `lib/main.dart` | 当前单文件，需自行拆分 |
| 定义新的 Dart↔Rust 信号 | `native/hub/src/signals/mod.rs` | 修改后必须运行 `rinf gen` |
| 添加新 Actor | `native/hub/src/actors/` | 参考 `first.rs` 模式 |
| Actor 注册 | `native/hub/src/actors/mod.rs` | `create_actors()` 中 spawn |
| 查看生成的 Dart 绑定 | `lib/src/bindings/signals/` | 只读参考，勿编辑 |
| 平台特定配置 | `android/` `ios/` `windows/` 等 | 标准 Flutter 平台壳 |

## CONVENTIONS

### Rust 端

- **错误处理**：禁止 `.unwrap()` 和 `.expect()`（Clippy deny），必须用 `?` 或 `match`
- **导入**：禁止通配符导入 `use module::*`（Clippy deny），必须显式导入
- **异步**：始终使用非阻塞 async 函数；阻塞操作用 `tokio::task::spawn_blocking`
- **Actor 模式**：通过 `messages` crate 实现 Actor，避免共享内存，使用消息传递
- **信号定义**：`DartSignal` = Dart→Rust, `RustSignal` = Rust→Dart, `SignalPiece` = 嵌套片段
- **Crate 名**：`hub` 不可更改（Rinf 框架硬编码依赖）
- **Edition**：Rust 2024

### Dart 端

- 使用 `flutter_lints` 推荐规则集
- `lib/src/bindings/` 全部为自动生成，头部有 `// ignore_for_file: type=lint`
- **UI 组件库**：全程使用 [shadcn_ui](https://flutter-shadcn-ui.mariuti.com/)（`^0.45.2`），禁止使用原生 Material/Cupertino 组件

### shadcn_ui 规范

- **统一导入**：`import 'package:shadcn_ui/shadcn_ui.dart';`（单入口，含 LucideIcons、flutter_animate 等）
- **App 根组件**：使用 `ShadApp` 替代 `MaterialApp`；若需 Material 互操作用 `ShadApp.custom` + `ShadAppBuilder`
- **主题**：通过 `ShadThemeData` + `ShadXxxColorScheme.light()/.dark()` 配置，支持 `ThemeMode.system` 自动明暗切换
- **主题访问**：`ShadTheme.of(context)` 获取主题数据，不用 `Theme.of(context)`（除 Material 互操作场景）
- **图标**：使用 `LucideIcons.xxx`（已内置，无需额外导入）
- **按钮变体**：`ShadButton()`、`ShadButton.secondary()`、`ShadButton.destructive()`、`ShadButton.outline()`、`ShadButton.ghost()`、`ShadButton.link()`
- **表单**：使用 `ShadForm` + `ShadXxxFormField`（如 `ShadInputFormField`、`ShadSelectFormField`），通过 `GlobalKey<ShadFormState>` 管理状态
- **对话框**：使用 `showShadDialog()` 而非 `showDialog()`
- **表格**：使用 `ShadTable.list()` 而非 `DataTable`
- **可用颜色方案**：Slate / Zinc / Blue / Gray / Green / Neutral / Orange / Red / Rose / Stone / Violet / Yellow
- **组件命名映射**：Popover（替代 HoverCard）、ContextMenu（替代 DropdownMenu）、Sonner（Toast 通知）

### 通用

- Web 支持已预留但注释掉（见 `tokio_with_wasm` 注释）

## ANTI-PATTERNS

| 禁止 | 原因 |
|------|------|
| 手动编辑 `lib/src/bindings/**` | 会被 `rinf gen` 覆盖 |
| Rust 中使用 `.unwrap()` / `.expect()` | Clippy deny，编译失败 |
| Rust 中使用 `use foo::*` | Clippy deny，编译失败 |
| 更改 crate name `hub` | Rinf 框架依赖此名称 |
| 在 async 上下文中使用阻塞 I/O | 会阻塞 tokio 单线程 runtime |
| 使用原生 Material/Cupertino 组件 | 全程使用 shadcn_ui 组件库 |
| `MaterialApp` 作为根组件 | 使用 `ShadApp`（或 `ShadApp.custom` 互操作） |
| `showDialog()` | 使用 `showShadDialog()` |
| `Theme.of(context)` 获取主题 | 使用 `ShadTheme.of(context)` |
| 手动创建 `ThemeData(...)` | ShadApp 自动生成，Material 互操作时用 `Theme.of(context)` |

## COMMANDS

```bash
# 开发
flutter run                    # 运行应用
flutter run -d windows         # 指定 Windows 平台

# 信号绑定生成（修改 Rust 信号后必须执行）
rinf gen

# Rust
cargo build                    # 构建 Rust 后端
cargo clippy                   # Lint 检查（严格模式）

# 测试
flutter test                   # Dart widget 测试
flutter analyze                # Dart 静态分析

# 依赖
flutter pub get                # Dart 依赖
cargo install rinf_cli         # Rinf CLI 工具（首次）
```
