//! Sentence-bank quiz modes: cloze, sentence translation, and listening.
//!
//! These modes draw questions from the graded `sentence_bank` table (Tatoeba +
//! Wiktionary, both open-licensed) rather than from the plain word list. Every
//! sentence is tagged with the exam syllabi whose vocabulary it contains, so
//! users can practise 四级/六级/专四/专八/托福/GRE-flavoured material without the
//! app ever redistributing copyrighted 真题 content.

use dioxus::prelude::*;

use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Sentence, WordProgress};
use crate::srs::{self, Rating};
use crate::state::{now_ts, ExamFilter, Page, QuizMode, Settings};
use crate::tts;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Running,
    Done,
}

/// One multiple-choice question built from a sentence-bank row.
///
/// `options` are the four answers presented to the user; `correct_index`
/// points into that vec. For `Cloze` and `ListenChoose` the options are
/// English word forms; for `SentenceTranslation` they are Chinese meanings.
#[derive(Clone, PartialEq)]
struct SentenceQuestion {
    sentence: Sentence,
    /// The word id being tested (a `is_target = 1` occurrence in the sentence).
    target_word_id: i64,
    /// Whitespace-token index of the target within `sentence.text`.
    target_token_index: i64,
    /// Raw token as it appears in the sentence (may include punctuation).
    target_token_text: String,
    options: Vec<String>,
    correct_index: usize,
}

/// Deterministic Fisher-Yates shuffle driven by an LCG so option ordering
/// varies per question without pulling in the `rand` crate. `seed` should be
/// unique per question (e.g. the sentence id).
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut s = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    for i in (1..items.len()).rev() {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let j = ((s >> 33) as usize) % (i + 1);
        items.swap(i, j);
    }
}

/// First non-empty line of a translation; used to keep the option list compact.
fn first_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Extract the alphabetic core of a raw token so we can look it up in the
/// dictionary (drops trailing punctuation like `,` / `.` / `'s`).
fn clean_token(tok: &str) -> String {
    tok.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase()
}

/// Build a multiple-choice question for `sentence` in `mode`. Returns `None`
/// when the sentence has no usable target or when distractors cannot be
/// produced (e.g. a nearly-empty word bank).
fn build_question(
    dict: &DictRepo,
    sentence: Sentence,
    mode: QuizMode,
    exam: ExamFilter,
) -> Option<SentenceQuestion> {
    let targets = dict.sentence_words(sentence.id, true);
    if targets.is_empty() {
        return None;
    }
    // Pick a target deterministically (sentence id) so a re-visit of the same
    // sentence shows the same question, but different sentences vary.
    let pick = targets[(sentence.id as usize) % targets.len()].clone();
    let target_word = dict.get_by_id(pick.word_id)?;
    let target_form = clean_token(&pick.token_text);
    if target_form.is_empty() {
        return None;
    }
    // For SentenceTranslation we need at least a one-line Chinese gloss.
    let target_zh = first_line(&target_word.translation);
    if mode == QuizMode::SentenceTranslation && target_zh.is_empty() {
        return None;
    }
    // Distractors come from the same exam syllabus so difficulty is comparable.
    let distractors = dict.random_exam_words(exam.key(), pick.word_id, 12);
    let mut options: Vec<String> = Vec::with_capacity(4);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    match mode {
        QuizMode::SentenceTranslation => {
            options.push(target_zh.clone());
            seen.insert(target_zh.clone());
            for w in distractors {
                let line = first_line(&w.translation);
                if line.is_empty() || seen.contains(&line) {
                    continue;
                }
                seen.insert(line.clone());
                options.push(line);
                if options.len() == 4 {
                    break;
                }
            }
        }
        _ => {
            // Cloze / ListenChoose: options are English word forms.
            options.push(target_word.word.clone());
            seen.insert(target_word.word.to_lowercase());
            for w in distractors {
                let key = w.word.to_lowercase();
                if seen.contains(&key) || key == target_form {
                    continue;
                }
                seen.insert(key);
                options.push(w.word.clone());
                if options.len() == 4 {
                    break;
                }
            }
        }
    }
    if options.len() < 2 {
        // Not enough vocabulary to build a meaningful distractor set.
        return None;
    }
    // Pad with placeholders if the syllabus is tiny (dev/sample data).
    while options.len() < 4 {
        let pad = format!("—{}", options.len());
        options.push(pad);
    }
    let correct = options
        .iter()
        .position(|o| {
            if mode == QuizMode::SentenceTranslation {
                o == &target_zh
            } else {
                o.eq_ignore_ascii_case(&target_word.word)
            }
        })
        .unwrap_or(0);
    let mut indexed: Vec<(usize, String)> = options.into_iter().enumerate().collect();
    shuffle(&mut indexed, sentence.id as u64 * 31 + pick.word_id as u64);
    let new_correct = indexed
        .iter()
        .position(|(i, _)| *i == correct)
        .unwrap_or(0);
    let options: Vec<String> = indexed.into_iter().map(|(_, s)| s).collect();
    Some(SentenceQuestion {
        sentence,
        target_word_id: pick.word_id,
        target_token_index: pick.token_index,
        target_token_text: pick.token_text,
        options,
        correct_index: new_correct,
    })
}

