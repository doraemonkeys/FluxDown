#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────
# deploy.sh — 服务器端自动部署脚本
#
# 拉取最新代码 → 重建镜像 → 滚动重启容器 → 清理悬空镜像
#
# 用法（在服务器 website/ 目录执行，或由 CI 远程调用）:
#   ./deploy.sh
#
# 约定:
#   - 仅当远端有新提交时才重建（无变更时快速退出，幂等可重复跑）
#   - 失败立即中止，不会留下半启动状态
# ─────────────────────────────────────────────────────────────
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

# 仓库根目录（website 的上一级）
REPO_DIR="$(git -C "${SCRIPT_DIR}" rev-parse --show-toplevel)"
BRANCH="${DEPLOY_BRANCH:-main}"

echo "=========================================="
echo "  FluxDown Website — Deploy"
echo "  仓库   : ${REPO_DIR}"
echo "  分支   : ${BRANCH}"
echo "  时间   : $(date '+%Y-%m-%d %H:%M:%S')"
echo "------------------------------------------"

# ── 1. 拉取最新代码 ──────────────────────────
echo "[1/4] 拉取最新代码..."
git -C "${REPO_DIR}" fetch origin "${BRANCH}"

LOCAL_SHA="$(git -C "${REPO_DIR}" rev-parse HEAD)"
REMOTE_SHA="$(git -C "${REPO_DIR}" rev-parse "origin/${BRANCH}")"

if [ "${LOCAL_SHA}" = "${REMOTE_SHA}" ]; then
  echo "      已是最新 (${LOCAL_SHA:0:8})，无需部署。"
  exit 0
fi

git -C "${REPO_DIR}" reset --hard "origin/${BRANCH}"
echo "      ✓ 更新到 ${REMOTE_SHA:0:8}"

# ── 2. 重建镜像 ──────────────────────────────
echo "[2/4] 重建 Docker 镜像..."
docker compose build website

# ── 3. 滚动重启 ──────────────────────────────
echo "[3/4] 重启容器..."
docker compose up -d website

# ── 4. 清理悬空镜像 ──────────────────────────
echo "[4/4] 清理悬空镜像..."
docker image prune -f >/dev/null 2>&1 || true

echo "------------------------------------------"
echo "  ✓ 部署完成: ${REMOTE_SHA:0:8}"
echo "=========================================="
