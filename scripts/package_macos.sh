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
#   bash scripts/package_macos.sh --no-dmg               # 注入图标但不重建 .dmg
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
# * dmg 必须在图标注入**之后**重建：dx bundle 会顺手把当时的 .app 打进 .dmg，而它自产
#   的 Synstudy.icns 混入了长度不合规范的 legacy 位图表示（is32/il32/it32 + 1-bit 掩码），
#   macOS 取到坏表示时图标呈现「深色方块 + 白色圆角边框」；iconutil 产的 icon.icns 才是
#   全现代 PNG 规范结构。故本脚本注入图标后用打过补丁的 .app 重新生成同路径 dmg。
# * 本脚本按**默认 feature** 打包，产出的 .app **不含「截图/拍照翻译」**（dx bundle
#   未传 --features translate）。要打带翻译的包，须先用 check_translate.sh 同款工具链
#   环境跑 `dx bundle --release --features translate`，再 `bash scripts/package_macos.sh
#   --no-build` 注入图标并重建 dmg。详见 docs/translate.md 第 4 节「打包带翻译功能的桌面安装包」。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="release"
TARGET=""
DO_BUILD=1
DO_DMG=1

usage() {
  cat <<'EOF'
用法: bash scripts/package_macos.sh [选项]
  --release        构建 release 包（默认）
  --debug          构建 debug 包
  --target TRIPLE  指定 rustc 三元组（如 x86_64-apple-darwin），用于跨架构打包
  --no-build       跳过 dx bundle，仅向已存在的 .app 注入图标
  --no-dmg         注入图标但不重建 .dmg
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
    --no-dmg)   DO_DMG=0 ;;
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

# 5) 用打过补丁的 .app 重建 dmg：dx bundle 早已把「未注入图标」的 .app 打进同目录的
#    .dmg（且 dx 自产的 Synstudy.icns 含残缺 legacy 表示），不重建则安装后图标异常。
#    先写到临时文件再原子覆盖，避免 hdiutil 中途失败把旧包也删了（-ov 会先删目标）。
if [ "$DO_DMG" -eq 1 ]; then
  VOL_NAME="$(basename "$APP_DIR" .app)"
  DMG_PATH="$(find "$BUNDLE_DIR" -name '*.dmg' -type f 2>/dev/null -exec stat -f '%m %N' {} + | sort -nr | head -1 | cut -d' ' -f2- || true)"
  if [ -z "$DMG_PATH" ] || [ ! -f "$DMG_PATH" ]; then
    VER="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DIR/Contents/Info.plist" 2>/dev/null || echo 0.0.0)"
    ARCH="$(uname -m)"; [ "$ARCH" = "arm64" ] && ARCH="aarch64"
    DMG_PATH="$(dirname "$APP_DIR")/${VOL_NAME}_${VER}_${ARCH}.dmg"
    echo "==> 未找到旧 dmg，新建安装包: ${DMG_PATH}"
  else
    echo "==> 重建安装包: ${DMG_PATH}"
  fi
  # 旧卷若仍挂载会阻碍覆写，能卸则卸（卸不掉也不阻断）。
  hdiutil detach "/Volumes/${VOL_NAME}" -force >/dev/null 2>&1 || true
  STAGE="$(mktemp -d)"; OUTTMP="$(mktemp -d)"
  trap 'rm -rf "$STAGE" "$OUTTMP"' EXIT
  cp -R "$APP_DIR" "$STAGE/$VOL_NAME.app"
  ln -s /Applications "$STAGE/Applications"
  hdiutil create -volname "$VOL_NAME" -srcfolder "$STAGE" -ov -format UDZO "$OUTTMP/out.dmg" >/dev/null
  mv -f "$OUTTMP/out.dmg" "$DMG_PATH"
  rm -rf "$STAGE" "$OUTTMP"; trap - EXIT
fi

echo "✅ 桌面图标已注入: $APP_DIR"
echo "   若 Finder 仍显示旧图标，清一次图标缓存或把 .app 移到别处再移回。"
