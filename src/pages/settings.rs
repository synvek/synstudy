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

/// The “翻译” settings group, built by a cfg-gated helper. This Dioxus version's
/// `rsx!` rejects `#[cfg]` attributes on child elements, so the whole group is
/// constructed here (item-level cfg is fine) and spliced into the page as an
/// expression child; it renders nothing when the `translate` feature is off.
#[cfg(feature = "translate")]
fn translate_settings_section(mut settings: Signal<Settings>, progress: ProgressRepo) -> Element {
    let cur = settings();
    let persist = {
        let progress = progress.clone();
        move |s: Settings| {
            settings.set(s.clone());
            if let Ok(json) = serde_json::to_string(&s) {
                progress.set_setting(SETTINGS_KEY, &json);
            }
        }
    };
    rsx! {
        section { class: "setting-group",
            h3 { "翻译（截图 / 拍照）" }
            div { class: "setting-row",
                label { "目标语言" }
                select {
                    class: "select",
                    value: "{cur.translate_target_lang}",
                    onchange: {
                        let mut persist = persist.clone();
                        move |e| {
                            let mut s = settings.read().clone();
                            s.translate_target_lang = e.data().value();
                            persist(s);
                        }
                    },
                    for (val, name) in crate::translate::LANGS {
                        option { key: "{val}", value: "{val}", "{name}" }
                    }
                }
            }
            div { class: "setting-row",
                label { "源语言" }
                select {
                    class: "select",
                    value: "{cur.translate_src_lang}",
                    onchange: {
                        let mut persist = persist.clone();
                        move |e| {
                            let mut s = settings.read().clone();
                            s.translate_src_lang = e.data().value();
                            persist(s);
                        }
                    },
                    option { key: "auto", value: "auto", "自动检测" }
                    for (val, name) in crate::translate::LANGS {
                        option { key: "{val}", value: "{val}", "{name}" }
                    }
                }
            }
            div { class: "setting-row",
                label { "GPU 加速（Metal / Vulkan / CUDA）" }
                input {
                    r#type: "checkbox",
                    checked: cur.translate_gpu,
                    onchange: {
                        let mut persist = persist.clone();
                        move |e| {
                            let mut s = settings.read().clone();
                            s.translate_gpu = e.data().checked();
                            persist(s);
                        }
                    }
                }
            }
            div { class: "setting-row",
                label { "翻译后保持模型常驻内存" }
                input {
                    r#type: "checkbox",
                    checked: cur.translate_keep_model_resident,
                    onchange: {
                        let mut persist = persist.clone();
                        move |e| {
                            let mut s = settings.read().clone();
                            s.translate_keep_model_resident = e.data().checked();
                            persist(s);
                        }
                    }
                }
            }
            p { class: "muted hint",
                "翻译全程本地离线完成：OCR 采用 PaddleOCR（MNN 推理），译文由 HY-MT1.5-1.8B 生成。模型不随安装包分发，首次在「翻译」页按需下载（OCR 约 28 MB、翻译模型约 980 MB）并做完整性校验。低配设备可关闭 GPU 加速或模型常驻以节省显存 / 内存。"
            }
        }
    }
}

