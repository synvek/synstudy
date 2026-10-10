pub mod category_tile;
pub mod heatmap;
pub mod nav_bar;
pub mod progress_bar;
pub mod streak_card;
pub mod word_card;
pub mod word_detail;
pub mod zh_detail;

use dioxus::prelude::*;

/// Render a multi-line text block (translation/definition) as stacked divs.
pub fn text_lines(text: &str, class: &'static str) -> Element {
    let lines: Vec<String> = text
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return rsx! { span { class: "muted", "-" } };
    }
    rsx! {
        {lines.into_iter().enumerate().map(|(i, l)| rsx! {
            div { key: "{i}", class: "{class}", "{l}" }
        })}
    }
}

/// Turn ECDICT `exchange` data ("d:went/p:gone/3:goes/i:going") into readable
/// `(label, value)` pairs for the word-form grid.
pub fn exchange_pairs(exchange: &str) -> Vec<(&'static str, String)> {
    let mut parts: Vec<(&'static str, String)> = Vec::new();
    for item in exchange.split('/') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let mut kv = item.splitn(2, ':');
        let (k, v) = match (kv.next(), kv.next()) {
            (Some(k), Some(v)) => (k, v),
            _ => continue,
        };
        let label = match k {
            "p" => "过去式",
            "d" => "过去分词",
            "i" => "现在分词",
            "3" => "第三人称单数",
            "r" => "比较级",
            "t" => "最高级",
            "s" => "复数",
            "0" => "原形(变体)",
            "1" => "变体",
            _ => continue,
        };
        if !v.is_empty() {
            parts.push((label, v.to_string()));
        }
    }
    parts
}
