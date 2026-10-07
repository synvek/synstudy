use dioxus::prelude::*;

use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::state::{Settings, SETTINGS_KEY};

#[component]
pub fn SettingsPage() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut confirm_reset = use_signal(|| false);

    let persist = {
        let progress = progress.clone();
        move |s: Settings| {
            settings.set(s.clone());
            if let Ok(json) = serde_json::to_string(&s) {
                progress.set_setting(SETTINGS_KEY, &json);
            }
        }
    };

    let cur = settings();
    let dict_path = dict.display_path();
    let available = dict.available();
    let dict_for_memo = dict.clone();
    let total_words = use_memo(move || dict_for_memo.word_total());

    rsx! {
        div { class: "page settings",
            div { class: "page-head", h1 { "设置" } }

            section { class: "setting-group",
                h3 { "学习" }

                div { class: "setting-row",
                    label { "每轮学习单词数" }
                    div { class: "stepper",
                        button {
                            class: "btn small",
                            onclick: {
                                let mut persist = persist.clone();
                                let cur = cur.clone();
                                move |_| {
                                    let mut s = cur.clone();
                                    s.words_per_session = s.words_per_session.saturating_sub(5).max(1);
                                    persist(s);
                                }
                            },
                            "−"
                        }
                        input {
                            r#type: "number",
                            class: "num-input",
                            min: "1",
                            max: "200",
                            value: "{cur.words_per_session}",
                            oninput: {
                                let mut persist = persist.clone();
                                move |e| {
                                    if let Ok(n) = e.value().parse::<usize>() {
                                        let mut s = settings.read().clone();
                                        s.words_per_session = n.clamp(1, 200);
                                        persist(s);
                                    }
                                }
                            }
                        }
                        button {
                            class: "btn small",
                            onclick: {
                                let mut persist = persist.clone();
                                let cur = cur.clone();
                                move |_| {
                                    let mut s = cur.clone();
                                    s.words_per_session = (s.words_per_session + 5).min(200);
                                    persist(s);
                                }
                            },
                            "+"
                        }
                    }
                }

                div { class: "setting-row",
                    label { "倒序学习（从词库末尾开始）" }
                    input {
                        r#type: "checkbox",
                        checked: cur.reverse,
                        onchange: {
                            let mut persist = persist.clone();
                            move |e| {
                                let mut s = settings.read().clone();
                                s.reverse = e.data().checked();
                                persist(s);
                            }
                        }
                    }
                }
            }

            section { class: "setting-group",
                h3 { "数据" }
                div { class: "setting-row column",
                    div { class: "kv", span { class: "k", "词典数据库" } span { class: "v", "{dict_path}" } }
                    div { class: "kv", span { class: "k", "收录单词" } span { class: "v", "{total_words()}" } }
                    div { class: "kv", span { class: "k", "进度数据库" } span { class: "v", "{progress.path.display()}" } }
                    if !available {
                        p { class: "warn",
                            "未找到词典数据库。请运行 scripts/build_dict_db.py 生成 assets/synstudy.db。"
                        }
                    }
                }
            }

            section { class: "setting-group danger-zone",
                h3 { "重置进度" }
                if let Some(c) = selected() {
                    div { class: "setting-row",
                        label { "重置「{c.name}」的学习进度" }
                        button {
                            class: "btn danger",
                            onclick: {
                                let progress = progress.clone();
                                move |_| progress.reset_category(c.id)
                            },
                            "重置该词库"
                        }
                    }
                }
                div { class: "setting-row",
                    label { "重置全部学习进度" }
                    if confirm_reset() {
                        div { class: "confirm-inline",
                            button {
                                class: "btn danger",
                                onclick: {
                                    let progress = progress.clone();
                                    move |_| {
                                        progress.reset_all();
                                        confirm_reset.set(false);
                                    }
                                },
                                "确认重置"
                            }
                            button {
                                class: "btn",
                                onclick: move |_| confirm_reset.set(false),
                                "取消"
                            }
                        }
                    } else {
                        button {
                            class: "btn danger",
                            onclick: move |_| confirm_reset.set(true),
                            "重置全部"
                        }
                    }
                }
            }
        }
    }
}
