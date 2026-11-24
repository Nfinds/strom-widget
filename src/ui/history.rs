//! History tab: day / week / month / year from the day book (averages, projection, budget),
//! when the PC draws power (weekday x hour), and the completed sessions.

use crate::daybook::{self, days_in_month, month_before, month_start, DayStats};
use crate::format::{fmt_clock, fmt_day, fmt_dur, fmt_euro};
use crate::i18n::{self, t};
use crate::power::class_of;
use crate::session::SessionSummary;
use crate::theme::{card, class_bg_color, draw_class_arrow, kpi, section_title, ThemePalette};
use crate::ui::bars::{bar_chart, heatmap, segmented, Bar};
use crate::{ActiveTab, HistPeriod, StromWidgetApp};
use chrono::{Datelike, Duration as Days, Local, NaiveDate, Timelike};
use eframe::egui::{self, Align, Color32, CursorIcon, FontId, Layout, Pos2, RichText, Rounding, Sense, Vec2};

/// Warning color for "more than before" / "over budget".
const WARN: Color32 = Color32::from_rgb(0xF5, 0xB3, 0x01);
const OVER: Color32 = Color32::from_rgb(0xFF, 0x4D, 0x4D);

/// Short money for chart axes: cents below one unit, whole units above.
pub(crate) fn short_money(ct: f64) -> String {
    let usd = i18n::is_usd();
    if ct < 100.0 {
        if usd { format!("{:.0}\u{00a2}", ct) } else { format!("{:.0} ct", ct) }
    } else {
        let v = ct / 100.0;
        let num = if v < 10.0 { format!("{:.1}", v).replace('.', i18n::dec()) } else { format!("{:.0}", v) };
        if usd { format!("${}", num) } else { format!("{} \u{20ac}", num) }
    }
}

pub(crate) fn fmt_kwh(wh: f64) -> String {
    format!("{:.1} kWh", wh / 1000.0).replace('.', i18n::dec())
}

/// "+8 %" / "−5 %" of `now` against `before`, with a color: more is a warning.
fn delta(now: f64, before: f64) -> Option<(String, Color32)> {
    if before <= 0.0 {
        return None;
    }
    let pct = (now - before) / before * 100.0;
    let sign = if pct >= 0.0 { "+" } else { "\u{2212}" };
    Some((format!("{}{:.0} %", sign, pct.abs()), if pct > 3.0 { WARN } else { Color32::from_rgb(0x22, 0xC5, 0x5E) }))
}

fn weekday_short(d: NaiveDate) -> &'static str {
    let de = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
    let en = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let i = d.weekday().num_days_from_monday() as usize;
    if i18n::is_en() { en[i] } else { de[i] }
}

fn month_name(m: u32) -> &'static str {
    const DE: [&str; 12] = ["Januar", "Februar", "M\u{00e4}rz", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
    const EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let i = (m.clamp(1, 12) - 1) as usize;
    if i18n::is_en() { EN[i] } else { DE[i] }
}

fn day_tip(d: NaiveDate, s: &DayStats) -> String {
    format!("{} {:02}.{:02}. \u{00b7} {} \u{00b7} {}", weekday_short(d), d.day(), d.month(), fmt_kwh(s.wh), fmt_euro(s.cost))
}

/// What one period of the History tab shows.
struct PeriodView {
    label: String,
    kpis: [(String, String, String, Option<Color32>); 3],
    bars: Vec<Bar>,
    avg: Option<f64>,
}

