use dioxus::prelude::*;

use crate::components::word_detail::WordDetail;
use crate::db::dict_repo::DictRepo;
use crate::models::{Word, WordRoot};

/// 词根背单词：左侧词根/词缀列表，右侧该词根的含义与聚合词，点击单词看详情。
#[component]
pub fn Roots() -> Element {
    let dict: DictRepo = use_context();
    let mut sel_root: Signal<Option<String>> = use_context();
    let mut sel_word = use_signal(|| Option::<Word>::None);
    let mut filter = use_signal(String::new);

    let dict_roots = dict.clone();
    let roots = use_memo(move || dict_roots.roots());
    let all: Vec<WordRoot> = roots();
    let total = all.len();

    let f = filter().trim().to_lowercase();
    let list: Vec<WordRoot> = if f.is_empty() {
        all.clone()
    } else {
        all.iter()
            .filter(|r| {
                r.root_key.to_lowercase().contains(&f)
                    || r.meaning.to_lowercase().contains(&f)
                    || r.class.to_lowercase().contains(&f)
                    || r.note.to_lowercase().contains(&f)
            })
            .cloned()
            .collect()
    };

    let cur_key = sel_root();
    let cur_root = cur_key
        .as_ref()
        .and_then(|k| all.iter().find(|r| &r.root_key == k).cloned());
    let dict_words = dict.clone();
    let words: Vec<Word> = match &cur_key {
        Some(k) => dict_words.root_words(k),
        None => Vec::new(),
    };
    let sel = sel_word().clone();
    // Copyable selected-word id for highlighting chips without moving `sel`.
    let sel_word_id = sel.as_ref().map(|w| w.id);
    // On narrow screens the layout pushes to the detail pane once a root is
    // picked; `has-detail` drives that CSS master-detail toggle.
    let has_detail = cur_root.is_some();

    rsx! {
        div { class: "page roots",
            div { class: "page-head",
                h1 { "词根" }
                span { class: "muted", "共 {total} 个词根 / 词缀" }
            }

            if total == 0 {
                div { class: "session-start",
                    h2 { "暂无词根数据" }
                    p { class: "muted",
                        "当前词典库未包含词根表。请运行 scripts/build_dict_db.py 重新构建（schema v6）。"
                    }
                }
            } else {
                div { class: "search-bar",
                    input {
                        r#type: "text",
                        class: "search-input",
                        placeholder: "筛选词根或含义，如 hom / without …",
                        value: "{filter}",
                        oninput: move |e| {
                            filter.set(e.value());
                            // Typing in the filter should bring back the list on
                            // narrow screens, so drop any pushed detail.
                            sel_root.set(None);
                            sel_word.set(None);
                        },
                    }
                }

                div { class: if has_detail { "dict-body has-detail" } else { "dict-body" },
                    div { class: "dict-results",
                        if list.is_empty() {
                            p { class: "muted pad", "无匹配词根" }
                        } else {
                            for r in list.iter() {
                                button {
                                    key: "{r.root_key}",
                                    class: if cur_key.as_deref() == Some(r.root_key.as_str()) { "result active" } else { "result" },
                                    onclick: {
                                        let k = r.root_key.clone();
                                        move |_| {
                                            sel_root.set(Some(k.clone()));
                                            sel_word.set(None);
                                        }
                                    },
                                    span { class: "result-word", "{r.root_key}" }
                                    span { class: "result-trans", "{r.meaning}" }
                                }
                            }
                        }
                    }

                    div { class: "dict-detail",
                        if !has_detail || sel.is_none() {
                            button {
                                class: "dict-back",
                                onclick: move |_| {
                                    sel_root.set(None);
                                    sel_word.set(None);
                                },
                                "\u{2190} 返回词根列表"
                            }
                        }
                        match cur_root {
                            Some(r) => rsx! {
                                if let Some(sw) = sel {
                                    div { class: "wd-sec root-word-detail",
                                        div { class: "root-detail-top",
                                            button {
                                                class: "btn small ghost",
                                                onclick: move |_| sel_word.set(None),
                                                "← 返回词根"
                                            }
                                            span { class: "muted", "词根 {r.root_key} · 相关单词释义" }
                                        }
                                        WordDetail { key: "{sw.id}", word: sw }
                                    }
                                }
                                div { class: "root-head",
                                    h2 { class: "root-key", "{r.root_key}" }
                                    if !r.class.is_empty() {
                                        span { class: "chip", "{r.class_label()}" }
                                    }
                                    if !r.origin.is_empty() {
                                        span { class: "chip", "{r.origin_label()}" }
                                    }
                                }
                                div { class: "wd-sec",
                                    h4 { "含义" }
                                    p { class: "root-meaning", "{r.meaning}" }
                                }
                                if !r.note.is_empty() {
                                    div { class: "wd-sec",
                                        h4 { "用法讲解" }
                                        p { class: "root-note", "{r.note}" }
                                    }
                                }
                                if !r.synonyms.is_empty() {
                                    div { class: "wd-row",
                                        span { class: "wd-k", "同义" }
                                        span { class: "wd-v", "{r.synonyms}" }
                                    }
                                }
                                if !r.antonyms.is_empty() {
                                    div { class: "wd-row",
                                        span { class: "wd-k", "反义" }
                                        span { class: "wd-v", "{r.antonyms}" }
                                    }
                                }
                                div { class: "wd-sec",
                                    h4 { "相关单词（{words.len()}）" }
                                    if words.is_empty() {
                                        if !r.examples.is_empty() {
                                            p { class: "root-examples muted", "源示例词：{r.examples}" }
                                        } else {
                                            p { class: "muted", "该词根暂无已收录的关联单词。" }
                                        }
                                    } else {
                                        div { class: "root-words",
                                            for w in words.iter() {
                                                button {
                                                    key: "{w.id}",
                                                    class: if sel_word_id == Some(w.id) { "root-word-item active" } else { "root-word-item" },
                                                    onclick: {
                                                        let w = w.clone();
                                                        move |_| sel_word.set(Some(w.clone()))
                                                    },
                                                    span { class: "rw-word", "{w.word}" }
                                                    span {
                                                        class: "rw-gloss muted",
                                                        // First gloss line of the ECDICT translation keeps the row scannable;
                                                        // the full definition opens below on click.
                                                        { w.translation.lines().next().unwrap_or("").to_string() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            None => rsx! {
                                div { class: "empty-detail",
                                    p { class: "muted", "从左侧选择一个词根查看详情" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
