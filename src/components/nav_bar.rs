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
                span { class: "brand-mark", "s" }
                span { class: "brand-name", "synstudy" span { class: "brand-dot", "." } }
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
/// one ("我的") is not a page — it opens `MoreSheet` for the secondary links.
const TABS: [(Option<Page>, &str, &str); 5] = [
    (Some(Page::Dictionary), "词典", "book"),
    (Some(Page::Categories), "词库", "layers"),
    (Some(Page::Study), "学习", "cap"),
    (Some(Page::Wordbook), "生词本", "heart"),
    (None, "我的", "user"),
];

/// Pages reachable only through the "我的" sheet on mobile.
const MORE_PAGES: [(Page, &str); 5] = [
    (Page::Review, "背诵"),
    (Page::Quiz, "测验"),
    (Page::Roots, "词根"),
    (Page::Stats, "统计"),
    (Page::Settings, "设置"),
];

/// Fixed bottom tab bar shown on narrow viewports (see the mobile media query).
#[component]
pub fn MobileTabBar(on_more: EventHandler<()>) -> Element {
    let mut page: Signal<Page> = use_context();
    let current = page();
    // "我的" lights up while any secondary page is open.
    let more_active = MORE_PAGES.iter().any(|(p, _)| *p == current);

    rsx! {
        div { class: "mobile-tabbar",
            nav { class: "mobile-tabbar-bar",
                for (i, (target, label, icon)) in TABS.iter().enumerate() {
                    {
                        let active = match target {
                            Some(p) => current == *p,
                            None => more_active,
                        };
                        let icon = icon.to_string();
                        let label = label.to_string();
                        let target = *target;
                        rsx! {
                            button {
                                key: "{i}",
                                class: if active { "tab-item active" } else { "tab-item" },
                                onclick: move |_| match target {
                                    Some(p) => page.set(p),
                                    None => on_more.call(()),
                                },
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

/// Bottom sheet listing the secondary destinations behind the "我的" tab.
#[component]
pub fn MoreSheet(on_close: EventHandler<()>) -> Element {
    let mut page: Signal<Page> = use_context();

    rsx! {
        div { class: "sheet-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "sheet",
                // Swallow clicks so tapping the panel does not dismiss it.
                onclick: move |evt| evt.stop_propagation(),
                div { class: "sheet-title", "更多" }
                for (p, label) in MORE_PAGES.iter().copied() {
                    button {
                        key: "{p:?}",
                        class: "sheet-item",
                        onclick: move |_| {
                            page.set(p);
                            on_close.call(());
                        },
                        span { class: "sheet-ico", "{sheet_icon(p)}" }
                        span { "{label}" }
                    }
                }
            }
        }
    }
}

/// A glyph for each secondary destination in the sheet.
fn sheet_icon(p: Page) -> &'static str {
    match p {
        Page::Review => "\u{1f501}",
        Page::Quiz => "\u{270f}\u{fe0f}",
        Page::Roots => "\u{1f331}",
        Page::Stats => "\u{1f4ca}",
        _ => "\u{2699}\u{fe0f}",
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
