//! 为 iOS 的 `translate` 构建补链 compiler-rt。
//!
//! 背景：`translate` 会把 llama.cpp 的 Metal 后端（`ggml-metal-device.m`）编进来，
//! 源码里有 `@available(macOS 15.0, iOS 18.0, *)` 这类高于 deployment target
//! （Dioxus.toml 的 15.0）的判断，clang 为此生成对 `___isPlatformVersionAtLeast`
//! 的运行时调用。该符号定义在 Xcode 的 `libclang_rt.{ios,iossim}.a` 里，平时由
//! clang 作为驱动自动链接；但 rustc 链接可执行文件时传了 `-nodefaultlibs`，
//! compiler-rt 不会被带上 → 真机链接报
//! `Undefined symbols for architecture arm64: "___isPlatformVersionAtLeast"`。
//! （模拟器包此前能链上，只是因为 Metal 的目标文件没被引用、没拉进来。）
//!
//! 这里只做一件事：找到当前 clang 的 compiler-rt 归档，切成单架构后交给链接器。
//! 归档里只有被引用的成员会被取出，对其它代码无影响。
//!
//! 为什么要切：Xcode 的 `libclang_rt.ios.a` 是 **fat（universal）归档**（magic
//! `cafebabe`，含 armv7/armv7s/arm64/arm64e）。本 crate 的 crate-type 带
//! staticlib/cdylib，rustc 会把 native 归档重新打包 → 需要读归档，而它的归档
//! 读取器只认单架构的 `!<arch>`，直接链接原文件会报
//! `failed to add native library ...: Unsupported archive identifier`。
//! 所以先用 `lipo -thin` 把目标架构切出来放进 OUT_DIR，再让 `-L` 指向 OUT_DIR。

use std::{path::PathBuf, process::Command};

fn main() {
    println!("cargo::rerun-if-env-changed=CC");
    println!("cargo::rerun-if-changed=build.rs");

    let target = std::env::var("TARGET").unwrap_or_default();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") {
        return;
    }
    // 只有 translate（llama.cpp / Metal）会引用这个符号，其它构建不动链接参数。
    if std::env::var_os("CARGO_FEATURE_TRANSLATE").is_none() {
        return;
    }

    // 模拟器与真机的 compiler-rt 归档名不同；rustc 的模拟器 triple 以 `-sim` 结尾。
    let rt_lib = if target.ends_with("-sim") || target.contains("-sim-") {
        "clang_rt.iossim"
    } else {
        "clang_rt.ios"
    };
    // rustc 的 arch 名与 lipo 的 arch 名对应关系。
    let lipo_arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => return,
    };

    // 用实际参与构建的 clang 问它自己的 resource dir（优先 $CC，回退 PATH 上的 clang）。
    let cc = std::env::var("CC").unwrap_or_else(|_| "clang".to_string());
    let file_name = format!("lib{rt_lib}.a");
    let probe = Command::new(&cc)
        .arg(format!("-print-file-name={file_name}"))
        .output();
    let printed = match probe {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    };
    let src = PathBuf::from(&printed);
    if printed.is_empty() || !src.is_absolute() || !src.exists() {
        // 找不到也不能让构建失败：真机链接期给出明确提示即可。
        println!(
            "cargo::warning=未能在 `{cc}` 中找到 {file_name}，若链接报 \
             ___isPlatformVersionAtLeast 未定义请检查 Xcode command line tools"
        );
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let thin = out_dir.join(&file_name);
    if !thin.exists() {
        let status = Command::new("lipo")
            .args(["-thin", lipo_arch])
            .arg(&src)
            .arg("-output")
            .arg(&thin)
            .status();
        match status {
            Ok(s) if s.success() => {}
            other => {
                let _ = std::fs::remove_file(&thin);
                println!(
                    "cargo::warning=lipo -thin {lipo_arch} 切出 {file_name} 失败（{other:?}），\
                     链接可能报 ___isPlatformVersionAtLeast 未定义"
                );
                return;
            }
        }
    }
    // OUT_DIR 里已是单架构归档，rustc 能正常读取与打包。
    println!("cargo::rustc-link-search=native={}", out_dir.display());
    println!("cargo::rustc-link-lib=static={rt_lib}");
}
