//! Android backend.
//!
//! Wraps `ndk-context` + `jni` to reach the Java `Activity` handle that
//! `tao::android_binding!` stashes during startup. `dioxus-desktop` never hands
//! us the `AndroidApp` (our `main` is reached via `dlsym` from its `start_app`
//! trampoline), so we recover the activity `jobject` from `ndk_context` and
//! drive it through JNI:
//!
//! * `filesDir` (writable) comes from `Activity#getFilesDir()#getAbsolutePath()`.
//! * The 235 MB dictionary ships inside the APK under `assets/` and is copied to
//!   `filesDir` on first launch. We obtain the NDK `AAssetManager*` by fetching
//!   `Activity#getAssets()` and converting it with `AAssetManager_fromJava`,
//!   then stream the asset out with the `AAsset_*` NDK C API.
//!
//! On non-Android targets this module is compiled out (see `mod.rs`).

#![cfg(target_os = "android")]

use std::io::Read;
use std::path::PathBuf;

/// Writable `filesDir` for the app. Falls back to CWD if JNI is unavailable.
pub fn files_dir() -> PathBuf {
    fn inner() -> jni::errors::Result<PathBuf> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context() as jni::sys::jobject) };
        let file = env.call_method(&activity, "getFilesDir", "()Ljava/io/File;", &[])?.l()?;
        let path = env.call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])?.l()?;
        let jstr = jni::objects::JString::from(path);
        let s: String = env.get_string(&jstr)?.into();
        Ok(PathBuf::from(s))
    }
    inner().unwrap_or_else(|_| PathBuf::from("."))
}

/// Reserved for future init work; the ndk_context is populated by tao's glue
/// before our exported `main` runs, so there is nothing to seed here.
pub fn init() {}

// ---------------------------------------------------------------------------
// AAssetManager NDK C bindings (from libandroid.so, already linked by the
// Dioxus Android packaging).
// ---------------------------------------------------------------------------
mod ffi {
    use std::os::raw::{c_char, c_int, c_void};

    #[repr(C)]
    pub struct AAssetManager {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct AAsset {
        _private: [u8; 0],
    }

    extern "C" {
        /// Converts a Java `AssetManager` object to an NDK `AAssetManager*`.
        pub fn AAssetManager_fromJava(env: *mut c_void, asset_manager: *mut c_void)
            -> *mut AAssetManager;
        pub fn AAssetManager_open(
            mgr: *const AAssetManager,
            filename: *const c_char,
            mode: c_int,
        ) -> *mut AAsset;
        pub fn AAsset_read(asset: *mut AAsset, buf: *mut c_void, count: usize) -> c_int;
        pub fn AAsset_getLength64(asset: *mut AAsset) -> i64;
        pub fn AAsset_close(asset: *mut AAsset);
    }

    /// `AASSET_MODE_BUFFER` keeps the whole (uncompressed) asset mapped so
    /// `AAsset_getLength64` and streaming reads are both reliable.
    pub const AASSET_MODE_BUFFER: c_int = 3;
}

/// Copy `assets/synstudy.db` to `filesDir/synstudy.db` on first launch (or when
/// the size marker mismatches). Returns the destination path.
///
/// Marker scheme: `<filesDir>/.dict_version` holds the byte length of the
/// extracted DB. A `partial.tmp` staging file prevents SQLite from opening a
/// torn copy if extraction is interrupted.
pub fn bootstrap_dict_from_assets() -> Option<PathBuf> {
    use std::ffi::CString;
    use std::fs;
    use std::io::Write;

    let files = files_dir();
    if files == PathBuf::from(".") {
        return None;
    }
    let _ = fs::create_dir_all(&files);
    let target = files.join("synstudy.db");
    let marker = files.join(".dict_version");

    // Resolve the NDK AAssetManager* through JNI.
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context() as jni::sys::jobject) };
    let am_jobj = env
        .call_method(&activity, "getAssets", "()Landroid/content/res/AssetManager;", &[])
        .ok()?
        .l()
        .ok()?;
    let mgr_ptr: *mut ffi::AAssetManager = unsafe {
        ffi::AAssetManager_fromJava(
            env.get_raw() as *mut std::os::raw::c_void,
            am_jobj.as_raw() as *mut std::os::raw::c_void,
        )
    };
    if mgr_ptr.is_null() {
        return None;
    }

    let name = CString::new("synstudy.db").ok()?;
    let asset =
        unsafe { ffi::AAssetManager_open(mgr_ptr, name.as_ptr(), ffi::AASSET_MODE_BUFFER) };
    if asset.is_null() {
        return None;
    }
    let asset_size = unsafe { ffi::AAsset_getLength64(asset) } as u64;

    // Skip extraction when an up-to-date copy already exists.
    if target.exists() {
        if let Ok(prev) = fs::read_to_string(&marker) {
            if prev.trim() == asset_size.to_string() {
                unsafe { ffi::AAsset_close(asset) };
                return Some(target);
            }
        }
    }

    // Stream the asset out to a tmp file, then atomically rename.
    let tmp = files.join("synstudy.db.partial.tmp");
    let mut out = match fs::File::create(&tmp) {
        Ok(f) => f,
        Err(_) => {
            unsafe { ffi::AAsset_close(asset) };
            return None;
        }
    };
    let mut reader = AAssetStream { asset };
    let mut buf = vec![0u8; 1024 * 1024];
    let mut written: u64 = 0;
    let copy_ok = loop {
        match reader.read(&mut buf) {
            Ok(0) => break true,
            Ok(n) => {
                if out.write_all(&buf[..n]).is_err() {
                    break false;
                }
                written += n as u64;
            }
            Err(_) => break false,
        }
    };
    drop(out);
    drop(reader); // AAssetStream's Drop closes the handle.

    if !copy_ok || written != asset_size {
        let _ = fs::remove_file(&tmp);
        return None;
    }
    let _ = fs::remove_file(&target);
    if fs::rename(&tmp, &target).is_err() {
        return None;
    }
    let _ = fs::write(&marker, asset_size.to_string());
    Some(target)
}

struct AAssetStream {
    asset: *mut ffi::AAsset,
}

impl Read for AAssetStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = unsafe {
            ffi::AAsset_read(self.asset, buf.as_mut_ptr() as *mut std::os::raw::c_void, buf.len())
        };
        if n < 0 {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("AAsset_read failed: {n}"),
            ))
        } else {
            Ok(n as usize)
        }
    }
}

impl Drop for AAssetStream {
    fn drop(&mut self) {
        unsafe { ffi::AAsset_close(self.asset) };
    }
}
