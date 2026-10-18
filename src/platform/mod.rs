//! Platform-specific storage & resource layout.
//!
//! The rest of the app talks to `data_dir()`, `resource_dir()`,
//! `resolve_dict_db()`, and `progress_db_path()`; each backend decides how to
//! satisfy them on macOS/Linux/Windows, iOS, or Android.

// Gate each backend to the target that actually has its native dependency
// available: `android` needs `ndk-context`/`jni`, `ios` needs `objc2`, and
// `native` needs `directories`. Declaring them unconditionally would pull a
// crate that Cargo omits for the current target and fail to resolve.
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod native;

use std::path::PathBuf;

/// Initialize platform singletons. Called from `synstudy::main` before
/// Dioxus launches so downstream code (TTS bridge, asset extractor) can rely
/// on the state being present.
pub fn init() {
    #[cfg(target_os = "android")]
    android::init();
}

/// Writable directory used for user state (`progress.db`, extracted assets).
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "ios")]
    {
        ios::documents_dir()
    }
    #[cfg(target_os = "android")]
    {
        android::files_dir()
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        native::desktop_data_dir()
    }
}

/// Read-only directory holding bundled resources. Undefined on Android where
/// resources are streamed through `AAssetManager` instead of the filesystem.
#[cfg(not(target_os = "android"))]
#[allow(dead_code)] // consumed by future asset loaders; unused on desktop today
pub fn resource_dir() -> Option<PathBuf> {
    #[cfg(target_os = "ios")]
    {
        ios::bundle_resource_dir()
    }
    #[cfg(not(target_os = "ios"))]
    {
        native::desktop_resource_dir()
    }
}

/// Locate the read-only dictionary database. Returns `None` only when the
/// database genuinely isn't reachable (which surfaces as the "dictionary not
/// built" hint in the settings page).
pub fn resolve_dict_db() -> Option<PathBuf> {
    // Explicit override wins on every platform; useful for tests and for
    // desktop developers iterating on `assets/synstudy.db` in-tree.
    if let Ok(p) = std::env::var("SYNSTUDY_DICT_DB") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }

    #[cfg(target_os = "android")]
    {
        android::bootstrap_dict_from_assets()
    }
    #[cfg(target_os = "ios")]
    {
        ios::bundle_dict_db()
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        native::desktop_dict_db()
    }
}

/// Path of the writable user-progress database (created on first run).
pub fn progress_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("SYNSTUDY_PROGRESS_DB") {
        let pb = PathBuf::from(p);
        if let Some(dir) = pb.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        return pb;
    }
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("progress.db")
}

/// Directory for on-demand downloaded translation models (OCR + MT weights).
///
/// Models are never bundled with the app (they are ~1 GB and would break App
/// Store size limits), so they live under the writable data dir and are
/// fetched/verified at first use. Created lazily. Only compiled when the
/// `translate` feature is on.
#[cfg(feature = "translate")]
pub fn models_dir() -> PathBuf {
    let dir = data_dir().join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
