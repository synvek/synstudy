use dioxus::prelude::*;

use crate::components::word_detail::WordDetail;
use crate::db::dict_repo::DictRepo;
use crate::db::progress_repo::ProgressRepo;
use crate::models::Word;
use crate::state::{first_line, Page};

/// 生词本（收藏）：浏览已加星标的单词、查看详情、移除，或跳转词典。
#[component]
pub fn Wordbook() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut page: Signal<Page> = use_context();

    // Bumped after any removal so the list re-reads from the database.
    let mut revision = use_signal(|| 0u64);
    let mut selected = use_signal(|| Option::<Word>::None);

    // Reading `revision` here makes the render reactive to list changes.
    let _ = revision();
    let words: Vec<Word> = progress
        .list_starred()
        .iter()
        .filter_map(|id| dict.get_by_id(*id))
        .collect();

    let sel = selected().clone();
    let selected_id = sel.as_ref().map(|w| w.id);
    // Keep the selection valid: drop it if the word left the wordbook.
    if let Some(id) = selected_id {
        if !words.iter().any(|w| w.id == id) {
            selected.set(None);
        }
    }
    let total = words.len();

    rsx! {
        div { class: "page wordbook",
            div { class: "page-head",
                h1 { "生词本" }
                span { class: "muted", "共收藏 {total} 个单词" }
            }

            if total == 0 {
                div { class: "session-start",
                    h2 { "还没有收藏单词" }
                    p { class: "muted", "在词典中打开任意单词，点击标题旁的 ☆ 即可加入生词本。" }
                    button {
                        class: "btn primary big",
                        onclick: move |_| page.set(Page::Dictionary),
                        "去词典看看"
                    }
                }
            } else {
                div { class: "dict-body",
                    div { class: "dict-results",
                        for w in words.iter() {
                            div {
                                key: "{w.id}",
                                class: if selected_id == Some(w.id) { "result active" } else { "result" },
                                button {
                                    class: "wb-row-main",
                                    onclick: {
                                        let w = w.clone();
                                        move |_| selected.set(Some(w.clone()))
                                    },
                                    span { class: "result-word", "{w.word}" }
                                    span { class: "result-trans", "{first_line(&w.translation)}" }
                                }
                                button {
                                    class: "wb-row-remove",
                                    title: "移出生词本",
                                    onclick: {
                                        let id = w.id;
                                        let progress = progress.clone();
                                        move |_| {
                                            progress.remove_star(id);
                                            if selected_id == Some(id) {
                                                selected.set(None);
                                            }
                                            revision += 1;
                                        }
                                    },
                                    "\u{2715}"
                                }
                            }
                        }
                    }

                    div { class: "dict-detail",
                        match sel {
                            Some(w) => rsx! { WordDetail { key: "{w.id}", word: w } },
                            None => rsx! {
                                div { class: "empty-detail",
                                    p { class: "muted", "从左侧选择一个单词查看详情" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
