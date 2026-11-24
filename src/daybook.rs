//! Day book: energy, cost, running time and network volume per calendar day, plus 24 hourly
//! energy values. Written to `days.json` next to the sessions, so month and year figures need
//! no reading of session files. Filled live from every sample; on the first start it is
//! rebuilt once from the session archives (energy only; network was not recorded before).

use crate::session::SessionFileArchive;
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Save at most this often (ms) during normal running; `flush` saves at once.
const SAVE_EVERY_MS: u64 = 60_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DayStats {
    /// Energy in Wh.
    pub wh: f64,
    /// Cost in cents (same unit as the sessions).
    pub cost: f64,
    /// Seconds the widget measured on this day.
    pub on_s: f64,
    /// Bytes received / sent.
    pub rx: f64,
    pub tx: f64,
    /// Highest power (W) and fastest download (bytes/s) with their time (ms).
    pub peak_w: f64,
    pub peak_rx: f64,
    pub peak_rx_t: u64,
    /// Energy in Wh per local hour 0..23.
    pub hours: [f64; 24],
}

impl DayStats {
    /// Average power while measured, in W.
    pub fn avg_w(&self) -> f64 {
        if self.on_s > 0.0 {
            self.wh / (self.on_s / 3600.0)
        } else {
            0.0
        }
    }
}

pub struct DayBook {
    path: PathBuf,
    /// Key: local date. BTreeMap keeps the days in order for ranges.
    days: BTreeMap<NaiveDate, DayStats>,
    last_save: u64,
    dirty: bool,
}

/// Local date and hour of a timestamp in ms.
fn local_day_hour(ms: u64) -> Option<(NaiveDate, usize)> {
    let dt = Local.timestamp_millis_opt(ms as i64).single()?;
    Some((dt.date_naive(), dt.hour() as usize))
}

impl DayBook {
    /// A book with no days that is never saved over an existing file before `load` replaced it.
    pub fn empty(dir: &Path) -> Self {
        Self { path: dir.join("days.json"), days: BTreeMap::new(), last_save: u64::MAX, dirty: false }
    }

