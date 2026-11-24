//! "Now" tab: power chart, where the watts go, the running session and its latest spikes.

use crate::{i18n, ActiveTab, StromWidgetApp};
use crate::format::{fmt_clock, fmt_euro, fmt_money_ct};
use crate::i18n::t;
use crate::power::class_of;
use crate::theme::{draw_legend_dot, faint, kpi, number_color, section_title, ThemePalette};
use eframe::egui::{self, Color32, CursorIcon, FontId, Pos2, Rect, RichText, Rounding, Sense, Vec2};

/// Gap between the segments of the stacked bar.
const SEG_GAP: f32 = 2.0;

impl StromWidgetApp {
    pub(crate) fn render_tab_live(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        self.render_chart_section(ui, pal);
        ui.add_space(16.0);
        self.render_breakdown(ui, pal);
        ui.add_space(16.0);
        self.render_session_facts(ui, pal);
        ui.add_space(16.0);
        self.render_spike_overview(ui, pal);
        ui.add_space(14.0);

        ui.label(
            RichText::new(tr!(
                "Gemessen: CPU-Paket (RAPL), Grafikkarte (NVML). Wirkungsgrad {:.0} % \u{00b7} Tarif {} {}/kWh.", "Measured: CPU package (RAPL), graphics card (NVML). Efficiency {:.0} % \u{00b7} Tariff {} {}/kWh.",
                self.reading.eff * 100.0,
                format!("{:.1}", self.settings.price_ct).replace('.', i18n::dec()),
                i18n::cent_unit()
            ))
            .font(FontId::proportional(10.5))
            .color(faint(pal)),
        );
    }

