use dioxus::prelude::*;

use crate::components::text_lines;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Category, WordProgress};
use crate::srs::{self, Rating};
use crate::state::{now_ts, Page, QuizMode, Settings};
use crate::tts;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Running,
    Done,
}

/// Where the quiz pulls its words from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// The currently selected category (needs a selection).
    Category,
    /// The wordbook / starred words (no selection needed).
    Starred,
    /// Weak words aggregated from lapses + `Again` ratings (needs a selection).
    Weak,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Category => "当前词库",
            Source::Starred => "生词本",
            Source::Weak => "错词本",
        }
    }

    fn needs_category(self) -> bool {
        matches!(self, Source::Category | Source::Weak)
    }
}

#[component]
pub fn Quiz() -> Element {
    // Dispatch to the sentence-bank quiz when the user picked one of the
    // sentence modes; otherwise run the classic spelling flow below.
    let settings: Signal<Settings> = use_context();
    if settings().quiz_mode.is_sentence_mode() {
        return rsx! { crate::pages::sentence_quiz::SentenceQuiz {} };
    }
    rsx! { SpellingQuiz {} }
}

#[component]
fn SpellingQuiz() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let mut phase = use_signal(|| Phase::Idle);
    let mut source = use_signal(|| Source::Category);
    let mut queue = use_signal(|| Vec::<i64>::new());
    let mut idx = use_signal(|| 0usize);
    let mut input = use_signal(String::new);
    // None = unanswered; Some(true/false) = graded.
    let mut checked = use_signal(|| Option::<bool>::None);
    let mut correct_count = use_signal(|| 0usize);

    // Auto-pronounce the current word in 听音拼写 mode whenever the question
    // changes. Reads `idx`/`phase` so it re-fires on advance.
    let dict_tts = dict.clone();
    use_effect(move || {
        if phase() != Phase::Running {
            return;
        }
        let s = settings();
        if s.quiz_mode != QuizMode::FromSound || !s.quiz_auto_tts || !s.tts_enabled {
            return;
        }
        let i = idx();
        if let Some(wid) = queue().get(i).copied() {
            if let Some(w) = dict_tts.get_by_id(wid) {
                tts::speak(&w.word, s.tts_rate);
            }
        }
    });

    let start_session = {
        let dict = dict.clone();
        let progress = progress.clone();
        move || {
            let s = settings();
            let per = s.words_per_session.max(1);
            let cat_id = selected().map(|c| c.id);
            let ids: Vec<i64> = match source() {
                Source::Starred => progress.list_starred(),
                Source::Weak => cat_id
                    .map(|c| progress.weak_word_ids(c, 200))
                    .unwrap_or_default(),
                Source::Category => cat_id
                    .map(|c| dict.category_word_ids(c, s.reverse))
                    .unwrap_or_default(),
            };
            let ids: Vec<i64> = ids.into_iter().take(per).collect();
            let n = ids.len();
            queue.set(ids);
            idx.set(0);
            input.set(String::new());
            checked.set(None);
            correct_count.set(0);
            phase.set(if n == 0 { Phase::Done } else { Phase::Running });
        }
    };

    let submit = {
        let dict = dict.clone();
        let progress = progress.clone();
        move || {
            let i = idx();
            let Some(wid) = queue().get(i).copied() else {
                return;
            };
            let Some(w) = dict.get_by_id(wid) else {
                return;
            };
            let guess = input().trim().to_lowercase();
            let answer = w.word.trim().to_lowercase();
            let correct = !guess.is_empty() && guess == answer;
            checked.set(Some(correct));
            if correct {
                correct_count += 1;
            } else if settings().quiz_record_miss {
                // Fold the miss into 错词本 by logging an `Again` against the
                // selected category (skipped when no category is chosen).
                if let Some(cat_id) = selected().map(|c| c.id) {
                    let now = now_ts();
                    let mut p = progress
                        .get(wid, cat_id)
                        .unwrap_or_else(|| WordProgress::new(wid, cat_id));
                    srs::schedule(&mut p, Rating::Again, now, settings().scheduler);
                    progress.upsert(&p);
                    progress.log_review(wid, cat_id, Rating::Again.code(), now);
                }
            }
        }
    };

    let advance = move || {
        let i = idx();
        let len = queue().len();
        if i + 1 >= len {
            phase.set(Phase::Done);
        } else {
            idx.set(i + 1);
            input.set(String::new());
            checked.set(None);
        }
    };

    match phase() {
        Phase::Idle => {
            let cat_opt = selected();
            let cat_id = cat_opt.as_ref().map(|c| c.id);
            let src = source();
            let preview: usize = match src {
                Source::Starred => progress.list_starred().len(),
                Source::Weak => cat_id
                    .map(|c| progress.weak_word_ids(c, 200).len())
                    .unwrap_or(0),
                Source::Category => cat_opt
                    .as_ref()
                    .map(|c| c.word_count.max(0) as usize)
                    .unwrap_or(0),
            };
            let missing_cat = src.needs_category() && cat_id.is_none();
            rsx! {
                div { class: "page quiz",
                    QuizHeader { page }
                    div { class: "session-start",
                        h2 { "拼写测验" }
                        div { class: "seg",
                            for s in [Source::Category, Source::Starred, Source::Weak] {
                                button {
                                    key: "{s:?}",
                                    class: if src == s { "seg-btn active" } else { "seg-btn" },
                                    onclick: move |_| source.set(s),
                                    "{s.label()}"
                                }
                            }
                        }
                        if missing_cat {
                            p { class: "muted", "该测验范围需要先选择一个词库。" }
                            button {
                                class: "btn primary big",
                                onclick: move |_| page.set(Page::Categories),
                                "去选择词库"
                            }
                        } else {
                            p { class: "muted",
                                "范围：{src.label()} · 每轮 {settings().words_per_session} 题 · 模式：{settings().quiz_mode.label()}"
                            }
                            button {
                                class: "btn primary big",
                                disabled: preview == 0,
                                onclick: {
                                    let mut start = start_session.clone();
                                    move |_| start()
                                },
                                "开始测验"
                            }
                            if preview == 0 {
                                p { class: "muted hint", "该范围暂无单词。" }
                            } else {
                                p { class: "muted hint", "输入单词后回车提交，再次回车进入下一题。" }
                            }
                        }
                    }
                }
            }
        }
        Phase::Running => {
            let i = idx();
            let len = queue().len();
            let answered = checked();
            let cur = queue().get(i).copied().and_then(|id| dict.get_by_id(id));
            let s = settings();

            let onkey = {
                let mut submit = submit.clone();
                let mut advance = advance.clone();
                move |evt: Event<KeyboardData>| {
                    if matches!(evt.code(), Code::Enter | Code::NumpadEnter) {
                        evt.prevent_default();
                        if checked().is_none() {
                            submit();
                        } else {
                            advance();
                        }
                    }
                }
            };

            rsx! {
                div { class: "page quiz",
                    QuizHeader { page }
                    div { class: "session-progress", "{i + 1} / {len} · 正确 {correct_count()}" }
                    match cur {
                        Some(w) => {
                            let play_word = w.word.clone();
                            let rate = s.tts_rate;
                            rsx! {
                                div { class: "card quiz-card",
                                    if s.quiz_mode == QuizMode::FromSound {
                                        button {
                                            class: "btn primary quiz-play",
                                            onclick: move |_| tts::speak(&play_word, rate),
                                            "\u{1f50a} 播放读音"
                                        }
                                        if answered.is_none() && !w.phonetic.is_empty() {
                                            span { class: "card-phonetic", "{w.phonetic}" }
                                        }
                                    } else {
                                        div { class: "card-trans", {text_lines(&w.translation, "line")} }
                                        if !w.phonetic.is_empty() {
                                            span { class: "card-phonetic", "{w.phonetic}" }
                                        }
                                    }

                                    if let Some(ok) = answered {
                                        div { class: if ok { "quiz-fb ok" } else { "quiz-fb bad" },
                                            if ok { "\u{2713} 正确：{w.word}" } else { "\u{2717} 正确答案：{w.word}" }
                                        }
                                        div { class: "card-trans quiz-reveal",
                                            {text_lines(&w.translation, "line")}
                                        }
                                    }
                                }
                                div { class: "quiz-input-row",
                                    input {
                                        r#type: "text",
                                        class: "quiz-input",
                                        placeholder: "输入英文单词，回车提交",
                                        value: "{input}",
                                        readonly: answered.is_some(),
                                        autofocus: true,
                                        oninput: move |e| input.set(e.value()),
                                        onkeydown: onkey,
                                    }
                                    if answered.is_none() {
                                        button {
                                            class: "btn primary",
                                            onclick: {
                                                let mut submit = submit.clone();
                                                move |_| submit()
                                            },
                                            "提交"
                                        }
                                    } else {
                                        button {
                                            class: "btn primary",
                                            onclick: {
                                                let mut advance = advance.clone();
                                                move |_| advance()
                                            },
                                            "下一题"
                                        }
                                    }
                                }
                            }
                        }
                        None => rsx! { p { class: "muted", "无更多单词" } }
                    }
                }
            }
        }
        Phase::Done => {
            let len = queue().len();
            let got = correct_count();
            rsx! {
                div { class: "page quiz",
                    QuizHeader { page }
                    div { class: "session-done",
                        h2 { "测验完成" }
                        p { class: "muted", "答对 {got} / {len} 题。" }
                        div { class: "answer-row",
                            button {
                                class: "btn",
                                onclick: move |_| phase.set(Phase::Idle),
                                "再来一轮"
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
fn QuizHeader(page: Signal<Page>) -> Element {
    let mut page = page;
    rsx! {
        div { class: "session-header",
            button { class: "btn ghost", onclick: move |_| page.set(Page::Categories), "← 词库" }
            span { class: "session-title", "拼写测验" }
        }
    }
}
