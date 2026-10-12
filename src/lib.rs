//! Library entry that hosts the whole app.
//!
//! The desktop `[[bin]]` (`src/main.rs`) and the mobile packaging both route
//! through [`run`]. On Android, `dx` links our `[[bin]]` into a fat shared
//! library and `dioxus_desktop::mobile::start_app` `dlsym`s the `main` symbol
//! to launch it. A plain Rust `fn main` already emits a C-callable `main`, so
//! this library must NOT export one too — doing so produced `duplicate symbol:
//! main` at link time. iOS likewise links the `[[bin]]`'s `main` from `main.m`.

mod app;
mod components;
mod db;
mod models;
mod pages;
mod platform;
mod srs;
mod state;
mod tts;

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
        dioxus::desktop::launch::launch_virtual_dom(
            VirtualDom::new(app::App),
            Config::new().with_window(window),
        );
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    dioxus::launch(app::App);
}
