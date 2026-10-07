use dioxus::prelude::*;

use crate::components::progress_bar::ProgressBar;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::state::now_ts;

#[component]
pub fn Stats() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();

    let cats = use_memo(move || dict.categories());
    let list = cats();
    let now = now_ts();

    let mut tot_new = 0i64;
    let mut tot_learning = 0i64;
    let mut tot_reviewing = 0i64;
    let mut tot_mastered = 0i64;
    let mut tot_words = 0i64;
    for c in list.iter() {
        let s = progress.stats(c.id, c.word_count, now);
        tot_new += s.new_count;
        tot_learning += s.learning;
        tot_reviewing += s.reviewing;
        tot_mastered += s.mastered;
        tot_words += c.word_count;
    }

    rsx! {
        div { class: "page stats",
            div { class: "page-head",
                h1 { "学习统计" }
            }

            div { class: "stat-cards",
                div { class: "stat-card", span { class: "sc-num", "{tot_mastered}" } span { class: "sc-lbl", "已掌握" } }
                div { class: "stat-card", span { class: "sc-num", "{tot_reviewing}" } span { class: "sc-lbl", "复习中" } }
                div { class: "stat-card", span { class: "sc-num", "{tot_learning}" } span { class: "sc-lbl", "学习中" } }
                div { class: "stat-card", span { class: "sc-num", "{tot_new}" } span { class: "sc-lbl", "未学" } }
            }

            div { class: "stats-table",
                div { class: "stats-row head",
                    span { class: "sr-name", "词库" }
                    span { "未学" }
                    span { "学习中" }
                    span { "复习中" }
                    span { "已掌握" }
                    span { class: "sr-bar", "进度" }
                }
                {list.iter().map(|c| {
                    let s = progress.stats(c.id, c.word_count, now);
                    let pct = if c.word_count > 0 {
                        (s.started() as f64 / c.word_count as f64 * 100.0).round()
                    } else { 0.0 };
                    rsx! {
                        div { key: "{c.id}", class: "stats-row",
                            span { class: "sr-name", "{c.name}" }
                            span { "{s.new_count}" }
                            span { "{s.learning}" }
                            span { "{s.reviewing}" }
                            span { "{s.mastered}" }
                            span { class: "sr-bar",
                                ProgressBar { value: s.started(), max: c.word_count, label: "{pct}%" }
                            }
                        }
                    }
                })}
                div { class: "stats-row total",
                    span { class: "sr-name", "合计" }
                    span { "{tot_new}" }
                    span { "{tot_learning}" }
                    span { "{tot_reviewing}" }
                    span { "{tot_mastered}" }
                    span { class: "sr-bar", "{tot_words} 词" }
                }
            }
        }
    }
}
