use dioxus::prelude::*;

use crate::components::text_lines;
use crate::models::Word;

#[component]
pub fn WordCard(word: Word, revealed: bool, on_flip: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: if revealed { "card revealed" } else { "card" },
            onclick: move |_| on_flip.call(()),
            div { class: "card-word", "{word.word}" }
            if !word.phonetic.is_empty() {
                div { class: "card-phonetic", "{word.phonetic}" }
            }
            div { class: "card-divider" }
            if revealed {
                div { class: "card-trans", {text_lines(&word.translation, "line")} }
                if !word.definition.trim().is_empty() {
                    div { class: "card-def", {text_lines(&word.definition, "line")} }
                }
            } else {
                div { class: "card-hidden", "点击卡片显示释义" }
            }
        }
    }
}
