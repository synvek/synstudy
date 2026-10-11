//! iOS backend.
//!
//! Uses `objc2` + `objc2-foundation` to reach `NSHomeDirectory()` for the
//! sandbox Documents folder and `Bundle.main.resourceURL` for read-only
//! resources shipped inside the `.app`/`.ipa`.
//!
//! On non-iOS targets this module is inert; the parent `#[cfg]` in `mod.rs`
//! strips all calls to it.

// The stub `not(target_os = "ios")` variants below exist purely so this file
// still typechecks when the crate is compiled for desktop or Android.
#![allow(dead_code)]

use std::path::PathBuf;

/// Absolute path of `NSHomeDirectory()/Documents` — the writable sandbox area
/// that survives app upgrades and is exposed in the Files app.
#[cfg(target_os = "ios")]
pub fn documents_dir() -> PathBuf {
    use objc2_foundation::NSHomeDirectory;
    let home = NSHomeDirectory();
    let home_str: String = home.to_string();
    PathBuf::from(home_str).join("Documents")
}

#[cfg(not(target_os = "ios"))]
pub fn documents_dir() -> PathBuf {
    PathBuf::from(".")
}

/// `Bundle.main.resourceURL.path()` — where the packer drops `synstudy.db`
/// alongside the executable inside the app bundle.
#[cfg(target_os = "ios")]
pub fn bundle_resource_dir() -> Option<PathBuf> {
    use objc2_foundation::NSBundle;
    // `mainBundle()` returns a non-null `Retained<NSBundle>`; `resourceURL()`
    // and `path()` are the only fallible steps.
    let bundle = NSBundle::mainBundle();
    let url = bundle.resourceURL()?;
    let path = url.path()?;
    Some(PathBuf::from(path.to_string()))
}

#[cfg(not(target_os = "ios"))]
pub fn bundle_resource_dir() -> Option<PathBuf> {
    None
}

/// Bundled read-only dictionary database inside the `.ipa`.
#[cfg(target_os = "ios")]
pub fn bundle_dict_db() -> Option<PathBuf> {
    let dir = bundle_resource_dir()?;
    let cand = dir.join("synstudy.db");
    if cand.exists() {
        Some(cand)
    } else {
        None
    }
}

#[cfg(not(target_os = "ios"))]
pub fn bundle_dict_db() -> Option<PathBuf> {
    None
}
