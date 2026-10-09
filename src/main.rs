mod app;
mod components;
mod db;
mod models;
mod pages;
mod srs;
mod state;
mod tts;

fn main() {
    dioxus::launch(app::App);
}
