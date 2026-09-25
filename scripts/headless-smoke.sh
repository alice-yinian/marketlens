#!/usr/bin/env bash
#
# 无头冒烟：真实启动 MarketLens 并截图。
#
# 为什么需要它：本机是 AlmaLinux 10，而 RHEL 10 已移除 X.Org 服务器
# （Xvfb 在 EL10 上不存在），EPEL 的 ImageMagick 又没编译 X11 支持，
# weston 的 screenshooter 协议则拒绝授权。可行组合是：
#   weston headless（Wayland 显示）+ Xwayland（真实 X display）
#   + 自建 xshot（x11rb 抓窗口，见 MARKETLENS_SHOT）
#
# 注意 Tauri 的 debug 构建会连接 devUrl 而不是内嵌前端资源，
# 因此 debug 二进制必须先起 vite dev server（MARKETLENS_DEV=1）。
# release 构建内嵌 dist，不需要 dev server。
#
# 用法：
#   MARKETLENS_DEV=1 scripts/headless-smoke.sh /tmp/shot.png
#   MARKETLENS_BIN=src-tauri/target/release/marketlens scripts/headless-smoke.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${MARKETLENS_BIN:-$ROOT/src-tauri/target/debug/marketlens}"
# 截图/输入工具在仓库内（tools/xshot），构建：cd tools/xshot && cargo build --release
SHOT="${MARKETLENS_SHOT:-$ROOT/tools/xshot/target/release/xshot}"
OUT="${1:-/tmp/marketlens-smoke.png}"
DEV="${MARKETLENS_DEV:-0}"

if [[ ! -x "$BIN" ]]; then
  echo "找不到可执行文件：$BIN" >&2
  exit 1
fi
if [[ ! -x "$SHOT" ]]; then
  echo "找不到截图工具：$SHOT（需先构建 x11rb 抓屏工具）" >&2
  exit 1
fi

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/marketlens-xdg}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

SOCKET="marketlens-wl"
WIDTH=1280
HEIGHT=840
WESTON_INI=/tmp/marketlens-weston.ini
WESTON_LOG=/tmp/marketlens-weston.log
APP_LOG=/tmp/marketlens-app.log
PPM=/tmp/marketlens-shot.ppm

cat >"$WESTON_INI" <<EOF
[core]
renderer=pixman

[output]
name=headless
mode=${WIDTH}x${HEIGHT}
EOF

cleanup() {
  [[ -n "${APP_PID:-}" ]] && kill "$APP_PID" 2>/dev/null || true
  [[ -n "${VITE_PID:-}" ]] && kill "$VITE_PID" 2>/dev/null || true
  [[ -n "${WESTON_PID:-}" ]] && kill "$WESTON_PID" 2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup EXIT

if [[ "$DEV" == "1" ]]; then
  echo "[0/5] 启动 vite dev server（debug 构建连接 devUrl，不内嵌前端资源）"
  (cd "$ROOT" && npm run dev >/tmp/marketlens-vite.log 2>&1) &
  VITE_PID=$!
  for _ in $(seq 1 80); do
    curl -sf -o /dev/null http://localhost:1420 && break
    sleep 0.25
  done
  curl -sf -o /dev/null http://localhost:1420 || {
    echo "vite dev server 未就绪，日志：" >&2; tail -20 /tmp/marketlens-vite.log >&2; exit 1; }
fi

echo "[1/5] 启动 weston headless + Xwayland（${WIDTH}x${HEIGHT}）"
weston --backend=headless-backend.so --socket="$SOCKET" --config="$WESTON_INI" \
       --idle-time=0 --xwayland >"$WESTON_LOG" 2>&1 &
WESTON_PID=$!

for _ in $(seq 1 60); do
  grep -q 'xserver listening on display' "$WESTON_LOG" 2>/dev/null && break
  sleep 0.25
done
XDISPLAY="$(grep -oE 'xserver listening on display (:[0-9]+)' "$WESTON_LOG" | head -1 | grep -oE ':[0-9]+' || true)"
if [[ -z "$XDISPLAY" ]]; then
  echo "Xwayland 未就绪，weston 日志：" >&2
  tail -30 "$WESTON_LOG" >&2
  exit 1
fi
echo "      X display = $XDISPLAY"

echo "[2/5] 启动应用"
DISPLAY="$XDISPLAY" GDK_BACKEND=x11 \
  WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  LIBGL_ALWAYS_SOFTWARE=1 \
  "$BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

for _ in $(seq 1 80); do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "应用启动即退出，日志：" >&2
    cat "$APP_LOG" >&2
    exit 1
  fi
  grep -q "数据库就绪" "$APP_LOG" 2>/dev/null && break
  sleep 0.25
done
# 迁移完成 ≠ 首屏渲染完成。实盘页挂载后会立刻发起一次真实采集
# （3 个标的约 22 个请求，实测 2~4 秒），所以等待要覆盖它。
sleep "${MARKETLENS_WAIT:-12}"

echo "[3/5] 应用日志"
cat "$APP_LOG"

echo "[4/5] 抓屏"
DISPLAY="$XDISPLAY" "$SHOT" "$PPM"
magick "$PPM" "$OUT"

echo "[5/5] 完成 → $OUT"
ls -la "$OUT"
