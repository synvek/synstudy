#!/usr/bin/env bash
# Build the Android APK and inject the read-only dictionary database into it.
#
# 为什么需要这个脚本
# ------------------
# dioxus-cli 0.8 的 `dx build --platform android` 会重新生成整个 Gradle 工程，
# 但不会把 [bundle] resources（assets/synstudy.db，约 235MB）拷进 APK 的 assets。
# 运行时 App 通过 AAssetManager 打开 `assets/synstudy.db` 再流式拷贝到 filesDir
# （见 src/platform/android.rs 的 bootstrap_dict_from_assets），所以必须：
#   1) 把 db 放进 app 模块的 src/main/assets/；
#   2) 在 build.gradle.kts 里对 .db 关闭压缩（noCompress），否则 AAsset 长度/
#      流式读取会因 deflate 而低效甚至出错。
# 这两步在 target/ 下，`dx build` 每次都会覆盖，故做成构建后置注入。
#
# 用法
# ----
#   bash scripts/package_android.sh              # debug 构建 + 注入 + gradlew 出 APK
#   bash scripts/package_android.sh --release    # release 构建（需自行配置签名）
#   bash scripts/package_android.sh --install    # 出包后 adb install -r 到已连接设备
#   bash scripts/package_android.sh --no-build   # 跳过 dx build，仅注入 + gradlew 打包
#
# 注意
# ----
# * debug APK 用 Android 默认 debug keystore 自动签名，可直接安装。
# * release APK 需要在 build.gradle.kts 配置 signingConfig（keystore），本脚本不
#   代为签名；未配置时 assembleRelease 产出的是未签名 APK，无法直接安装。
# * 依赖：dx（dioxus-cli 0.8.x）、Android SDK（ANDROID_HOME 或 ~/Library/Android/sdk）、
#   JDK。--install 需要 platform-tools/adb。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PACKAGE="com.synvek.synstudy"
DB_SRC="assets/synstudy.db"
NOTICES_SRC="assets/THIRD-PARTY-NOTICES.txt"

PROFILE="debug"        # debug | release
GRADLE_TASK="assembleDebug"
DO_BUILD=1
DO_INSTALL=0

usage() {
  cat <<'EOF'
用法: bash scripts/package_android.sh [选项]
  --release    构建 release APK（需自行配置签名；默认 debug）
  --debug      构建 debug APK（默认）
  --install    打包后用 adb install -r 安装到已连接设备
  --no-build   跳过 dx build，仅向已生成的 Gradle 工程注入并 gradlew 打包
  -h, --help   显示本帮助
EOF
}

for arg in "$@"; do
  case "$arg" in
    --release)  PROFILE="release"; GRADLE_TASK="assembleRelease" ;;
    --debug)    PROFILE="debug";   GRADLE_TASK="assembleDebug" ;;
    --install)  DO_INSTALL=1 ;;
    --no-build) DO_BUILD=0 ;;
    -h|--help)  usage; exit 0 ;;
    *) echo "未知参数: $arg" >&2; usage; exit 2 ;;
  esac
done

ANDROID_DIR="target/dx/synstudy/${PROFILE}/android"
GRADLE_ROOT="${ANDROID_DIR}/app"                 # 含 gradlew 的工程根
MODULE_DIR="${GRADLE_ROOT}/app"                  # app 模块（含 build.gradle.kts）
ASSETS_DIR="${MODULE_DIR}/src/main/assets"
GRADLE_FILE="${MODULE_DIR}/build.gradle.kts"

# 词典源文件必须存在。
if [ ! -f "$DB_SRC" ]; then
  echo "❌ 缺少词典库 $DB_SRC；先运行: python3 scripts/build_dict_db.py" >&2
  exit 1
fi

# 1) 生成/刷新 Gradle 工程（不带 --no-build 时）。
if [ "$DO_BUILD" -eq 1 ]; then
  DX_ARGS=(build --platform android)
  [ "$PROFILE" = "release" ] && DX_ARGS+=(--release)
  echo "==> dx ${DX_ARGS[*]}"
  dx "${DX_ARGS[@]}"
fi

# 2) 定位 Gradle 工程与 app 模块。
if [ ! -x "${GRADLE_ROOT}/gradlew" ] && [ ! -f "${GRADLE_ROOT}/gradlew" ]; then
  echo "❌ 未找到 Gradle 工程（${GRADLE_ROOT}）。先不带 --no-build 跑一次构建。" >&2
  exit 1
fi

