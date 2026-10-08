use dioxus::prelude::*;

use crate::models::ZhEntry;

/// Detail panel for a Chinese->English (汉英) entry from CC-CEDICT.
///
/// `on_pick` is fired with an English headword when the user clicks a related
/// word chip, letting the page cross-link back into the ECDICT (英汉) detail.
#[component]
pub fn ZhDetail(entry: ZhEntry, on_pick: EventHandler<String>) -> Element {
    let glosses = entry.gloss_list();
    let heads = entry.english_heads();
    let show_trad = entry.traditional != entry.simplified;

    rsx! {
        div { class: "word-detail zh-detail",
            div { class: "wd-head",
                h2 { class: "wd-zh-word", "{entry.simplified}" }
                if show_trad {
                    span { class: "wd-zh-trad", "繁体 {entry.traditional}" }
                }
            }

            if !entry.pinyin.is_empty() {
                div { class: "wd-pinyin", "[ {entry.pinyin} ]" }
            }

            div { class: "wd-sec",
                h4 { "英文释义" }
                if glosses.is_empty() {
                    span { class: "muted", "-" }
                } else {
                    div { class: "wd-gloss",
                        for (i, g) in glosses.iter().enumerate() {
                            div { key: "{i}", class: "gloss-line", "{g}" }
                        }
                    }
                }
            }

            if !heads.is_empty() {
                div { class: "wd-sec",
                    h4 { "关联英文词 · 点击查看英汉详情" }
                    div { class: "wd-links",
                        for h in heads.iter() {
                            button {
                                key: "{h}",
                                class: "chip chip-link",
                                onclick: {
                                    let h = h.clone();
                                    move |_| on_pick.call(h.clone())
                                },
                                "{h}"
                            }
                        }
                    }
                }
            }

            div { class: "wd-row",
                span { class: "wd-k", "来源" }
                span { class: "wd-v", "CC-CEDICT · CC-BY-SA 4.0" }
            }
        }
    }
}
