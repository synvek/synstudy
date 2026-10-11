#!/usr/bin/env bash
# Build the iOS app for a PHYSICAL device, inject the dictionary DB, re-sign,
# and install via `xcrun devicectl`.
#
# 为什么需要这个脚本
# ------------------
# 与模拟器不同，真机包必须满足两点才能安装/启动：
#   1) .app 里带一份覆盖「本设备 UDID + bundle id」的 provisioning profile，并用
#      对应的开发证书签名；
#   2) 词典库 assets/synstudy.db（约 235MB）要在【签名之前】注入到 .app 根目录
#      （= Bundle.main.resourceURL，见 src/platform/ios.rs 的 bundle_dict_db）。
#      改动已签名的 .app 会使签名失效，所以注入必须早于 codesign。
# dx 0.8 的 `dx build --platform ios --device` 既不打包 [bundle] resources，也不
# 保证完成免费个人团队的签名，故本脚本把「构建 → 注入 → 签名 → 安装」固化下来。
#
# 用法
# ----
#   # 最简：自动选第一台已配对真机 + 第一个 Apple Development 证书
#   bash scripts/deploy_ios_device.sh --profile /path/to/dev.mobileprovision
#
#   bash scripts/deploy_ios_device.sh --release                 # release 构建
#   bash scripts/deploy_ios_device.sh --device <UDID|名称>       # 指定设备
#   bash scripts/deploy_ios_device.sh --identity "Apple Development: ..."  # 指定证书
#   bash scripts/deploy_ios_device.sh --no-build                # 复用已构建的 .app
#   bash scripts/deploy_ios_device.sh --skip-sign               # 只构建+注入，不签名/安装
#   bash scripts/deploy_ios_device.sh --dry-run                 # 打印计划，不执行改动
#
# Provisioning profile 从哪来（免费个人团队）
# -------------------------------------------
# 用 Xcode 打开任意工程 -> Signing & Capabilities -> 选你的 Team -> Xcode 会生成
# 一份含当前设备的开发 profile；再到
#   ~/Library/Developer/Xcode/UserData/Provisioning Profiles/
# 找到对应 .mobileprovision 复制出来，用 --profile 传给本脚本。
# （付费开发者账号的 profile 可在 developer.apple.com 下载，同理。）
#
# 依赖：dx 0.8.x、Xcode（xcrun / devicectl / security / PlistBuddy）、
#       钥匙串里有一枚 Apple Development 证书、一台已配对的 iPhone/iPad。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BUNDLE_ID="com.synstudy.synstudy"
APP_NAME="Synstudy"
DB_SRC="assets/synstudy.db"
NOTICES_SRC="assets/THIRD-PARTY-NOTICES.txt"

PROFILE="debug"       # debug | release
DO_BUILD=1
DO_SIGN=1
DRY_RUN=0
DEVICE=""             # 设备 UDID 或名称（空则自动探测）
IDENTITY="${IOS_CODESIGN_IDENTITY:-}"
PROVISION="${IOS_PROVISIONING_PROFILE:-}"   # .mobileprovision 路径

usage() {
  cat <<'EOF'
用法: bash scripts/deploy_ios_device.sh [选项]
  --release                构建 release 包（默认 debug）
  --device <UDID|名称>     指定目标真机（默认自动选第一台已配对设备）
  --identity "<名称>"      指定 codesign 身份（默认第一个 Apple Development）
  --profile <.mobileprovision>  指定覆盖该设备的描述文件（签名必需）
  --no-build               跳过 dx 构建，复用已存在的 device .app
  --skip-sign              只构建+注入词典，不签名不安装（便于检查产物）
  --dry-run                打印将执行的动作，不落改动
  -h, --help               显示本帮助
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --release)   PROFILE="release"; shift ;;
    --debug)     PROFILE="debug"; shift ;;
    --device)    DEVICE="${2:?--device 需要 UDID 或名称}"; shift 2 ;;
    --identity)  IDENTITY="${2:?--identity 需要证书名称}"; shift 2 ;;
    --profile)   PROVISION="${2:?--profile 需要 .mobileprovision 路径}"; shift 2 ;;
    --no-build)  DO_BUILD=0; shift ;;
    --skip-sign) DO_SIGN=0; shift ;;
    --dry-run)   DRY_RUN=1; shift ;;
    -h|--help)   usage; exit 0 ;;
    *) echo "未知参数: $1" >&2; usage; exit 2 ;;
  esac
done

run() {  # 受 --dry-run 控制的执行包装
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "DRY-RUN> $*"
  else
    "$@"
  fi
}

# 词典源文件必须存在。
if [ ! -f "$DB_SRC" ]; then
  echo "❌ 缺少词典库 $DB_SRC；先运行: python3 scripts/build_dict_db.py" >&2
  exit 1
fi

# 1) 构建真机 .app（--device => aarch64-apple-ios，需签名后才能装）。
if [ "$DO_BUILD" -eq 1 ]; then
  DX_ARGS=(build --platform ios --device)
  [ "$PROFILE" = "release" ] && DX_ARGS+=(--release)
  echo "==> dx ${DX_ARGS[*]}"
  if [ "$DRY_RUN" -eq 1 ]; then echo "DRY-RUN> dx ${DX_ARGS[*]}"; else dx "${DX_ARGS[@]}"; fi
fi

