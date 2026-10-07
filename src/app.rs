use dioxus::prelude::*;

use crate::components::nav_bar::NavBar;
use crate::db::{self, dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::models::Category;
use crate::pages;
use crate::state::{Page, Settings, SETTINGS_KEY};

const CSS: &str = include_str!("../assets/main.css");

/// Root component: wires up shared state and renders the app shell.
#[allow(non_snake_case)]
pub fn App() -> Element {
    // Writable progress store (creates schema on first run).
    use_context_provider(|| ProgressRepo::new(db::progress_db_path()));

    // Read-only dictionary store (path resolved once).
    use_context_provider(|| DictRepo::from_resolved(db::resolve_dict_db()));

    // Navigation + selection + settings.
    use_context_provider(|| Signal::new(Page::Dictionary));
    use_context_provider(|| Signal::new(Option::<Category>::None));
    use_context_provider(|| {
        let repo = ProgressRepo::new(db::progress_db_path());
        let settings = repo
            .get_setting(SETTINGS_KEY)
            .and_then(|v| serde_json::from_str::<Settings>(&v).ok())
            .unwrap_or_default();
        Signal::new(settings)
    });

    rsx! {
        document::Style { "{CSS}" }
        AppShell {}
    }
}

#[component]
fn AppShell() -> Element {
    let page: Signal<Page> = use_context();

    rsx! {
        div { class: "app",
            NavBar {}
            main { class: "content",
                match page() {
                    Page::Dictionary => rsx! { pages::dictionary::Dictionary {} },
                    Page::Categories => rsx! { pages::categories::Categories {} },
                    Page::Study => rsx! { pages::study::Study {} },
                    Page::Review => rsx! { pages::review::Review {} },
                    Page::Stats => rsx! { pages::stats::Stats {} },
                    Page::Settings => rsx! { pages::settings::SettingsPage {} },
                }
            }
        }
    }
}
