#!/usr/bin/env bash
# Bundle the macOS desktop app with `dx` and inject the branded app icon.
#
# 为什么需要这个脚本
# ------------------
# dioxus-cli 0.8.0-alpha.1 的桌面打包不会生成 .app 图标：`dx build` 只写
# CFBundleIconFile=icon.icns 却不产出该文件，`dx bundle`（真正带 resources 的那条）
# 干脆连 key 都不写——两种情况 Finder / Dock 都退回通用占位图标。本脚本在
# `dx bundle` 之后把 scripts/gen_icons.py 生成的 app-icon.icns 注入 Contents/
# Resources 并补全 Info.plist（见 inject_macos_icon.py），让桌面图标与 iOS /
# splash 使用同一套品牌图标。词典库等资源由 dx 桌面链路自动打包，无需额外注入。
#
# 用法
# ----
#   bash scripts/package_macos.sh                        # release 打包 + 注入图标
#   bash scripts/package_macos.sh --debug                # debug 打包
#   bash scripts/package_macos.sh --no-build             # 跳过 dx bundle，仅向已存在的 .app 注入
#   bash scripts/package_macos.sh --target x86_64-apple-darwin   # 跨架构打包（供 CI 出 Intel 包）
#
# --target 说明：dx 0.8.0-alpha.1 支持 `bundle --target <rustc 三元组>`，它会把该三元组
# 转给 cargo（产物落 target/<triple>/release/），但 **bundle 输出目录只按 OS 归档**
# （target/dx/synstudy/bundle/macos），因此同一台机器上先后打 arm64 与 x86_64 会互相覆盖。
# CI 里每个架构用独立 runner/job，本脚本一次只打一个架构。
#
# 注意
# ----
# * 图标注入须在签名之前（改动已签名 .app 会使签名失效）。
# * 依赖：dx（dioxus-cli 0.8.x）、macOS 的 iconutil / python3（生成 icns 见 gen_icons.py）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="release"
TARGET=""
DO_BUILD=1

usage() {
  cat <<'EOF'
用法: bash scripts/package_macos.sh [选项]
  --release        构建 release 包（默认）
  --debug          构建 debug 包
  --target TRIPLE  指定 rustc 三元组（如 x86_64-apple-darwin），用于跨架构打包
  --no-build       跳过 dx bundle，仅向已存在的 .app 注入图标
  -h, --help       显示本帮助
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --release)  PROFILE="release" ;;
    --debug)    PROFILE="debug" ;;
    --target)
      shift
      TARGET="${1:?--target 需要一个 rustc 三元组（例：x86_64-apple-darwin）}"
      ;;
    --no-build) DO_BUILD=0 ;;
    -h|--help)  usage; exit 0 ;;
    *) echo "未知参数: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

# dx bundle 产物目录（macOS 下多一层 bundle/macos/macos）。
BUNDLE_DIR="target/dx/synstudy/bundle/macos"

# 1) 打包（除非 --no-build）。桌面端 dx 会自动把 [bundle] resources 打进 .app。
if [ "$DO_BUILD" -eq 1 ]; then
  DX_ARGS=(bundle)
  [ "$PROFILE" = "release" ] && DX_ARGS+=(--release)
  [ -n "$TARGET" ] && DX_ARGS+=(--target "$TARGET")
  echo "==> dx ${DX_ARGS[*]}"
  dx "${DX_ARGS[@]}"
fi

# 2) 定位刚产出的 .app（取 bundle 目录下最新的）。
APP_DIR="$(find "$BUNDLE_DIR" -name '*.app' -type d 2>/dev/null -exec stat -f '%m %N' {} + | sort -nr | head -1 | cut -d' ' -f2- || true)"
if [ -z "$APP_DIR" ] || [ ! -d "$APP_DIR" ]; then
  echo "❌ 未找到打包产物（目录: ${BUNDLE_DIR}）。先不带 --no-build 跑一次打包。" >&2
  exit 1
fi

# 3) 图标源文件必须存在（由 gen_icons.py 在 macOS 上生成）。
if [ ! -f "assets/icons/app-icon.icns" ]; then
  echo "❌ 缺少 assets/icons/app-icon.icns；先运行: python3 scripts/gen_icons.py" >&2
  exit 1
fi

# 4) 注入图标（拷贝 icon.icns + 补全 CFBundleIconFile）。
python3 scripts/inject_macos_icon.py "$APP_DIR"

echo "✅ 桌面图标已注入: $APP_DIR"
echo "   若 Finder 仍显示旧图标，清一次图标缓存或把 .app 移到别处再移回。"
