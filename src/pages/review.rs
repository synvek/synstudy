use dioxus::prelude::*;

use crate::components::word_card::WordCard;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Category, WordProgress};
use crate::pages::PickCategory;
use crate::srs::{self, Rating};
use crate::state::{now_ts, Page, Settings};

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Running,
    Done,
}

#[component]
pub fn Review() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let _settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let mut phase = use_signal(|| Phase::Idle);
    let mut queue = use_signal(|| Vec::<i64>::new());
    let mut idx = use_signal(|| 0usize);
    let mut revealed = use_signal(|| false);

    let cat = match selected() {
        Some(c) => c,
        None => return rsx! { PickCategory { mode: "背诵" } },
    };

    let start_session = {
        let progress = progress.clone();
        let cat_id = cat.id;
        move |_| {
            let now = now_ts();
            let ids = progress.due_word_ids(cat_id, now);
            let n = ids.len();
            queue.set(ids);
            idx.set(0);
            revealed.set(false);
            phase.set(if n == 0 { Phase::Done } else { Phase::Running });
        }
    };

    match phase() {
        Phase::Idle => {
            let due = progress.due_word_ids(cat.id, now_ts()).len();
            rsx! {
                div { class: "page review",
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-start",
                        h2 { "{cat.name} · 背诵" }
                        p { class: "muted", "根据间隔重复算法，当前有 {due} 个单词待复习。" }
                        button {
                            class: "btn primary big",
                            disabled: due == 0,
                            onclick: start_session,
                            "开始背诵"
                        }
                        if due == 0 {
                            p { class: "muted", "暂无到期单词，先去「学习」认识一些新词吧。" }
                        }
                    }
                }
            }
        }
        Phase::Running => {
            let ids = queue();
            let i = idx();
            let len = ids.len();
            let current_id = ids.get(i).copied();
            let word = current_id.and_then(|id| dict.get_by_id(id));

            let rate = {
                let progress = progress.clone();
                let cat_id = cat.id;
                move |rating: Rating| {
                    if let Some(wid) = current_id {
                        let mut p = progress
                            .get(wid, cat_id)
                            .unwrap_or_else(|| WordProgress::new(wid, cat_id));
                        let now = now_ts();
                        srs::review(&mut p, rating, now);
                        progress.upsert(&p);
                        progress.log_review(wid, cat_id, rating.code(), now);
                        if rating == Rating::Again {
                            // Give the word another chance later in this session.
                            queue.write().push(wid);
                        }
                    }
                    let total = queue.read().len();
                    if i + 1 >= total {
                        phase.set(Phase::Done);
                    } else {
                        idx.set(i + 1);
                        revealed.set(false);
                    }
                }
            };

            rsx! {
                div { class: "page review",
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-progress", "{i + 1} / {len}" }
                    match word {
                        Some(w) => rsx! {
                            WordCard {
                                key: "{w.id}",
                                word: w,
                                revealed: revealed(),
                                on_flip: move |_| {
                                    let cur = revealed();
                                    revealed.set(!cur);
                                }
                            }
                            div { class: "rating-row",
                                button {
                                    class: "btn rate-again",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Again)
                                    },
                                    "{Rating::Again.label()}"
                                }
                                button {
                                    class: "btn rate-hard",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Hard)
                                    },
                                    "{Rating::Hard.label()}"
                                }
                                button {
                                    class: "btn rate-good",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Good)
                                    },
                                    "{Rating::Good.label()}"
                                }
                                button {
                                    class: "btn rate-easy",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Easy)
                                    },
                                    "{Rating::Easy.label()}"
                                }
                            }
                        },
                        None => rsx! { p { class: "muted", "无更多单词" } }
                    }
                }
            }
        }
        Phase::Done => {
            rsx! {
                div { class: "page review",
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-done",
                        h2 { "本轮背诵完成" }
                        p { class: "muted", "单词已按记忆曲线安排下次复习时间。" }
                        div { class: "answer-row",
                            button {
                                class: "btn",
                                onclick: move |_| phase.set(Phase::Idle),
                                "返回"
                            }
                            button {
                                class: "btn primary",
                                onclick: move |_| page.set(Page::Stats),
                                "查看统计"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionHeader(name: String, page: Signal<Page>) -> Element {
    let mut page = page;
    rsx! {
        div { class: "session-header",
            button { class: "btn ghost", onclick: move |_| page.set(Page::Categories), "← 词库" }
            span { class: "session-title", "{name}" }
        }
    }
}
