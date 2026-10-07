use dioxus::prelude::*;

use crate::components::{format_exchange, text_lines};
use crate::models::Word;

#[component]
pub fn WordDetail(word: Word) -> Element {
    let tags = word.tag_labels();
    let exchange = format_exchange(&word.exchange);

    rsx! {
        div { class: "word-detail",
            div { class: "wd-head",
                h2 { class: "wd-word", "{word.word}" }
                if !word.phonetic.is_empty() {
                    span { class: "wd-phonetic", "{word.phonetic}" }
                }
            }

            div { class: "wd-sec",
                h4 { "释义" }
                div { class: "wd-trans", {text_lines(&word.translation, "line")} }
            }

            if !word.definition.trim().is_empty() {
                div { class: "wd-sec",
                    h4 { "英文释义" }
                    div { class: "wd-def", {text_lines(&word.definition, "line")} }
                }
            }

            if !word.pos.trim().is_empty() {
                div { class: "wd-row",
                    span { class: "wd-k", "词性分布" }
                    span { class: "wd-v", "{word.pos}" }
                }
            }

            if !exchange.is_empty() {
                div { class: "wd-row",
                    span { class: "wd-k", "词形变化" }
                    span { class: "wd-v", "{exchange}" }
                }
            }

            div { class: "wd-row",
                span { class: "wd-k", "词频排名" }
                span { class: "wd-v", "BNC {word.bnc} · COCA {word.frq}" }
            }

            if !tags.is_empty() {
                div { class: "wd-tags",
                    for t in tags.iter() {
                        span { key: "{t}", class: "chip", "{t}" }
                    }
                }
            }
        }
    }
}
