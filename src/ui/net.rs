//! Network tab: download/upload now with the last 15 minutes, data volume per day with its
//! average and the month before, records and the active connection.

use crate::daybook::{self, days_in_month, month_before, month_start, DayStats};
use crate::format::{fmt_clock, fmt_rate, fmt_volume};
use crate::i18n::t;
use crate::session::now_ms;
use crate::theme::{faint, kpi, section_title, ThemePalette};
use crate::ui::bars::{bar_chart, nice_ceiling, segmented, Bar};
use crate::StromWidgetApp;
use chrono::{Datelike, Duration as Days, NaiveDate};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};

const DOWN_COL: Color32 = Color32::from_rgb(0x5A, 0xA9, 0xFF);
const UP_COL: Color32 = Color32::from_rgb(0xB4, 0x8C, 0xFF);

fn day_label(d: NaiveDate) -> String {
    format!("{:02}.{:02}.", d.day(), d.month())
}

impl StromWidgetApp {
    pub(crate) fn render_tab_net(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // 1. Now: two big figures and the last 15 minutes.
        ui.columns(2, |cols| {
            for (i, (lbl, v, col)) in [
                (t("\u{2193} Download jetzt", "\u{2193} Download now"), self.sys_view.down, DOWN_COL),
                (t("\u{2191} Upload jetzt", "\u{2191} Upload now"), self.sys_view.up, UP_COL),
            ]
            .into_iter()
            .enumerate()
            {
                let ui = &mut cols[i];
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(lbl).font(FontId::proportional(10.5)).color(col));
                ui.label(RichText::new(fmt_rate(v)).font(FontId::proportional(20.0)).strong().color(pal.ink));
            }
        });
        ui.add_space(6.0);
        self.draw_net_lines(ui, pal);
        ui.add_space(18.0);

        // 2. Volume per day.
        let today = daybook::today();
        let book = &self.session_mgr.days;
        let empty = DayStats::default();
        let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
        ui.painter().text(
            Pos2::new(row.left(), row.center().y),
            egui::Align2::LEFT_CENTER,
            t("Datenmenge", "Data volume"),
            FontId::proportional(11.5),
            pal.ink_muted,
        );
        if let Some(i) = segmented(ui, pal, row, "net_period", &[t("Woche", "Week"), t("Monat", "Month")], self.net_month as usize) {
            self.net_month = i == 1;
        }
        ui.add_space(8.0);

        let (from, prev_from, prev_to) = if self.net_month {
            let first = month_start(today);
            // The month before over the same number of days, so the comparison is fair.
            let pf = month_start(month_before(today));
            (first, pf, month_before(today))
        } else {
            let f = today - Days::days(6);
            (f, f - Days::days(7), f - Days::days(1))
        };
        let counted_from = book.first_net_day(from, today).unwrap_or(from);
        let span = (today - counted_from).num_days() as f64 + 1.0;
        let cur = book.total(from, today);
        let prev = book.total(prev_from, prev_to);
        let t_day = book.day(today).unwrap_or(&empty);
        let cur_vol = cur.rx + cur.tx;
        let prev_vol = prev.rx + prev.tx;
        let (vs_txt, vs_col) = if prev_vol > 0.0 {
            let pct = (cur_vol - prev_vol) / prev_vol * 100.0;
            let sign = if pct >= 0.0 { "+" } else { "\u{2212}" };
            (
                tr!("{}{:.0} % zum {}", "{}{:.0} % vs. {}", sign, pct.abs(), if self.net_month { t("Vormonat", "last month") } else { t("Vorwoche", "week before") }),
                Some(faint(pal)),
            )
        } else {
            (String::new(), None)
        };
        ui.columns(3, |cols| {
            kpi(
                &mut cols[0],
                pal,
                t("Heute", "Today"),
                &fmt_volume(t_day.rx + t_day.tx),
                &format!("\u{2193} {} \u{00b7} \u{2191} {}", fmt_volume(t_day.rx), fmt_volume(t_day.tx)),
                None,
            );
            kpi(&mut cols[1], pal, t("\u{00d8} pro Tag", "Avg per day"), &fmt_volume(cur_vol / span), "", None);
            kpi(
                &mut cols[2],
                pal,
                if self.net_month { t("Monat bisher", "Month so far") } else { t("7 Tage", "7 days") },
                &fmt_volume(cur_vol),
                &vs_txt,
                vs_col,
            );
        });
        ui.add_space(12.0);

        let to = if self.net_month { month_start(today) + Days::days(days_in_month(today) as i64 - 1) } else { today };
        let n = (to - from).num_days() + 1;
        let bars: Vec<Bar> = (0..n)
            .map(|i| {
                let day = from + Days::days(i);
                let s = book.day(day).unwrap_or(&empty);
                let label = if self.net_month {
                    if day.day() == 1 || day.day().is_multiple_of(5) { day.day().to_string() } else { String::new() }
                } else {
                    day_label(day)
                };
                Bar {
                    value: s.rx + s.tx,
                    label,
                    tip: format!("{} \u{00b7} \u{2193} {} \u{00b7} \u{2191} {}", day_label(day), fmt_volume(s.rx), fmt_volume(s.tx)),
                    current: day == today,
                    future: day > today,
                }
            })
            .collect();
        bar_chart(ui, pal, &bars, Some(cur_vol / span), 110.0, DOWN_COL, &fmt_volume);
        ui.add_space(18.0);

