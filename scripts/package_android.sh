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
  echo "❌ 缺少词典库 ${DB_SRC}；先运行: python3 scripts/build_dict_db.py" >&2
  exit 1
fi

# 上一次运行由我们注入的图标文件（路径相对 res 目录），用于构建前清扫。
strip_injected_icons() {
  local res="${MODULE_DIR}/src/main/res"
  [ -d "$res" ] || return 0
  rm -f "$res"/mipmap-*/ic_launcher.png \
        "$res"/mipmap-*/ic_launcher_background.png \
        "$res"/mipmap-*/ic_launcher_foreground.png \
        "$res"/mipmap-anydpi-v26/ic_launcher.xml
  return 0
}

# 1) 生成/刷新 Gradle 工程（不带 --no-build 时）。
if [ "$DO_BUILD" -eq 1 ]; then
  # 必须先清掉我们上次注入的图标：dx 会用模板重新写回 ic_launcher.webp，却不会删
  # 我们留下的 ic_launcher.png，同名不同扩展落在同一个 mipmap 目录里是 AGP 的
  # 「Duplicate resources」硬错误。而 dx build 自己就会跑一次 gradle assemble，
  # 于是脚本还没走到注入环节就挂了（表现：Bundling app... Failed to assemble apk）。
  # 只在第二次及以后的全量构建触发，所以首次能跑通不代表安全。
  echo "==> 清理上次注入的图标文件（避开 dx 模板 webp 的 Duplicate resources）"
  strip_injected_icons
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

# 4b) 替换启动器图标（dx alpha.1 不消费 [bundle] icon，工程里是默认 Dioxus 图）。
# 两套图层都要覆盖，因为启动器按 API level 读不同资源：
#   * legacy mipmap（API <26，也是 min_sdk=24 的兜底）：现在图源是 app-icon-ios.png
#     那张满幅不透明的 master。之前用透明 logo，启动器会在透明区补一块**白色
#     底板**、再把大片留白的位图额外内缩一次——正是「白底 + 图案偏小」的成因。
#   * adaptive 图层（API 26+）：108dp 画布的 background + foreground，由下面写出的
#     mipmap-anydpi-v26/ic_launcher.xml 引用，启动器按圆形/胶囊/方形遮罩裁它。
#     dx 生成的工程里只有 AS 模板的两个 vector drawable、**没有这份 xml**，所以
#     系统一直退回 legacy 分支（旧脚本还会主动删掉它）。
RES_DIR="${MODULE_DIR}/src/main/res"
ICON_SRC_DIR="assets/icons/android"
if [ -d "$ICON_SRC_DIR" ]; then
  echo "==> 覆盖启动器图标（legacy mipmap + adaptive 图层）<- ${ICON_SRC_DIR}"
  ADAPTIVE_OK=1
  for d in "$ICON_SRC_DIR"/mipmap-*; do
    dpi="$(basename "$d")"
    mkdir -p "${RES_DIR}/${dpi}"
    for f in ic_launcher.png ic_launcher_background.png ic_launcher_foreground.png; do
      [ -f "$d/$f" ] || continue
      # 默认图是 webp（ic_launcher.webp），连同旧的 png 一起清掉再放我们的。
      # 删 webp 是必须的：与同名 png 共存就是上面那个 Duplicate resources。
      rm -f "${RES_DIR}/${dpi}/${f%.png}.webp" "${RES_DIR}/${dpi}/${f}"
      cp -f "$d/$f" "${RES_DIR}/${dpi}/${f}"
    done
    for f in ic_launcher_background.png ic_launcher_foreground.png; do
      [ -f "$d/$f" ] || ADAPTIVE_OK=0
    done
  done
  if [ "$ADAPTIVE_OK" -eq 1 ]; then
    mkdir -p "${RES_DIR}/mipmap-anydpi-v26"
    cat > "${RES_DIR}/mipmap-anydpi-v26/ic_launcher.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@mipmap/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
XML
    echo "==> 已写出 mipmap-anydpi-v26/ic_launcher.xml（adaptive 图标）"
  else
    # 只有旧版 gen_icons.py 产出物的情况：不写 xml，免得 aapt 找不到被引用的图层。
    echo "⚠️  缺少 adaptive 图层；请重跑 python3 scripts/gen_icons.py（本次仅 legacy）" >&2
    rm -f "${RES_DIR}/mipmap-anydpi-v26/ic_launcher.xml"
  fi
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
