use dioxus::prelude::*;

use crate::components::word_card::WordCard;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Category, WordProgress};
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
pub fn Review() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let mut phase = use_signal(|| Phase::Idle);
    let mut queue = use_signal(|| Vec::<i64>::new());
    let mut idx = use_signal(|| 0usize);
    let mut revealed = use_signal(|| false);
    // Whether the running session is a "错词强化" (weak-word) queue rather than
    // the normal due queue. Drives the on-screen labels.
    let mut weak_mode = use_signal(|| false);

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
        None => return rsx! { PickCategory { mode: "背诵" } },
    };

    let start_session = {
        let progress = progress.clone();
        let cat_id = cat.id;
        move |weak: bool| {
            let ids = if weak {
                progress.weak_word_ids(cat_id, 200)
            } else {
                progress.due_word_ids(cat_id, now_ts())
            };
            let n = ids.len();
            queue.set(ids);
            idx.set(0);
            revealed.set(false);
            weak_mode.set(weak);
            phase.set(if n == 0 { Phase::Done } else { Phase::Running });
        }
    };

    match phase() {
        Phase::Idle => {
            let due = progress.due_word_ids(cat.id, now_ts()).len();
            let weak_count = progress.weak_word_ids(cat.id, 200).len();
            let onkeydown = {
                let mut start = start_session.clone();
                let can_start = due > 0;
                move |evt: Event<KeyboardData>| {
                    if can_start && matches!(evt.code(), Code::Enter | Code::NumpadEnter | Code::Space) {
                        evt.prevent_default();
                        start(false);
                    }
                }
            };
            rsx! {
                div { class: "page review", tabindex: "-1", autofocus: true, onkeydown,
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-start",
                        h2 { "{cat.name} · 背诵" }
                        p { class: "muted", "根据间隔重复算法，当前有 {due} 个单词待复习。" }
                        button {
                            class: "btn primary big",
                            disabled: due == 0,
                            onclick: {
                                let mut start = start_session.clone();
                                move |_| start(false)
                            },
                            "开始背诵"
                        }
                        if weak_count > 0 {
                            button {
                                class: "btn big",
                                onclick: {
                                    let mut start = start_session.clone();
                                    move |_| start(true)
                                },
                                "错词强化（{weak_count}）"
                            }
                            p { class: "muted hint", "错词强化：集中复习曾经忘记或答错的单词。" }
                        }
                        if due == 0 {
                            p { class: "muted hint", "暂无到期单词，先去「学习」认识一些新词吧。" }
                        } else {
                            p { class: "muted hint desktop-only", "快捷键：Enter / 空格 开始" }
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
                let scheduler = settings().scheduler;
                move |rating: Rating| {
                    if let Some(wid) = current_id {
                        let mut p = progress
                            .get(wid, cat_id)
                            .unwrap_or_else(|| WordProgress::new(wid, cat_id));
                        let now = now_ts();
                        srs::schedule(&mut p, rating, now, scheduler);
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

            let onkeydown = {
                let key_rate = rate.clone();
                move |evt: Event<KeyboardData>| {
                    if evt.code() == Code::Space {
                        evt.prevent_default();
                        let cur = revealed();
                        revealed.set(!cur);
                        return;
                    }
                    let digit = match evt.code() {
                        Code::Digit1 | Code::Numpad1 => "1",
                        Code::Digit2 | Code::Numpad2 => "2",
                        Code::Digit3 | Code::Numpad3 => "3",
                        Code::Digit4 | Code::Numpad4 => "4",
                        _ => "",
                    };
                    if digit.is_empty() {
                        return;
                    }
                    if let Some(rating) = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy]
                        .into_iter()
                        .find(|r| r.key_hint() == digit)
                    {
                        evt.prevent_default();
                        let mut rr = key_rate.clone();
                        rr(rating);
                    }
                }
            };

            rsx! {
                div { class: "page review", tabindex: "-1", autofocus: true, onkeydown,
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-progress",
                        if weak_mode() { span { class: "chip", "错词强化" } " " }
                        "{i + 1} / {len}"
                    }
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
                                    "{Rating::Again.label()} ({Rating::Again.key_hint()})"
                                }
                                button {
                                    class: "btn rate-hard",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Hard)
                                    },
                                    "{Rating::Hard.label()} ({Rating::Hard.key_hint()})"
                                }
                                button {
                                    class: "btn rate-good",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Good)
                                    },
                                    "{Rating::Good.label()} ({Rating::Good.key_hint()})"
                                }
                                button {
                                    class: "btn rate-easy",
                                    onclick: {
                                        let mut rate = rate.clone();
                                        move |_| rate(Rating::Easy)
                                    },
                                    "{Rating::Easy.label()} ({Rating::Easy.key_hint()})"
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
                        h2 { if weak_mode() { "错词强化完成" } else { "本轮背诵完成" } }
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
