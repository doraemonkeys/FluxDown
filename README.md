# FluxDown

A new Flutter project.

## Getting Started

This project is a starting point for a Flutter application.

A few resources to get you started if this is your first Flutter project:

- [Lab: Write your first Flutter app](https://docs.flutter.dev/get-started/codelab)
- [Cookbook: Useful Flutter samples](https://docs.flutter.dev/cookbook)

For help getting started with Flutter development, view the
[online documentation](https://docs.flutter.dev/), which offers tutorials,
samples, guidance on mobile development, and a full API reference.

## Using Rust Inside Flutter

This project leverages Flutter for GUI and Rust for the backend logic,
utilizing the capabilities of the
[Rinf](https://pub.dev/packages/rinf) framework.

To run and build this app, you need to have
[Flutter SDK](https://docs.flutter.dev/get-started/install)
and [Rust toolchain](https://www.rust-lang.org/tools/install)
installed on your system.
You can check that your system is ready with the commands below.
Note that all the Flutter subcomponents should be installed.

```shell
rustc --version
flutter doctor
```

You also need to have the CLI tool for Rinf ready.

```shell
cargo install rinf_cli
```

Signals sent between Dart and Rust are implemented using signal attributes.
If you've modified the signal structs, run the following command
to generate the corresponding Dart classes:

```shell
rinf gen
```

Now you can run and build this app just like any other Flutter projects.

```shell
flutter run
```

For detailed instructions on writing Rust and Flutter together,
please refer to Rinf's [documentation](https://rinf.cunarist.org).

## 发布版本

项目使用 `scripts/release_tag.py` 脚本发布新版本。脚本会自动提取 commit 记录，调用 Claude CLI 生成中文 Release Notes，创建 annotated tag 后推送触发 CI 构建。

前置要求：本地已安装 [Claude CLI](https://docs.anthropic.com/en/docs/claude-cli)。

```shell
# 日常发布（推荐）
python scripts/release_tag.py v0.1.7 --push --github-release --update-changelog

# 高质量双语发布
python scripts/release_tag.py v0.1.7 --model sonnet --lang both --push --github-release

# 仅预览效果
python scripts/release_tag.py v0.1.7 --dry-run
```
