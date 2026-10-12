use dioxus::prelude::*;

use crate::components::category_tile::{category_code, category_visual};
use crate::components::progress_bar::ProgressBar;
use crate::components::word_card::WordCard;
use crate::components::word_detail::WordDetail;
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::{Category, ProgressStatus, Word, WordProgress};
use crate::pages::PickCategory;
use crate::srs::{self, Rating};
use crate::state::{now_ts, first_line, Page, Settings, StudyOrder, SETTINGS_KEY};
use crate::tts;

/// Choices offered for the per-round study word count (kept in sync with the
/// settings page).
const STUDY_SIZES: [usize; 4] = [20, 30, 40, 50];

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Running,
    Done,
}

/// Deterministic in-place shuffle (LCG) so a `Random` plan varies per start
/// without pulling in the `rand` crate.
fn shuffle<T>(items: &mut [T], seed: u64) {
    if items.len() < 2 {
        return;
    }
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    for i in (1..items.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        items.swap(i, j);
    }
}

/// Persisted ordering key stored on each `study_round` row.
fn order_key(o: StudyOrder) -> &'static str {
    match o {
        StudyOrder::Sequential => "seq",
        StudyOrder::Reverse => "reverse",
        StudyOrder::Random => "random",
    }
}

/// `(badge class, label)` for a word's current progress status.
fn status_badge(status: ProgressStatus) -> (&'static str, &'static str) {
    match status {
        ProgressStatus::Mastered => ("ws mastered", "已掌握"),
        ProgressStatus::Reviewing => ("ws reviewing", "复习中"),
        ProgressStatus::Learning => ("ws learning", "学习中"),
        ProgressStatus::New => ("ws new", "未学"),
    }
}

