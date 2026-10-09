use chrono::{Datelike, Duration, Local, NaiveDate};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const CSS: &str = include_str!("../assets/date-range.css");
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
const PRESETS: [&str; 8] = [
    "Today",
    "Yesterday",
    "Last week",
    "Last month",
    "Last quarter",
    "This week",
    "This month",
    "Last 7 days",
];

fn parse(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}
fn month_shift(date: NaiveDate, offset: i32) -> NaiveDate {
    let month = date.year() * 12 + date.month0() as i32 + offset;
    NaiveDate::from_ymd_opt(month.div_euclid(12), month.rem_euclid(12) as u32 + 1, 1).unwrap()
}
fn preset(index: usize, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let week = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    match index {
        1 => (today - Duration::days(1), today - Duration::days(1)),
        2 => (week - Duration::days(7), week - Duration::days(1)),
        3 => (
            month_shift(today, -1),
            today.with_day(1).unwrap() - Duration::days(1),
        ),
        4 => {
            let quarter =
                NaiveDate::from_ymd_opt(today.year(), today.month0() / 3 * 3 + 1, 1).unwrap();
            (month_shift(quarter, -3), quarter - Duration::days(1))
        }
        5 => (week, today),
        6 => (today.with_day(1).unwrap(), today),
        7 => (today - Duration::days(6), today),
        _ => (today, today),
    }
}
fn close(id: &str) {
    let id = serde_json::to_string(id).unwrap();
    let _ = document::eval(&format!("window.KayDateRange?.close({id})"));
}

