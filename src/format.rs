//! Number, money, time and size formatting for the labels.

use crate::i18n;

// =============================================================================
// GERMAN NUMBER & TIME FORMATTERS (Pure Unicode Escapes)
// =============================================================================
pub(crate) fn format_de_price(v: f64) -> String {
    if (v - v.round()).abs() < 0.01 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v).replace('.', crate::i18n::dec())
    }
}

/// Small amounts in cents ("7,0 ct" / "7.0\u{00a2}"), from one euro/dollar on as money.
pub(crate) fn fmt_money_ct(ct: f64) -> String {
    if ct >= 100.0 {
        return i18n::money(ct);
    }
    let num = if ct < 1.0 { format!("{:.2}", ct) } else { format!("{:.1}", ct) }.replace('.', crate::i18n::dec());
    if i18n::is_usd() {
        format!("{}\u{00a2}", num)
    } else {
        format!("{} ct", num)
    }
}

/// Money amount in the chosen currency (name kept for history: it is not euro-only).
pub(crate) fn fmt_euro(ct: f64) -> String {
    i18n::money(ct)
}

pub(crate) fn fmt_dur(secs: f64) -> String {
    let m = (secs / 60.0).floor() as u64;
    if m < 60 {
        format!("{} min", m)
    } else {
        format!("{}:{:02} h", m / 60, m % 60)
    }
}

pub(crate) fn fmt_clock(ms: u64) -> String {
    use chrono::{Local, TimeZone};
    if let Some(dt) = Local.timestamp_millis_opt(ms as i64).single() {
        dt.format("%H:%M").to_string()
    } else {
        "--:--".to_string()
    }
}

pub(crate) fn fmt_day(ms: u64) -> String {
    use chrono::{Datelike, Local, TimeZone, Weekday};
    if let Some(dt) = Local.timestamp_millis_opt(ms as i64).single() {
        if crate::i18n::is_en() {
            return dt.format("%a %b %d").to_string();
        }
        let wd = match dt.weekday() {
            Weekday::Mon => "Mo",
            Weekday::Tue => "Di",
            Weekday::Wed => "Mi",
            Weekday::Thu => "Do",
            Weekday::Fri => "Fr",
            Weekday::Sat => "Sa",
            Weekday::Sun => "So",
        };
        format!("{} {:02}.{:02}.", wd, dt.day(), dt.month())
    } else {
        String::new()
    }
}

pub(crate) fn fmt_bytes(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.1} GB", (b as f64) / 1_073_741_824.0).replace('.', crate::i18n::dec())
    } else {
        format!("{:.0} MB", (b as f64) / 1_048_576.0)
    }
}

pub(crate) fn fmt_rate(b: f64) -> String {
    if b >= 1_048_576.0 {
        format!("{:.1} MB/s", b / 1_048_576.0).replace('.', crate::i18n::dec())
    } else {
        format!("{:.0} KB/s", b / 1024.0)
    }
}

pub(crate) fn fmt_rate_short(b: f64) -> String {
    if b >= 1_048_576.0 {
        format!("{:.1}M", b / 1_048_576.0).replace('.', crate::i18n::dec())
    } else {
        format!("{:.0}K", b / 1024.0)
    }
}
