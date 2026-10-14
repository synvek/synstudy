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
[ -d "$DX_ROOT" ] || fail "dx 目录不存在：$DX_ROOT"
[ -d "$DIST_DIR" ] || fail "dist 目录不存在：$DIST_DIR"

# ── 0) 架构解析：默认用纯 coreutils（od），能用 python 时拿它做交叉校对 ─────────
# 为什么不再依赖 python：Windows runner（尤其 ARM64 镜像）上的 python3 常常是
# Microsoft Store 占位 stub——它把提示打到 **stdout** 并以非 0 退出。旧写法
#   ARCH="$("$PY" ... | cut -f1)"
# 在 `set -eo pipefail` 下有两个叠加效果：pipefail 让赋值拿到非 0 状态 → errexit
# 直接中止（这一行不是 `&&` 列表里的非末位命令，不被豁免），而 `| cut` 又把 stdout
# 吞进变量——于是脚本一行错误都不输出，表现为“banner 之后静默 exit 1”。
bytes_at() { od -An -v -tx1 -j "$2" -N "$3" "$1" | tr -d ' \n'; }   # 十六进制串
u_le() { local h="$1" i o=; for ((i = ${#h} - 2; i >= 0; i -= 2)); do o+="${h:i:2}"; done; echo "$((16#$o))"; }
u_be() { echo "$((16#$1))"; }
macho_cpu_name() {
  case $(( $1 & 0x00ffffff )) in
    7) echo x86_64 ;;
    12) echo arm64 ;;
    *) echo "" ;;
  esac
}

arch_od() {
  local f="$1" m cpu n i a seen="" elf endian val
  m="$(bytes_at "$f" 0 4)" || return 1
  case "$m" in
    cffaedfe|feedfacf|cffaedce|feedface)          # Mach-O thin（只处理小端，CI 目标均为小端）
      cpu="$(u_le "$(bytes_at "$f" 4 4)")"
      macho_cpu_name "$cpu"
      return
      ;;
    cafebabe)                                      # Mach-O fat：fat_header = magic(4) + nfat_arch(4)，
                                                   # 每项 fat_arch 20B，cputype 在条目起始（偏移 8）
      n="$(u_be "$(bytes_at "$f" 4 4)")"
      # 真实 fat 文件只会有 1~4 个架构；遇到离谱的 n 说明不是 fat 或文件已损坏，
      # 上限在此直接判定失败，避免循环刷出成百上千条 od 错误。
      if [ "$n" -lt 1 ] || [ "$n" -gt 8 ]; then return 1; fi
      for ((i = 0; i < n; i++)); do
        a="$(macho_cpu_name "$(u_be "$(bytes_at "$f" $((8 + 20 * i)) 4)")")"
        [ -n "$a" ] && seen+="$a "
      done
      case "$seen" in
        *"x86_64 arm64"*|*"arm64 x86_64"*) echo universal; return ;;
      esac
      echo "${seen%% *}"                            # 只含一种则当作单架构
      [ -n "$seen" ] || return 1
      return
      ;;
    7f454c46)                                      # ELF：e_ident[EI_DATA] 定端序，e_machine 在 18
      elf="$(bytes_at "$f" 18 2)"
      endian="$(bytes_at "$f" 5 1)"
      [ "$endian" = "01" ] && val="$(u_le "$elf")" || val="$(u_be "$elf")"
      case "$val" in
        62) echo x86_64 ;;
        183) echo arm64 ;;
        *) echo "" ;;
      esac
      return
      ;;
  esac
  if [ "${m:0:4}" = "4d5a" ]; then                 # PE：e_lfanew 在 0x3C，其后 'PE\0\0' + Machine
    local off="$(u_le "$(bytes_at "$f" 60 4)")"
    if [ "$(bytes_at "$f" "$off" 4)" = "50450000" ]; then
      case "$(u_le "$(bytes_at "$f" $((off + 4)) 2)")" in
        34404) echo x86_64; return ;;               # 0x8664
        43620) echo arm64; return ;;                # 0xAA64
      esac
    fi
  fi
  return 1
}

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

# 解析失败必须**有输出**：od 不可用或文件头不认识时，把前 8 字节打出来方便定位。
if ! ARCH="$(arch_od "$MAIN")" || [ -z "$ARCH" ]; then
  echo "❌ 无法识别主程序架构：$MAIN" >&2
  echo "   前 8 字节：$(bytes_at "$MAIN" 0 8)" >&2
  exit 1
fi

# 有可用的真 python（能跑 `-c pass`）时再用 ci_binary_arch.py 交叉校对一下；
# 没 python（Windows stub / 精简镜像）直接跳过，不阻断校验。
# Git Bash 下 ROOT/pwd 给出的是 MSYS 路径（/d/a/…），原生 python.exe 打不开，需 cygpath 转换。
ARCH_PY_RUN="$ARCH_PY"
if command -v cygpath >/dev/null 2>&1; then
  ARCH_PY_RUN="$(cygpath -m "$ARCH_PY" 2>/dev/null || echo "$ARCH_PY")"
fi
PY="$(command -v python3 || command -v python || true)"
if [ -n "$PY" ] && "$PY" -c 'pass' >/dev/null 2>&1; then
  # 末尾的 `|| true` 是必须的：否则赋值同样会在 pipefail 下静默触发 errexit。
  PY_ARCH="$("$PY" "$ARCH_PY_RUN" "$MAIN" 2>/dev/null | cut -f1 || true)"
  if [ -n "$PY_ARCH" ] && [ "$PY_ARCH" != "$ARCH" ]; then
    fail "两种解析器结论不一致：od=${ARCH} python=${PY_ARCH}（${MAIN}）"
  fi
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