/// Emits a complete, ordered ISO date pair only after a preset or second day is chosen.
/// Native popovers provide light dismissal and escape handling without clipping in scroll panels.
#[component]
pub fn DateRangePicker(
    from: String,
    to: String,
    onchange: EventHandler<(String, String)>,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] allow_empty: bool,
) -> Element {
    let today = Local::now().date_naive();
    let start = parse(&from).unwrap_or(today);
    let end = parse(&to).unwrap_or(start);
    let initial = end.with_day(1).unwrap();
    let mut month = use_signal(move || initial);
    let mut pending = use_signal(|| None::<NaiveDate>);
    let mut hovered = use_signal(|| None::<NaiveDate>);
    let id = use_signal(|| format!("kay-date-range-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)))();
    use_effect(|| {
        let _ = document::eval(include_str!("../assets/date-range.js"));
    });
    let has_range = parse(&from).is_some() && parse(&to).is_some();
    let label = if !has_range && allow_empty {
        "All dates".into()
    } else {
        format!("{} – {}", start.format("%d %b %y"), end.format("%d %b %y"))
    };
    let first = month() - Duration::days(month().weekday().num_days_from_monday() as i64);
    let draft = pending();
    let preview_end = hovered().or(draft).unwrap_or(end);
    let (a, b) = if let Some(anchor) = draft {
        (anchor.min(preview_end), anchor.max(preview_end))
    } else {
        (start.min(end), start.max(end))
    };
    let month_title = month().format("%B %Y").to_string();
    let hint = draft
        .map(|d| format!("{} · Choose end date", d.format("%d %b %Y")))
        .unwrap_or_else(|| "Choose start date, then end date".into());
    rsx! {
        style { "{CSS}" }
        div { class: "date_range_picker",
            span { class: "date_range_label", "Date range" }
            button {
                r#type: "button", class: "date_range_trigger", disabled,
                "popovertarget": "{id}", aria_haspopup: "dialog", aria_expanded: "false", aria_controls: "{id}",
                onclick: move |_| { pending.set(None); hovered.set(None); month.set(end.with_day(1).unwrap()); },
                crate::icons::Icon { name: "calendar" }
                span { "{label}" }
                span { class: "date_range_chevron", aria_hidden: "true", crate::icons::Icon { name: "chevron_down" } }
            }
            div { id: "{id}", class: "date_range_popup", "popover": "auto", role: "dialog", aria_label: "Choose date range",
                div { class: "date_range_presets",
                    for (index, name) in PRESETS.iter().enumerate() {
                        button {
                            r#type: "button", class: "date_range_preset", disabled,
                            aria_pressed: has_range && draft.is_none() && (start, end) == preset(index, today),
                            onclick: {
                                let id = id.clone();
                                move |_| { let (a,b) = preset(index, Local::now().date_naive()); onchange.call((a.to_string(), b.to_string())); close(&id); }
                            }, "{name}"
                        }
                    }
                    button { r#type: "button", class: "date_range_reset", disabled,
                        title: if allow_empty {"Reset to all dates"} else {"Reset to today"}, onclick: { let id = id.clone(); move |_| { let date=if allow_empty {String::new()} else {Local::now().date_naive().to_string()}; onchange.call((date.clone(), date)); close(&id); } }, "Reset"
                    }
                }
                div { class: "date_range_calendar",
                    header { class: "date_range_month",
                        strong { "{month_title}" }
                        button { r#type: "button", aria_label: "Previous month", disabled, onclick: move |_| month.set(month_shift(month(), -1)), "‹" }
                        button { r#type: "button", aria_label: "Next month", disabled, onclick: move |_| month.set(month_shift(month(), 1)), "›" }
                    }
                    div { class: "date_range_weekdays", aria_hidden: "true",
                        for day in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] { span { "{day}" } }
                    }
                    div { class: "date_range_days",
                        onmouseleave: move |_| hovered.set(None),
                        for index in 0..42i64 {
                            {let date = first + Duration::days(index);
                             let selected = (has_range || draft.is_some()) && a <= date && date <= b;
                             let endpoint = selected && (date == a || date == b);
                             let class = format!("date_range_day{}{}{}{}{}", if selected {" in_range"} else {""}, if endpoint {" endpoint"} else {""}, if selected && date == a {" range_start"} else {""}, if selected && date == b {" range_end"} else {""}, if date.month() != month().month() {" outside_month"} else {""});
                             let id = id.clone();
                             rsx! { button {
                                key: "{date}", r#type: "button", class, disabled,
                                aria_label: date.format("%A, %d %B %Y").to_string(), aria_pressed: selected,
                                aria_current: if date == today {"date"} else {"false"},
                                onmouseenter: move |_| { if pending().is_some() { hovered.set(Some(date)); } },
                                onfocus: move |_| { if pending().is_some() { hovered.set(Some(date)); } },
                                onclick: move |_| {
                                    if let Some(anchor) = pending() {
                                        onchange.call((anchor.min(date).to_string(), anchor.max(date).to_string()));
                                        pending.set(None); hovered.set(None); close(&id);
                                    } else { pending.set(Some(date)); hovered.set(None); }
                                }, span { "{date.day()}" }
                             }} }
                        }
                    }
                    footer { class: "date_range_hint", role: "status", aria_live: "polite",
                        "{hint}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previous_calendar_periods_handle_year_and_leap_boundaries() {
        let jan = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(
            preset(4, jan),
            (
                NaiveDate::from_ymd_opt(2025, 10, 1).unwrap(),
                NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()
            )
        );
        assert_eq!(
            preset(2, jan),
            (
                NaiveDate::from_ymd_opt(2025, 12, 22).unwrap(),
                NaiveDate::from_ymd_opt(2025, 12, 28).unwrap()
            )
        );
        assert_eq!(
            preset(3, NaiveDate::from_ymd_opt(2024, 3, 5).unwrap()).1,
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
    }
}

#[component]
pub fn DateRangeShortcuts(#[props(default)] disabled:bool, onchange:EventHandler<(String,String)>) -> Element {
    rsx! { div { class:"date_range_shortcuts",
        for (label,days) in [("Today",0i64),("Last 7 days",6),("Last 30 days",29)] {
            button { disabled, onclick:move |_| {
                let today=Local::now().date_naive();
                onchange.call(((today-Duration::days(days)).to_string(),today.to_string()));
            }, "{label}" }
        }
    } }
}
