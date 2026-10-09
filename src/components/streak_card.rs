use dioxus::prelude::*;

/// Compact "连续打卡" card showing the current and longest review streaks.
#[component]
pub fn StreakCard(current: i64, longest: i64) -> Element {
    rsx! {
        div { class: "streak-card",
            div { class: "streak-main",
                span { class: "streak-flame", "\u{1f525}" }
                span { class: "streak-num", "{current}" }
                span { class: "streak-unit", "天连续" }
            }
            div { class: "streak-sub", "最长连续 {longest} 天" }
        }
    }
}

/// Memory-retention card: the share of reviews rated Good/Easy.
#[component]
pub fn RetentionCard(rate: Option<f64>, reviews: i64) -> Element {
    let pct = rate.map(|r| (r * 100.0).round() as i64);
    rsx! {
        div { class: "streak-card",
            div { class: "streak-main",
                span { class: "streak-num",
                    match pct {
                        Some(p) => format!("{p}%"),
                        None => "—".to_string(),
                    }
                }
                span { class: "streak-unit", "记忆保持率" }
            }
            div { class: "streak-sub", "基于 {reviews} 次复习评分（良好/轻松占比）" }
        }
    }
}
