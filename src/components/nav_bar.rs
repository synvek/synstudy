use dioxus::prelude::*;

use crate::state::Page;

#[component]
pub fn NavBar() -> Element {
    let mut page: Signal<Page> = use_context();
    let current = page();

    rsx! {
        nav { class: "navbar",
            div { class: "brand",
                span { class: "brand-mark", "S" }
                span { class: "brand-name", "synstudy" }
            }
            div { class: "nav-links",
                for p in Page::nav_items().iter().copied() {
                    button {
                        key: "{p:?}",
                        class: if current == p { "nav-btn active" } else { "nav-btn" },
                        onclick: move |_| page.set(p),
                        "{p.label()}"
                    }
                }
            }
            button {
                class: if current == Page::Settings { "nav-btn gear active" } else { "nav-btn gear" },
                onclick: move |_| page.set(Page::Settings),
                "设置"
            }
        }
    }
}