# 3) 注入词典库 + 声明文件到 assets/。
mkdir -p "$ASSETS_DIR"
DB_SIZE="$(du -h "$DB_SRC" | cut -f1)"
echo "==> 注入词典库 (${DB_SIZE}) -> ${ASSETS_DIR}/synstudy.db"
cp -f "$DB_SRC" "${ASSETS_DIR}/synstudy.db"
[ -f "$NOTICES_SRC" ] && cp -f "$NOTICES_SRC" "${ASSETS_DIR}/THIRD-PARTY-NOTICES.txt"

# 4) 幂等给 build.gradle.kts 打 noCompress 补丁（dx 每次重建都会覆盖该文件）。
if ! grep -q 'noCompress' "$GRADLE_FILE"; then
  echo "==> 为 build.gradle.kts 追加 androidResources.noCompress += \"db\""
  awk '
    /^android \{/ && !done {
      print
      print "    // Auto-added by scripts/package_android.sh: keep the dictionary DB"
      print "    // uncompressed so AAssetManager can stream it by length on first launch."
      print "    androidResources {"
      print "        noCompress += \"db\""
      print "    }"
      done = 1
      next
    }
    { print }
  ' "$GRADLE_FILE" > "${GRADLE_FILE}.tmp" && mv "${GRADLE_FILE}.tmp" "$GRADLE_FILE"
else
  echo "==> build.gradle.kts 已含 noCompress 配置，跳过补丁"
fi

# 4b) 替换启动器图标（dx alpha.1 不消费 [bundle] icon，工程里是默认 Dioxus 图）：
# 用 gen_icons.py 生成的各密度 mipmap PNG 覆盖默认 webp，并删除 adaptive-icon
# xml（其引用的默认前景/背景 drawable 一并失效），让桌面回退到我们的 legacy 图标。
RES_DIR="${MODULE_DIR}/src/main/res"
ICON_SRC_DIR="assets/icons/android"
if [ -d "$ICON_SRC_DIR" ]; then
  echo "==> 覆盖 mipmap 启动器图标 <- ${ICON_SRC_DIR}"
  for d in "$ICON_SRC_DIR"/mipmap-*; do
    dpi="$(basename "$d")"
    mkdir -p "${RES_DIR}/${dpi}"
    rm -f "${RES_DIR}/${dpi}"/ic_launcher.webp "${RES_DIR}/${dpi}"/ic_launcher.png
    cp -f "$d/ic_launcher.png" "${RES_DIR}/${dpi}/ic_launcher.png"
  done
  rm -f "${RES_DIR}/mipmap-anydpi-v26/ic_launcher.xml"
else
  echo "⚠️  缺少 ${ICON_SRC_DIR}；先跑 python3 scripts/gen_icons.py（本次沿用默认图标）" >&2
fi

# 5) 用 Gradle 产出 APK（这一步才会把 assets 打进包里）。
echo "==> ./gradlew $GRADLE_TASK"
( cd "$GRADLE_ROOT" && ./gradlew "$GRADLE_TASK" )

# 6) 定位 APK 产物。
APK="${MODULE_DIR}/build/outputs/apk/${PROFILE}/app-${PROFILE}.apk"
if [ ! -f "$APK" ]; then
  FOUND="$(find "${MODULE_DIR}/build/outputs/apk" -name '*.apk' -type f 2>/dev/null | head -1 || true)"
  [ -n "$FOUND" ] && APK="$FOUND"
fi
if [ ! -f "$APK" ]; then
  echo "❌ 未找到 APK 产物（${MODULE_DIR}/build/outputs/apk/${PROFILE}/）" >&2
  exit 1
fi
echo "✅ APK 已产出: $APK ($(du -h "$APK" | cut -f1))"

# 7) 可选：安装到已连接设备。
if [ "$DO_INSTALL" -eq 1 ]; then
  ADB="$(command -v adb || true)"
  [ -z "$ADB" ] && ADB="${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb"
  if [ ! -x "$ADB" ]; then
    echo "❌ 未找到 adb（设置 ANDROID_HOME 或把 platform-tools 加入 PATH）" >&2
    exit 1
  fi
  if [ -z "$("$ADB" devices | awk 'NR>1 && $2=="device"')" ]; then
    echo "❌ 没有已授权连接的设备；插上手机并允许调试弹窗后重试" >&2
    exit 1
  fi
  echo "==> adb install -r $APK"
  "$ADB" install -r "$APK"
  echo "✅ 已安装 ${PACKAGE}（启动: $ADB shell monkey -p ${PACKAGE} 1）"
fi
