use dioxus::prelude::*;

use crate::components::progress_bar::ProgressBar;
use crate::models::Category;

/// `(abbr, tone class, group label)` for a category, driving the coloured book
/// icon and the group caption on the wordbank cards.
pub fn category_visual(key: &str) -> (&'static str, &'static str, &'static str) {
    match key {
        "xiaoxue" => ("Aa", "tone-teal", "基础教育"),
        "chuzhong" => ("Ab", "tone-teal", "基础教育"),
        "gaozhong" => ("Ac", "tone-teal", "基础教育"),
        "cet4" => ("C4", "tone-indigo", "大学与专业"),
        "cet6" => ("C6", "tone-indigo", "大学与专业"),
        "kaoyan" => ("研", "tone-indigo", "大学与专业"),
        "tem4" => ("T4", "tone-indigo", "大学与专业"),
        "tem8" => ("T8", "tone-indigo", "大学与专业"),
        "toefl" => ("\u{1f310}", "tone-amber", "留学考试"),
        "ielts" => ("雅", "tone-amber", "留学考试"),
        "gre" => ("G", "tone-amber", "留学考试"),
        _ => ("词", "tone-indigo", ""),
    }
}

/// Short display code for a category, shown as the caption prefix on the study
/// overview card (e.g. `cet6` -> `CET-6`).
pub fn category_code(key: &str) -> &'static str {
    match key {
        "xiaoxue" => "小学",
        "chuzhong" => "初中",
        "gaozhong" => "高中",
        "cet4" => "CET-4",
        "cet6" => "CET-6",
        "kaoyan" => "考研",
        "tem4" => "TEM-4",
        "tem8" => "TEM-8",
        "toefl" => "TOEFL",
        "ielts" => "IELTS",
        "gre" => "GRE",
        _ => "词库",
    }
}

#[component]
pub fn CategoryTile(
    category: Category,
    started: i64,
    due: i64,
    #[props(default)] active: bool,
    on_study: EventHandler<()>,
    on_review: EventHandler<()>,
) -> Element {
    let (abbr, tone, group) = category_visual(&category.key);
    let icon_tone = if active { "tone-grad" } else { tone };
    let tile_class = if active { "cat-tile active" } else { "cat-tile" };

    rsx! {
        div { class: "{tile_class}",
            if active {
                span { class: "badge-status", "学习中" }
            }
            div { class: "cat-top",
                div { class: "bookicon {icon_tone}", "{abbr}" }
                div {
                    h3 { class: "cat-title", "{category.name}" }
                    p { class: "cat-sub", "{category.word_count} 词 · {group}" }
                }
            }
            div { class: "cat-status",
                if started > 0 {
                    span { class: "started", "已学 " b { "{started}" } " / {category.word_count}" }
                } else {
                    span { "尚未开始" }
                }
                if due > 0 {
                    span { class: "due", "待复习 {due}" }
                } else {
                    span { "无待复习" }
                }
            }
            ProgressBar { value: started, max: category.word_count }
            div { class: "cat-actions",
                button {
                    class: "btn primary",
                    onclick: move |_| on_study.call(()),
                    if active { "继续学习" } else { "学习" }
                }
                button { class: "btn", onclick: move |_| on_review.call(()), "背诵" }
            }
        }
    }
}
