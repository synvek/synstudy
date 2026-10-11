//! Desktop / non-mobile backends: macOS, Linux, Windows.
//!
//! Behavior matches the pre-mobile code exactly: `directories` for user data,
//! a search through `current_exe()` and CWD candidates for the bundled DB.

use std::path::{Path, PathBuf};

pub fn desktop_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "synstudy", "synstudy")
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
