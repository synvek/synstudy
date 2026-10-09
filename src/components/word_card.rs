use dioxus::prelude::*;

use crate::components::text_lines;
use crate::models::Word;
use crate::state::Settings;
use crate::tts;

#[component]
pub fn WordCard(word: Word, revealed: bool, on_flip: EventHandler<()>) -> Element {
    let settings: Signal<Settings> = use_context();
    let speak_word = word.word.clone();
    rsx! {
        div {
            class: if revealed { "card revealed" } else { "card" },
            onclick: move |_| on_flip.call(()),
            div { class: "card-word-row",
                span { class: "card-word", "{word.word}" }
                if settings().tts_enabled {
                    button {
                        class: "card-speak",
                        title: "朗读",
                        onclick: move |evt| {
                            // Don't let the click also flip the card.
                            evt.stop_propagation();
                            tts::speak(&speak_word, settings().tts_rate);
                        },
                        "\u{1f50a}"
                    }
                }
            }
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
