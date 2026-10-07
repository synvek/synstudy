use dioxus::prelude::*;

#[component]
pub fn ProgressBar(value: i64, max: i64, #[props(default)] label: String) -> Element {
    let pct = if max > 0 {
        ((value as f64 / max as f64) * 100.0).round().clamp(0.0, 100.0)
    } else {
        0.0
    };
    rsx! {
        div { class: "progress",
            div { class: "progress-fill", style: "width: {pct}%" }
            if !label.is_empty() {
                span { class: "progress-label", "{label}" }
            }
        }
    }
}
