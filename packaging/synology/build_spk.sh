#!/bin/sh
# 纯脚本手打群晖 SPK（无需官方 toolkit / chroot 环境，做法同 SynoCommunity spksrc）。
# .spk = 顶层 tar（INFO + package.tgz + scripts/ + conf/ + 图标），仅在 Linux CI 上运行。
#
# 用法：build_spk.sh <version> <dsm6|dsm7> <x86_64|armv8> <binary> <webroot_dir> <out_spk_path>
#   dsm7: os_min_ver=7.0，conf/privilege 以套件专属用户运行（DSM 7 禁止 root）
#   dsm6: os_min_ver=6.0 + os_max_ver=7.0 上界，root 运行（DSM 6 默认）
#   arch 为群晖架构家族值（官方 Appendix A）：x86_64 覆盖全部 Intel/AMD 机型，
#   armv8 覆盖 rtd1296/rtd1619b/armada37xx 等 ARM64 机型。
#
# 前置：imagemagick（convert）用于从 assets/logo/fluxdown_logo.png 生成套件图标。
set -eu

[ $# -eq 6 ] || { echo "usage: $0 <version> <dsm6|dsm7> <x86_64|armv8> <binary> <webroot> <out_spk>" >&2; exit 2; }
VERSION=$1 DSM=$2 ARCH=$3 BIN=$4 WEBROOT=$5 OUT=$6

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
LOGO="$REPO_ROOT/assets/logo/fluxdown_logo.png"

[ -f "$BIN" ] || { echo "binary not found: $BIN" >&2; exit 1; }
[ -d "$WEBROOT" ] || { echo "webroot not found: $WEBROOT" >&2; exit 1; }
command -v convert >/dev/null || { echo "imagemagick 'convert' not in PATH" >&2; exit 1; }

case "$DSM" in
	dsm6|dsm7) ;;
	*) echo "invalid dsm generation: $DSM (expect dsm6|dsm7)" >&2; exit 2 ;;
esac
case "$ARCH" in
	x86_64|armv8) ;;
	*) echo "invalid arch: $ARCH (expect x86_64|armv8)" >&2; exit 2 ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage="$work/stage"
payload="$work/payload"

# ── package.tgz 载荷：bin/fluxdown-server + web/ ──
mkdir -p "$payload/bin"
cp "$BIN" "$payload/bin/fluxdown-server"
chmod 755 "$payload/bin/fluxdown-server"
cp -r "$WEBROOT" "$payload/web"

mkdir -p "$stage"
tar -czf "$stage/package.tgz" --owner=0 --group=0 --numeric-owner -C "$payload" bin web

# ── INFO ──
EXTRACT_KB=$(du -sk "$payload" | cut -f1)
CHECKSUM=$(md5sum "$stage/package.tgz" | cut -d' ' -f1)
{
	echo 'package="FluxDown"'
	echo "version=\"$VERSION\""
	echo 'displayname="FluxDown Server"'
	echo 'description="Blazing fast, multi-protocol download manager. Rust engine with HTTP/HTTPS/FTP/BitTorrent/HLS support, intelligent segmentation, and a full Web UI on port 17800."'
	echo 'maintainer="zerx-lab"'
	echo 'maintainer_url="https://fluxdown.zerx.dev"'
	echo 'support_url="https://github.com/zerx-lab/FluxDown/issues"'
	echo "arch=\"$ARCH\""
	echo 'thirdparty="yes"'
	echo 'startable="yes"'
	echo 'adminport="17800"'
	echo "extractsize=\"$EXTRACT_KB\""
	echo "checksum=\"$CHECKSUM\""
	if [ "$DSM" = "dsm7" ]; then
		echo 'os_min_ver="7.0-40000"'
	else
		echo 'os_min_ver="6.0-7321"'
		echo 'os_max_ver="7.0-40000"'
	fi
} > "$stage/INFO"

# ── conf/privilege：DSM 7 强制非 root，以套件专属用户运行；DSM 6 维持 root ──
mkdir -p "$stage/conf"
if [ "$DSM" = "dsm7" ]; then
	printf '{"defaults":{"run-as":"package"}}\n' > "$stage/conf/privilege"
else
	printf '{"defaults":{"run-as":"root"}}\n' > "$stage/conf/privilege"
fi

# ── scripts/（生命周期脚本；除 start-stop-status 外均为幂等空脚本） ──
mkdir -p "$stage/scripts"
cp "$SCRIPT_DIR/scripts/start-stop-status" "$stage/scripts/start-stop-status"
for s in preinst postinst preuninst postuninst preupgrade postupgrade; do
	printf '#!/bin/sh\nexit 0\n' > "$stage/scripts/$s"
done
chmod 755 "$stage/scripts/"*

# ── 图标（DSM 7 要求 64x64，DSM 6 要求 72x72；256 两代通用） ──
if [ "$DSM" = "dsm7" ]; then
	convert "$LOGO" -resize 64x64 "$stage/PACKAGE_ICON.PNG"
else
	convert "$LOGO" -resize 72x72 "$stage/PACKAGE_ICON.PNG"
fi
convert "$LOGO" -resize 256x256 "$stage/PACKAGE_ICON_256.PNG"

# ── 顶层 tar 即 .spk ──
mkdir -p "$(dirname "$OUT")"
tar -cf "$OUT" --owner=0 --group=0 --numeric-owner -C "$stage" \
	INFO PACKAGE_ICON.PNG PACKAGE_ICON_256.PNG package.tgz scripts conf
echo "built: $OUT"
