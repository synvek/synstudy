use dioxus::prelude::*;

use crate::components::nav_bar::{MobileTabBar, NavBar};
use crate::components::splash::Splash;
use crate::db::{self, dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::pages;
use crate::state::{DictLookup, Page, Settings, LAST_CATEGORY_KEY, SETTINGS_KEY};

const CSS: &str = include_str!("../assets/main.css");

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
                    Page::Settings => rsx! { pages::settings::SettingsPage {} },
                    Page::Mine => rsx! { pages::mine::Mine {} },
                }
            }
            MobileTabBar {}
        }
    }
}
