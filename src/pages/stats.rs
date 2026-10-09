use dioxus::prelude::*;

use crate::components::heatmap::Heatmap;
use crate::components::progress_bar::ProgressBar;
use crate::components::streak_card::{RetentionCard, StreakCard};
use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
use crate::state::{day_index, now_ts, DAY_SECS};

/// Heatmap span in weeks (~half a year).
const HEATMAP_WEEKS: usize = 26;
/// Due-forecast horizon in days.
const FORECAST_DAYS: i64 = 7;

fn forecast_label(i: i64) -> String {
    match i {
        0 => "今天".to_string(),
        1 => "明天".to_string(),
        n => format!("+{n}"),
    }
}

fn pct(rate: Option<f64>) -> String {
    match rate {
        Some(r) => format!("{:.0}%", (r * 100.0).round()),
        None => "—".to_string(),
    }
}

#[component]
pub fn Stats() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();

    let cats = use_memo(move || dict.categories());
    let list = cats();
    let now = now_ts();
    let today = day_index(now);

    let mut tot_new = 0i64;
    let mut tot_learning = 0i64;
    let mut tot_reviewing = 0i64;
    let mut tot_mastered = 0i64;
    let mut tot_words = 0i64;
    for c in list.iter() {
        let s = progress.stats(c.id, c.word_count, now);
        tot_new += s.new_count;
        tot_learning += s.learning;
        tot_reviewing += s.reviewing;
        tot_mastered += s.mastered;
        tot_words += c.word_count;
    }

    // Activity / retention aggregates (all pure reads over review_log & progress).
    let since_ts = (today - (HEATMAP_WEEKS as i64) * 7) * DAY_SECS;
    let counts = progress.review_day_counts(since_ts);
    let (cur_streak, longest_streak) = progress.streaks(now);
    let retention = progress.retention_rate(None);
    let review_total = progress.review_total();
    let forecast = progress.due_forecast(now, FORECAST_DAYS);
    let forecast_total: i64 = forecast.iter().sum();
    let fmax = forecast.iter().copied().max().unwrap_or(0).max(1) as f64;

    rsx! {
        div { class: "page stats",
            div { class: "page-head",
                h1 { "学习统计" }
            }

            div { class: "stat-cards",
                div { class: "stat-card", span { class: "sc-num", "{tot_mastered}" } span { class: "sc-lbl", "已掌握" } }
                div { class: "stat-card", span { class: "sc-num", "{tot_reviewing}" } span { class: "sc-lbl", "复习中" } }
                div { class: "stat-card", span { class: "sc-num", "{tot_learning}" } span { class: "sc-lbl", "学习中" } }
                div { class: "stat-card", span { class: "sc-num", "{tot_new}" } span { class: "sc-lbl", "未学" } }
            }

            div { class: "stat-cards triple",
                StreakCard { current: cur_streak, longest: longest_streak }
                RetentionCard { rate: retention, reviews: review_total }
                div { class: "streak-card",
                    div { class: "streak-main",
                        span { class: "streak-num", "{forecast_total}" }
                        span { class: "streak-unit", "词待复习" }
                    }
                    div { class: "streak-sub", "未来 {FORECAST_DAYS} 天到期" }
                }
            }

            section { class: "stat-panel",
                h3 { "复习热力图 · 近 {HEATMAP_WEEKS} 周" }
                Heatmap { counts, today, weeks: HEATMAP_WEEKS }
                div { class: "hm-legend",
                    span { class: "hm-legend-lbl", "少" }
                    for l in 0..=4 {
                        div { key: "{l}", class: "hm-cell hm-{l}" }
                    }
                    span { class: "hm-legend-lbl", "多" }
                }
            }

            section { class: "stat-panel",
                h3 { "未来 {FORECAST_DAYS} 天到期预测" }
                div { class: "forecast",
                    {forecast.iter().enumerate().map(|(i, c)| {
                        let h = (*c as f64 / fmax * 100.0).round();
                        let ii = i as i64;
                        rsx! {
                            div { key: "{i}", class: "fc-col",
                                span { class: "fc-count", "{c}" }
                                div { class: "fc-bar-track",
                                    div { class: "fc-bar", style: "height: {h}%" }
                                }
                                span { class: "fc-label", "{forecast_label(ii)}" }
                            }
                        }
                    })}
                }
            }

            div { class: "stats-table",
                div { class: "stats-row head",
                    span { class: "sr-name", "词库" }
                    span { "未学" }
                    span { "学习中" }
                    span { "复习中" }
                    span { "已掌握" }
                    span { "保持率" }
                    span { class: "sr-bar", "进度" }
                }
                {list.iter().map(|c| {
                    let s = progress.stats(c.id, c.word_count, now);
                    let ret = progress.retention_rate(Some(c.id));
                    let pctv = if c.word_count > 0 {
                        (s.started() as f64 / c.word_count as f64 * 100.0).round()
                    } else { 0.0 };
                    rsx! {
                        div { key: "{c.id}", class: "stats-row",
                            span { class: "sr-name", "{c.name}" }
                            span { "{s.new_count}" }
                            span { "{s.learning}" }
                            span { "{s.reviewing}" }
                            span { "{s.mastered}" }
                            span { "{pct(ret)}" }
                            span { class: "sr-bar",
                                ProgressBar { value: s.started(), max: c.word_count, label: "{pctv}%" }
                            }
                        }
                    }
                })}
                div { class: "stats-row total",
                    span { class: "sr-name", "合计" }
                    span { "{tot_new}" }
                    span { "{tot_learning}" }
                    span { "{tot_reviewing}" }
                    span { "{tot_mastered}" }
                    span { "{pct(retention)}" }
                    span { class: "sr-bar", "{tot_words} 词" }
                }
            }
        }
    }
}
