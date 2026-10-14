#!/usr/bin/env bash
# CI 产物校验：安装包必须 (1) 主程序架构与请求的 triple 一致，(2) 内嵌词典库，
# (3) release 安装包体积不低于下限。任何一条不满足就 **显式失败**。
#
# 为什么需要这个脚本
# ------------------
# dioxus-cli 0.8.0-alpha.1 的 copy_resources 对**缺失的 resource 只打印一条
# `Resource not found` 警告然后继续**（src/bundler/mod.rs:366-370），所以词典库
# assets/synstudy.db 没还原到位时，打包照样"成功"，产出几 MB 的空壳安装包（macOS 上
# 真实发生过：.app.zip 3.91MB / .dmg 3.87MB，正常应 >100MB）。更麻烦的是 Windows/Linux
# 还会额外从"暂存 asset_dir"扫到 db，唯独 macOS 只依赖 [bundle] resources 那条扁平拷贝，
# 于是同一份缺失只在 macOS 暴露。体积与文件头校验把这些静默退化变成硬失败。
#
# 用法
# ----
#   bash scripts/ci_verify_artifacts.sh <dx_root> <dist_dir> <期望架构> [build_type]
#     dx_root：搜索根，**必须传 target/dx/synstudy**（而不是它的 bundle/ 子目录）。
#       因为 Linux/Windows 的资源不在 bundle 目录里：bundle/<os> 只有最终归档
#       （.deb / .AppImage / *-setup.exe），db 会先被暂存到
#       target/dx/synstudy/<profile>/<os>/assets/ 再打进归档。
#     期望架构：x86_64 | arm64（universal 也算通过）
#     build_type：release（默认，做全部校验）| debug（只校验架构，debug 不保证内嵌 db）
#
# 示例（本地）：
#   bash scripts/ci_verify_artifacts.sh target/dx/synstudy dist arm64 release
set -euo pipefail

echo "==> ci_verify_artifacts.sh 已加载（host: $(uname -s)）"

DX_ROOT="${1:?用法: ci_verify_artifacts.sh <dx_root> <dist_dir> <期望架构> [build_type]}"
DIST_DIR="${2:?缺少 <dist_dir>}"
EXPECT_ARCH="${3:?缺少 <期望架构>（x86_64|arm64）}"
BUILD_TYPE="${4:-release}"

# release 安装包体积下限（MB）：词典库约 240MB，压缩后最合法的包也远大于此值。
MIN_MB=30

fail() { echo "❌ $*" >&2; exit 1; }
note() { echo "   $*"; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCH_PY="$ROOT/scripts/ci_binary_arch.py"
PY="$(command -v python3 || command -v python || true)"
[ -n "$PY" ] || fail "需要 python3 或 python 来解析可执行文件头"
[ -d "$DX_ROOT" ] || fail "dx 目录不存在：$DX_ROOT"
[ -d "$DIST_DIR" ] || fail "dist 目录不存在：$DIST_DIR"

# ── 1) 主程序架构 ──────────────────────────────────────────────────────────────
# 各平台裸可执行文件在 dx 树里的位置不同，按 host 类型定位（CI 的 runner 与
# 目标 OS 一致；macOS 的 x86_64 包是交叉编译产物，host 仍是 darwin）。
case "$(uname -s)" in
  Darwin)
    MAIN="$(find "$DX_ROOT" -type f -path '*/Contents/MacOS/*' | head -1 || true)"
    ;;
  Linux)
    MAIN="$(find "$DX_ROOT" -type f -name 'synstudy' | head -1 || true)"
    ;;
  *)
    # Windows（Git Bash：MINGW*/MSYS*）：只取裸 exe，安装包名带 -setup/ .msi 不会命中。
    MAIN="$(find "$DX_ROOT" -type f -name 'synstudy.exe' | head -1 || true)"
    ;;
esac

if [ -z "$MAIN" ] || [ ! -f "$MAIN" ]; then
  echo "❌ 在 $DX_ROOT 找不到主可执行文件，目录结构：" >&2
  find "$DX_ROOT" -maxdepth 4 >&2 || true
  exit 1
fi

