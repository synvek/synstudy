use dioxus::prelude::*;

use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::state::{ExamFilter, FontScale, QuizMode, Settings, StudyOrder, Theme, SETTINGS_KEY};
use crate::srs::Scheduler;
use crate::tts;

/// Choices offered for the per-round study word count.
const STUDY_SIZES: [usize; 4] = [20, 30, 40, 50];

/// Sub-tabs of the full-screen "关于" panel (About / License / Privacy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AboutTab {
    Info,
    License,
    Privacy,
}

impl AboutTab {
    fn label(self) -> &'static str {
        match self {
            AboutTab::Info => "关于本软件",
            AboutTab::License => "版权与许可",
            AboutTab::Privacy => "隐私政策",
        }
    }

    fn all() -> [AboutTab; 3] {
        [AboutTab::Info, AboutTab::License, AboutTab::Privacy]
    }
}

#[component]
pub fn SettingsPage() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut settings: Signal<Settings> = use_context();
    let selected: Signal<Option<Category>> = use_context();
    let mut confirm_reset = use_signal(|| false);
    // Which "关于" sub-tab is open, if the overlay panel is showing.
    let mut about_tab = use_signal(|| Option::<AboutTab>::None);

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
    let progress_path = progress.path.display().to_string();

    // About panel takes over the whole page area (navbar / tab bar stay put),
    // reached from the "关于" section below and dismissed via "返回设置".
    if let Some(tab) = about_tab() {
        let total = total_words();
        return rsx! {
            AboutOverlay {
                tab,
                dict_path: dict_path.clone(),
                progress_path: progress_path.clone(),
                total_words: total,
                on_select: move |t| about_tab.set(Some(t)),
                on_close: move |_| about_tab.set(None),
            }
        };
    }

    rsx! {
        div { class: "page settings",
            div { class: "page-head", h1 { "设置" } }

            section { class: "setting-group",
                h3 { "学习" }

                div { class: "setting-row",
                    label { "学习顺序" }
                    div { class: "seg",
                        for o in StudyOrder::all() {
                            button {
                                key: "{o:?}",
                                class: if cur.study_order == *o { "seg-btn active" } else { "seg-btn" },
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

                div { class: "setting-row",
                    label { "每轮学习单词数" }
                    div { class: "seg",
                        for n in STUDY_SIZES {
                            button {
                                key: "{n}",
                                class: if cur.study_size == n { "seg-btn active" } else { "seg-btn" },
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
                p { class: "muted hint", "学习顺序与每轮数量会记住，也可在学习页顶随时切换。随机顺序每轮重新打散。" }
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
                div { class: "setting-row",
                    label { "字体大小" }
                    div { class: "seg",
                        for f in FontScale::all() {
                            button {
                                key: "{f:?}",
                                class: if cur.font_scale == *f { "seg-btn active" } else { "seg-btn" },
                                onclick: {
                                    let mut persist = persist.clone();
                                    let f = *f;
                                    move |_| {
                                        let mut s = settings.read().clone();
                                        s.font_scale = f;
                                        persist(s);
                                    }
                                },
                                "{f.label()}"
                            }
                        }
                    }
                }
                p { class: "muted hint", "真机上若觉得文字偏小可调大档位；界面（含按钮与间距）会整体等比放大，选择即时生效并记住。" }
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
                    label { "测验题型" }
                    div { class: "seg wrap",
                        for m in QuizMode::all() {
                            button {
                                key: "{m:?}",
                                class: if cur.quiz_mode == *m { "seg-btn active" } else { "seg-btn" },
                                onclick: {
                                    let mut persist = persist.clone();
                                    let m = *m;
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
                if cur.quiz_mode.is_sentence_mode() {
                    div { class: "setting-row",
                        label { "考试级别过滤（句库题）" }
                        div { class: "seg wrap",
                            for f in ExamFilter::all() {
                                button {
                                    key: "{f:?}",
                                    class: if cur.quiz_exam_filter == *f { "seg-btn active" } else { "seg-btn" },
                                    onclick: {
                                        let mut persist = persist.clone();
                                        let f = *f;
                                        move |_| {
                                            let mut s = settings.read().clone();
                                            s.quiz_exam_filter = f;
                                            persist(s);
                                        }
                                    },
                                    "{f.label()}"
                                }
                            }
                        }
                    }
                    p { class: "muted hint",
                        "句库题使用 Tatoeba 开放句库（CC-BY 2.0 FR），按考纲词自动标记到四级/六级/专四/专八/托福/GRE。"
                    }
                }
                div { class: "setting-row",
                    label { "出题时自动发音（听音拼写 / 听音辨词）" }
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

            section { class: "setting-group desktop-only",
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

            section { class: "setting-group desktop-only",
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

            section { class: "setting-group about-entry",
                h3 { "关于" }
                p { class: "muted hint", "了解本项目的功能、许可与数据处理方式。" }
                for t in AboutTab::all() {
                    div { key: "{t:?}", class: "setting-row",
                        label { "{t.label()}" }
                        button {
                            class: "btn ghost",
                            onclick: move |_| about_tab.set(Some(t)),
                            "查看"
                        }
                    }
                }
            }
        }
    }
}

/// Third-party data sources bundled into `assets/synstudy.db`, shown under the
/// "版权与许可" tab as (name, what we use, license).
const DATA_SOURCES: [(&str, &str, &str); 7] = [
    ("ECDICT", "英汉释义、音标、词形、词频、考试标签", "MIT"),
    ("CC-CEDICT", "汉英词典（中文主键）", "CC-BY-SA 4.0"),
    ("Wiktionary / kaikki.org", "分级词库自然例句与近义/反义词", "CC BY-SA 4.0"),
    ("WordNet", "近义/反义词补充", "Princeton 许可"),
    ("Tatoeba", "考试分级句库与参考译文", "CC-BY 2.0 FR"),
    ("OpenCC", "构建期繁→简转换（映射表不入库）", "Apache-2.0"),
    ("CEFR-J", "A1–C2 难度等级标注", "免费商用须引用"),
];

/// Full-screen "关于" panel with three sub-tabs (About / License / Privacy),
/// reached from the "关于" section of Settings.
#[component]
fn AboutOverlay(
    tab: AboutTab,
    dict_path: String,
    progress_path: String,
    total_words: i64,
    on_select: EventHandler<AboutTab>,
    on_close: EventHandler<()>,
) -> Element {
    let version = env!("CARGO_PKG_VERSION");
    rsx! {
        div { class: "page settings about",
            div { class: "page-head",
                button {
                    class: "btn small ghost",
                    onclick: move |_| on_close.call(()),
                    "← 返回设置"
                }
                h1 { "关于" }
            }
            div { class: "seg about-tabs",
                for t in AboutTab::all() {
                    button {
                        key: "{t:?}",
                        class: if tab == t { "seg-btn active" } else { "seg-btn" },
                        onclick: move |_| on_select.call(t),
                        "{t.label()}"
                    }
                }
            }

            match tab {
                AboutTab::Info => rsx! {
                    div { class: "about-card",
                        div { class: "about-app",
                            div { class: "about-logo", "s" }
                            div { class: "about-title",
                                h2 { "synstudy" }
                                p { class: "muted", "版本 {version} · 离线英语学习工具" }
                            }
                        }
                        p { "使用 Rust + Dioxus 构建的纯客户端、离线优先英语词典与词汇学习工具：内置英汉 / 汉英双向词典与分级词库，支持学习、背诵（间隔重复 SRS）、测验、词根与生词本，所有进度仅保存在本地。" }
                        div { class: "about-kv setting-row column",
                            div { class: "kv", span { class: "k", "平台" } span { class: "v", "macOS / Windows / Linux 桌面端" } }
                            div { class: "kv", span { class: "k", "技术栈" } span { class: "v", "Rust · Dioxus 0.8 · SQLite" } }
                            div { class: "kv", span { class: "k", "收录单词" } span { class: "v", "{total_words}" } }
                            div { class: "kv", span { class: "k", "词典数据库（只读）" } span { class: "v", "{dict_path}" } }
                            div { class: "kv", span { class: "k", "进度数据库（本地可写）" } span { class: "v", "{progress_path}" } }
                        }
                    }
                },
                AboutTab::License => rsx! {
                    div { class: "about-card",
                        h3 { "软件许可" }
                        p { "synstudy 的源代码采用 MIT OR Apache-2.0 双许可，Copyright © 2026 yangxiongbo。你可以自由使用、修改与再分发，包括商业用途。" }
                        h3 { "数据许可（分轨授权）" }
                        p { "打包的词典数据库收录了以下第三方开放数据，各自遵循原始许可证；数据许可与代码许可相互独立。" }
                        div { class: "about-srcs",
                            for (name, usage, lic) in DATA_SOURCES {
                                div { key: "{name}", class: "about-src",
                                    div { class: "about-src-main",
                                        span { class: "about-src-name", "{name}" }
                                        span { class: "about-src-usage muted", "{usage}" }
                                    }
                                    span { class: "about-src-lic", "{lic}" }
                                }
                            }
                        }
                        p { class: "muted hint", "对外分发含 CC-BY-SA 成分的数据（如 zh_entries / examples 表）时，该部分演绎数据须以 CC-BY-SA 4.0 相同许可共享；本项目源代码不受其传染。随附的开源构建脚本（scripts/）可自助重建数据库。" }
                        p { class: "muted hint", "本项目不分发也不爬取 CET/TEM/TOEFL/GRE 等受版权保护的真题内容；考试级别仅基于开放的 ECDICT 考纲元数据自行标注。" }
                    }
                },
                AboutTab::Privacy => rsx! {
                    div { class: "about-card",
                        h3 { "隐私政策" }
                        p { "synstudy 完全离线运行：无后端服务、不联网、无账户体系，不收集也不上传任何个人信息。" }
                        ul { class: "about-list",
                            li { "学习进度、设置、错词本与生词本等数据仅写入本地设备的应用数据目录（见“关于本软件”中的进度数据库路径）。" }
                            li { "无遥测、无埋点、无广告，应用不会向任何第三方共享你的数据。" }
                            li { "单词发音调用操作系统内置的语音合成（Web Speech API），朗读文本为本地单词，不上传网络；若你的系统语音实现需联网，可在设置中关闭发音。" }
                            li { "如需清除全部数据，可在“设置 → 重置进度”清空，或直接删除本地进度数据库文件。" }
                        }
                    }
                },
            }
        }
    }
}
