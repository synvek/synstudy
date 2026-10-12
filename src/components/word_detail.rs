use dioxus::prelude::*;

use crate::components::{exchange_pairs, text_lines};
use crate::db::dict_repo::DictRepo;
use crate::db::progress_repo::ProgressRepo;
use crate::models::Word;
use crate::state::{now_ts, DictLookup, Page, Settings};
use crate::tts;

#[component]
pub fn WordDetail(word: Word) -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let settings: Signal<Settings> = use_context();
    let mut page: Signal<Page> = use_context();
    let mut sel_root: Signal<Option<String>> = use_context();
    let dict_lookup: DictLookup = use_context();
    let tags = word.tag_labels();
    let forms = exchange_pairs(&word.exchange);
    let cefr = word.cefr.clone();
    let examples = dict.get_examples(word.id, 3);
    let graded_sentences = dict.sentences_for_word(word.id, None, 5);
    let word_roots = dict.roots_for_word(word.id);
    let relations = dict.relations(word.id, 10);
    let syns: Vec<String> = relations
        .iter()
        .filter(|r| r.kind == "syn")
        .map(|r| r.target.clone())
        .collect();
    let ants: Vec<String> = relations
        .iter()
        .filter(|r| r.kind == "ant")
        .map(|r| r.target.clone())
        .collect();

    // Star (wordbook) state. Kept in a signal so toggling repaints the button;
    // re-synced during render whenever a different word is shown.
    let mut starred = use_signal(|| progress.is_starred(word.id));
    let mut synced_id = use_signal(|| word.id);
    if synced_id.cloned() != word.id {
        synced_id.set(word.id);
        starred.set(progress.is_starred(word.id));
    }
    let star_word_id = word.id;

    // Optional auto-pronunciation when the detail panel opens. The component is
    // keyed by word id at its call sites, so this fires once per word.
    let speak_word = word.word.clone();
    use_effect(move || {
        let s = settings();
        if s.tts_enabled && s.tts_autoplay {
            tts::speak(&speak_word, s.tts_rate);
        }
    });
    let tts_word = word.word.clone();

    rsx! {
        div { class: "word-detail",
            div { class: "wd-head",
                div { class: "wd-head-main",
                    div { class: "wd-title-row",
                        h2 { class: "wd-word", "{word.word}" }
                        if word.has_cefr() {
                            span { class: "wd-cefr", "{cefr}" }
                        }
                    }
                    div { class: "wd-meta",
                        if !word.phonetic.is_empty() {
                            span { class: "wd-phonetic", "英 {word.phonetic}" }
                        }
                        if settings().tts_enabled {
                            button {
                                class: "wd-speak",
                                title: "朗读",
                                onclick: move |_| tts::speak(&tts_word, settings().tts_rate),
                                "\u{1f50a}"
                            }
                        }
                    }
                }
                button {
                    class: if starred() { "wd-star-btn on" } else { "wd-star-btn" },
                    title: if starred() { "从生词本移除" } else { "加入生词本" },
                    onclick: move |_| {
                        let now = progress.toggle_star(star_word_id, now_ts());
                        starred.set(now);
                    },
                    span { if starred() { "\u{2605}" } else { "\u{2606}" } }
                    span { if starred() { "已加入" } else { "加入生词本" } }
                }
            }

            div { class: "wd-sec",
                h4 { "释义" }
                div { class: "wd-trans", {text_lines(&word.translation, "line")} }
                if !word.definition.trim().is_empty() {
                    div { class: "wd-def-box",
                        div { class: "label", "英文释义" }
                        div { class: "wd-def", {text_lines(&word.definition, "line")} }
                    }
                }
            }

            if !syns.is_empty() || !ants.is_empty() {
                div { class: "wd-sec",
                    h4 { "近义词 / 反义词" }
                    if !syns.is_empty() {
                        div { class: "wd-row",
                            span { class: "wd-k", "近义" }
                            div { class: "wd-links",
                                for t in syns.iter() {
                                    button {
                                        key: "{t}",
                                        class: "chip chip-link",
                                        title: "在词典中查询 {t}",
                                        onclick: {
                                            let w = t.clone();
                                            let mut dl = dict_lookup;
                                            let mut p = page;
                                            move |_| {
                                                dl.0.set(Some(w.clone()));
                                                p.set(Page::Dictionary);
                                            }
                                        },
                                        "{t}"
                                    }
                                }
                            }
                        }
                    }
                    if !ants.is_empty() {
                        div { class: "wd-row",
                            span { class: "wd-k", "反义" }
                            div { class: "wd-links",
                                for t in ants.iter() {
                                    button {
                                        key: "{t}",
                                        class: "chip chip-link",
                                        title: "在词典中查询 {t}",
                                        onclick: {
                                            let w = t.clone();
                                            let mut dl = dict_lookup;
                                            let mut p = page;
                                            move |_| {
                                                dl.0.set(Some(w.clone()));
                                                p.set(Page::Dictionary);
                                            }
                                        },
                                        "{t}"
                                    }
                                }
                            }
                        }
                    }
                    p { class: "wd-example-note", "- Wiktionary/WordNet" }
                }
            }

            if !forms.is_empty() {
                div { class: "wd-sec",
                    h4 { "词形变化" }
                    div { class: "wd-forms",
                        for (i, (lbl, val)) in forms.iter().enumerate() {
                            div { key: "{i}", class: "wd-form",
                                div { class: "label", "{lbl}" }
                                div { class: "val", "{val}" }
                            }
                        }
                    }
                }
            }

            div { class: "wd-sec",
                h4 { "考试标签" }
                div { class: "wd-tags",
                    if tags.is_empty() {
                        span { class: "muted", "暂无考试标签" }
                    } else {
                        for t in tags.iter() {
                            span { key: "{t}", class: "chip", "{t}" }
                        }
                    }
                    if word.bnc > 0 || word.frq > 0 {
                        span { class: "wd-freq",
                            "词频 BNC "
                            b { "{word.bnc}" }
                            " · COCA "
                            b { "{word.frq}" }
                        }
                    }
                }
            }

            if !examples.is_empty() {
                div { class: "wd-sec",
                    h4 { "例句" }
                    div { class: "wd-examples",
                        {examples.iter().enumerate().map(|(i, ex)| {
                            let text = ex.text.clone();
                            let src = ex.source.clone();
                            let speak_text = ex.text.clone();
                            let rate = settings().tts_rate;
                            let tts_on = settings().tts_enabled;
                            rsx! {
                                p { key: "{i}", class: "wd-example",
                                    span { class: "wd-example-text", {highlighted(&text, &word.word)} }
                                    if tts_on {
                                        button {
                                            class: "wd-ex-speak",
                                            title: "朗读例句",
                                            onclick: move |_| tts::speak(&speak_text, rate),
                                            "\u{1f50a}"
                                        }
                                    }
                                    if !src.is_empty() {
                                        span { class: "wd-example-src", "— {src}" }
                                    }
                                }
                            }
                        })}
                    }
                    p { class: "wd-example-note", "- Wiktionary" }
                }
            }

            if !graded_sentences.is_empty() {
                div { class: "wd-sec",
                    h4 { "考试句库" }
                    div { class: "wd-examples",
                        {graded_sentences.iter().enumerate().map(|(i, s)| {
                            let text = s.text.clone();
                            let zh = s.translation.clone();
                            let labels = s.exam_labels();
                            let speak_text = s.text.clone();
                            let rate = settings().tts_rate;
                            let tts_on = settings().tts_enabled;
                            let cefr = s.cefr_min.clone();
                            rsx! {
                                div { key: "{i}", class: "wd-example wd-example-sent",
                                    div { class: "wd-example-chips",
                                        for l in labels.iter() {
                                            span { key: "{l}", class: "chip chip-sm", "{l}" }
                                        }
                                        if !cefr.is_empty() {
                                            span { class: "chip chip-sm chip-cefr", "CEFR {cefr}" }
                                        }
                                    }
                                    p { class: "wd-example-text", {highlighted(&text, &word.word)} }
                                    if !zh.is_empty() {
                                        p { class: "wd-example-zh muted", "{zh}" }
                                    }
                                    if tts_on {
                                        button {
                                            class: "wd-ex-speak",
                                            title: "朗读句子",
                                            onclick: move |_| tts::speak(&speak_text, rate),
                                            "\u{1f50a}"
                                        }
                                    }
                                }
                            }
                        })}
                    }
                    p { class: "wd-example-note", "- Tatoeba" }
                }
            }

            if !word_roots.is_empty() {
                div { class: "wd-sec",
                    h4 { "词根词缀" }
                    div { class: "wd-links",
                        for r in word_roots.iter() {
                            button {
                                key: "{r.root_key}",
                                class: "chip chip-link",
                                title: "{r.meaning}",
                                onclick: {
                                    let k = r.root_key.clone();
                                    move |_| {
                                        sel_root.set(Some(k.clone()));
                                        page.set(Page::Roots);
                                    }
                                },
                                "{r.root_key}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render `text` with the first case-insensitive occurrence of `target` wrapped
/// in a highlight span. Falls back to plain text when `target` is absent or the
/// match is not on a clean ASCII boundary.
fn highlighted(text: &str, target: &str) -> Element {
    if target.is_empty() {
        return rsx! { "{text}" };
    }
    // Only slice when lowercasing preserves byte length (ASCII), so the index
    // found in the lowercased string maps onto the original.
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return rsx! { "{text}" };
    }
    match lower.find(&target.to_lowercase()) {
        Some(pos) if pos + target.len() <= text.len() => {
            let before = &text[..pos];
            let matched = &text[pos..pos + target.len()];
            let after = &text[pos + target.len()..];
            rsx! {
                "{before}"
                span { class: "hl", "{matched}" }
                "{after}"
            }
        }
        _ => rsx! { "{text}" },
    }
}
