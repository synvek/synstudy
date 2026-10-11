use dioxus::prelude::*;

use crate::db::progress_repo::ProgressRepo;
use crate::state::{now_ts, Page};

/// One entry in the "我的" hub card list.
struct MineLink {
    page: Page,
    icon: &'static str,
    title: &'static str,
    desc: &'static str,
}

/// Secondary destinations, grouped like a typical app "我的" screen.
const LINKS: [MineLink; 5] = [
    MineLink {
        page: Page::Review,
        icon: "\u{1f501}",
        title: "背诵",
        desc: "按记忆曲线复习已学单词",
    },
    MineLink {
        page: Page::Quiz,
        icon: "\u{270f}\u{fe0f}",
        title: "测验",
        desc: "拼写 / 选词填空 / 听音辨词",
    },
    MineLink {
        page: Page::Roots,
        icon: "\u{1f331}",
        title: "词根",
        desc: "词根词缀串联扩展词汇",
    },
    MineLink {
        page: Page::Stats,
        icon: "\u{1f4ca}",
        title: "统计",
        desc: "学习轨迹、连续天数与记忆保持率",
    },
    MineLink {
        page: Page::Settings,
        icon: "\u{2699}\u{fe0f}",
        title: "设置",
        desc: "主题、发音、复习算法与数据重置",
    },
];

/// Mobile "我的" tab: a card-list hub (profile summary + secondary links),
/// replacing the former popup menu with a conventional page.
#[component]
pub fn Mine() -> Element {
    let mut page: Signal<Page> = use_context();
    let progress: ProgressRepo = use_context();
    let (current_streak, longest_streak) = progress.streaks(now_ts());

    rsx! {
        div { class: "page mine",
            div { class: "mine-head",
                div { class: "mine-avatar", "s" }
                div { class: "mine-id",
                    h1 { "我的学习" }
                    span { class: "muted", "synstudy · 离线英语学习" }
                }
            }

            div { class: "mine-stats",
                div { class: "mine-stat",
                    span { class: "mine-stat-num", "{current_streak}" }
                    span { class: "mine-stat-lbl", "连续天数" }
                }
                div { class: "mine-stat",
                    span { class: "mine-stat-num", "{longest_streak}" }
                    span { class: "mine-stat-lbl", "最长连续" }
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
