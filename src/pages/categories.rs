use dioxus::prelude::*;

use crate::components::category_tile::CategoryTile;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::state::{now_ts, Page};

#[component]
pub fn Categories() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut page: Signal<Page> = use_context();
    let mut selected: Signal<Option<Category>> = use_context();

    let cats = use_memo(move || dict.categories());
    let list = cats();
    let now = now_ts();
    let count = list.len();
    let selected_id = selected().map(|c| c.id);

    rsx! {
        div { class: "page categories",
            div { class: "page-hero",
                div {
                    div { class: "eyebrow", "BUILD YOUR VOCABULARY" }
                    h1 { "词库" }
                    p { class: "sub", "选择词库开始学习或背诵，让每一次积累都有方向。" }
                }
                span { class: "count-pill", "⚡ {count} 个精选词库" }
            }

            div { class: "legend", style: "margin: 22px 0 18px;",
                span { class: "dot teal" } "基础教育"
                span { class: "dot indigo" } "大学与专业"
                span { class: "dot amber" } "留学考试"
            }

            div { class: "cat-grid",
                {list.into_iter().map(|c| {
                    let stats = progress.stats(c.id, c.word_count, now);
                    let cid = c.id;
                    let active = selected_id == Some(cid) && stats.started() > 0;
                    let for_study = c.clone();
                    let for_review = c.clone();
                    rsx! {
                        CategoryTile {
                            key: "{cid}",
                            category: c,
                            started: stats.started(),
                            due: stats.due,
                            active,
                            on_study: move |_| {
                                selected.set(Some(for_study.clone()));
                                page.set(Page::Study);
                            },
                            on_review: move |_| {
                                selected.set(Some(for_review.clone()));
                                page.set(Page::Review);
                            }
                        }
                    }
                })}
            }
        }
    }
}
