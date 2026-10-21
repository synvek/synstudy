use dioxus::prelude::*;

use crate::state::Page;

/// One entry in the "巩固" hub card list.
struct ConsolidateLink {
    page: Page,
    icon: &'static str,
    title: &'static str,
    desc: &'static str,
}

/// The consolidation flows gathered under the mobile "巩固" tab: the word-list
/// picker, the saved-word book and the three practice modes (recitation /
/// quiz / roots). `翻译` used to sit here too; in `translate` builds it now owns
/// a tab of its own (see `TABS_WITH_TRANSLATE`), and `词库` took its old slot.
const LINKS: [ConsolidateLink; 5] = [
    ConsolidateLink {
        page: Page::Categories,
        icon: "\u{1f4da}",
        title: "词库",
        desc: "选择或切换要学的词库",
    },
    ConsolidateLink {
        page: Page::Wordbook,
        icon: "\u{1f516}",
        title: "生词本",
        desc: "已收藏单词的浏览与复习",
    },
    ConsolidateLink {
        page: Page::Review,
        icon: "\u{1f501}",
        title: "背诵",
        desc: "按记忆曲线复习已学单词",
    },
    ConsolidateLink {
        page: Page::Quiz,
        icon: "\u{270f}\u{fe0f}",
        title: "测验",
        desc: "拼写 / 选词填空 / 听音辨词",
    },
    ConsolidateLink {
        page: Page::Roots,
        icon: "\u{1f331}",
        title: "词根",
        desc: "词根词缀串联扩展词汇",
    },
];

/// Mobile "巩固" tab: a card-list hub gathering 词库/生词本/背诵/测验/词根, so
/// these study-and-reinforce flows live together instead of under "我的".
#[component]
pub fn Consolidate() -> Element {
    let mut page: Signal<Page> = use_context();

    rsx! {
        div { class: "page mine consolidate",
            div { class: "mine-head",
                div { class: "mine-id",
                    h1 { "巩固" }
                    span { class: "muted", "选词、收藏与练习，把学过的词真正记住" }
                }
            }

            div { class: "mine-group",
                for l in LINKS.iter() {
                    button {
                        key: "{l.title}",
                        class: "mine-card",
                        onclick: {
                            let target = l.page;
                            move |_| page.set(target)
                        },
                        span { class: "mine-card-ico", "{l.icon}" }
                        span { class: "mine-card-text",
                            span { class: "mine-card-title", "{l.title}" }
                            span { class: "mine-card-desc", "{l.desc}" }
                        }
                        span { class: "mine-card-chevron", "\u{203a}" }
                    }
                }
            }
        }
    }
}
