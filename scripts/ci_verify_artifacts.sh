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
#   bash scripts/ci_verify_artifacts.sh <bundle_dir> <dist_dir> <期望架构> [build_type]
#     期望架构：x86_64 | arm64（universal 也算通过）
#     build_type：release（默认，做全部校验）| debug（只校验架构，debug 不保证内嵌 db）
#
# 示例（本地）：
#   bash scripts/ci_verify_artifacts.sh target/dx/synstudy/bundle dist arm64 release
set -euo pipefail

BUNDLE_DIR="${1:?用法: ci_verify_artifacts.sh <bundle_dir> <dist_dir> <期望架构> [build_type]}"
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
[ -d "$BUNDLE_DIR" ] || fail "bundle 目录不存在：$BUNDLE_DIR"
[ -d "$DIST_DIR" ] || fail "dist 目录不存在：$DIST_DIR"

# ── 1) 主程序架构 ──────────────────────────────────────────────────────────────
# 各平台裸可执行文件在 bundle 树里的位置不同，按 host 类型定位（CI 的 runner 与
# 目标 OS 一致；macOS 的 x86_64 包是交叉编译产物，host 仍是 darwin）。
case "$(uname -s)" in
  Darwin)
    MAIN="$(find "$BUNDLE_DIR" -type f -path '*/Contents/MacOS/*' | head -1 || true)"
    ;;
  Linux)
    MAIN="$(find "$BUNDLE_DIR" -type f -name 'synstudy' | head -1 || true)"
    ;;
  *)
    # Windows（Git Bash：MINGW*/MSYS*）：只取裸 exe，安装包名带 -setup/ .msi 不会命中。
    MAIN="$(find "$BUNDLE_DIR" -type f -name 'synstudy.exe' | head -1 || true)"
    ;;
esac

if [ -z "$MAIN" ] || [ ! -f "$MAIN" ]; then
  echo "❌ 在 $BUNDLE_DIR 找不到主可执行文件，目录结构：" >&2
  find "$BUNDLE_DIR" -maxdepth 4 >&2 || true
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

# ── 2) 词典库与第三方声明必须打进产物 ─────────────────────────────────────────
if [ "$BUILD_TYPE" = "release" ]; then
  DB="$(find "$BUNDLE_DIR" -type f -name 'synstudy.db' | head -1 || true)"
  if [ -z "$DB" ]; then
    echo "❌ bundle 里没有 synstudy.db —— dx 对缺失 resource 只 warn 不报错，" >&2
    echo "   这样产出的安装包是空壳（打开即无词库）。检查 [bundle] resources 与" >&2
    echo "   assets/synstudy.db 是否在打包前就位。" >&2
    find "$BUNDLE_DIR" -maxdepth 4 -type f | head -40 >&2 || true
    exit 1
  fi
  DB_BYTES="$(wc -c < "$DB" | tr -d ' ')"
  if [ "$DB_BYTES" -lt 50000000 ]; then
    fail "bundle 里的 synstudy.db 只有 ${DB_BYTES} 字节（<50MB），疑似残缺：$DB"
  fi
  echo "✅ 词典库已内嵌：$(basename "$DB") $((DB_BYTES / 1024 / 1024))MB"
  note "路径：${DB#*bundle/}"

  if find "$BUNDLE_DIR" -type f -name 'THIRD-PARTY-NOTICES.txt' | grep -q .; then
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
