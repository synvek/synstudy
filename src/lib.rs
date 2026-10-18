//! Library entry that hosts the whole app.
//!
//! The desktop `[[bin]]` (`src/main.rs`) and the mobile packaging both route
//! through [`run`]. On Android, `dx` links our `[[bin]]` into a fat shared
//! library and `dioxus_desktop::mobile::start_app` `dlsym`s the `main` symbol
//! to launch it. A plain Rust `fn main` already emits a C-callable `main`, so
//! this library must NOT export one too — doing so produced `duplicate symbol:
//! main` at link time. iOS likewise links the `[[bin]]`'s `main` from `main.m`.

mod app;
mod assets;
mod components;
mod db;
mod models;
mod pages;
mod platform;
mod srs;
mod state;
mod tts;

// Screenshot/photo translation stack. Fully gated behind the opt-in `translate`
// feature so the default build never pulls the native OCR/LLM dependencies.
// `capture` (desktop screen grab) is additionally limited to non-mobile targets;
// mobile image input arrives through the webview file picker in Phase 3.
#[cfg(feature = "translate")]
mod translate;
#[cfg(all(feature = "translate", not(any(target_os = "ios", target_os = "android"))))]
mod capture;

/// Application body: initialize platform singletons, then hand off to Dioxus.
///
/// Called by the `[[bin]]`'s `fn main` on every platform.
pub fn run() {
    platform::init();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Without an explicit title dioxus-desktop falls back to "Dioxus App"
        // when the binary isn't built through `dx` (plain `cargo run` doesn't
        // inject DIOXUS_APP_TITLE). Initial size mirrors Dioxus.toml's
        // [desktop] block, which dx only applies on its own builds.
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
        use dioxus::prelude::VirtualDom;
        // NOTE: deliberately no min_inner_size (Dioxus.toml's 900x640 would
        // pin the window above the 768px mobile breakpoint and make the
        // responsive/mobile layout impossible to preview on desktop).
        let window = WindowBuilder::new()
            .with_title("英语学习词典")
            .with_inner_size(LogicalSize::new(1180.0, 800.0));
        // Runtime window / taskbar / Dock icon, embedded from the generated
        // icon set (scripts/gen_icons.py) so `cargo run` shows it too, not
        // just dx-bundled apps.
        let mut config = Config::new().with_window(window);
        if let Ok(icon) = dioxus::desktop::icon_from_memory::<dioxus::desktop::tao::window::Icon>(
            include_bytes!("../assets/icons/app-icon-256.png"),
        ) {
            config = config.with_icon(icon);
        }
        dioxus::desktop::launch::launch_virtual_dom(
            VirtualDom::new(app::App),
            config,
        );
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        // Kill the startup white flash. The in-app splash
        // (components/splash.rs) only paints on the *first Dioxus frame*, but
        // the WKWebView/Android WebView shows its own native canvas from the
        // moment the view is created — white by default — during the gap
        // between process launch and that first frame. We seed the webview
        // background with the splash's dark base (#0d0f1a, main.css `.splash`
        // gradient top) so that pre-frame canvas is dark and the handoff into
        // the splash is seamless. On iOS wry applies this to the container view
        // at creation time, exactly to "prevent flashing when the page loads".
        use dioxus::desktop::Config;
        dioxus::LaunchBuilder::new()
            .with_cfg(Config::new().with_background_color((13, 15, 26, 255)))
            .launch(app::App);
    }
}
