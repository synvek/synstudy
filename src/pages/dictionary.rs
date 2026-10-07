use dioxus::prelude::*;

use crate::components::word_detail::WordDetail;
use crate::db::dict_repo::DictRepo;
use crate::models::Word;
use crate::state::first_line;

#[component]
pub fn Dictionary() -> Element {
    let dict: DictRepo = use_context();
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| Option::<Word>::None);

    let dict_for_total = dict.clone();
    let results = use_memo(move || {
        let text = query.read().clone();
        dict.search(&text, 40)
    });

    let total = use_memo(move || dict_for_total.word_total());
    let list = results();
    let sel = selected().clone();
    let selected_id = sel.as_ref().map(|w| w.id);

    rsx! {
        div { class: "page dictionary",
            div { class: "page-head",
                h1 { "英汉词典" }
                span { class: "muted", "共收录 {total()} 词" }
            }

            div { class: "search-bar",
                input {
                    r#type: "text",
                    class: "search-input",
                    placeholder: "输入英文单词查询，如 achieve …",
                    value: "{query}",
                    autofocus: true,
                    oninput: move |e| query.set(e.value()),
                }
                if !query().is_empty() {
                    button {
                        class: "search-clear",
                        onclick: move |_| {
                            query.set(String::new());
                            selected.set(None);
                        },
                        "✕"
                    }
                }
            }

            div { class: "dict-body",
                div { class: "dict-results",
                    if query().trim().is_empty() {
                        p { class: "muted pad", "输入单词开始查询" }
                    } else if list.is_empty() {
                        p { class: "muted pad", "未找到匹配的单词" }
                    } else {
                        for w in list.iter() {
                            button {
                                key: "{w.id}",
                                class: if selected_id == Some(w.id) { "result active" } else { "result" },
                                onclick: {
                                    let w = w.clone();
                                    move |_| selected.set(Some(w.clone()))
                                },
                                span { class: "result-word", "{w.word}" }
                                span { class: "result-trans", "{first_line(&w.translation)}" }
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
