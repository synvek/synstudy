pub mod categories;
pub mod consolidate;
pub mod dictionary;
pub mod mine;
pub mod quiz;
pub mod review;
pub mod roots;
pub mod sentence_quiz;
pub mod settings;
pub mod stats;
pub mod study;
pub mod translate;
pub mod wordbook;

use dioxus::prelude::*;

use crate::state::Page;

/// Shown when Study/Review is opened without a selected category.
#[component]
pub fn PickCategory(#[props(default)] mode: &'static str) -> Element {
    let mut page: Signal<Page> = use_context();
    rsx! {
        div { class: "page empty-pick",
            h1 { "{mode}" }
            p { class: "muted", "请先在「词库」中选择一个词库。" }
            button {
                class: "btn primary",
                onclick: move |_| page.set(Page::Categories),
                "去选择词库"
            }
        }
    }
}