impl StromWidgetApp {
    pub(crate) fn render_tab_sessions(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let today = daybook::today();

        // Period switch with the period's name on the left.
        let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
        let periods = [HistPeriod::Day, HistPeriod::Week, HistPeriod::Month, HistPeriod::Year];
        let sel = periods.iter().position(|p| *p == self.hist_period).unwrap_or(2);
        if let Some(i) = segmented(ui, pal, row, "hist_period", &[t("Tag", "Day"), t("Woche", "Week"), t("Monat", "Month"), t("Jahr", "Year")], sel) {
            self.hist_period = periods[i];
        }
        let view = self.period_view(today);
        ui.painter().text(
            Pos2::new(row.left(), row.center().y),
            egui::Align2::LEFT_CENTER,
            &view.label,
            FontId::proportional(13.0),
            pal.ink,
        );
        ui.add_space(12.0);

        ui.columns(3, |cols| {
            for (i, (l, v, s, c)) in view.kpis.iter().enumerate() {
                kpi(&mut cols[i], pal, l, v, s, *c);
            }
        });
        ui.add_space(12.0);
        bar_chart(ui, pal, &view.bars, view.avg, 124.0, pal.action, &short_money);
        ui.add_space(18.0);

        self.render_budget(ui, pal, today);
        ui.add_space(18.0);
        self.render_heatmap(ui, pal, today);
        ui.add_space(18.0);
        self.render_session_list(ui, pal);
    }

