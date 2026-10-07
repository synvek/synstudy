use dioxus::prelude::*;

use crate::components::word_card::WordCard;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Category, ProgressStatus, WordProgress};
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
pub fn Study() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let mut phase = use_signal(|| Phase::Idle);
    let mut queue = use_signal(|| Vec::<i64>::new());
    let mut idx = use_signal(|| 0usize);
    let mut revealed = use_signal(|| false);

    let cat = match selected() {
        Some(c) => c,
        None => return rsx! { PickCategory { mode: "学习" } },
    };

    // --- Start-of-session handler -------------------------------------------
    let start_session = {
        let dict = dict.clone();
        let progress = progress.clone();
        let cat_id = cat.id;
        move |_| {
            let reverse = settings().reverse;
            let per = settings().words_per_session.max(1);
            let all = dict.category_word_ids(cat_id, reverse);
            let pmap = progress.map_for_category(cat_id);
            let new_ids: Vec<i64> = all
                .into_iter()
                .filter(|id| {
                    pmap.get(id)
                        .map(|p| p.status == ProgressStatus::New)
                        .unwrap_or(true)
                })
                .take(per)
                .collect();
            let n = new_ids.len();
            queue.set(new_ids);
            idx.set(0);
            revealed.set(false);
            phase.set(if n == 0 { Phase::Done } else { Phase::Running });
        }
    };

    match phase() {
        Phase::Idle => {
            let remaining = {
                let all = dict.category_word_ids(cat.id, false);
                let pmap = progress.map_for_category(cat.id);
                all.iter()
                    .filter(|id| {
                        pmap.get(id)
                            .map(|p| p.status == ProgressStatus::New)
                            .unwrap_or(true)
                    })
                    .count()
            };
            rsx! {
                div { class: "page study",
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-start",
                        h2 { "{cat.name}" }
                        p { class: "muted", "共 {cat.word_count} 词 · 未学 {remaining} 词" }
                        p { class: "muted", "每轮学习 {settings().words_per_session} 个新词" }
                        button { class: "btn primary big", onclick: start_session, "开始学习" }
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

            let answer = {
                let progress = progress.clone();
                let cat_id = cat.id;
                move |known: bool| {
                    if let Some(wid) = current_id {
                        let rating = if known { Rating::Good } else { Rating::Again };
                        let mut p = progress
                            .get(wid, cat_id)
                            .unwrap_or_else(|| WordProgress::new(wid, cat_id));
                        let now = now_ts();
                        srs::review(&mut p, rating, now);
                        progress.upsert(&p);
                        progress.log_review(wid, cat_id, rating.code(), now);
                    }
                    if i + 1 >= len {
                        phase.set(Phase::Done);
                    } else {
                        idx.set(i + 1);
                        revealed.set(false);
                    }
                }
            };

            rsx! {
                div { class: "page study",
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
                            div { class: "answer-row",
                                button {
                                    class: "btn danger",
                                    onclick: {
                                        let mut answer = answer.clone();
                                        move |_| answer(false)
                                    },
                                    "不认识"
                                }
                                button {
                                    class: "btn success",
                                    onclick: {
                                        let mut answer = answer.clone();
                                        move |_| answer(true)
                                    },
                                    "认识"
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
                div { class: "page study",
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-done",
                        h2 { "本组学习完成" }
                        p { class: "muted", "已学习的单词会进入背诵队列，按时复习记得更牢。" }
                        div { class: "answer-row",
                            button {
                                class: "btn",
                                onclick: move |_| phase.set(Phase::Idle),
                                "再学一组"
                            }
                            button {
                                class: "btn primary",
                                onclick: move |_| page.set(Page::Review),
                                "去背诵"
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
