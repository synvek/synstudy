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
                // Branded logo (was a gradient badge with a literal "S"); see
                // assets::brand_mark_uri.
                img { class: "brand-mark", src: "{crate::assets::brand_mark_uri()}", alt: "Synstudy" }
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

/// The five primary destinations shown in the mobile bottom tab bar. `巩固`
/// gathers the study-and-reinforce flows (词库/生词本/背诵/测验/词根) behind a
/// card-list hub; `我的` is the remaining personal hub.
///
/// Slot #2 depends on the build: `翻译` when the `translate` feature is
/// compiled in, otherwise `词库`. Without the feature the translate page is
/// only a「当前构建未包含翻译功能」placeholder (mobile builds default to off —
/// see `FEATURES=""` in scripts/package_ios.sh / deploy_ios_device.sh), so it
/// must not take a prime tab. `词库` is never orphaned either way: the Study
/// page links straight to it and it also stays in the 巩固 hub.
const TABS_WITH_TRANSLATE: [(Page, &str, &str); 5] = [
    (Page::Dictionary, "词典", "book"),
    (Page::Translate, "翻译", "translate"),
    (Page::Study, "学习", "cap"),
    (Page::Consolidate, "巩固", "heart"),
    (Page::Mine, "我的", "user"),
];
const TABS_WITHOUT_TRANSLATE: [(Page, &str, &str); 5] = [
    (Page::Dictionary, "词典", "book"),
    (Page::Categories, "词库", "layers"),
    (Page::Study, "学习", "cap"),
    (Page::Consolidate, "巩固", "heart"),
    (Page::Mine, "我的", "user"),
];

fn tabs() -> &'static [(Page, &'static str, &'static str)] {
    if cfg!(feature = "translate") {
        &TABS_WITH_TRANSLATE
    } else {
        &TABS_WITHOUT_TRANSLATE
    }
}

/// Pages grouped under the "巩固" hub. Kept here so the tab stays lit while any
/// of them is open. `翻译` is no longer one of them: it moved up to a tab (in
/// feature builds), while `词库` came the other way.
const CONSOLIDATE_PAGES: [(Page, &str); 5] = [
    (Page::Categories, "词库"),
    (Page::Wordbook, "生词本"),
    (Page::Review, "背诵"),
    (Page::Quiz, "测验"),
    (Page::Roots, "词根"),
];

/// Pages grouped under the "我的" hub; same tab-lit purpose.
const MINE_PAGES: [(Page, &str); 2] = [(Page::Stats, "统计"), (Page::Settings, "设置")];

/// Whether the "巩固" tab should light up while `current` is open: the hub
/// itself, or any grouped page — except one that already owns a tab. Without
/// the `translate` feature `词库` is both a tab and a hub entry, and lighting
/// both would leave two tabs active at once.
fn consolidate_tab_active(current: Page) -> bool {
    if current == Page::Consolidate {
        return true;
    }
    let owns_a_tab = tabs().iter().any(|(p, _, _)| *p == current);
    !owns_a_tab && CONSOLIDATE_PAGES.iter().any(|(p, _)| *p == current)
}

/// Same for the "我的" hub.
fn mine_tab_active(current: Page) -> bool {
    current == Page::Mine || MINE_PAGES.iter().any(|(p, _)| *p == current)
}

/// Fixed bottom tab bar shown on narrow viewports (see the mobile media query).
#[component]
pub fn MobileTabBar() -> Element {
    let mut page: Signal<Page> = use_context();
    let current = page();
    let tabs = tabs();

    rsx! {
        div { class: "mobile-tabbar",
            nav { class: "mobile-tabbar-bar",
                for (i, (tab, label, icon)) in tabs.iter().enumerate() {
                    {
                        let active = match tab {
                            Page::Consolidate => consolidate_tab_active(current),
                            Page::Mine => mine_tab_active(current),
                            _ => current == *tab,
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
                // Lucide "languages": the 文/A glyph pair reads as translation at 24px.
                "translate" => rsx! {
                    path { d: "m5 8 6 6" }
                    path { d: "m4 14 6-6 2-3" }
                    path { d: "M2 5h12" }
                    path { d: "M7 2h1" }
                    path { d: "m22 22-5-10-5 10" }
                    path { d: "M14 18h6" }
                },
                _ => rsx! {
                    path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                    circle { cx: "12", cy: "7", r: "4" }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(tabs: &[(Page, &str, &str)]) -> Vec<Page> {
        tabs.iter().map(|(p, _, _)| *p).collect()
    }

    /// The 5-slot mobile bar is a tight budget: swapping 词库 and 翻译 in and out
    /// of slot #2 must never drop a fixed tab, double one up, or desync a label
    /// from `Page::label()`.
    #[test]
    fn mobile_tabs_keep_five_slots_when_translation_takes_the_wordbook_slot() {
        for variant in [
            &TABS_WITH_TRANSLATE as &[_],
            &TABS_WITHOUT_TRANSLATE as &[_],
        ] {
            let ps = pages(variant);
            assert_eq!(ps.len(), 5);
            for p in ps.iter() {
                assert_eq!(ps.iter().filter(|q| q == &p).count(), 1, "tab 重复: {ps:?}");
            }
            for must in [Page::Dictionary, Page::Study, Page::Consolidate, Page::Mine] {
                assert!(ps.contains(&must), "固定 tab 丢失: {must:?}");
            }
            for (page, label, _) in variant {
                assert_eq!(*label, page.label(), "tab 文案与 Page::label() 漂移");
            }
        }
        let with = pages(&TABS_WITH_TRANSLATE);
        let without = pages(&TABS_WITHOUT_TRANSLATE);
        assert_eq!(with[1], Page::Translate);
        assert_eq!(without[1], Page::Categories);
        assert!(!with.contains(&Page::Categories));
        assert!(!without.contains(&Page::Translate));
        let expected = if cfg!(feature = "translate") {
            with
        } else {
            without
        };
        assert_eq!(pages(tabs()), expected);
    }

    /// Opening a page must light exactly one tab: 词库 lives in the 巩固 hub in
    /// feature builds but *is* a tab in the default mobile build.
    #[test]
    fn hub_pages_light_only_their_own_tab() {
        let categories_is_tab = tabs().iter().any(|(p, _, _)| *p == Page::Categories);
        assert_eq!(consolidate_tab_active(Page::Categories), !categories_is_tab);
        assert!(consolidate_tab_active(Page::Consolidate));
        assert!(consolidate_tab_active(Page::Wordbook));
        // 翻译升到 tab 后，它不再属于巩固 hub（否则两个 tab 同时亮）。
        assert!(!CONSOLIDATE_PAGES.iter().any(|(p, _)| *p == Page::Translate));
        assert!(!consolidate_tab_active(Page::Translate));
        assert!(!mine_tab_active(Page::Translate));
        assert!(mine_tab_active(Page::Settings));
    }
}