    fn period_view(&self, today: NaiveDate) -> PeriodView {
        let book = &self.session_mgr.days;
        let empty = DayStats::default();
        match self.hist_period {
            HistPeriod::Day => {
                let d = book.day(today).unwrap_or(&empty);
                let ct_per_wh = if d.wh > 0.0 { d.cost / d.wh } else { self.settings.price_ct / 1000.0 };
                let now_h = Local::now().hour() as usize;
                let bars = (0..24)
                    .map(|h| Bar {
                        value: d.hours[h] * ct_per_wh,
                        label: if h.is_multiple_of(6) { h.to_string() } else { String::new() },
                        tip: format!("{:02}:00 \u{00b7} {:.0} Wh \u{00b7} {}", h, d.hours[h], fmt_euro(d.hours[h] * ct_per_wh)),
                        current: h == now_h,
                        future: h > now_h,
                    })
                    .collect();
                let y = book.day(today - Days::days(1)).unwrap_or(&empty);
                PeriodView {
                    label: tr!("Heute, {}", "Today, {}", fmt_day(Local::now().timestamp_millis() as u64)),
                    kpis: [
                        (t("Heute", "Today").into(), fmt_euro(d.cost), fmt_kwh(d.wh), None),
                        (t("Laufzeit", "Running").into(), fmt_dur(d.on_s), tr!("\u{00d8} {:.0} W", "avg {:.0} W", d.avg_w()), None),
                        (t("Gestern", "Yesterday").into(), fmt_euro(y.cost), fmt_kwh(y.wh), None),
                    ],
                    bars,
                    avg: Some(d.cost / (now_h + 1) as f64),
                }
            }
            HistPeriod::Week => {
                let from = today - Days::days(6);
                let bars: Vec<Bar> = (0..7)
                    .map(|i| {
                        let day = from + Days::days(i);
                        let s = book.day(day).unwrap_or(&empty);
                        Bar { value: s.cost, label: weekday_short(day).into(), tip: day_tip(day, s), current: day == today, future: false }
                    })
                    .collect();
                let week = book.total(from, today);
                let prev = book.total(from - Days::days(7), from - Days::days(1));
                let (d_txt, d_col) = delta(week.cost, prev.cost).map(|(s, c)| (s, Some(c))).unwrap_or_default();
                PeriodView {
                    label: format!("{:02}.{:02}. \u{2013} {:02}.{:02}.", from.day(), from.month(), today.day(), today.month()),
                    kpis: [
                        (t("7 Tage", "7 days").into(), fmt_euro(week.cost), fmt_kwh(week.wh), None),
                        (t("\u{00d8} pro Tag", "Avg per day").into(), fmt_euro(week.cost / 7.0), tr!("\u{00d8} {:.0} W", "avg {:.0} W", week.avg_w()), None),
                        (t("Vorwoche", "Week before").into(), fmt_euro(prev.cost), d_txt, d_col),
                    ],
                    bars,
                    avg: Some(week.cost / 7.0),
                }
            }
            HistPeriod::Month => {
                let first = month_start(today);
                let dim = days_in_month(today);
                let bars: Vec<Bar> = (1..=dim)
                    .filter_map(|n| first.with_day(n))
                    .map(|day| {
                        let s = book.day(day).unwrap_or(&empty);
                        Bar {
                            value: s.cost,
                            label: if day.day() == 1 || day.day().is_multiple_of(5) { day.day().to_string() } else { String::new() },
                            tip: day_tip(day, s),
                            current: day == today,
                            future: day > today,
                        }
                    })
                    .collect();
                let so_far = book.total(first, today);
                let elapsed = today.day() as f64;
                let prev_first = month_start(month_before(today));
                let prev = book.total(prev_first, first - Days::days(1));
                let projection = daybook::month_projection(book, today);
                let vs = match delta(projection, prev.cost) {
                    Some((d, c)) => (tr!("{}: {} \u{00b7} {}", "{}: {} \u{00b7} {}", t("Vormonat", "Last month"), fmt_euro(prev.cost), d), Some(c)),
                    None => (t("bis Monatsende", "by month end").to_string(), None),
                };
                PeriodView {
                    label: format!("{} {}", month_name(today.month()), today.year()),
                    kpis: [
                        (t("\u{00d8} pro Tag", "Avg per day").into(), fmt_euro(so_far.cost / elapsed), tr!("{} \u{00b7} \u{00d8} {:.0} W", "{} \u{00b7} avg {:.0} W", fmt_kwh(so_far.wh / elapsed), so_far.avg_w()), None),
                        (t("Monat bisher", "Month so far").into(), fmt_euro(so_far.cost), fmt_kwh(so_far.wh), None),
                        (t("Hochrechnung", "Projection").into(), fmt_euro(projection), vs.0, vs.1),
                    ],
                    bars,
                    avg: Some(so_far.cost / elapsed),
                }
            }
            HistPeriod::Year => {
                let year = today.year();
                let bars: Vec<Bar> = (1..=12u32)
                    .filter_map(|m| NaiveDate::from_ymd_opt(year, m, 1))
                    .map(|first| {
                        let last = first + Days::days(days_in_month(first) as i64 - 1);
                        let s = book.total(first, last);
                        Bar {
                            value: s.cost,
                            label: month_name(first.month()).chars().next().map(String::from).unwrap_or_default(),
                            tip: format!("{} \u{00b7} {} \u{00b7} {}", month_name(first.month()), fmt_kwh(s.wh), fmt_euro(s.cost)),
                            current: first.month() == today.month(),
                            future: first.month() > today.month(),
                        }
                    })
                    .collect();
                let jan1 = NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or(today);
                let so_far = book.total(jan1, today);
                let months = (today.month() - 1) as f64 + today.day() as f64 / days_in_month(today) as f64;
                let per_month = so_far.cost / months.max(0.03);
                let prev_year = NaiveDate::from_ymd_opt(year - 1, 1, 1)
                    .zip(NaiveDate::from_ymd_opt(year - 1, 12, 31))
                    .map(|(a, b)| book.total(a, b).cost)
                    .unwrap_or(0.0);
                let vs = match delta(per_month * 12.0, prev_year) {
                    Some((d, c)) => (tr!("{}: {} \u{00b7} {}", "{}: {} \u{00b7} {}", year - 1, fmt_euro(prev_year), d), Some(c)),
                    None => (t("bis Jahresende", "by year end").to_string(), None),
                };
                PeriodView {
                    label: year.to_string(),
                    kpis: [
                        (t("Jahr bisher", "Year so far").into(), fmt_euro(so_far.cost), fmt_kwh(so_far.wh), None),
                        (t("\u{00d8} pro Monat", "Avg per month").into(), fmt_euro(per_month), fmt_kwh(so_far.wh / months.max(0.03)), None),
                        (t("Hochrechnung", "Projection").into(), fmt_euro(per_month * 12.0), vs.0, vs.1),
                    ],
                    bars,
                    avg: Some(per_month),
                }
            }
        }
    }

