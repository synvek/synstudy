//! Desktop screen capture (Phase 1/2). Mobile image input arrives through the
//! webview file/camera picker in Phase 3, so this module is compiled only on
//! non-mobile targets (see `lib.rs`).
//!
//! Returns a full-screen `image::DynamicImage`; ROI selection is done by the
//! page (cropping is pure `image` work and needs no platform bridge).

use image::DynamicImage;
use xcap::Monitor;

/// Capture the primary display (falling back to the first enumerated one).
pub fn capture_primary() -> Result<DynamicImage, String> {
    let monitors = Monitor::all().map_err(|e| format!("无法枚举显示器: {e}"))?;
    if monitors.is_empty() {
        return Err("未找到可用显示器".to_string());
    }
    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .unwrap_or(&monitors[0]);
    let rgba = monitor
        .capture_image()
        .map_err(|e| format!("截屏失败: {e}"))?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// Human-readable name of the display that `capture_primary` would grab, for
/// the UI status line. Returns `None` when no display is available.
#[allow(dead_code)] // reserved for a display-picker in the translate UI
pub fn primary_name() -> Option<String> {
    let monitors = Monitor::all().ok()?;
    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())?;
    monitor.name().ok()
}

/// Outcome of a failed [`capture_region`], distinguishing a deliberate cancel
/// from an environment/permission failure, so the caller can fall back to a
/// full-screen grab only when the region tool genuinely could not produce an
/// image (not when the user pressed Esc).
#[derive(Debug)]
pub enum RegionError {
    /// The user cancelled the interactive selection.
    Cancelled,
    /// The grab failed (permission denied / empty rect / unsupported platform).
    Failed(String),
}

/// macOS: hand region picking to the system overlay (`screencapture -i`), which
/// lets the user drag an arbitrarily positioned/resized rectangle with pixel
/// accuracy — the most natural way to "截取部分屏幕" on macOS.
///
/// `screencapture` inherits Screen Recording permission from the *responsible*
/// process. A bare `cargo run` debug binary is not a `.app` bundle, so TCC
/// attributes it to the launching terminal/IDE — which usually lacks the grant
/// and makes `screencapture` fail with "could not create image from rect". The
/// caller falls back to a full-screen grab in that case.
#[cfg(target_os = "macos")]
pub fn capture_region() -> Result<DynamicImage, RegionError> {
    use std::process::Stdio;
    let path = std::env::temp_dir().join(format!("synstudy_region_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // -i: interactive rectangle; -x: mute the shutter sound.
    let out = std::process::Command::new("screencapture")
        .args(["-i", "-x"])
        .arg(&path)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| RegionError::Failed(format!("调用系统截图失败: {e}")))?;
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    if path.exists() {
        let opened = image::open(&path);
        let _ = std::fs::remove_file(&path);
        return opened
            .map_err(|e| RegionError::Failed(format!("读取区域截图失败: {e}")));
    }

    // No file written: separate a genuine failure from a plain Esc cancel so the
    // caller knows when to fall back to a full-screen grab.
    let lower = stderr.to_lowercase();
    if lower.contains("could not create image")
        || lower.contains("not authorized")
        || lower.contains("permission")
        || lower.contains("denied")
    {
        Err(RegionError::Failed(
            "屏幕录制权限未授予，或 macOS 返回空图（跨屏/副屏框选常见）".to_string(),
        ))
    } else if !stderr.is_empty() {
        Err(RegionError::Failed(stderr))
    } else {
        Err(RegionError::Cancelled)
    }
}

/// Non-macOS desktops (Windows/Linux) have no uniformly scriptable interactive
/// rectangle grabber, so region selection is done in-app over a full-screen
/// capture. Reported as `Failed` so the caller falls straight through to a
/// full-screen grab.
#[cfg(not(target_os = "macos"))]
pub fn capture_region() -> Result<DynamicImage, RegionError> {
    Err(RegionError::Failed(
        "当前平台不支持系统框选，请拖拽或用数值框选区".to_string(),
    ))
}