/// Reconstruct the sentence with the target token replaced by a blank, keeping
/// surrounding whitespace and punctuation intact.
fn blanked_sentence(text: &str, token_index: i64) -> String {
    let mut out = String::new();
    for (i, tok) in text.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if i as i64 == token_index {
            // Preserve trailing punctuation: "apple," -> "_____,"
            let cleaned: String = tok.chars().filter(|c| c.is_ascii_alphabetic()).collect();
            let rest = &tok[cleaned.len()..];
            out.push_str("_____");
            out.push_str(rest);
        } else {
            out.push_str(tok);
        }
    }
    out
}

#[component]
pub fn SentenceQuiz() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let settings: Signal<Settings> = use_context();
    let selected: Signal<Option<crate::models::Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let mut phase = use_signal(|| Phase::Idle);
    let mut queue = use_signal(|| Vec::<SentenceQuestion>::new());
    let mut idx = use_signal(|| 0usize);
    let mut picked = use_signal(|| Option::<usize>::None);
    let mut correct_count = use_signal(|| 0usize);

    // Auto-play the sentence in ListenChoose mode whenever the question flips.
    let dict_tts = dict.clone();
    let _ = &dict_tts;
    use_effect(move || {
        if phase() != Phase::Running {
            return;
        }
        let s = settings();
        if s.quiz_mode != QuizMode::ListenChoose || !s.quiz_auto_tts || !s.tts_enabled {
            return;
        }
        let i = idx();
        if let Some(q) = queue().get(i) {
            tts::speak(&q.sentence.text, s.tts_rate);
        }
    });

    let start_session = {
        let dict = dict.clone();
        move || {
            let s = settings();
            let per = s.words_per_session.max(1);
            let exam = s.quiz_exam_filter;
            let needs_zh = s.quiz_mode == QuizMode::SentenceTranslation;
            // Fetch a generous pool then filter to buildable questions; some
            // rows may lack a usable target or gloss.
            let pool = dict.random_sentences(exam.key(), needs_zh, (per * 3).max(30) as i64);
            let mut built: Vec<SentenceQuestion> = Vec::with_capacity(per);
            for sent in pool {
                if built.len() >= per {
                    break;
                }
                if let Some(q) = build_question(&dict, sent, s.quiz_mode, exam) {
                    built.push(q);
                }
            }
            let n = built.len();
            queue.set(built);
            idx.set(0);
            picked.set(None);
            correct_count.set(0);
            phase.set(if n == 0 { Phase::Done } else { Phase::Running });
        }
    };

    let submit_pick = {
        let progress = progress.clone();
        let dict = dict.clone();
        move |choice: usize| {
            let i = idx();
            let Some(q) = queue().get(i).cloned() else {
                return;
            };
            let ok = choice == q.correct_index;
            picked.set(Some(choice));
            if ok {
                correct_count += 1;
            } else if settings().quiz_record_miss {
                // Fold misses into 错词本 against the currently selected category
                // so sentence-quiz mistakes reinforce the SRS loop.
                if let Some(cat_id) = selected().map(|c| c.id) {
                    let now = now_ts();
                    let mut p = progress
                        .get(q.target_word_id, cat_id)
                        .unwrap_or_else(|| WordProgress::new(q.target_word_id, cat_id));
                    srs::schedule(&mut p, Rating::Again, now, settings().scheduler);
                    progress.upsert(&p);
                    progress.log_review(q.target_word_id, cat_id, Rating::Again.code(), now);
                }
            }
            let _ = &dict;
        }
    };

    let advance = move || {
        let i = idx();
        let len = queue().len();
        if i + 1 >= len {
            phase.set(Phase::Done);
        } else {
            idx.set(i + 1);
            picked.set(None);
        }
    };

    match phase() {
        Phase::Idle => {
            let s = settings();
            let mode = s.quiz_mode;
            let exam = s.quiz_exam_filter;
            let bank_total = dict.sentence_total();
            rsx! {
                div { class: "page quiz",
                    QuizHeader { page, title: mode.label() }
                    div { class: "session-start",
                        h2 { "{mode.label()}" }
                        p { class: "muted",
                            "范围：{exam.label()} · 每轮 {s.words_per_session} 题 · 句库共 {bank_total} 句"
                        }
                        if bank_total == 0 {
                            p { class: "warn",
                                "句库为空。请先运行 scripts/fetch_data.sh 下载 Tatoeba 数据，再执行 scripts/build_dict_db.py 重建数据库。"
                            }
                        } else {
                            button {
                                class: "btn primary big",
                                onclick: {
                                    let mut start = start_session.clone();
                                    move |_| start()
                                },
                                "开始测验"
                            }
                            p { class: "muted hint", {mode_hint(mode)} }
                        }
                        div { class: "answer-row",
                            button {
                                class: "btn ghost",
                                onclick: move |_| page.set(Page::Settings),
                                "切换题型 / 考试级别"
                            }
                        }
                    }
                }
            }
        }
        Phase::Running => {
            let i = idx();
            let len = queue().len();
            let picked_now = picked();
            let s = settings();
            let mode = s.quiz_mode;
            let cur = queue().get(i).cloned();
            let rate = s.tts_rate;
            let tts_on = s.tts_enabled;
            // Look up the target word outside the rsx! macro so the render body
            // only sees plain data (rsx! parses `{ ... }` as child content and
            // would reject a `let` statement inside an `if let` arm).
            let target_word = cur
                .as_ref()
                .and_then(|q| dict.get_by_id(q.target_word_id));
            // Pre-compute the post-answer feedback payload; rsx!'s `if let`
            // arm cannot host `let` statements, so everything the reveal block
            // needs must be bound before the macro.
            let reveal = cur.as_ref().and_then(|q| {
                picked_now.map(|p| {
                    let correct = p == q.correct_index;
                    let answer = q
                        .options
                        .get(q.correct_index)
                        .cloned()
                        .unwrap_or_default();
                    let token_for_tts = q.target_token_text.clone();
                    (correct, answer, token_for_tts)
                })
            });
            let target_word_ref = target_word.clone();

            rsx! {
                div { class: "page quiz",
                    QuizHeader { page, title: mode.label() }
                    div { class: "session-progress", "{i + 1} / {len} · 正确 {correct_count()}" }
                    match cur {
                        Some(q) => {
                            let sentence_for_audio = q.sentence.text.clone();
                            let labels = q.sentence.exam_labels();
                            let blanked = blanked_sentence(&q.sentence.text, q.target_token_index);
                            let show_text = mode != QuizMode::ListenChoose || picked_now.is_some();
                            let submit_pick = submit_pick.clone();
                            let mut advance = advance.clone();
                            rsx! {
                                div { class: "card quiz-card sentence-card",
                                    div { class: "sentence-meta",
                                        for l in labels.iter() {
                                            span { key: "{l}", class: "chip", "{l}" }
                                        }
                                        if !q.sentence.cefr_min.is_empty() {
                                            span { class: "chip chip-cefr", "CEFR {q.sentence.cefr_min}" }
                                        }
                                        span { class: "chip chip-src", "{q.sentence.source}" }
                                    }
                                    if mode == QuizMode::ListenChoose {
                                        button {
                                            class: "btn primary quiz-play",
                                            onclick: move |_| tts::speak(&sentence_for_audio, rate),
                                            "\u{1f50a} 播放句子"
                                        }
                                    }
                                    if show_text {
                                        if mode == QuizMode::Cloze {
                                            p { class: "sentence-text", "{blanked}" }
                                        } else {
                                            p { class: "sentence-text", "{q.sentence.text}" }
                                        }
                                    } else {
                                        p { class: "sentence-text muted", "（听音后选择正确答案）" }
                                    }
                                    if mode == QuizMode::SentenceTranslation && !q.sentence.translation.is_empty() {
                                        p { class: "sentence-zh muted", "参考译文：{q.sentence.translation}" }
                                    }
                                    div { class: "opt-list",
                                        {q.options.iter().enumerate().map(|(oi, opt)| {
                                            let is_correct = oi == q.correct_index;
                                            let is_picked = picked_now == Some(oi);
                                            let cls = match (picked_now, is_correct, is_picked) {
                                                (None, _, _) => "opt",
                                                (Some(_), true, _) => "opt ok",
                                                (Some(_), false, true) => "opt bad",
                                                (Some(_), false, false) => "opt dim",
                                            };
                                            let opt_clone = opt.clone();
                                            let mut sb = submit_pick.clone();
                                            rsx! {
                                                button {
                                                    key: "{oi}",
                                                    class: cls,
                                                    disabled: picked_now.is_some(),
                                                    onclick: move |_| sb(oi),
                                                    span { class: "opt-key", "{(b'A' + oi as u8) as char}" }
                                                    span { class: "opt-text", "{opt_clone}" }
                                                }
                                            }
                                        })}
                                    }
                                    if let Some((correct, answer, token_for_tts)) = reveal.clone() {
                                        div { class: if correct { "quiz-fb ok" } else { "quiz-fb bad" },
                                            if correct {
                                                "\u{2713} 正确"
                                            } else {
                                                "\u{2717} 正确答案：{answer}"
                                            }
                                        }
                                        if let Some(w) = target_word_ref.clone() {
                                            div { class: "card-trans quiz-reveal",
                                                div { class: "line", "目标词：{w.word}" }
                                                {crate::components::text_lines(&w.translation, "line")}
                                            }
                                        }
                                        if tts_on {
                                            button {
                                                class: "btn small",
                                                onclick: move |_| tts::speak(&token_for_tts, rate),
                                                "\u{1f50a} 朗读目标词"
                                            }
                                        }
                                        button {
                                            class: "btn primary",
                                            onclick: move |_| advance(),
                                            "下一题"
                                        }
                                    }
                                }
                            }
                        }
                        None => rsx! { p { class: "muted", "无更多题目" } }
                    }
                }
            }
        }
        Phase::Done => {
            let len = queue().len();
            let got = correct_count();
            rsx! {
                div { class: "page quiz",
                    QuizHeader { page, title: settings().quiz_mode.label() }
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

fn mode_hint(mode: QuizMode) -> &'static str {
    match mode {
        QuizMode::Cloze => "从 4 个选项中选出填入空格的正确单词。",
        QuizMode::SentenceTranslation => "阅读英文句子，选出目标词的正确中文释义。",
        QuizMode::ListenChoose => "听句子后，选出你听到的目标单词。",
        _ => "",
    }
}

#[component]
fn QuizHeader(page: Signal<Page>, title: &'static str) -> Element {
    let mut page = page;
    rsx! {
        div { class: "session-header",
            button { class: "btn ghost", onclick: move |_| page.set(Page::Categories), "← 词库" }
            span { class: "session-title", "{title}" }
            button { class: "btn ghost", onclick: move |_| page.set(Page::Settings), "⚙ 设置" }
        }
    }
}
