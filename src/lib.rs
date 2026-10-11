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
    dioxus::launch(app::App);
}
