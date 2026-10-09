//! Offline text-to-speech built on the webview's `SpeechSynthesis` API.
//!
//! Dioxus' `document::eval` dispatches JavaScript into the host webview on
//! desktop (wry) and is a harmless no-op where no DOM exists, so the same
//! helper works across targets without a native dependency or network audio.
//! All calls are fire-and-forget: we never wait on the returned evaluator.

use dioxus::document;

/// Escape a string for safe embedding inside a JS double-quoted literal.
fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Speak `text` aloud at `rate` (clamped to 0.5–2.0). Cancels any in-flight
/// utterance first so rapid clicks don't queue up. Empty input is ignored.
pub fn speak(text: &str, rate: f32) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let escaped = js_escape(trimmed);
    let rate = rate.clamp(0.5, 2.0);
    document::eval(&format!(
        "try {{ \
           if (window.speechSynthesis) {{ \
             window.speechSynthesis.cancel(); \
             const u = new SpeechSynthesisUtterance(\"{escaped}\"); \
             u.lang = 'en-US'; u.rate = {rate}; \
             window.speechSynthesis.speak(u); \
           }} \
         }} catch (e) {{}}"
    ));
}

/// Stop any current speech.
pub fn stop() {
    document::eval("try { if (window.speechSynthesis) { window.speechSynthesis.cancel(); } } catch (e) {}");
}

#[cfg(test)]
mod tests {
    use super::js_escape;

    #[test]
    fn escapes_quotes_and_control_chars() {
        assert_eq!(js_escape("a\"b"), "a\\\"b");
        assert_eq!(js_escape("a\\b"), "a\\\\b");
        assert_eq!(js_escape("a\nb"), "a\\nb");
        // Plain text is untouched.
        assert_eq!(js_escape("achieve"), "achieve");
    }
}