ARCH="$("$PY" "$ARCH_PY" "$MAIN" | cut -f1)"
if [ "$ARCH" = "unknown" ]; then
  fail "无法识别主程序架构：$MAIN"
fi
if [ "$ARCH" != "$EXPECT_ARCH" ] && [ "$ARCH" != "universal" ]; then
  fail "主程序架构是 ${ARCH}，但本次请求的目标架构是 ${EXPECT_ARCH}（${MAIN}）"
fi
echo "✅ 架构匹配：$ARCH  <- $(basename "$MAIN")"

# ── 2) 词典库必须进包─────────────────────────────────────────────────────────
# 取证分三档，拿到任一正向证据即认为已内嵌；全部拿不到时也不直接失败，而是由
# 第 3 步的体积下限兜底（空壳包的特征就是几 MB，远低于下限）：
#   a) 磁盘上的 loose 文件：macOS 在 .app/Contents/Resources 下；Linux/Windows 则在
#      暂存目录 target/dx/synstudy/<profile>/<os>/assets/ 里（bundle/<os> 只有归档）。
#   b) 直接列包内清单：.deb 用 dpkg-deb -c（dx 的 deb 是自己用 ar 拼的，资源落在
#      ./usr/lib/<pkg>/…，见 bundler/linux.rs:129；dpkg 在 Ubuntu runner 上自带）。
#   c) AppImage 是 squashfs、NSIS 是自定义容器，CI 里不便枚举 → 只能降级到体积下限。
if [ "$BUILD_TYPE" = "release" ]; then
  DB_EVIDENCE=""

  DB="$(find "$DX_ROOT" -type f -name 'synstudy.db' | head -1 || true)"
  if [ -n "$DB" ] && [ -f "$DB" ]; then
    DB_BYTES="$(wc -c < "$DB" | tr -d ' ')"
    if [ "$DB_BYTES" -lt 50000000 ]; then
      fail "找到 synstudy.db 但只有 ${DB_BYTES} 字节（<50MB），疑似残缺：$DB"
    fi
    DB_EVIDENCE="loose 文件 ${DB#$DX_ROOT/}（$((DB_BYTES / 1024 / 1024))MB）"
  fi

  if [ -z "$DB_EVIDENCE" ] && command -v dpkg-deb >/dev/null 2>&1; then
    for deb in "$DIST_DIR"/*.deb; do
      [ -f "$deb" ] || continue
      if dpkg-deb -c "$deb" 2>/dev/null | grep -q 'synstudy\.db$'; then
        DB_EVIDENCE="包内清单 $(basename "$deb") 已含 synstudy.db"
        break
      fi
    done
  fi

  if [ -n "$DB_EVIDENCE" ]; then
    echo "✅ 词典库已内嵌：$DB_EVIDENCE"
  else
    echo "⚠️ 未能直接取证 synstudy.db（归档容器不便枚举），改由体积下限把关"
  fi

  if find "$DX_ROOT" -type f -name 'THIRD-PARTY-NOTICES.txt' | grep -q .; then
    echo "✅ 第三方声明已内嵌"
  else
    echo "⚠️ 未找到 THIRD-PARTY-NOTICES.txt（合规提示缺失，不阻断）"
  fi
fi

# ── 3) 安装包体积下限（防止把空壳发布出去）───────────────────────────────────
COUNT=0
for f in "$DIST_DIR"/*; do
  [ -f "$f" ] || continue
  COUNT=$((COUNT + 1))
  NAME="$(basename "$f")"
  if [ "$BUILD_TYPE" != "release" ]; then
    note "debug 产物，跳过体积下限检查：$NAME"
    continue
  fi
  MB=$(( $(wc -c < "$f" | tr -d ' ') / 1024 / 1024 ))
  if [ "$MB" -lt "$MIN_MB" ]; then
    fail "安装包 $NAME 只有 ${MB}MB（< ${MIN_MB}MB 下限）——多半没带词典库"
  fi
  echo "✅ $NAME ${MB}MB"
done
[ "$COUNT" -gt 0 ] || fail "$DIST_DIR 里没有任何安装包"

echo "==> 校验通过：$COUNT 个产物，架构 $ARCH"