#[cfg(not(feature = "translate"))]
fn translate_settings_section(_settings: Signal<Settings>, _progress: ProgressRepo) -> Element {
    rsx! {}
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

            { translate_settings_section(settings, progress.clone()) }

            section { class: "setting-group desktop-only",
                h3 { "快捷键" }
                div { class: "setting-row column",
                    div { class: "kv", span { class: "k", "切换页面" } span { class: "v", "Ctrl / ⌘ + 1~9" } }
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

/// A single license entry shown under the "版权与许可" tab: what it covers,
/// the license name, a canonical external link, and the full license text
/// embedded at compile time from the repo's license files (so users can read
/// the actual terms in-app, even offline).
struct LicenseDoc {
    name: &'static str,
    usage: &'static str,
    lic: &'static str,
    url: &'static str,
    text: &'static str,
}

/// The source-code license(s) Synstudy is distributed under.
const SOFTWARE_LICENSES: &[LicenseDoc] = &[
    LicenseDoc {
        name: "MIT License",
        usage: "Synstudy 源代码可选许可之一",
        lic: "MIT",
        url: "https://opensource.org/licenses/MIT",
        text: include_str!("../../LICENSE-MIT.txt"),
    },
    LicenseDoc {
        name: "Apache License 2.0",
        usage: "Synstudy 源代码可选许可之一",
        lic: "Apache-2.0",
        url: "https://www.apache.org/licenses/LICENSE-2.0",
        text: include_str!("../../LICENSE-APACHE.txt"),
    },
];

/// Third-party data sources bundled into `assets/synstudy.db`, each with its
/// official license link and the full terms embedded from `license/`.
const DATA_LICENSES: &[LicenseDoc] = &[
    LicenseDoc {
        name: "ECDICT",
        usage: "英汉释义、音标、词形、词频、考试标签",
        lic: "MIT",
        url: "https://github.com/skywind3000/ECDICT",
        text: include_str!("../../license/ECDICT-LICENSE.txt"),
    },
    LicenseDoc {
        name: "CC-CEDICT",
        usage: "汉英词典（中文主键）",
        lic: "CC-BY-SA 4.0",
        url: "https://www.mdbg.net/chinese/dictionary?page=cc-cedict",
        text: include_str!("../../license/CC-CEDICT-LICENSE.txt"),
    },
    LicenseDoc {
        name: "Wiktionary / kaikki.org",
        usage: "分级词库自然例句与近义/反义词",
        lic: "CC BY-SA 4.0",
        url: "https://kaikki.org/dictionary/English/",
        text: include_str!("../../license/Wiktionary-kaikki-LICENSE.txt"),
    },
    LicenseDoc {
        name: "WordNet",
        usage: "近义/反义词补充",
        lic: "Princeton 许可",
        url: "https://wordnet.princeton.edu/",
        text: include_str!("../../license/WORDNET-LICENSE.txt"),
    },
    LicenseDoc {
        name: "Tatoeba",
        usage: "考试分级句库与参考译文",
        lic: "CC-BY 2.0 FR",
        url: "https://tatoeba.org/",
        text: include_str!("../../license/Tatoeba-LICENSE.txt"),
    },
    LicenseDoc {
        name: "OpenCC",
        usage: "构建期繁→简转换（映射表不入库）",
        lic: "Apache-2.0",
        url: "https://github.com/BYVoid/OpenCC",
        text: include_str!("../../license/OpenCC-LICENSE.txt"),
    },
    LicenseDoc {
        name: "CEFR-J",
        usage: "A1–C2 难度等级标注",
        lic: "免费商用须引用",
        url: "https://github.com/openlanguageprofiles/olp-en-cefrj",
        text: include_str!("../../license/CEFR-J-NOTICE.txt"),
    },
];

/// Components of the optional `translate` feature: statically-linked inference
/// engines (MNN, llama.cpp) plus models the user downloads at runtime and that
/// synstudy never redistributes (PaddleOCR, HY-MT). Empty when the feature is
/// off, so the About → License tab simply omits the whole group.
#[cfg(feature = "translate")]
const TRANSLATE_LICENSES: &[LicenseDoc] = &[
    LicenseDoc {
        name: "MNN",
        usage: "OCR 推理引擎（经 rust-paddle-ocr 静态链接）",
        lic: "Apache-2.0",
        url: "https://github.com/alibaba/MNN",
        text: include_str!("../../license/MNN-NOTICE.txt"),
    },
    LicenseDoc {
        name: "PaddleOCR",
        usage: "PP-OCRv5 mobile 检测 / 识别模型（运行时下载）",
        lic: "Apache-2.0",
        url: "https://github.com/PaddlePaddle/PaddleOCR",
        text: include_str!("../../license/PaddleOCR-NOTICE.txt"),
    },
    LicenseDoc {
        name: "llama.cpp",
        usage: "翻译模型推理引擎（经 llama-cpp-2 静态链接）",
        lic: "MIT",
        url: "https://github.com/ggerganov/llama.cpp",
        text: include_str!("../../license/llama-cpp-NOTICE.txt"),
    },
    LicenseDoc {
        name: "HY-MT1.5-1.8B",
        usage: "翻译模型权重（运行时下载，不再分发）",
        lic: "以官方模型卡为准",
        url: "https://github.com/Tencent-Hunyuan/HY-MT",
        text: include_str!("../../license/HY-MT-NOTICE.txt"),
    },
];

#[cfg(not(feature = "translate"))]
const TRANSLATE_LICENSES: &[LicenseDoc] = &[];

/// Render one "版权与许可" accordion group: each source is a clickable row that
/// expands to reveal its embedded license text and links out to the official
/// license page. `open` is the shared single-open state across all groups.
/// A plain fn (not a component) so the `Signal` can be passed by value without
/// pulling in component-props trait bounds.
fn license_docs_view(docs: &'static [LicenseDoc], open: Signal<Option<&'static str>>) -> Element {
    let mut open = open;
    rsx! {
        div { class: "about-srcs",
            for d in docs {
                {
                    let name = d.name;
                    let is_open = open().is_some_and(|o| o == name);
                    rsx! {
                        div { key: "{name}", class: if is_open { "about-src open" } else { "about-src" },
                            div { class: "about-src-row",
                                button {
                                    class: "about-src-head",
                                    onclick: move |_| open.set(if is_open { None } else { Some(name) }),
                                    span { class: "about-src-main",
                                        span { class: "about-src-name", "{d.name}" }
                                        span { class: "about-src-usage muted", "{d.usage}" }
                                    }
                                    span { class: "about-src-lic", "{d.lic}" }
                                    span { class: "about-src-chevron", if is_open { "\u{2304}" } else { "\u{2303}" } }
                                }
                                // Plain anchor (no target): dioxus-desktop's navigation
                                // handler opens http(s) links in the system browser and
                                // blocks the webview, so this never hijacks the app.
                                a { class: "about-src-link", href: "{d.url}", "{d.url}" }
                            }
                            if is_open {
                                pre { class: "about-license-text", "{d.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

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
    // Which license entry is expanded in the "版权与许可" tab (single-open).
    let open = use_signal(|| Option::<&'static str>::None);
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
                            img { class: "about-logo", src: "{crate::assets::brand_mark_uri()}", alt: "Synstudy" }
                            div { class: "about-title",
                                h2 { "Synstudy" }
                                p { class: "muted", "版本 {version} · 英语学习词典" }
                            }
                        }
                        p { "离线优先英语词典与词汇学习工具：内置英汉 / 汉英双向词典与分级词库，支持学习、背诵（间隔重复 SRS）、测验、词根与生词本，所有进度仅保存在本地。请访问主页: https://github.com/synvek/synstudy" }
                    }
                },
                AboutTab::License => rsx! {
                    div { class: "about-card",
                        h3 { "软件许可" }
                        p { "Synstudy 的源代码采用 MIT OR Apache-2.0 双许可，Copyright © 2026 Synvek。你可以自由使用、修改与再分发，包括商业用途。点击任一项可查看许可证全文，也可访问其官方链接。" }
                        { license_docs_view(SOFTWARE_LICENSES, open) }
                        h3 { "数据许可（分轨授权）" }
                        p { "打包的词典数据库收录了以下第三方开放数据，各自遵循原始许可证；数据许可与代码许可相互独立。展开可查看随包嵌入的完整条款。" }
                        { license_docs_view(DATA_LICENSES, open) }
                        p { class: "muted hint", "对外分发含 CC-BY-SA 成分的数据（如 zh_entries / examples 表）时，该部分演绎数据须以 CC-BY-SA 4.0 相同许可共享；本项目源代码不受其传染。随附的开源构建脚本（scripts/）可自助重建数据库。" }
                        p { class: "muted hint", "本项目不分发也不爬取 CET/TEM/TOEFL/GRE 等受版权保护的真题内容；考试级别仅基于开放的 ECDICT 考纲元数据自行标注。" }
                        if !TRANSLATE_LICENSES.is_empty() {
                            h3 { "翻译组件许可" }
                            p { "以下组件在启用「翻译」功能时使用：推理引擎（MNN / llama.cpp），模型权重（PaddleOCR / HY-MT）在运行时下载。" }
                            { license_docs_view(TRANSLATE_LICENSES, open) }
                        }
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
