use dioxus::prelude::*;

use crate::components::{format_exchange, text_lines};
use crate::db::dict_repo::DictRepo;
use crate::db::progress_repo::ProgressRepo;
use crate::models::Word;
use crate::state::{now_ts, Page, Settings};
use crate::tts;

#[component]
pub fn WordDetail(word: Word) -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let settings: Signal<Settings> = use_context();
    let mut page: Signal<Page> = use_context();
    let mut sel_root: Signal<Option<String>> = use_context();
    let tags = word.tag_labels();
    let exchange = format_exchange(&word.exchange);
    let cefr = word.cefr.clone();
    let examples = dict.get_examples(word.id, 3);
    let graded_sentences = dict.sentences_for_word(word.id, None, 5);
    let word_roots = dict.roots_for_word(word.id);

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
                h2 { class: "wd-word", "{word.word}" }
                if !word.phonetic.is_empty() {
                    span { class: "wd-phonetic", "{word.phonetic}" }
                }
                if settings().tts_enabled {
                    button {
                        class: "wd-speak",
                        title: "朗读",
                        onclick: move |_| tts::speak(&tts_word, settings().tts_rate),
                        "\u{1f50a}"
                    }
                }
                button {
                    class: if starred() { "wd-star on" } else { "wd-star" },
                    title: if starred() { "从生词本移除" } else { "加入生词本" },
                    onclick: move |_| {
                        let now = progress.toggle_star(star_word_id, now_ts());
                        starred.set(now);
                    },
                    if starred() { "\u{2605}" } else { "\u{2606}" }
                }
                if word.has_cefr() {
                    span { class: "wd-cefr", "{cefr}" }
                }
            }

            div { class: "wd-sec",
                h4 { "释义" }
                div { class: "wd-trans", {text_lines(&word.translation, "line")} }
            }

            if !word.definition.trim().is_empty() {
                div { class: "wd-sec",
                    h4 { "英文释义" }
                    div { class: "wd-def", {text_lines(&word.definition, "line")} }
                }
            }

            if !word.pos.trim().is_empty() {
                div { class: "wd-row",
                    span { class: "wd-k", "词性分布" }
                    span { class: "wd-v", "{word.pos}" }
                }
            }

            if !exchange.is_empty() {
                div { class: "wd-row",
                    span { class: "wd-k", "词形变化" }
                    span { class: "wd-v", "{exchange}" }
                }
            }

            div { class: "wd-row",
                span { class: "wd-k", "词频排名" }
                span { class: "wd-v", "BNC {word.bnc} · COCA {word.frq}" }
            }

            if !tags.is_empty() {
                div { class: "wd-tags",
                    for t in tags.iter() {
                        span { key: "{t}", class: "chip", "{t}" }
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
                                    span { class: "wd-example-text", "{text}" }
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
                    p { class: "wd-example-note", "例句来自 Wiktionary（CC BY-SA 4.0）" }
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
                                    p { class: "wd-example-text", "{text}" }
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
                    p { class: "wd-example-note", "句子来自 Tatoeba（CC-BY 2.0 FR），按考纲词自动标记。" }
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