    /// One stacked bar for the parts, a two-column legend and the measured/estimated line.
    fn render_breakdown(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let r = &self.reading;
        let parts: [(&str, f64, bool, Color32); 5] = [
            (t("Prozessor", "Processor"), r.parts.cpu.w, r.parts.cpu.src == "gem.", pal.action),
            (t("Grafikkarte", "Graphics card"), r.parts.gpu.w, r.parts.gpu.src == "gem.", Color32::from_rgb(0x5A, 0xA9, 0xFF)),
            (t("Restsystem", "Rest of system"), r.parts.rest.w, false, pal.ink_muted),
            (t("Monitore", "Monitors"), r.parts.mon.w, false, Color32::from_rgb(0xB4, 0x8C, 0xFF)),
            (t("Netzteilverlust", "PSU loss"), r.parts.psu.w, false, pal.rule_strong),
        ];

        section_title(
            ui,
            pal,
            &tr!("Wohin die {:.0} W gehen", "Where the {:.0} W go", r.watts),
            t("\u{25cf} gemessen  \u{25cb} gesch\u{00e4}tzt", "\u{25cf} measured  \u{25cb} estimated"),
        );

        let total: f64 = parts.iter().map(|p| p.1.max(0.0)).sum::<f64>().max(1.0);
        let bar_w = ui.available_width();
        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 10.0), Sense::hover());
        let shown = parts.iter().filter(|p| p.1 > 0.5).count().max(1);
        let usable = bar_w - SEG_GAP * (shown as f32 - 1.0);
        let mut x = bar_rect.left();
        for (name, w, _, col) in parts.iter().filter(|p| p.1 > 0.5) {
            let seg_w = (usable * (*w / total) as f32).max(2.0);
            let seg = Rect::from_min_size(Pos2::new(x, bar_rect.top()), Vec2::new(seg_w, bar_rect.height()));
            ui.painter().rect_filled(seg, Rounding::same(2.5), *col);
            ui.interact(seg, egui::Id::new(("seg", *name)), Sense::hover())
                .on_hover_text(format!("{} {:.0} W", name, w));
            x += seg_w + SEG_GAP;
        }

        ui.add_space(8.0);
        ui.columns(2, |cols| {
            for (i, (name, w, measured, col)) in parts.iter().enumerate() {
                let ui = &mut cols[i % 2];
                let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
                draw_legend_dot(ui.painter(), Pos2::new(row.left() + 4.0, row.center().y), *col, *measured);
                ui.painter().text(
                    Pos2::new(row.left() + 14.0, row.center().y),
                    egui::Align2::LEFT_CENTER,
                    *name,
                    FontId::proportional(11.5),
                    pal.ink_muted,
                );
                ui.painter().text(
                    Pos2::new(row.right() - 4.0, row.center().y),
                    egui::Align2::RIGHT_CENTER,
                    format!("{:.0} W", w),
                    FontId::proportional(11.5),
                    pal.ink,
                );
            }
        });

        ui.add_space(4.0);
        let diff_pct = if r.measured_w > 0.0 {
            ((r.estimated_w - r.measured_w) / r.measured_w) * 100.0
        } else {
            0.0
        };
        let sign = if diff_pct >= 0.0 { "+" } else { "\u{2212}" };
        let gpu_str = r.gpu_util.map(|u| format!("{:.0} %", u)).unwrap_or_else(|| "\u{2013}".to_string());
        let temp_str = r.gpu_temp.map(|t| format!(", {:.0} \u{00b0}C", t)).unwrap_or_default();
        let fg_str = r
            .fg
            .as_ref()
            .map(|f| tr!(" \u{00b7} Fokus: {}", " \u{00b7} Focus: {}", f))
            .unwrap_or_default();
        ui.label(
            RichText::new(tr!(
                "Sch\u{00e4}tzung {:.0} W ({}{:.0} %) \u{00b7} CPU {:.0} % \u{00b7} GPU {}{}{}", "Estimate {:.0} W ({}{:.0} %) \u{00b7} CPU {:.0} % \u{00b7} GPU {}{}{}",
                r.estimated_w,
                sign,
                diff_pct.abs(),
                r.util,
                gpu_str,
                temp_str,
                fg_str
            ))
            .font(FontId::proportional(11.0))
            .color(pal.ink_muted),
        );
    }

    /// Cost, average and peak of the running session, three across.
    fn render_session_facts(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let s = &self.session_mgr.s;
        let dur_s = self.session_mgr.dur_s();
        // Below a minute the average is mostly start-up noise.
        let (avg_txt, avg_sub) = if dur_s >= 60.0 {
            let avg_w = s.wh / (dur_s / 3600.0);
            (format!("{:.0} W", avg_w), tr!("Klasse {}", "Class {}", class_of(avg_w)))
        } else {
            ("\u{2013}".to_string(), t("ab 1 min", "after 1 min").to_string())
        };
        let kwh = format!("{:.2} kWh", s.wh / 1000.0).replace('.', i18n::dec());
        let (cost, peak_w, peak_t) = (s.cost, s.peak.w, s.peak.t);

        section_title(ui, pal, t("Diese Session", "This session"), &tr!("seit {}", "since {}", fmt_clock(s.start)));
        ui.columns(3, |cols| {
            kpi(&mut cols[0], pal, t("Kosten", "Cost"), &fmt_euro(cost), &kwh, None);
            kpi(&mut cols[1], pal, t("Durchschnitt", "Average"), &avg_txt, &avg_sub, None);
            kpi(
                &mut cols[2],
                pal,
                t("Spitze", "Peak"),
                &format!("{:.0} W", peak_w),
                &tr!("um {}", "at {}", fmt_clock(peak_t)),
                None,
            );
        });
    }

    /// The latest spikes as short rows; a click opens the full spike list at that spike.
    fn render_spike_overview(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let total = self.session_mgr.s.spikes.len();
        let note = if total == 0 { String::new() } else { tr!("{} \u{00b7} alle ansehen", "{} \u{00b7} see all", total) };
        let all = section_title(ui, pal, "Spikes", &note);
        if total > 0 && all.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.open_spike_idx = None;
            self.session_mgr.mark_seen();
            self.active_tab = ActiveTab::Spikes;
        }

        if total == 0 {
            ui.label(
                RichText::new(t(
                    "Noch keine in dieser Session. Ein Spike ist ein Anstieg um mindestens 60 W und 35 % \u{00fc}ber die Grundlast.",
                    "None in this session yet. A spike is a rise of at least 60 W and 35 % above the base load.",
                ))
                .font(FontId::proportional(11.0))
                .color(pal.ink_muted),
            );
            return;
        }

        let mut open: Option<usize> = None;
        for idx in self.overview_spikes() {
            let Some(sp) = self.session_mgr.s.spikes.get(idx) else { continue };
            let row_w = ui.available_width();
            let (row, resp) = ui.allocate_exact_size(Vec2::new(row_w, 36.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(row.expand2(Vec2::new(4.0, 0.0)), Rounding::same(6.0), pal.hover);
            }
            let dot_col = if sp.seen { faint(pal) } else { number_color(sp.peak) };
            ui.painter().circle_filled(Pos2::new(row.left() + 4.0, row.center().y), 3.5, dot_col);
            ui.painter().text(
                Pos2::new(row.left() + 16.0, row.center().y - 7.5),
                egui::Align2::LEFT_CENTER,
                tr!("{} \u{00b7} bis {:.0} W", "{} \u{00b7} up to {:.0} W", fmt_clock(sp.start), sp.peak),
                FontId::proportional(12.0),
                pal.ink,
            );
            let cause_clip = Rect::from_min_max(row.min, Pos2::new(row.right() - 70.0, row.bottom()));
            ui.painter().with_clip_rect(cause_clip).text(
                Pos2::new(row.left() + 16.0, row.center().y + 8.0),
                egui::Align2::LEFT_CENTER,
                sp.cause_line(),
                FontId::proportional(10.5),
                pal.ink_muted,
            );
            ui.painter().text(
                Pos2::new(row.right(), row.center().y),
                egui::Align2::RIGHT_CENTER,
                format!("+{}", fmt_money_ct(sp.extra_ct)),
                FontId::proportional(12.0),
                pal.ink,
            );
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                open = Some(idx);
            }
        }
        if let Some(idx) = open {
            self.open_spike_idx = Some(idx);
            self.session_mgr.mark_seen();
            self.active_tab = ActiveTab::Spikes;
        }
    }
}
