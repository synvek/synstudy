//! Tiny asset helpers shared across components.

/// Encode PNG bytes as an inline `data:image/png;base64,...` URI.
///
/// Used for images that must paint without relying on any asset server: the
/// mobile splash and the navbar brand mark embed their bytes at compile time
/// via `include_bytes!`, so a data URI is the one form that always resolves
/// (including the very first frame, before anything is served). Dependency-free
/// standard base64 with `=` padding.
pub fn png_data_uri(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    format!("data:image/png;base64,{out}")
}

// Branded app icon (the logo on the dark graphite gradient), embedded at
// compile time. Used by the navbar brand mark and the Settings > About logo so
// the in-app brand matches the real app icon instead of a hand-styled "S".
const BRAND_MARK: &[u8] = include_bytes!("../assets/icons/app-icon-256.png");

/// Data URI for the branded app icon, base64-encoded once and cached for the
/// process lifetime (these views re-render often; don't re-encode each time).
pub fn brand_mark_uri() -> &'static str {
    static URI: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URI.get_or_init(|| png_data_uri(BRAND_MARK)).as_str()
}