# 2) 定位构建产物：优先 ios 目录下的 ${APP_NAME}.app，否则回退扫描任意 .app。
IOS_DIR="target/dx/synstudy/${PROFILE}/ios"
APP_DIR="${IOS_DIR}/${APP_NAME}.app"
if [ ! -d "$APP_DIR" ]; then
  FOUND="$(find "target/dx/synstudy/${PROFILE}" -maxdepth 3 -name '*.app' -type d 2>/dev/null | grep -i ios | head -1 || true)"
  [ -n "$FOUND" ] && APP_DIR="$FOUND"
fi
if [ ! -d "$APP_DIR" ]; then
  echo "❌ 未找到真机 .app（目录: $IOS_DIR）。先不带 --no-build 跑一次构建。" >&2
  exit 1
fi
echo "==> 目标 .app: $APP_DIR"

# 3) 【签名前】注入词典 + 声明到 bundle 根。
echo "==> 注入词典库 ($(du -h "$DB_SRC" | cut -f1)) -> ${APP_DIR}/synstudy.db"
run cp -f "$DB_SRC" "${APP_DIR}/synstudy.db"
[ -f "$NOTICES_SRC" ] && run cp -f "$NOTICES_SRC" "${APP_DIR}/THIRD-PARTY-NOTICES.txt"

if [ "$DO_SIGN" -eq 0 ]; then
  echo "✅ 已按 --skip-sign 完成构建+注入，未签名/未安装: $APP_DIR"
  exit 0
fi

# 4) 解析签名身份（默认钥匙串里第一个 Apple Development）。
if [ -z "$IDENTITY" ]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | awk -F'"' '/Apple Development/ {print $2; exit}')"
fi
if [ -z "$IDENTITY" ]; then
  echo "❌ 未找到可用的 Apple Development 证书；用 --identity 指定，或在 Xcode 里配置开发者账号" >&2
  exit 1
fi
echo "==> 签名身份: $IDENTITY"

# 5) 解析 provisioning profile（真机必需，且必须覆盖该设备 + bundle id）。
if [ -z "$PROVISION" ]; then
  if [ -f "${APP_DIR}/embedded.mobileprovision" ]; then
    PROVISION="${APP_DIR}/embedded.mobileprovision"
    echo "==> 复用 .app 内已有的 embedded.mobileprovision"
  else
    echo "❌ 缺少描述文件：用 --profile <.mobileprovision> 指定（见脚本顶部说明），或先 --skip-sign" >&2
    exit 1
  fi
fi
[ -f "$PROVISION" ] || { echo "❌ 描述文件不存在: $PROVISION" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ENT_PLIST="${TMP}/entitlements.plist"

# 从 profile 解出 entitlements（沿用其 team / get-task-allow 等字段，避免签装不匹配）。
if [ "$DRY_RUN" -eq 1 ]; then
  echo "DRY-RUN> 从 $PROVISION 提取 entitlements 并 codesign --force --deep"
else
  security cms -D -i "$PROVISION" > "${TMP}/profile.plist"
  /usr/libexec/PlistBuddy -x -k Entitlements "${TMP}/profile.plist" > "$ENT_PLIST"
  cp -f "$PROVISION" "${APP_DIR}/embedded.mobileprovision"
  codesign --force --deep --sign "$IDENTITY" \
    --entitlements "$ENT_PLIST" \
    --timestamp=none \
    "$APP_DIR"
  echo "==> 校验签名: codesign --verify"
  codesign --verify --verbose=2 "$APP_DIR" && echo "✅ 签名有效"
fi

# 6) 解析目标设备 UDID（未指定则自动选第一台已配对的物理设备）。
UDID="$DEVICE"
if [ -z "$UDID" ]; then
  # devicectl JSON 的新 schema 把字段放在 properties.* 下（旧的 hardwareProperties/
  # connectionProperties 已标记 deprecated）；模拟器靠 reality=="simulated" 排除。
  UDID="$(xcrun devicectl list devices --json-output - 2>/dev/null | python3 -c '
import sys, json
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
for dev in data.get("result", {}).get("devices", []):
    props = dev.get("properties", {}) or {}
    hw = props.get("hardware", {}) or dev.get("hardwareProperties", {}) or {}
    conn = props.get("connection", {}) or dev.get("connectionProperties", {}) or {}
    udid = hw.get("udid") or ""
    platform = str(hw.get("platform", "")).lower()
    reality = str(hw.get("reality", "")).lower()
    state = str(conn.get("state", "")).lower()
    pairing = str(conn.get("pairingState", "")).lower()
    if not udid or platform != "ios":
        continue
    if reality == "simulated":          # 跳过 iOS 模拟器
        continue
    if state == "connected" or pairing == "paired":
        print(udid); break
' || true)"
fi
if [ -z "$UDID" ]; then
  echo "❌ 未检测到已配对的物理设备；插上设备、解锁并信任后用 --device <UDID> 指定" >&2
  echo "   （查看设备: xcrun devicectl list devices）" >&2
  exit 1
fi
echo "==> 目标设备: $UDID"

# 7) 安装并启动。
if [ "$DRY_RUN" -eq 1 ]; then
  echo "DRY-RUN> xcrun devicectl device install app --device $UDID $APP_DIR"
  echo "DRY-RUN> xcrun devicectl device process launch --device $UDID $BUNDLE_ID"
else
  xcrun devicectl device install app --device "$UDID" "$APP_DIR"
  xcrun devicectl device process launch --device "$UDID" "$BUNDLE_ID" || \
    echo "⚠️  自动启动失败，可在设备上手动打开 ${BUNDLE_ID}"
  echo "✅ 已安装并启动到真机 $UDID"
fi
