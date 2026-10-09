use dioxus::prelude::*;

use crate::components::word_card::WordCard;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Category, ProgressStatus, WordProgress};
use crate::pages::PickCategory;
use crate::srs::{self, Rating};
use crate::state::{now_ts, Page, Settings};
use crate::tts;

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

    // Auto-pronounce the current word when its card is revealed (opt-in).
    let dict_tts = dict.clone();
    use_effect(move || {
        if !revealed() {
            return;
        }
        let s = settings();
        if !s.tts_enabled || !s.tts_autoplay {
            return;
        }
        let i = idx();
        if let Some(wid) = queue().get(i).copied() {
            if let Some(w) = dict_tts.get_by_id(wid) {
                tts::speak(&w.word, s.tts_rate);
            }
        }
    });

    let cat = match selected() {
        Some(c) => c,
        None => return rsx! { PickCategory { mode: "学习" } },
    };

    // --- Start-of-session handler -------------------------------------------
    let start_session = {
        let dict = dict.clone();
        let progress = progress.clone();
        let cat_id = cat.id;
        move || {
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
            let onkeydown = {
                let mut start = start_session.clone();
                move |evt: Event<KeyboardData>| {
                    if matches!(evt.code(), Code::Enter | Code::NumpadEnter | Code::Space) {
                        evt.prevent_default();
                        start();
                    }
                }
            };
            rsx! {
                div { class: "page study", tabindex: "-1", autofocus: true, onkeydown,
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-start",
                        h2 { "{cat.name}" }
                        p { class: "muted", "共 {cat.word_count} 词 · 未学 {remaining} 词" }
                        p { class: "muted", "每轮学习 {settings().words_per_session} 个新词" }
                        button {
                            class: "btn primary big",
                            onclick: {
                                let mut start = start_session.clone();
                                move |_| start()
                            },
                            "开始学习"
                        }
                        p { class: "muted hint", "快捷键：Enter / 空格 开始" }
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
                let scheduler = settings().scheduler;
                move |known: bool| {
                    if let Some(wid) = current_id {
                        let rating = if known { Rating::Good } else { Rating::Again };
                        let mut p = progress
                            .get(wid, cat_id)
                            .unwrap_or_else(|| WordProgress::new(wid, cat_id));
                        let now = now_ts();
                        srs::schedule(&mut p, rating, now, scheduler);
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

            let onkeydown = {
                let key_answer = answer.clone();
                move |evt: Event<KeyboardData>| {
                    match evt.code() {
                        Code::Space => {
                            evt.prevent_default();
                            let cur = revealed();
                            revealed.set(!cur);
                        }
                        Code::Enter | Code::NumpadEnter => {
                            evt.prevent_default();
                            let mut a = key_answer.clone();
                            a(true);
                        }
                        Code::Backspace => {
                            evt.prevent_default();
                            let mut a = key_answer.clone();
                            a(false);
                        }
                        _ => {}
                    }
                }
            };

            rsx! {
                div { class: "page study", tabindex: "-1", autofocus: true, onkeydown,
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
