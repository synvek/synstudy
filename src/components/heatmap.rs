use std::collections::HashMap;

use dioxus::prelude::*;

use crate::state::weekday_sun0;

/// Intensity bucket for a day's review count (0 = none .. 4 = busiest).
fn level(count: i64) -> usize {
    match count {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        6..=9 => 3,
        _ => 4,
    }
}

fn cell_class(count: i64, future: bool) -> String {
    if future {
        "hm-future".to_string()
    } else {
        format!("hm-{}", level(count))
    }
}

/// Human date label for a UTC day index (days since 1970-01-01).
fn date_label(day: i64) -> String {
    use chrono::{Duration, NaiveDate};
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|d| d.checked_add_signed(Duration::days(day)))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// GitHub-style review activity heatmap.
///
/// `counts` maps a UTC day index to the number of reviews that day, `today` is
/// the current day index, and `weeks` is the number of columns to render. The
/// grid ends with the week containing `today`; days after today render as
/// inactive placeholders so the columns stay aligned to Sun..Sat.
#[component]
pub fn Heatmap(
    counts: HashMap<i64, i64>,
    today: i64,
    #[props(default = 26)] weeks: usize,
) -> Element {
    let weeks = weeks.max(1);
    let start_sunday = today - weekday_sun0(today) - ((weeks - 1) as i64) * 7;
    let weekdays = ["日", "一", "二", "三", "四", "五", "六"];

    rsx! {
        div { class: "heatmap-wrap",
            div { class: "hm-weekdays",
                for (i, lab) in weekdays.iter().enumerate() {
                    span { key: "{i}", class: "hm-wd", "{lab}" }
                }
            }
            div { class: "heatmap",
                for col in 0..weeks as i64 {
                    for row in 0..7i64 {
                        {
                            let day = start_sunday + col * 7 + row;
                            let count = counts.get(&day).copied().unwrap_or(0);
                            let future = day > today;
                            let title = if future {
                                date_label(day)
                            } else {
                                format!("{} · 复习 {} 次", date_label(day), count)
                            };
                            rsx! {
                                div {
                                    key: "{day}",
                                    class: "hm-cell {cell_class(count, future)}",
                                    title: "{title}",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
