#!/usr/bin/env bash
# Build the iOS app with `dx` and inject the read-only dictionary database into
# the produced .app bundle (and optionally install + launch it on the simulator).
#
# 为什么需要这个脚本
# ------------------
# dioxus-cli 0.8 的 `dx build --platform ios` 不会把 [bundle] resources
# （assets/synstudy.db，约 235MB）拷进 iOS/Android 包——只有 macOS/Windows 会。
# 运行时 App 通过 `Bundle.main.resourceURL` 解析词典（见 src/platform/ios.rs 的
# bundle_dict_db()），对 iOS .app 而言该目录就是 bundle 根（与可执行文件同级），
# 也就是 `Synstudy.app/synstudy.db`。本脚本把这一步做成可复现的构建后置注入，
# 并可一键安装/启动到已启动的模拟器。
#
# 用法
# ----
#   bash scripts/package_ios.sh              # debug 构建 + 注入 db
#   bash scripts/package_ios.sh --release    # release 构建
#   bash scripts/package_ios.sh --install    # 构建后安装并启动到 booted 模拟器
#   bash scripts/package_ios.sh --no-build   # 跳过 dx 构建，仅向已存在的 .app 注入
#   bash scripts/package_ios.sh --features translate   # 打包带截图/拍照翻译（Phase 3）
#
# 注意
# ----
# * 模拟器包无需代码签名，注入的 db 直接生效。
# * 真机部署须在“签名之前”注入 db（改动已签名的 .app 会使签名失效）。真机打包 +
#   免费个人团队签名用 scripts/deploy_ios_device.sh（同样支持 --features translate）。
# * 依赖：dx（dioxus-cli 0.8.x）、Xcode 命令行工具（xcrun / simctl）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BUNDLE_ID="com.synvek.synstudy"
APP_NAME="Synstudy"
DB_SRC="assets/synstudy.db"
NOTICES_SRC="assets/THIRD-PARTY-NOTICES.txt"

PROFILE="debug"
DO_BUILD=1
DO_INSTALL=0
FEATURES=""

usage() {
  cat <<'EOF'
用法: bash scripts/package_ios.sh [选项]
  --release    构建 release 包（默认 debug）
  --debug      构建 debug 包（默认）
  --install    构建/注入后安装并启动到已启动(booted)的模拟器
  --no-build   跳过 dx 构建，仅向已存在的 .app 注入 db
  --features   透传给 dx build 的 cargo features（例如 "translate"）
  -h, --help   显示本帮助
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --release)  PROFILE="release"; shift ;;
    --debug)    PROFILE="debug"; shift ;;
    --install)  DO_INSTALL=1; shift ;;
    --no-build) DO_BUILD=0; shift ;;
    --features)
      [ $# -ge 2 ] || { echo "❌ --features 需要一个参数" >&2; exit 2; }
      FEATURES="$2"; shift 2 ;;
    --features=*) FEATURES="${1#*=}"; shift ;;
    -h|--help)  usage; exit 0 ;;
    *) echo "未知参数: $1" >&2; usage; exit 2 ;;
  esac
done

IOS_DIR="target/dx/synstudy/${PROFILE}/ios"
APP_DIR="${IOS_DIR}/${APP_NAME}.app"

# 1) 构建（除非 --no-build）。不带 --device => Apple Silicon 上产出
#    aarch64-apple-ios-sim，可直接装进模拟器、无需签名。
if [ "$DO_BUILD" -eq 1 ]; then
  DX_ARGS=(build --platform ios)
  [ "$PROFILE" = "release" ] && DX_ARGS+=(--release)
  [ -n "$FEATURES" ] && DX_ARGS+=(--features "$FEATURES")

  # translate feature 的 iOS 交叉编译环境（CMake toolchain / sysroot / cc-rs 与
  # bindgen flags）由 scripts/ios_translate_env.sh 统一注入，模拟器与真机共用一份
  # 实现（坑位说明见该文件顶部注释）。
  # shellcheck source=ios_translate_env.sh
  source "$(dirname "${BASH_SOURCE[0]}")/ios_translate_env.sh"
  ios_translate_env sim "$FEATURES"

  echo "==> dx ${DX_ARGS[*]}"
  dx "${DX_ARGS[@]}"
fi

# 2) 定位构建产物；若默认名不存在，回退到 ios 目录下任意 .app。
if [ ! -d "$APP_DIR" ]; then
  FOUND="$(find "$IOS_DIR" -maxdepth 1 -name '*.app' -type d 2>/dev/null | head -1 || true)"
  [ -n "$FOUND" ] && APP_DIR="$FOUND"
fi
if [ ! -d "$APP_DIR" ]; then
  echo "❌ 未找到构建产物（目录: ${IOS_DIR}）。先不带 --no-build 跑一次构建。" >&2
  exit 1
fi

# 3) 词典源文件必须存在。
if [ ! -f "$DB_SRC" ]; then
  echo "❌ 缺少词典库 ${DB_SRC}；先运行: python3 scripts/build_dict_db.py" >&2
  exit 1
fi

# 4) 把资源注入 bundle 根（= Bundle.main.resourceURL）。
DB_SIZE="$(du -h "$DB_SRC" | cut -f1)"
echo "==> 注入词典库 (${DB_SIZE}) -> ${APP_DIR}/synstudy.db"
cp -f "$DB_SRC" "${APP_DIR}/synstudy.db"
if [ -f "$NOTICES_SRC" ]; then
  cp -f "$NOTICES_SRC" "${APP_DIR}/THIRD-PARTY-NOTICES.txt"
fi

# 5) 校验确实落在运行时会读取的位置。
if [ ! -f "${APP_DIR}/synstudy.db" ]; then
  echo "❌ 注入失败：${APP_DIR}/synstudy.db 不存在" >&2
  exit 1
fi
echo "✅ 词典库已就位: ${APP_DIR}/synstudy.db"

# 4b) 注入应用图标（dx alpha.1 的 iOS 链路不处理 [bundle] icon，模拟器会显示
# 半透明占位图）：拷入 CFBundleIconFiles 传统图标集。
python3 scripts/inject_ios_icon.py "$APP_DIR"

# 6) 可选：安装并启动到 booted 模拟器。
if [ "$DO_INSTALL" -eq 1 ]; then
  if ! xcrun simctl list devices booted | grep -q '(Booted)'; then
    echo "❌ 没有已启动的模拟器；先执行: open -a Simulator 并启动一台设备" >&2
    exit 1
  fi
  echo "==> 安装并启动到 booted 模拟器"
  xcrun simctl uninstall booted "$BUNDLE_ID" 2>/dev/null || true
  xcrun simctl install booted "$APP_DIR"
  xcrun simctl launch booted "$BUNDLE_ID"
  echo "✅ 已启动 ${BUNDLE_ID}（截图验证: xcrun simctl io booted screenshot /tmp/ios.png）"
fi