        // 3. Records within the shown period.
        section_title(ui, pal, t("Rekorde", "Records"), if self.net_month { t("diesen Monat", "this month") } else { t("7 Tage", "7 days") });
        let mut any = false;
        if cur.peak_rx > 0.0 {
            any = true;
            self.record_row(ui, pal, DOWN_COL, t("Schnellster Download", "Fastest download"), &format!("{} \u{00b7} {}", day_label(chrono::DateTime::from_timestamp_millis(cur.peak_rx_t as i64).map(|d| d.with_timezone(&chrono::Local).date_naive()).unwrap_or(today)), fmt_clock(cur.peak_rx_t)), &fmt_rate(cur.peak_rx));
        }
        if let Some((day, vol)) = book.busiest_net_day(from, today) {
            any = true;
            self.record_row(ui, pal, UP_COL, t("Meiste Daten an einem Tag", "Most data in one day"), &day_label(day), &fmt_volume(vol));
        }
        if !any {
            ui.label(
                RichText::new(t("Noch keine Netzdaten. Sie werden ab jetzt jede Minute mitgeschrieben.", "No network data yet. It is recorded every minute from now on."))
                    .font(FontId::proportional(11.0))
                    .color(pal.ink_muted),
            );
        }
        ui.add_space(16.0);

        // 4. Connection and a note on what is not measured.
        if !self.sys_view.net_if.is_empty() {
            section_title(ui, pal, t("Verbindung", "Connection"), "");
            ui.label(RichText::new(&self.sys_view.net_if).font(FontId::proportional(12.0)).color(pal.ink));
            ui.add_space(12.0);
        }
        ui.label(
            RichText::new(t(
                "Verbrauch pro Programm br\u{00e4}uchte Admin-Rechte (Ereignisprotokoll des Kernels); das Widget l\u{00e4}uft bewusst ohne.",
                "Usage per program would need admin rights (kernel event tracing); the widget deliberately runs without.",
            ))
            .font(FontId::proportional(10.5))
            .color(faint(pal)),
        );
    }

    fn record_row(&self, ui: &mut egui::Ui, pal: &ThemePalette, dot: Color32, title: &str, sub: &str, value: &str) {
        let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
        ui.painter().circle_filled(Pos2::new(row.left() + 4.0, row.center().y), 3.5, dot);
        ui.painter().text(Pos2::new(row.left() + 16.0, row.center().y - 7.5), egui::Align2::LEFT_CENTER, title, FontId::proportional(12.0), pal.ink);
        ui.painter().text(Pos2::new(row.left() + 16.0, row.center().y + 8.0), egui::Align2::LEFT_CENTER, sub, FontId::proportional(10.5), pal.ink_muted);
        ui.painter().text(Pos2::new(row.right(), row.center().y), egui::Align2::RIGHT_CENTER, value, FontId::proportional(12.0), pal.ink);
    }

    /// Download and upload of the last 15 minutes as two lines.
    fn draw_net_lines(&self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.0), Sense::hover());
        let plot = Rect::from_min_max(Pos2::new(rect.left() + 38.0, rect.top() + 4.0), Pos2::new(rect.right(), rect.bottom() - 14.0));
        let from = now_ms().saturating_sub(15 * 60 * 1000);
        let pts: Vec<(u64, f64, f64)> = self.session_mgr.live.iter().filter(|s| s.t >= from).map(|s| (s.t, s.rx, s.tx)).collect();
        let peak = pts.iter().map(|p| p.1.max(p.2)).fold(0.0_f64, f64::max);
        // Readable axis end, at least 100 KB/s so an idle line stays flat at the bottom.
        let max = nice_ceiling((peak * 1.1).max(102_400.0));
        let axis_col = faint(pal);
        for f in [0.0, 0.5, 1.0] {
            let y = plot.bottom() - f * plot.height();
            ui.painter().line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0_f32, pal.rule));
            ui.painter().text(Pos2::new(rect.left(), y), egui::Align2::LEFT_CENTER, fmt_rate(max * f as f64), FontId::proportional(9.5), axis_col);
        }
        ui.painter().text(Pos2::new(plot.left(), rect.bottom() - 5.0), egui::Align2::LEFT_CENTER, "\u{2212}15 min", FontId::proportional(9.5), axis_col);
        ui.painter().text(Pos2::new(plot.right(), rect.bottom() - 5.0), egui::Align2::RIGHT_CENTER, t("jetzt", "now"), FontId::proportional(9.5), axis_col);
        if pts.len() < 2 {
            return;
        }
        let span = 15.0 * 60.0 * 1000.0;
        let to_pos = |t: u64, v: f64| {
            Pos2::new(
                plot.left() + ((t.saturating_sub(from)) as f64 / span) as f32 * plot.width(),
                plot.bottom() - (v / max).clamp(0.0, 1.0) as f32 * plot.height(),
            )
        };
        let down: Vec<Pos2> = pts.iter().map(|p| to_pos(p.0, p.1)).collect();
        let up: Vec<Pos2> = pts.iter().map(|p| to_pos(p.0, p.2)).collect();
        ui.painter().add(Shape::line(up, Stroke::new(1.4_f32, UP_COL)));
        ui.painter().add(Shape::line(down, Stroke::new(1.6_f32, DOWN_COL)));
    }
}
