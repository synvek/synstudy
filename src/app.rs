use dioxus::prelude::*;

use crate::components::nav_bar::{MobileTabBar, NavBar};
use crate::components::splash::Splash;
use crate::db::{self, dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::pages;
use crate::state::{DictLookup, Page, Settings, LAST_CATEGORY_KEY, SETTINGS_KEY};

const CSS: &str = include_str!("../assets/main.css");

/// 移动端键盘/缩放复位钩子（仅 iOS/Android 注入，见下面的 use_effect）。
///
/// 与 main.css 末尾的字号兜底配套：字号兜底负责「不触发」WebKit 的
/// snap-to-zoom，这里负责「触发了也能恢复」——包括用户自己双指放大过的场景。
/// JS 里没有 `{}` 需要转义，因为它不在 format! 的字面量内。
#[cfg(any(target_os = "ios", target_os = "android"))]
const MOBILE_VIEWPORT_GUARD: &str = r#"
(function () {
  if (window.__ssViewportGuard) return;
  window.__ssViewportGuard = true;
  var editable = function (el) {
    var tag = el && el.tagName;
    if (tag === 'TEXTAREA' || tag === 'SELECT') return true;
    if (tag !== 'INPUT') return false;
    var t = (el.type || '').toLowerCase();
    return t !== 'button' && t !== 'submit' && t !== 'checkbox' &&
           t !== 'radio' && t !== 'range' && t !== 'file';
  };
  var restore = function () {
    // 1) 键盘收起后 WKWebView 的根滚动视图常停在非零 offset（页面被整体上移，
    //    顶部内容被遮）。文档本身 overflow:hidden 不可滚，这里就是无害的空操作。
    window.scrollTo(0, 0);
    // 2) 页面仍处于放大状态时复位。JS 没有设置缩放/还原 visualViewport.scale 的
    //    API，可行做法是临时收紧 maximum-scale —— WebKit 会立刻把页面缩回 1x，
    //    下一帧再恢复原 content，双指缩放能力不受影响。
    var vv = window.visualViewport;
    if (!vv || vv.scale <= 1.01) return;
    var meta = document.querySelector('meta[name="viewport"]');
    if (!meta) return;
    var keep = meta.getAttribute('content') || '';
    meta.setAttribute('content', keep + ', maximum-scale=1');
    requestAnimationFrame(function () {
      requestAnimationFrame(function () { meta.setAttribute('content', keep); });
    });
  };
  // 编辑控件失焦（= 键盘收起）之后复位一次；延迟等 WebKit 把 offset 落定。
  document.addEventListener('focusout', function (e) {
    if (editable(e.target)) setTimeout(restore, 60);
  }, true);
})();
"#;

/// Root component: wires up shared state and renders the app shell.
#[allow(non_snake_case)]
pub fn App() -> Element {
    // Writable progress store (creates schema on first run).
    let progress = use_context_provider(|| ProgressRepo::new(db::progress_db_path()));

    // Read-only dictionary store (path resolved once).
    let dict = use_context_provider(|| DictRepo::from_resolved(db::resolve_dict_db()));

    // Navigation + selection + settings.
    use_context_provider(|| Signal::new(Page::Dictionary));
    // Selected word list, restored from the last pick so Study/Review reopen
    // where the user left off instead of prompting to choose again.
    use_context_provider({
        let progress = progress.clone();
        let dict = dict.clone();
        move || {
            let id = progress
                .get_setting(LAST_CATEGORY_KEY)
                .and_then(|v| v.parse::<i64>().ok());
            let cat = id.and_then(|id| dict.categories().into_iter().find(|c| c.id == id));
            Signal::new(cat)
        }
    });
    // Currently selected word root (shared so WordDetail can deep-link to it).
    use_context_provider(|| Signal::new(Option::<String>::None));
    // Cross-page dictionary lookup channel (synonym chips -> Dictionary page).
    use_context_provider(|| DictLookup(Signal::new(Option::<String>::None)));
    use_context_provider(|| {
        let repo = ProgressRepo::new(db::progress_db_path());
        let settings = repo
            .get_setting(SETTINGS_KEY)
            .and_then(|v| serde_json::from_str::<Settings>(&v).ok())
            .unwrap_or_default();
        Signal::new(settings)
    });

    // Persist the active selection whenever it changes so the choice survives a
    // restart. Only `selected` is tracked here; the write hits the KV store and
    // never mutates a signal, so there is no feedback loop.
    let selected: Signal<Option<Category>> = use_context();
    {
        let progress = progress.clone();
        use_effect(move || match selected() {
            Some(c) => progress.set_setting(LAST_CATEGORY_KEY, &c.id.to_string()),
            None => progress.set_setting(LAST_CATEGORY_KEY, ""),
        });
    }

    // Reflect the color-scheme preference onto <html data-theme="..."> so the
    // CSS variables in main.css switch between light and dark palettes, and
    // the font-scale preference onto <html data-font="..."> (root zoom).
    let settings: Signal<Settings> = use_context();
    use_effect(move || {
        let theme = settings().theme.as_attr();
        let font = settings().font_scale.as_attr();
        document::eval(&format!(
            "document.documentElement.setAttribute('data-theme','{theme}');\
             document.documentElement.setAttribute('data-font','{font}')"
        ));
    });

    // 移动端视口兜底：给 <html> 打上 data-platform，让 main.css 末尾的规则把
    // 可编辑控件字号抬到 16px，同时装上面的失焦复位钩子。
    //
    // 真机症状（iOS）：在翻译页编辑「识别文本」textarea 后整页像是变大、超出屏幕
    // 宽高，顶部文字被遮；切到别的 tab 后 position:fixed 的底部 tabbar 也跑到视口
    // 外，要滚动才看得到。原因是 WebKit 聚焦「计算字号 < 16px」的 input/textarea/
    // select 时会自动放大整页（snap-to-zoom）且失焦不还原，键盘收起后还会留下根
    // 滚动偏移。翻译页 textarea 14px、.select 14px、ROI 数字框 13px 都在阈值以下。
    #[cfg(any(target_os = "ios", target_os = "android"))]
    use_effect(move || {
        let marker = if cfg!(target_os = "ios") {
            "document.documentElement.setAttribute('data-platform','ios');"
        } else {
            "document.documentElement.setAttribute('data-platform','android');"
        };
        document::eval(&format!("{marker}{MOBILE_VIEWPORT_GUARD}"));
    });

    // On desktop with the translate stack, release the resident llama/Metal
    // models as the window closes. ggml's Metal device destructor asserts
    // (`Abort trap: 6`) if any GPU resource is still alive when the process
    // exits, and the translation worker thread that owns them is never unwound
    // by `exit()` — so free them here, while the runtime is still healthy, and
    // let the process tear down cleanly.
    #[cfg(all(feature = "translate", not(any(target_os = "ios", target_os = "android"))))]
    {
        use dioxus::desktop::tao::event::{Event, WindowEvent};
        dioxus::desktop::use_wry_event_handler(|event, _target| {
            let closing = match event {
                Event::WindowEvent {
                    event: WindowEvent::Destroyed,
                    ..
                } => true,
                Event::LoopDestroyed => true,
                _ => false,
            };
            if closing {
                crate::translate::shutdown();
            }
        });
    }

    rsx! {
        // dx's built-in HTML template ships a bare `width=device-width,
        // initial-scale=1` viewport. Adding viewport-fit=cover (WebKit merges
        // duplicate viewport metas, last descriptor wins) makes the WKWebView
        // layout viewport the full screen so `100dvh`/fixed elements below can
        // trust it — otherwise the viewport/visible-area mismatch lets
        // document panning drag the fixed bottom tab bar during scrolls.
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Style { "{CSS}" }
        AppShell {}
        // Mobile-only startup splash; renders nothing on desktop (see Splash).
        {Splash()}
    }
}

#[component]
fn AppShell() -> Element {
    let mut page: Signal<Page> = use_context();

    // Global navigation shortcuts: Cmd/Ctrl+1..9 jump between pages (in the
    // order of `Page::shortcut_pages`), Cmd/Ctrl+K jumps to the dictionary
    // search. Handled at the app root so it works from any page (key events
    // bubble up from focused descendants).
    let onkeydown = move |evt: Event<KeyboardData>| {
        let mods = evt.modifiers();
        if !(mods.meta() || mods.ctrl()) {
            return;
        }
        if evt.code() == Code::KeyK {
            evt.prevent_default();
            evt.stop_propagation();
            page.set(Page::Dictionary);
            return;
        }
        let idx = match evt.code() {
            Code::Digit1 | Code::Numpad1 => 0,
            Code::Digit2 | Code::Numpad2 => 1,
            Code::Digit3 | Code::Numpad3 => 2,
            Code::Digit4 | Code::Numpad4 => 3,
            Code::Digit5 | Code::Numpad5 => 4,
            Code::Digit6 | Code::Numpad6 => 5,
            Code::Digit7 | Code::Numpad7 => 6,
            Code::Digit8 | Code::Numpad8 => 7,
            Code::Digit9 | Code::Numpad9 => 8,
            _ => return,
        };
        if let Some(&target) = Page::shortcut_pages().get(idx) {
            evt.prevent_default();
            evt.stop_propagation();
            page.set(target);
        }
    };

    rsx! {
        div { class: "app", tabindex: "-1", onkeydown,
            NavBar {}
            main { class: "content",
                match page() {
                    Page::Dictionary => rsx! { pages::dictionary::Dictionary {} },
                    Page::Categories => rsx! { pages::categories::Categories {} },
                    Page::Study => rsx! { pages::study::Study {} },
                    Page::Review => rsx! { pages::review::Review {} },
                    Page::Wordbook => rsx! { pages::wordbook::Wordbook {} },
                    Page::Quiz => rsx! { pages::quiz::Quiz {} },
                    Page::Roots => rsx! { pages::roots::Roots {} },
                    Page::Stats => rsx! { pages::stats::Stats {} },
                    Page::Translate => rsx! { pages::translate::Translate {} },
                    Page::Settings => rsx! { pages::settings::SettingsPage {} },
                    Page::Mine => rsx! { pages::mine::Mine {} },
                    Page::Consolidate => rsx! { pages::consolidate::Consolidate {} },
                }
            }
            MobileTabBar {}
        }
    }
}