    /// Loads `days.json` from `dir`; if it is missing, rebuilds it from the session archives.
    pub fn load(dir: &Path, sessions_dir: &Path) -> Self {
        let path = dir.join("days.json");
        let stored: Option<BTreeMap<NaiveDate, DayStats>> = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok());
        let mut book = Self {
            path,
            days: stored.clone().unwrap_or_default(),
            last_save: 0,
            dirty: false,
        };
        if stored.is_none() {
            book.backfill(sessions_dir);
            book.flush();
        }
        book
    }

    /// One-time rebuild from the per-minute averages in the session archives.
    fn backfill(&mut self, sessions_dir: &Path) {
        let Ok(entries) = fs::read_dir(sessions_dir) else { return };
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(arch) = fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<SessionFileArchive>(&raw).ok())
            else {
                continue;
            };
            // The price that session was billed at, so old days keep their real cost.
            let ct_per_wh = if arch.summary.wh > 0.0 { arch.summary.cost / arch.summary.wh } else { 0.0 };
            for m in &arch.minutes {
                let Some((day, hour)) = local_day_hour(m.t) else { continue };
                let wh = m.avg / 60.0;
                let d = self.days.entry(day).or_default();
                d.wh += wh;
                d.cost += wh * ct_per_wh;
                d.on_s += 60.0;
                d.hours[hour] += wh;
                d.peak_w = d.peak_w.max(m.max);
            }
        }
    }

    /// Adds one sample: `dt_s` seconds at `watts`, priced at `price_ct` per kWh, with the
    /// network rates in bytes/s.
    pub fn add(&mut self, now_ms: u64, dt_s: f64, watts: f64, price_ct: f64, rx_rate: f64, tx_rate: f64) {
        let Some((day, hour)) = local_day_hour(now_ms) else { return };
        let wh = watts * dt_s / 3600.0;
        let d = self.days.entry(day).or_default();
        d.wh += wh;
        d.cost += wh / 1000.0 * price_ct;
        d.on_s += dt_s;
        d.hours[hour] += wh;
        d.rx += rx_rate * dt_s;
        d.tx += tx_rate * dt_s;
        d.peak_w = d.peak_w.max(watts);
        if rx_rate > d.peak_rx {
            d.peak_rx = rx_rate;
            d.peak_rx_t = now_ms;
        }
        self.dirty = true;
        if now_ms.saturating_sub(self.last_save) > SAVE_EVERY_MS {
            self.last_save = now_ms;
            self.flush();
        }
    }

    /// Writes the file if anything changed (atomic: temp file, then rename).
    pub fn flush(&mut self) {
        let tmp = self.path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_string(&self.days) {
            if fs::write(&tmp, json).is_ok() && fs::rename(&tmp, &self.path).is_ok() {
                self.dirty = false;
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn day(&self, d: NaiveDate) -> Option<&DayStats> {
        self.days.get(&d)
    }

    /// Days from `from` to `to` (both included) that have data.
    pub fn range(&self, from: NaiveDate, to: NaiveDate) -> impl Iterator<Item = (&NaiveDate, &DayStats)> {
        self.days.range(from..=to)
    }

    /// Sum over a range of days.
    pub fn total(&self, from: NaiveDate, to: NaiveDate) -> DayStats {
        let mut t = DayStats::default();
        for (_, d) in self.range(from, to) {
            t.wh += d.wh;
            t.cost += d.cost;
            t.on_s += d.on_s;
            t.rx += d.rx;
            t.tx += d.tx;
            t.peak_w = t.peak_w.max(d.peak_w);
            if d.peak_rx > t.peak_rx {
                t.peak_rx = d.peak_rx;
                t.peak_rx_t = d.peak_rx_t;
            }
            for h in 0..24 {
                t.hours[h] += d.hours[h];
            }
        }
        t
    }

    /// All days, oldest first (for the CSV export).
    pub fn all(&self) -> impl Iterator<Item = (&NaiveDate, &DayStats)> {
        self.days.iter()
    }

    /// The day with the most network volume in a range.
    /// First day in the range with network data. Archived sessions carry none, so the network
    /// history only starts with this version; averages must not count the days before.
    pub fn first_net_day(&self, from: NaiveDate, to: NaiveDate) -> Option<NaiveDate> {
        self.range(from, to).find(|(_, d)| d.rx + d.tx > 0.0).map(|(k, _)| *k)
    }

    /// First recorded day in the range (any running time or cost).
    pub fn first_day(&self, from: NaiveDate, to: NaiveDate) -> Option<NaiveDate> {
        self.range(from, to).find(|(_, d)| d.on_s > 0.0 || d.cost > 0.0).map(|(k, _)| *k)
    }

    pub fn busiest_net_day(&self, from: NaiveDate, to: NaiveDate) -> Option<(NaiveDate, f64)> {
        self.range(from, to)
            .map(|(k, d)| (*k, d.rx + d.tx))
            .filter(|(_, v)| *v > 0.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// Expected cost (cents) of the whole month that `today` lies in, from the days recorded so
/// far. Used for "Projection" in History and for the budget warning.
pub fn month_projection(book: &DayBook, today: NaiveDate) -> f64 {
    let now = Local::now();
    let day_frac = if now.date_naive() == today { now.num_seconds_from_midnight() as f64 / 86_400.0 } else { 1.0 };
    month_projection_at(book, today, day_frac)
}

/// Completed days the daily rate is taken from: two weeks even out weekday and weekend.
const RATE_DAYS: i64 = 14;

/// `month_projection` at a given point of `today` (0.0 = midnight, 1.0 = day over).
///
/// Spent so far + the rest of today + a typical day for each day still to come. The typical
/// day is the average of the last `RATE_DAYS` completed days (counted from the first recorded
/// day, so a fresh install is not diluted; days the PC stayed off count as zero). How much of
/// today is still ahead comes from the hourly profile of those days, so an evening with
/// the PC usually off does not add a full day's share. Without any completed day yet, today
/// is extrapolated from the time elapsed.
pub(crate) fn month_projection_at(book: &DayBook, today: NaiveDate, day_frac: f64) -> f64 {
    let so_far = book.total(month_start(today), today).cost;
    let today_cost = book.day(today).map_or(0.0, |d| d.cost);
    let days_after = (days_in_month(today) - today.day()) as f64;
    let day_frac = day_frac.clamp(0.0, 1.0);

    let to = today - chrono::Duration::days(1);
    let from = to - chrono::Duration::days(RATE_DAYS - 1);
    let Some(first) = book.first_day(from, to) else {
        // Nothing before today: today so far, scaled to a whole day (at least one hour seen).
        let rate = today_cost / day_frac.max(1.0 / 24.0);
        return so_far + rate * (1.0 - day_frac) + rate * days_after;
    };
    let n = (to - first).num_days() as f64 + 1.0;
    let rate = book.total(first, to).cost / n;

    // Share of a typical day's energy that falls after `day_frac`.
    let mut profile = [0.0f64; 24];
    for (_, d) in book.range(first, to) {
        for (p, h) in profile.iter_mut().zip(d.hours.iter()) {
            *p += h;
        }
    }
    let total: f64 = profile.iter().sum();
    let rest_share = if total > 0.0 {
        let pos = day_frac * 24.0;
        let hour = (pos as usize).min(23);
        let in_hour = profile[hour] * (1.0 - (pos - hour as f64));
        (in_hour + profile[hour + 1..].iter().sum::<f64>()) / total
    } else {
        1.0 - day_frac
    };
    so_far + rate * rest_share + rate * days_after
}

/// Today's local date.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// First day of the month of `d`.
pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// Number of days in the month of `d`.
pub fn days_in_month(d: NaiveDate) -> u32 {
    let first = month_start(d);
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    next.map(|n| (n - first).num_days() as u32).unwrap_or(30)
}

/// Same day one month earlier (clamped to the end of that month).
pub fn month_before(d: NaiveDate) -> NaiveDate {
    let (y, m) = if d.month() == 1 { (d.year() - 1, 12) } else { (d.year(), d.month() - 1) };
    let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(d);
    let day = d.day().min(days_in_month(first));
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_helpers() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        assert_eq!(days_in_month(d), 31);
        assert_eq!(month_before(d), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(month_before(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()), NaiveDate::from_ymd_opt(2025, 12, 15).unwrap());
        assert_eq!(days_in_month(NaiveDate::from_ymd_opt(2028, 2, 1).unwrap()), 29);
    }

    fn book_with(days: &[(NaiveDate, f64, [f64; 24])]) -> DayBook {
        let mut b = DayBook { path: PathBuf::new(), days: BTreeMap::new(), last_save: u64::MAX, dirty: false };
        for (d, cost, hours) in days {
            b.days.insert(*d, DayStats { cost: *cost, on_s: 3600.0, wh: hours.iter().sum(), hours: *hours, ..Default::default() });
        }
        b
    }

    #[test]
    fn projection_uses_typical_day_and_rest_of_today() {
        let flat = [10.0; 24];
        let june = |d| NaiveDate::from_ymd_opt(2026, 6, d).unwrap();
        // 14 completed days at 100 ct, today half done with 50 ct: 1450 + 50 + 15 x 100.
        let mut days: Vec<_> = (1..=14).map(|d| (june(d), 100.0, flat)).collect();
        days.push((june(15), 50.0, flat));
        let p = month_projection_at(&book_with(&days), june(15), 0.5);
        assert!((p - 3000.0).abs() < 1e-6, "{p}");

        // PC usually only on in the morning: at noon nothing of the day is left.
        let mut morning = [0.0; 24];
        morning[8] = 50.0;
        morning[9] = 50.0;
        let mut days: Vec<_> = (1..=14).map(|d| (june(d), 100.0, morning)).collect();
        days.push((june(15), 100.0, morning));
        let p = month_projection_at(&book_with(&days), june(15), 0.5);
        assert!((p - 3000.0).abs() < 1e-6, "{p}");
    }

    #[test]
    fn projection_edge_cases() {
        let june = |d| NaiveDate::from_ymd_opt(2026, 6, d).unwrap();
        // First day of the month, nothing before: 30 ct by 6:00 -> 120 ct a day x 30 days.
        let p = month_projection_at(&book_with(&[(june(1), 30.0, [0.0; 24])]), june(1), 0.25);
        assert!((p - 3600.0).abs() < 1e-6, "{p}");
        // Recording started three days ago: the rate comes from those three days only.
        let b = book_with(&[(june(12), 90.0, [0.0; 24]), (june(13), 90.0, [0.0; 24]), (june(14), 90.0, [0.0; 24])]);
        let p = month_projection_at(&b, june(15), 1.0);
        assert!((p - (270.0 + 15.0 * 90.0)).abs() < 1e-6, "{p}");
        // Empty book: no forecast beyond zero.
        assert_eq!(month_projection_at(&book_with(&[]), june(15), 0.5), 0.0);
    }

    #[test]
    fn add_splits_into_days_and_hours() {
        let dir = std::env::temp_dir().join(format!("sw-daybook-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mut b = DayBook { path: dir.join("days.json"), days: BTreeMap::new(), last_save: u64::MAX, dirty: false };
        let t = Local.with_ymd_and_hms(2026, 11, 24, 21, 30, 0).single().unwrap().timestamp_millis() as u64;
        b.add(t, 3600.0, 200.0, 35.0, 1000.0, 10.0);
        let d = b.day(NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()).unwrap();
        assert!((d.wh - 200.0).abs() < 1e-9);
        assert!((d.cost - 7.0).abs() < 1e-9);
        assert!((d.hours[21] - 200.0).abs() < 1e-9);
        assert!((d.rx - 3_600_000.0).abs() < 1e-6);
        assert!((d.avg_w() - 200.0).abs() < 1e-9);
        let _ = fs::remove_dir_all(&dir);
    }
}
