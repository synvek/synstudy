use dioxus::prelude::*;

use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::state::{QuizMode, Settings, Theme, SETTINGS_KEY};
use crate::srs::Scheduler;
use crate::tts;

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
                h3 { "外观" }
                div { class: "setting-row",
                    label { "主题" }
                    div { class: "seg",
                        for t in [Theme::System, Theme::Light, Theme::Dark] {
                            button {
                                key: "{t:?}",
                                class: if cur.theme == t { "seg-btn active" } else { "seg-btn" },
                                onclick: {
                                    let mut persist = persist.clone();
                                    move |_| {
                                        let mut s = settings.read().clone();
                                        s.theme = t;
                                        persist(s);
                                    }
                                },
                                "{t.label()}"
                            }
                        }
                    }
                }
            }

            section { class: "setting-group",
                h3 { "复习算法" }
                div { class: "setting-row",
                    label { "间隔重复调度器" }
                    div { class: "seg",
                        for sc in [Scheduler::Sm2, Scheduler::Fsrs] {
                            button {
                                key: "{sc:?}",
                                class: if cur.scheduler == sc { "seg-btn active" } else { "seg-btn" },
                                onclick: {
                                    let mut persist = persist.clone();
                                    move |_| {
                                        let mut s = settings.read().clone();
                                        s.scheduler = sc;
                                        persist(s);
                                    }
                                },
                                "{sc.label()}"
                            }
                        }
                    }
                }
                p { class: "muted hint", "SM-2 lite 为默认算法；FSRS 基于记忆稳定度与难度建模，切换后新的评分即生效，历史进度不会丢失。" }
            }

            section { class: "setting-group",
                h3 { "测验" }
                div { class: "setting-row",
                    label { "拼写测验模式" }
                    div { class: "seg",
                        for m in [QuizMode::FromMeaning, QuizMode::FromSound] {
                            button {
                                key: "{m:?}",
                                class: if cur.quiz_mode == m { "seg-btn active" } else { "seg-btn" },
                                onclick: {
                                    let mut persist = persist.clone();
                                    move |_| {
                                        let mut s = settings.read().clone();
                                        s.quiz_mode = m;
                                        persist(s);
                                    }
                                },
                                "{m.label()}"
                            }
                        }
                    }
                }
                div { class: "setting-row",
                    label { "出题时自动发音（听音拼写）" }
                    input {
                        r#type: "checkbox",
                        checked: cur.quiz_auto_tts,
                        onchange: {
                            let mut persist = persist.clone();
                            move |e| {
                                let mut s = settings.read().clone();
                                s.quiz_auto_tts = e.data().checked();
                                persist(s);
                            }
                        }
                    }
                }
                div { class: "setting-row",
                    label { "拼错的单词计入错词本" }
                    input {
                        r#type: "checkbox",
                        checked: cur.quiz_record_miss,
                        onchange: {
                            let mut persist = persist.clone();
                            move |e| {
                                let mut s = settings.read().clone();
                                s.quiz_record_miss = e.data().checked();
                                persist(s);
                            }
                        }
                    }
                }
            }

            section { class: "setting-group",
                h3 { "发音（TTS）" }
                div { class: "setting-row",
                    label { "启用发音" }
                    input {
                        r#type: "checkbox",
                        checked: cur.tts_enabled,
                        onchange: {
                            let mut persist = persist.clone();
                            move |e| {
                                let mut s = settings.read().clone();
                                s.tts_enabled = e.data().checked();
                                persist(s);
                            }
                        }
                    }
                }
                div { class: "setting-row",
                    label { "翻面 / 打开详情时自动朗读" }
                    input {
                        r#type: "checkbox",
                        checked: cur.tts_autoplay,
                        onchange: {
                            let mut persist = persist.clone();
                            move |e| {
                                let mut s = settings.read().clone();
                                s.tts_autoplay = e.data().checked();
                                persist(s);
                            }
                        }
                    }
                }
                div { class: "setting-row",
                    label { "语速（{cur.tts_rate:.1}x）" }
                    div { class: "stepper",
                        button {
                            class: "btn small",
                            onclick: {
                                let mut persist = persist.clone();
                                let cur = cur.clone();
                                move |_| {
                                    let mut s = cur.clone();
                                    s.tts_rate = (s.tts_rate - 0.1).clamp(0.5, 2.0);
                                    persist(s);
                                }
                            },
                            "−"
                        }
                        button {
                            class: "btn small",
                            onclick: {
                                let mut persist = persist.clone();
                                let cur = cur.clone();
                                move |_| {
                                    let mut s = cur.clone();
                                    s.tts_rate = (s.tts_rate + 0.1).clamp(0.5, 2.0);
                                    persist(s);
                                }
                            },
                            "+"
                        }
                        button {
                            class: "btn small",
                            onclick: {
                                let rate = cur.tts_rate;
                                move |_| tts::speak("Hello, this is synstudy.", rate)
                            },
                            "试听"
                        }
                        button {
                            class: "btn small ghost",
                            onclick: move |_| tts::stop(),
                            "停止"
                        }
                    }
                }
                p { class: "muted hint", "发音使用系统内置语音（离线），需操作系统已安装英文语音包。" }
            }

            section { class: "setting-group",
                h3 { "快捷键" }
                div { class: "setting-row column",
                    div { class: "kv", span { class: "k", "切换页面" } span { class: "v", "Ctrl / ⌘ + 1~8" } }
                    div { class: "kv", span { class: "k", "前往词典搜索" } span { class: "v", "Ctrl / ⌘ + K" } }
                    div { class: "kv", span { class: "k", "词典选择结果" } span { class: "v", "↑ / ↓，Enter 打开" } }
                    div { class: "kv", span { class: "k", "学习翻面 / 认识 / 不认识" } span { class: "v", "空格 / Enter / Backspace" } }
                    div { class: "kv", span { class: "k", "背诵翻面 / 评分" } span { class: "v", "空格 / 1 2 3 4" } }
                    div { class: "kv", span { class: "k", "测验提交 / 下一题" } span { class: "v", "Enter" } }
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