    /// Budget bar for the running month, or a link to set one.
    fn render_budget(&mut self, ui: &mut egui::Ui, pal: &ThemePalette, today: NaiveDate) {
        if self.settings.monthly_budget <= 0.0 {
            let link = section_title(ui, pal, t("Monatsbudget", "Monthly budget"), t("festlegen", "set one"));
            if link.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.active_tab = ActiveTab::Settings;
            }
            return;
        }
        let budget_ct = self.settings.monthly_budget * 100.0;
        let spent = self.session_mgr.days.total(month_start(today), today).cost;
        let projection = daybook::month_projection(&self.session_mgr.days, today);
        let share = (spent / budget_ct) as f32;
        section_title(
            ui,
            pal,
            &tr!("Monatsbudget {}", "Monthly budget {}", fmt_euro(budget_ct)),
            &format!("{} \u{00b7} {:.0} %", fmt_euro(spent), share * 100.0),
        );
        let col = if spent > budget_ct { OVER } else if projection > budget_ct { WARN } else { pal.action };
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
        crate::ui::draw_meter_bar(ui.painter(), bar, share.min(1.0), col, pal.rule);
        ui.add_space(4.0);
        let msg = if spent > budget_ct {
            tr!("Budget um {} \u{00fc}berschritten.", "Budget exceeded by {}.", fmt_euro(spent - budget_ct))
        } else if projection > budget_ct {
            tr!("Bei diesem Tempo landest du bei {}, {} \u{00fc}ber dem Budget.", "At this pace you end at {}, {} over budget.", fmt_euro(projection), fmt_euro(projection - budget_ct))
        } else {
            tr!("Bei diesem Tempo landest du bei {}, unter dem Budget.", "At this pace you end at {}, under budget.", fmt_euro(projection))
        };
        ui.label(RichText::new(msg).font(FontId::proportional(11.0)).color(pal.ink_muted));
    }

    /// Average power per weekday and hour over the last four weeks.
    fn render_heatmap(&mut self, ui: &mut egui::Ui, pal: &ThemePalette, today: NaiveDate) {
        let mut sums = [[0.0_f64; 24]; 7];
        let mut counts = [0u32; 7];
        for (day, s) in self.session_mgr.days.range(today - Days::days(27), today) {
            if s.on_s <= 0.0 {
                continue;
            }
            let wd = day.weekday().num_days_from_monday() as usize;
            counts[wd] += 1;
            for (sum, wh) in sums[wd].iter_mut().zip(s.hours.iter()) {
                *sum += wh;
            }
        }
        if counts.iter().all(|c| *c == 0) {
            return;
        }
        // Wh within one hour equals the average watts of that hour.
        let mut cells = [[0.0_f64; 24]; 7];
        for ((cell_row, sum_row), n) in cells.iter_mut().zip(sums.iter()).zip(counts) {
            if n > 0 {
                for (c, s) in cell_row.iter_mut().zip(sum_row) {
                    *c = s / n as f64;
                }
            }
        }
        section_title(ui, pal, t("Wann der PC Strom zieht", "When the PC draws power"), t("\u{00d8} W je Stunde, 4 Wochen", "avg W per hour, 4 weeks"));
        let names = if i18n::is_en() {
            ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        } else {
            ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]
        };
        heatmap(ui, pal, &cells, &names, pal.action);
    }

    fn render_session_list(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let total = self.session_mgr.session_count();
        let export = section_title(
            ui,
            pal,
            &tr!("Sessions ({})", "Sessions ({})", total),
            t("CSV exportieren", "Export CSV"),
        );
        if export.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.export_note = Some(match self.export_csv() {
                Ok(path) => {
                    let _ = open::that(&path);
                    tr!("Gespeichert: {}", "Saved: {}", path.display())
                }
                Err(e) => tr!("Export fehlgeschlagen: {}", "Export failed: {}", e),
            });
        }
        if let Some(note) = &self.export_note {
            ui.label(RichText::new(note).font(FontId::proportional(10.5)).color(pal.ink_muted));
            ui.add_space(4.0);
        }

        if self.sessions_cache.is_empty() {
            ui.label(
                RichText::new(t(
                    "Noch keine abgeschlossene Session. Die erste entsteht mit \"Gute Nacht\" oder beim Herunterfahren.",
                    "No completed session yet. The first one is created with \"Good night\" or when Windows shuts down.",
                ))
                .font(FontId::proportional(11.5))
                .color(pal.ink_muted),
            );
            return;
        }

        let mut clicked_summary: Option<SessionSummary> = None;
        for s in &self.sessions_cache {
            card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
                let r = ui
                    .horizontal(|ui| {
                        let cls = class_of(s.avg_w);
                        let (b_rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 15.0), Sense::hover());
                        draw_class_arrow(ui.painter(), b_rect, 5.5, class_bg_color(cls), cls, Color32::from_rgb(0x0B, 0x0C, 0x0E), 9.5);
                        ui.label(RichText::new(fmt_day(s.start)).font(FontId::proportional(11.5)).strong().color(pal.ink));
                        ui.label(
                            RichText::new(format!(
                                "{}\u{2013}{} \u{00b7} {} \u{00b7} {}",
                                fmt_clock(s.start),
                                fmt_clock(s.end),
                                fmt_dur(s.dur_s),
                                fmt_kwh(s.wh)
                            ))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink_muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(fmt_euro(s.cost)).font(FontId::proportional(12.5)).strong().color(pal.ink));
                        });
                    })
                    .response
                    .interact(Sense::click());
                ui.label(
                    RichText::new(tr!(
                        "{} \u{00b7} \u{00d8} {:.0} W \u{00b7} {} {}", "{} \u{00b7} avg {:.0} W \u{00b7} {} {}",
                        s.reason,
                        s.avg_w,
                        s.spikes,
                        if s.spikes == 1 { "Spike" } else if i18n::is_en() { "spikes" } else { "Spikes" }
                    ))
                    .font(FontId::proportional(10.5))
                    .color(pal.ink_muted),
                );
                if r.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    clicked_summary = Some(s.clone());
                }
            });
            ui.add_space(4.0);
        }

        if total > self.sessions_cache.len() {
            let more = ui.add(
                egui::Button::new(
                    RichText::new(tr!("{} weitere anzeigen", "Show {} more", (total - self.sessions_cache.len()).min(20)))
                        .font(FontId::proportional(11.0))
                        .color(pal.ink),
                )
                .fill(pal.card)
                .rounding(Rounding::same(6.0)),
            );
            if more.clicked() {
                self.sessions_shown += 20;
                self.sessions_cache = self.session_mgr.list_sessions(self.sessions_shown);
            }
        }

        if let Some(sum) = clicked_summary {
            self.open_summary_view(Some(sum), "Session", false);
        }
    }

    /// Writes all days as CSV (semicolons, decimal comma in German) into the Documents folder.
    fn export_csv(&self) -> Result<std::path::PathBuf, String> {
        let dec = i18n::dec();
        let num = |v: f64, p: usize| format!("{:.*}", p, v).replace('.', dec);
        let mut out = String::from(t(
            "Datum;kWh;Kosten;Laufzeit h;Durchschnitt W;Spitze W;Download GB;Upload GB\n",
            "Date;kWh;Cost;Running h;Average W;Peak W;Download GB;Upload GB\n",
        ));
        for (day, s) in self.session_mgr.days.all() {
            out.push_str(&format!(
                "{};{};{};{};{};{};{};{}\n",
                day,
                num(s.wh / 1000.0, 3),
                num(s.cost / 100.0, 2),
                num(s.on_s / 3600.0, 2),
                num(s.avg_w(), 0),
                num(s.peak_w, 0),
                num(s.rx / 1e9, 3),
                num(s.tx / 1e9, 3),
            ));
        }
        let docs = std::env::var("USERPROFILE")
            .map(|p| std::path::PathBuf::from(p).join("Documents"))
            .ok()
            .filter(|p| p.is_dir())
            .unwrap_or_else(crate::power::Settings::app_data_dir);
        let path = docs.join("Strom-Widget-Verlauf.csv");
        // A BOM makes Excel read the umlauts as UTF-8.
        std::fs::write(&path, format!("\u{feff}{}", out)).map_err(|e| e.to_string())?;
        Ok(path)
    }
}
