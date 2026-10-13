use dioxus::prelude::*;

use crate::state::Page;

/// Desktop top navigation bar: brand on the left, page links on the right,
/// with a divider before the settings shortcut.
#[component]
pub fn NavBar() -> Element {
    let mut page: Signal<Page> = use_context();
    let current = page();

    rsx! {
        nav { class: "navbar top",
            div { class: "brand",
                span { class: "brand-mark", "S" }
                span { class: "brand-name", "Synstudy" span { class: "brand-dot", "." } }
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
                span { class: "nav-divider" }
                button {
                    class: if current == Page::Settings { "nav-btn gear active" } else { "nav-btn gear" },
                    onclick: move |_| page.set(Page::Settings),
                    "设置"
                }
            }
        }
    }
}

/// The five primary destinations shown in the mobile bottom tab bar. The last
/// one is the "我的" hub (`Page::Mine`), a card-list page that gathers the
/// secondary links (replacing the old popup menu).
const TABS: [(Page, &str, &str); 5] = [
    (Page::Dictionary, "词典", "book"),
    (Page::Categories, "词库", "layers"),
    (Page::Study, "学习", "cap"),
    (Page::Wordbook, "生词本", "heart"),
    (Page::Mine, "我的", "user"),
];

/// Secondary destinations grouped under the "我的" hub. Kept here so the tab
/// bar can stay lit while any of them is open.
const MORE_PAGES: [(Page, &str); 5] = [
    (Page::Review, "背诵"),
    (Page::Quiz, "测验"),
    (Page::Roots, "词根"),
    (Page::Stats, "统计"),
    (Page::Settings, "设置"),
];

/// Fixed bottom tab bar shown on narrow viewports (see the mobile media query).
#[component]
pub fn MobileTabBar() -> Element {
    let mut page: Signal<Page> = use_context();
    let current = page();
    // "我的" lights up while it or any of its secondary pages is open.
    let mine_active = MORE_PAGES.iter().any(|(p, _)| *p == current);

    rsx! {
        div { class: "mobile-tabbar",
            nav { class: "mobile-tabbar-bar",
                for (i, (tab, label, icon)) in TABS.iter().enumerate() {
                    {
                        let active = if *tab == Page::Mine {
                            current == Page::Mine || mine_active
                        } else {
                            current == *tab
                        };
                        let icon = icon.to_string();
                        let label = label.to_string();
                        let tab = *tab;
                        rsx! {
                            button {
                                key: "{i}",
                                class: if active { "tab-item active" } else { "tab-item" },
                                onclick: move |_| page.set(tab),
                                TabIcon { name: icon.clone(), active }
                                span { "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Inline SVG icons for the tab bar (kept dependency-free, no icon CDN).
#[component]
fn TabIcon(name: String, active: bool) -> Element {
    let sw = if active { "2.2" } else { "1.8" };
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "{sw}",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            match name.as_str() {
                "book" => rsx! {
                    path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                    path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                },
                "layers" => rsx! {
                    path { d: "M12 2 2 7l10 5 10-5-10-5z" }
                    path { d: "M2 17l10 5 10-5" }
                    path { d: "M2 12l10 5 10-5" }
                },
                "cap" => rsx! {
                    path { d: "M22 10 12 5 2 10l10 5 10-5z" }
                    path { d: "M6 12v5c0 1 2.7 3 6 3s6-2 6-3v-5" }
                },
                "heart" => rsx! {
                    path { d: "M20.8 5.6a5 5 0 0 0-7.1 0L12 7.3l-1.7-1.7a5 5 0 1 0-7.1 7.1l1.7 1.7L12 21.5l7.1-7.1 1.7-1.7a5 5 0 0 0 0-7.1z" }
                },
                _ => rsx! {
                    path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                    circle { cx: "12", cy: "7", r: "4" }
                },
            }
        }
    }
}