#[component]
pub fn Study() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let phase = use_signal(|| Phase::Idle);
    let queue = use_signal(Vec::<i64>::new);
    let idx = use_signal(|| 0usize);
    let mut revealed = use_signal(|| false);
    let round_id = use_signal(|| Option::<i64>::None);
    // Bumped whenever rounds / progress change so the setup lists re-read.
    let revision = use_signal(|| 0u64);
    // A persisted round opened for review from the history list.
    let mut open_round = use_signal(|| Option::<i64>::None);
    // Full word list overlay for the current category.
    let mut show_all = use_signal(|| false);
    // Two-step confirmation for clearing the round plan.
    let mut confirm_clear = use_signal(|| false);
    // Word detail shown in the round-review / word-list panes.
    let mut detail = use_signal(|| Option::<Word>::None);

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

    // Round review takes over the whole page while a round is open.
    if let Some(rid) = open_round() {
        return rsx! {
            RoundReview {
                key: "{rid}",
                round_id: rid,
                on_back: move |_| {
                    open_round.set(None);
                    detail.set(None);
                },
                detail,
            }
        };
    }

    // Full word list overlay.
    if show_all() {
        return rsx! {
            WordList {
                on_back: move |_| {
                    show_all.set(false);
                    detail.set(None);
                },
                detail,
            }
        };
    }

    let cat = match selected() {
        Some(c) => c,
        None => return rsx! { PickCategory { mode: "学习" } },
    };

    // Persist a settings change from the in-page pickers.
    let persist = {
        let progress = progress.clone();
        move |s: Settings| {
            settings.set(s.clone());
            if let Ok(json) = serde_json::to_string(&s) {
                progress.set_setting(SETTINGS_KEY, &json);
            }
        }
    };

    // --- Begin / continue the earliest unfinished round ---------------------
    // The first call materializes the whole plan (every remaining new word is
    // split into rounds up front) so the round list is complete immediately.
    let begin = {
        let dict = dict.clone();
        let progress = progress.clone();
        let cat_id = cat.id;
        let mut phase = phase;
        let mut queue = queue;
        let mut idx = idx;
        let mut revealed = revealed;
        let mut round_id = round_id;
        move || {
            let s = settings();
            let rounds = progress.list_rounds(cat_id);
            let earliest = rounds
                .iter()
                .filter(|r| r.finished_at.is_none())
                .min_by_key(|r| r.number)
                .map(|r| r.id);
            let rid = match earliest {
                Some(id) => Some(id),
                None => {
                    let all = dict.category_word_ids(cat_id, s.study_order.reverse_fetch());
                    let pmap = progress.map_for_category(cat_id);
                    let mut new_ids: Vec<i64> = all
                        .into_iter()
                        .filter(|id| {
                            pmap.get(id)
                                .map(|p| p.status == ProgressStatus::New)
                                .unwrap_or(true)
                        })
                        .collect();
                    if s.study_order == StudyOrder::Random {
                        shuffle(&mut new_ids, now_ts() as u64);
                    }
                    if new_ids.is_empty() {
                        None
                    } else {
                        progress.create_rounds_batch(
                            cat_id,
                            order_key(s.study_order),
                            &new_ids,
                            s.study_size,
                            now_ts(),
                        );
                        progress
                            .list_rounds(cat_id)
                            .iter()
                            .filter(|r| r.finished_at.is_none())
                            .min_by_key(|r| r.number)
                            .map(|r| r.id)
                    }
                }
            };
            if let Some(rid) = rid {
                let words: Vec<i64> =
                    progress.round_words(rid).into_iter().map(|(id, _)| id).collect();
                if !words.is_empty() {
                    queue.set(words);
                    round_id.set(Some(rid));
                    idx.set(0);
                    revealed.set(false);
                    phase.set(Phase::Running);
                }
            }
        }
    };

    match phase() {
        Phase::Idle => {
            let _ = revision();
            let now = now_ts();
            let stats = progress.stats(cat.id, cat.word_count, now);
            let total = cat.word_count.max(0) as usize;
            let started = stats.started().max(0) as usize;
            let new_left = stats.new_count.max(0) as usize;
            let due = stats.due.max(0) as usize;
            let started_i = stats.started().max(0);

            let rounds = progress.list_rounds(cat.id);
            let locked = !rounds.is_empty();
            let unfinished: Vec<&_> = rounds
                .iter()
                .filter(|r| r.finished_at.is_none())
                .collect();
            let current_id = unfinished.iter().min_by_key(|r| r.number).map(|r| r.id);
            let current_no = unfinished.iter().min_by_key(|r| r.number).map(|r| r.number);
            let done_rounds = rounds.iter().filter(|r| r.finished_at.is_some()).count();
            let total_rounds = rounds.len();
            let mut rounds_asc = rounds.clone();
            rounds_asc.sort_by_key(|r| r.number);

            let cur = settings();
            let size = cur.study_size.max(1);
            let planned = new_left.div_ceil(size);

            let begin_label = match current_no {
                Some(no) => format!("继续第 {no} 轮"),
                None if new_left > 0 => format!("开始学习（共 {planned} 轮 · {new_left} 词）"),
                None => "本词库新词已全部学完".to_string(),
            };
            let begin_disabled = current_no.is_none() && new_left == 0;

            // Overview card visuals + progress percentage.
            let (abbr, tone, group) = category_visual(&cat.key);
            let hero_tone = if started > 0 { "tone-grad" } else { tone };
            let code = category_code(&cat.key);
            let pct = if total > 0 {
                started as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let pct_str = if pct > 0.0 && pct < 1.0 {
                format!("{:.2}", pct)
            } else {
                format!("{:.0}", pct)
            };
            let plan_total = if locked { total_rounds } else { planned };

            let onkeydown = {
                let mut begin = begin.clone();
                move |evt: Event<KeyboardData>| {
                    if matches!(evt.code(), Code::Enter | Code::NumpadEnter | Code::Space) {
                        evt.prevent_default();
                        begin();
                    }
                }
            };

            rsx! {
                div { class: "page study", tabindex: "-1", autofocus: true, onkeydown,
                    nav { class: "breadcrumb",
                        a { onclick: move |_| page.set(Page::Categories), "词库" }
                        span { class: "sep", "\u{203a}" }
                        span { class: "cur", "{cat.name}" }
                    }

                    // Overview card.
                    section { class: "panel wb-hero",
                        div { class: "wb-hero-top",
                            div { class: "wb-hero-id",
                                div { class: "bookicon lg {hero_tone}", "{abbr}" }
                                div {
                                    div { class: "wb-hero-title",
                                        h1 { "{cat.name}" }
                                        if started > 0 {
                                            span { class: "badge-status", "学习中" }
                                        }
                                    }
                                    p { class: "wb-hero-sub", "{code} / {group}词库" }
                                }
                            }
                            div { class: "wb-stats",
                                div { class: "wb-stat",
                                    p { class: "lbl", "已学单词" }
                                    p { class: "val primary", "{started}" small { " / {total}" } }
                                }
                                div { class: "wb-stat",
                                    p { class: "lbl", "未学单词" }
                                    p { class: "val", "{new_left}" }
                                }
                                div { class: "wb-stat",
                                    p { class: "lbl", "待复习" }
                                    p { class: "val warn", "{due}" }
                                }
                            }
                        }
                        div { class: "wb-hero-progress",
                            div { class: "wb-hero-progress-top",
                                span { "词库学习进度" }
                                span { class: "pct", "{pct_str}%" }
                            }
                            ProgressBar { value: started_i, max: cat.word_count }
                        }
                    }

                    // Config + plan columns.
                    div { class: "study-cols",
                        section { class: "panel study-config",
                            div { class: "panel-head",
                                span { class: "ico", "\u{1f39a}" }
                                h2 { "学习配置" }
                            }
                            p { class: "panel-sub", "按照自己的节奏，安排每一轮学习。" }

                            div { class: "config-row",
                                div { class: "cfg-label",
                                    h3 { "学习顺序" }
                                    p { "选择单词的排列方式" }
                                }
                                div { class: "seg",
                                    for o in StudyOrder::all() {
                                        button {
                                            key: "{o:?}",
                                            class: if cur.study_order == *o { "seg-btn active" } else { "seg-btn" },
                                            disabled: locked,
                                            onclick: {
                                                let mut persist = persist.clone();
                                                let o = *o;
                                                move |_| {
                                                    let mut s = settings.read().clone();
                                                    s.study_order = o;
                                                    persist(s);
                                                }
                                            },
                                            "{o.label()}"
                                        }
                                    }
                                }
                            }

                            div { class: "config-row",
                                div { class: "cfg-label",
                                    h3 { "每轮数量" }
                                    p { "每轮计划学习的单词数" }
                                }
                                div { class: "seg",
                                    for n in STUDY_SIZES {
                                        button {
                                            key: "{n}",
                                            class: if cur.study_size == n { "seg-btn active" } else { "seg-btn" },
                                            disabled: locked,
                                            onclick: {
                                                let mut persist = persist.clone();
                                                move |_| {
                                                    let mut s = settings.read().clone();
                                                    s.study_size = n;
                                                    persist(s);
                                                }
                                            },
                                            "{n}"
                                        }
                                    }
                                }
                            }

                            div { class: "config-note",
                                span { class: "ico", "\u{24d8}" }
                                if locked {
                                    p { "轮次计划已生成，学习顺序与每轮数量不可再修改；如需调整请先在下方「清除轮次记录」。" }
                                } else {
                                    p { "学习计划将根据未学单词、学习顺序和每轮数量自动生成。继续学习会从当前轮次开始，保留你的学习进度。" }
                                }
                            }

                            div { class: "config-actions",
                                button {
                                    class: "btn primary big",
                                    disabled: begin_disabled,
                                    onclick: {
                                        let mut begin = begin.clone();
                                        move |_| begin()
                                    },
                                    "{begin_label}"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        detail.set(None);
                                        show_all.set(true);
                                    },
                                    "查看全部单词"
                                }
                            }
                            if !locked {
                                p { class: "config-lock desktop-only", "快捷键：Enter / 空格 开始学习" }
                            }
                        }

                        aside { class: "panel study-plan",
                            div { class: "panel-head",
                                h2 { "我的学习计划" }
                            }
                            p { class: "panel-sub", "一点一滴，让词汇成为你的积累。" }
                            div { class: "plan-num",
                                span { class: "big", "{done_rounds}" }
                                span { class: "of", "/ {plan_total} 轮已完成" }
                            }
                            ProgressBar { value: done_rounds as i64, max: plan_total as i64 }
                            div { class: "plan-legend",
                                span { "共 {plan_total} 轮" }
                                span { "已完成 {done_rounds} 轮" }
                            }
                            div { class: "plan-tip",
                                div { class: "t", "\u{1f4a1} 保持小步前进" }
                                p { "不必一次记住所有单词。完成一轮，就是向目标更近一步。" }
                            }
                        }
                    }

                    // Round plan / history — every generated round, ascending.
                    if !rounds_asc.is_empty() {
                        section { class: "panel round-section",
                            div { class: "round-section-head",
                                h2 { "轮次记录" span { class: "muted", "当前学习计划" } }
                                if locked {
                                    div { class: "head-actions",
                                        if confirm_clear() {
                                            button {
                                                class: "btn danger",
                                                onclick: {
                                                    let progress = progress.clone();
                                                    let cat_id = cat.id;
                                                    let mut revision = revision;
                                                    move |_| {
                                                        progress.clear_rounds(cat_id);
                                                        revision += 1;
                                                        confirm_clear.set(false);
                                                    }
                                                },
                                                "确认清除"
                                            }
                                            button {
                                                class: "btn ghost",
                                                onclick: move |_| confirm_clear.set(false),
                                                "取消"
                                            }
                                        } else {
                                            button {
                                                class: "btn danger-outline",
                                                onclick: move |_| confirm_clear.set(true),
                                                "清除轮次记录"
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "round-grid",
                                for r in rounds_asc.iter() {
                                    {
                                        let done = r.finished_at.is_some();
                                        let ongoing = !done && current_id == Some(r.id);
                                        let state_cls = if done {
                                            "done"
                                        } else if ongoing {
                                            "ongoing"
                                        } else {
                                            "pending"
                                        };
                                        let state_label = if done {
                                            "已完成"
                                        } else if ongoing {
                                            "进行中"
                                        } else {
                                            "未开始"
                                        };
                                        let cell_class = format!("round-cell {state_cls}");
                                        let num_str = format!("{:02}", r.number);
                                        let tip = if done {
                                            format!("认识 {} · 不认识 {} / {}", r.known_count, r.unknown_count, r.size)
                                        } else if ongoing {
                                            format!("已答 {} / {}", r.answered(), r.size)
                                        } else {
                                            format!("{} · {}", r.order_label(), r.size)
                                        };
                                        let rid = r.id;
                                        let rno = r.number;
                                        rsx! {
                                            button {
                                                key: "{rid}",
                                                class: "{cell_class}",
                                                title: "{tip}",
                                                onclick: move |_| {
                                                    detail.set(None);
                                                    open_round.set(Some(rid));
                                                },
                                                div { class: "round-cell-main",
                                                    span { class: "round-cell-num", "{num_str}" }
                                                    span { class: "round-cell-name", "第 {rno} 轮" }
                                                }
                                                span { class: "round-state {state_cls}", "{state_label}" }
                                            }
                                        }
                                    }
                                }
                            }
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

            let answer = {
                let progress = progress.clone();
                let cat_id = cat.id;
                let scheduler = settings().scheduler;
                let mut phase = phase;
                let mut idx = idx;
                let mut revealed = revealed;
                let mut revision = revision;
                let round_id_r = round_id;
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
                        if let Some(rid) = round_id_r() {
                            progress.set_round_result(rid, wid, known);
                        }
                    }
                    if i + 1 >= len {
                        if let Some(rid) = round_id_r() {
                            progress.finish_round(rid, now_ts());
                        }
                        revision += 1;
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
                    div { class: "session-progress", "本轮 {i + 1} / {len}" }
                    ProgressBar { value: i as i64, max: len as i64 }
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
            let _ = revision();
            let now = now_ts();
            let stats = progress.stats(cat.id, cat.word_count, now);
            let new_left = stats.new_count.max(0) as usize;
            let rounds = progress.list_rounds(cat.id);
            let has_next = rounds.iter().any(|r| r.finished_at.is_none());
            let just = round_id().and_then(|rid| {
                rounds
                    .iter()
                    .find(|r| r.id == rid)
                    .cloned()
            });
            let (rno, rknown, rsize) = match &just {
                Some(r) => (Some(r.number), Some(r.known_count), Some(r.size)),
                None => (None, None, None),
            };
            rsx! {
                div { class: "page study",
                    SessionHeader { name: cat.name.clone(), page }
                    div { class: "session-done",
                        h2 { if let Some(no) = rno { "第 {no} 轮学习完成" } else { "本轮学习完成" } }
                        if let (Some(k), Some(sz)) = (rknown, rsize) {
                            p { class: "muted", "本轮 {sz} 词 · 认识 {k} · 不认识 {sz - k}" }
                        }
                        p { class: "muted", "已学单词会进入背诵队列，按时复习记得更牢。词库还剩 {new_left} 个新词。" }
                        div { class: "answer-row",
                            if let Some(rid) = round_id() {
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        detail.set(None);
                                        open_round.set(Some(rid));
                                    },
                                    "查看本轮"
                                }
                            }
                            button {
                                class: "btn",
                                onclick: {
                                    let mut revision = revision;
                                    let mut phase = phase;
                                    move |_| {
                                        revision += 1;
                                        phase.set(Phase::Idle);
                                    }
                                },
                                "返回轮次列表"
                            }
                            if has_next {
                                button {
                                    class: "btn primary",
                                    onclick: {
                                        let mut begin = begin.clone();
                                        move |_| begin()
                                    },
                                    "继续下一轮"
                                }
                            } else {
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
}

/// Review a persisted round: its words with 认识/不认识 markers, and a detail
/// pane for the selected word.
#[component]
fn RoundReview(
    round_id: i64,
    on_back: EventHandler<()>,
    mut detail: Signal<Option<Word>>,
) -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut page: Signal<Page> = use_context();
    let rows = progress.round_words(round_id);
    let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
    let words = dict.words_by_ids(&ids);
    // Align words to the round order and attach their recorded result.
    let known_of: Vec<Option<bool>> = rows.iter().map(|(_, k)| *k).collect();
    let pairs: Vec<(Word, Option<bool>)> = words
        .into_iter()
        .enumerate()
        .map(|(i, w)| (w, known_of.get(i).copied().flatten()))
        .collect();
    let known_count = pairs.iter().filter(|(_, k)| matches!(k, Some(true))).count();
    let unknown_count = pairs.iter().filter(|(_, k)| matches!(k, Some(false))).count();
    let sel = detail().clone();
    let has_detail = sel.is_some();
    let selected_id = sel.as_ref().map(|w| w.id);

    rsx! {
        div { class: "page study round-review",
            div { class: "session-header",
                button {
                    class: "btn ghost",
                    onclick: move |_| on_back.call(()),
                    "← 返回"
                }
                span { class: "session-title", "轮次回顾" }
                button { class: "btn ghost", onclick: move |_| page.set(Page::Categories), "词库" }
            }
            p { class: "muted", "共 {pairs.len()} 词 · 认识 {known_count} · 不认识 {unknown_count}" }
            div { class: if has_detail { "dict-body has-detail" } else { "dict-body" },
                div { class: "dict-results",
                    for (i, (w, known)) in pairs.iter().enumerate() {
                        div {
                            key: "{w.id}",
                            class: if selected_id == Some(w.id) { "result active round-result" } else { "result round-result" },
                            onclick: {
                                let w = w.clone();
                                move |_| detail.set(Some(w.clone()))
                            },
                            span { class: "round-pos", "{i + 1}" }
                            span { class: "result-word", "{w.word}" }
                            span { class: "result-trans", "{crate::state::first_line(&w.translation)}" }
                            span {
                                class: match known {
                                    Some(true) => "round-flag ok",
                                    Some(false) => "round-flag bad",
                                    None => "round-flag pending",
                                },
                                match known {
                                    Some(true) => rsx! { "认识" },
                                    Some(false) => rsx! { "不认识" },
                                    None => rsx! { "未答" },
                                }
                            }
                        }
                    }
                }
                div { class: "dict-detail",
                    button { class: "dict-back", onclick: move |_| detail.set(None), "\u{2190} 返回列表" }
                    match sel {
                        Some(w) => rsx! { WordDetail { key: "{w.id}", word: w } },
                        None => rsx! {
                            div { class: "empty-detail",
                                p { class: "muted", "从左侧选择一个单词查看详情" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Full word list of the current category with per-word progress status, and a
/// detail pane for the selected word.
#[component]
fn WordList(on_back: EventHandler<()>, mut detail: Signal<Option<Word>>) -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut page: Signal<Page> = use_context();

    let cat = match selected() {
        Some(c) => c,
        None => return rsx! { PickCategory { mode: "学习" } },
    };
    let ids = dict.category_word_ids(cat.id, false);
    let words = dict.words_by_ids(&ids);
    let pmap = progress.map_for_category(cat.id);
    // Precompute each word's badge so the rsx loop stays expression-only.
    let rows: Vec<(Word, &'static str, &'static str)> = words
        .into_iter()
        .map(|w| {
            let status = pmap
                .get(&w.id)
                .map(|p| p.status)
                .unwrap_or(ProgressStatus::New);
            let (cls, label) = status_badge(status);
            (w, cls, label)
        })
        .collect();
    let learned = rows.iter().filter(|(_, cls, _)| *cls != "ws new").count();
    let sel = detail().clone();
    let has_detail = sel.is_some();
    let selected_id = sel.as_ref().map(|w| w.id);

    rsx! {
        div { class: "page study round-review word-list",
            div { class: "session-header",
                button {
                    class: "btn ghost",
                    onclick: move |_| on_back.call(()),
                    "← 返回"
                }
                span { class: "session-title", "{cat.name} · 完整单词列表" }
                button { class: "btn ghost", onclick: move |_| page.set(Page::Categories), "词库" }
            }
            p { class: "muted", "共 {rows.len()} 词 · 已学 {learned} · 未学 {rows.len() - learned}" }
            div { class: if has_detail { "dict-body has-detail" } else { "dict-body" },
                div { class: "dict-results",
                    for (i, (w, cls, label)) in rows.iter().enumerate() {
                        div {
                            key: "{w.id}",
                            class: if selected_id == Some(w.id) { "result active round-result" } else { "result round-result" },
                            onclick: {
                                let w = w.clone();
                                move |_| detail.set(Some(w.clone()))
                            },
                            span { class: "round-pos", "{i + 1}" }
                            span { class: "result-word", "{w.word}" }
                            span { class: "result-trans", "{first_line(&w.translation)}" }
                            span { class: "ws-badge {cls}", "{label}" }
                        }
                    }
                }
                div { class: "dict-detail",
                    button { class: "dict-back", onclick: move |_| detail.set(None), "\u{2190} 返回列表" }
                    match sel {
                        Some(w) => rsx! { WordDetail { key: "{w.id}", word: w } },
                        None => rsx! {
                            div { class: "empty-detail",
                                p { class: "muted", "从左侧选择一个单词查看详情" }
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
