#!/usr/bin/env bash
# 以「桌面本地构建」的方式编译 / 运行带截图翻译功能的 synstudy。
#
# 为什么需要这个脚本
# ------------------
# 开启 `translate` feature 会拉起一批需要**本机 C/C++ 工具链**的依赖：
# llama.cpp(ggml)、MNN、bindgen(cbindgen) 等。这些依赖的 build.rs 默认调用
# PATH 上的第一个 clang，而这台机器的 PATH 把 **Android NDK 的 clang** 排在
# /usr/bin/clang 之前：
#
#   .../ndk/.../bin  ->  NDK clang（面向 Android，不会自动定位 macOS SDK 头文件）
#
# 于是编译 C 头（stdio.h）或 C++ 头（<functional>，MNN Interpreter.hpp 引入）时报
#   fatal error: 'stdio.h' / 'functional' file not found
# 这不是代码问题，是编译器选错 + 缺 `-isysroot`。
#
# 注意：项目根 `.cargo/config.toml` 的 [env] 用 `force = true` 把 SDKROOT /
# C_INCLUDE_PATH 等清空（为 Android 交叉编译消除 macOS SDK 泄漏），因此**仅靠
# export SDKROOT 无效**，必须在命令行显式传 `-isysroot`。
#
# 本脚本把这些一次性配好：强制使用 Apple clang + 显式 -isysroot，bindgen 走
# Xcode 自带的 libclang，git 依赖用系统 git 拉取。
#
# 用法
# ----
#   bash scripts/check_translate.sh            # cargo check --features translate
#   bash scripts/check_translate.sh build      # cargo build   --features translate
#   bash scripts/check_translate.sh run        # cargo run     --features translate
#   bash scripts/check_translate.sh clippy     # 附带 translate 的额外参数交给 cargo
#
# 网络提示：本脚本走在线构建（非 --offline）。rust-paddle-ocr 的 build.rs(cbindgen)
# 会额外调用 `cargo metadata` **非离线**地重新解析 crates.io；国内直连易卡死。若卡在
# 网络，请先在**同一个终端**导出一个覆盖 crates.io 的通用代理再运行本脚本：
#   export http_proxy=http://<your-proxy>:<port>
#   export https_proxy=http://<your-proxy>:<port>
# （仅加速 GitHub 的域名级代理不够，必须能访问 index.crates.io / static.crates.io。）
# 依赖已全部本地缓存后，日常可用 `bash scripts/check_translate.sh check --offline`。
set -euo pipefail

SDK="$(xcrun --show-sdk-path)"

export SDKROOT="$SDK"
export CC="/usr/bin/clang"
export CXX="/usr/bin/clang++"
export CFLAGS="-isysroot $SDK"
export CXXFLAGS="-isysroot $SDK"
# 只放语言中立的 -isysroot；切勿在此加 -std=c++17，否则会被套用到 C 头文件
# （llama 的 wrapper.h）导致 bindgen 报 "libclang error"。
export BINDGEN_EXTRA_CLANG_ARGS="-isysroot $SDK"
export LIBCLANG_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib"
export CARGO_NET_GIT_FETCH_WITH_CLI=true

mode="${1:-check}"
shift || true

case "$mode" in
  check)  cargo check  --features translate "$@" ;;
  build)  cargo build  --features translate "$@" ;;
  run)    cargo run    --features translate "$@" ;;
  clippy) cargo clippy --features translate "$@" ;;
  *)      exec cargo "$@" ;;
esac
