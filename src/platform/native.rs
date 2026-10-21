//! Desktop / non-mobile backends: macOS, Linux, Windows.
//!
//! Behavior matches the pre-mobile code exactly: `directories` for user data,
//! a search through `current_exe()` and CWD candidates for the bundled DB.

use std::path::{Path, PathBuf};

/// Reverse-DNS bundle id — the contract this module's directory naming is
/// derived from. Always lowercase: it must equal `[bundle] identifier` in
/// Dioxus.toml, i.e. `CFBundleIdentifier` on macOS/iOS and `applicationId` on
/// Android, none of which may be re-cased. Pinned by the test at the bottom.
#[allow(dead_code)] // documentation + test contract; the segments below carry the same value
const BUNDLE_ID: &str = "com.synvek.synstudy";

/// `directories::ProjectDirs` segments feeding [`desktop_data_dir`].
///
/// macOS composes the folder name from all three segments joined with `.`
/// (→ `com.synvek.synstudy`), so there they must carry the bundle id verbatim,
/// lowercase. Windows ignores the qualifier and nests
/// `organization\application` — a vendor folder whose name is what users actually
/// see in `%APPDATA%`, so the proper nouns are capitalized (`Synvek\Synstudy`).
/// Other products of the company become sibling subfolders; nothing is shared.
/// Linux is neither: `lin.rs` keeps only the application segment and
/// force-lowercases it, so it stays `~/.local/share/synstudy` (which also
/// matches the XDG lowercase convention) regardless of what we pass here.
const PROJECT_QUALIFIER: &str = "com";
#[cfg(windows)]
const PROJECT_ORG: &str = "Synvek";
#[cfg(windows)]
const PROJECT_APP: &str = "Synstudy";
#[cfg(not(windows))]
const PROJECT_ORG: &str = "synvek";
#[cfg(not(windows))]
const PROJECT_APP: &str = "synstudy";

/// Writable app-data directory. Composed per OS by `directories` 5 (see its
/// `mac.rs` / `lin.rs` / `win.rs`), so the three segments above do not read back
/// as the final path everywhere:
///
/// - macOS:   `~/Library/Application Support/com.synvek.synstudy`
/// - Linux:   `~/.local/share/synstudy`
/// - Windows: `%APPDATA%\Synvek\Synstudy\data`
///
/// Renaming any of these folders moves all user state (progress.db, downloaded
/// models), so the names must stay put once real users exist.
pub fn desktop_data_dir() -> PathBuf {
    directories::ProjectDirs::from(PROJECT_QUALIFIER, PROJECT_ORG, PROJECT_APP)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Directory that ships alongside the executable (used for `.app` bundles and
/// for `cargo run` where the assets folder sits next to `target/...`).
///
/// Returns the first directory that actually exists; `None` if nothing fits.
#[allow(dead_code)] // reserved for future asset loaders; `resource_dir` also currently unused
pub fn desktop_resource_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Inside a macOS .app: .../synstudy.app/Contents/MacOS/synstudy
            // Bundle Resources: .../synstudy.app/Contents/Resources/
            if dir.ends_with("MacOS") {
                if let Some(contents) = dir.parent() {
                    candidates.push(contents.join("Resources"));
                }
            }
            candidates.push(dir.join("assets"));
            candidates.push(dir.to_path_buf());
        }
    }
    candidates.push(PathBuf::from("assets"));
    candidates.push(PathBuf::from("."));
    candidates.into_iter().find(|p| p.is_dir())
}

pub fn desktop_dict_db() -> Option<PathBuf> {
    // 1. Sibling of the executable (dev / `cargo run` layout).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for cand in [dir.join("synstudy.db"), dir.join("assets").join("synstudy.db")] {
                if cand.exists() {
                    return Some(cand);
                }
            }
            // macOS .app bundle: Contents/Resources/synstudy.db
            if dir.ends_with("MacOS") {
                if let Some(contents) = dir.parent() {
                    let cand = contents.join("Resources").join("synstudy.db");
                    if cand.exists() {
                        return Some(cand);
                    }
                }
            }
        }
    }
    // 2. Working directory (dev fallback for `cargo run` at repo root).
    for rel in [Path::new("assets/synstudy.db"), Path::new("synstudy.db")] {
        if rel.exists() {
            return Some(rel.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{desktop_data_dir, BUNDLE_ID, PROJECT_APP, PROJECT_ORG, PROJECT_QUALIFIER};

    /// `[bundle] identifier` from Dioxus.toml (first `identifier` line in that table).
    fn toml_identifier() -> String {
        let mut in_bundle = false;
        for line in include_str!("../../Dioxus.toml").lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_bundle = line == "[bundle]";
                continue;
            }
            if in_bundle {
                if let Some(rest) = line.strip_prefix("identifier") {
                    if let Some(value) = rest.split('=').nth(1) {
                        return value.trim().trim_matches('"').to_string();
                    }
                }
            }
        }
        panic!("Dioxus.toml 的 [bundle] 里没有 identifier");
    }

    /// 数据目录名必须跟着 bundle id 走。历史上两者分叉过（identifier 改成
    /// com.synvek.synstudy，代码里的 ProjectDirs 三段仍是 com/synstudy/synstudy，
    /// 导致 macOS 数据长期落在 ~/Library/Application Support/com.synstudy.synstudy）。
    #[test]
    fn data_dir_name_matches_bundle_identifier() {
        let joined = format!("{PROJECT_QUALIFIER}.{PROJECT_ORG}.{PROJECT_APP}");
        // Windows 用大写专名（Synvek\\Synstudy），所以忽略大小写后比对。
        assert_eq!(
            joined.to_lowercase(),
            BUNDLE_ID,
            "三段拼出来必须就是 bundle id"
        );
        assert_eq!(
            BUNDLE_ID,
            toml_identifier(),
            "BUNDLE_ID 常量与 Dioxus.toml 的 [bundle] identifier 不一致"
        );

        // `directories` 各平台拼法不同，逐平台钉住最终路径（见 mac.rs/lin.rs/win.rs）。
        let dir = desktop_data_dir();
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        #[cfg(target_os = "macos")]
        assert_eq!(
            name, BUNDLE_ID,
            "macOS：三段用 . 拼接，必须小写等于 bundle id"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            name, PROJECT_APP,
            "Linux：只取 application 段（已被 crate 强制小写）"
        );
        #[cfg(windows)]
        {
            assert_eq!(name, "data", "Windows：data_dir 末段固定是 data");
            // 不用逐层取 parent：把分隔符归一后直接断言后缀，可读且不会写错。
            let flat = dir.to_string_lossy().replace('\\', "/");
            assert!(
                flat.ends_with("/Synvek/Synstudy/data"),
                "Windows 应为 %APPDATA%\\Synvek\\Synstudy\\data，实际 {flat}"
            );
        }
    }
}
