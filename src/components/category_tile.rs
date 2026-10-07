use dioxus::prelude::*;

use crate::components::progress_bar::ProgressBar;
use crate::models::Category;

#[component]
pub fn CategoryTile(
    category: Category,
    started: i64,
    due: i64,
    on_study: EventHandler<()>,
    on_review: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "cat-tile",
            div { class: "cat-head",
                h3 { "{category.name}" }
                span { class: "cat-count", "{category.word_count} 词" }
            }
            ProgressBar {
                value: started,
                max: category.word_count,
                label: "已学 {started}/{category.word_count}"
            }
            div { class: "cat-meta",
                if due > 0 {
                    span { class: "cat-due", "待复习 {due}" }
                } else {
                    span { class: "cat-due none", "无待复习" }
                }
            }
            div { class: "cat-actions",
                button { class: "btn primary", onclick: move |_| on_study.call(()), "学习" }
                button { class: "btn", onclick: move |_| on_review.call(()), "背诵" }
            }
        }
    }
}
