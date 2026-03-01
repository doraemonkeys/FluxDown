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

## 构建 Linux 安装包

确保已安装系统依赖：

```shell
# Debian/Ubuntu
sudo apt-get install cmake ninja-build clang pkg-config \
  libgtk-3-dev libayatana-appindicator3-dev libnotify-dev libsecret-1-dev patchelf zstd

# Arch Linux
sudo pacman -S cmake ninja clang pkgconf gtk3 libayatana-appindicator libnotify libsecret patchelf zstd
```

构建 Flutter 应用和 NMH 中继二进制：

```shell
flutter build linux --release
cargo build --release -p fluxdown_nmh
cp target/release/fluxdown_nmh build/linux/x64/release/bundle/
```

打包为 Arch `.pkg.tar.zst`：

```shell
VERSION=0.1.0  # 替换为实际版本号
BUNDLE_DIR="build/linux/x64/release/bundle"
PKG_DIR="arch_pkg"

mkdir -p "$PKG_DIR/opt/fluxdown" "$PKG_DIR/usr/bin" \
         "$PKG_DIR/usr/share/applications" \
         "$PKG_DIR/usr/share/icons/hicolor/256x256/apps"

cp -a "$BUNDLE_DIR/." "$PKG_DIR/opt/fluxdown/"
printf '#!/bin/bash\nexec /opt/fluxdown/flux_down "$@"\n' > "$PKG_DIR/usr/bin/flux_down"
chmod 755 "$PKG_DIR/usr/bin/flux_down"
cp linux/com.fluxdown.app.desktop "$PKG_DIR/usr/share/applications/"
cp assets/logo/fluxdown_logo.png "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/com.fluxdown.app.png"

INSTALLED_SIZE=$(du -sb "$PKG_DIR/opt/" "$PKG_DIR/usr/" | awk '{sum+=$1} END {print sum}')
cat > "$PKG_DIR/.PKGINFO" <<EOF
pkgname = fluxdown
pkgver = ${VERSION}-1
pkgdesc = Free IDM-alternative download manager
url = https://fluxdown.app
builddate = $(date +%s)
packager = FluxDown CI <ci@fluxdown.app>
size = ${INSTALLED_SIZE}
arch = x86_64
license = custom
depend = gtk3
depend = libayatana-appindicator
depend = libnotify
EOF

mkdir -p build/installer
cd "$PKG_DIR"
tar --zstd --owner=0 --group=0 -cf \
  "../build/installer/FluxDown-${VERSION}-linux-x64.pkg.tar.zst" \
  .PKGINFO opt/ usr/
```

安装：

```shell
sudo pacman -U build/installer/FluxDown-${VERSION}-linux-x64.pkg.tar.zst
```

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
