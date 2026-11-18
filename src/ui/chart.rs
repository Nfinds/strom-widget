//! The power chart of the expanded panel: range switcher, peak badge and the bars.

use crate::StromWidgetApp;
use crate::i18n::t;
use crate::session::{now_ms, Spike};
use crate::theme::{double_bezel_card, draw_gradient_bar, load_color, number_color, ThemePalette};
use eframe::egui::{self, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

impl StromWidgetApp {
    pub(crate) fn render_chart_section(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        double_bezel_card(ui, pal, 12.0, Vec2::new(11.0, 8.0), |ui| {
            // Allocate a single 24px tall header strip so Title, Peak Badge, and Time Range Switcher share an exact centerline
            let hdr_w = ui.available_width();
            let (hdr_rect, _) = ui.allocate_exact_size(Vec2::new(hdr_w, 24.0), Sense::hover());
            let cy = hdr_rect.center().y;

            let title = if let Some(ref fz) = self.frozen_chart {
                fz.label.clone()
            } else {
                match self.chart_range_s {
                    900 => t("Leistung \u{00b7} 15 min", "Power \u{00b7} 15 min").to_string(),
                    3600 => t("Leistung \u{00b7} 1 Stunde", "Power \u{00b7} 1 hour").to_string(),
                    _ => t("Leistung \u{00b7} Session", "Power \u{00b7} session").to_string(),
                }
            };
            let title_galley = ui.painter().layout_no_wrap(
                title,
                FontId::proportional(11.5),
                pal.ink_muted,
            );
            let title_w = title_galley.size().x;
            ui.painter().galley(
                Pos2::new(hdr_rect.left(), cy - title_galley.size().y * 0.5),
                title_galley,
                pal.ink_muted,
            );

            // Obsidian Linear Peak Wattage Callout Pill ("Spitze XXX W") aligned to cy
            let peak_w = self.session_mgr.s.peak.w;
            if peak_w > 1.0 {
                let p_col = number_color(peak_w);
                let p_txt = tr!("Spitze {:.0} W", "Peak {:.0} W", peak_w);
                let p_galley = ui.painter().layout_no_wrap(
                    p_txt,
                    FontId::proportional(10.2),
                    p_col,
                );
                let pill_w = p_galley.size().x + 14.0;
                let pill_h = 19.0;
                let pill_rect = Rect::from_min_size(
                    Pos2::new(hdr_rect.left() + title_w + 8.0, cy - pill_h * 0.5),
                    Vec2::new(pill_w, pill_h),
                );
                ui.painter().rect(
                    pill_rect,
                    Rounding::same(9.5),
                    p_col.gamma_multiply(0.12),
                    Stroke::new(1.0_f32, p_col.gamma_multiply(0.32)),
                );
                ui.painter().galley(
                    Pos2::new(
                        pill_rect.center().x - p_galley.size().x * 0.5,
                        cy - p_galley.size().y * 0.5,
                    ),
                    p_galley,
                    p_col,
                );
            }

            // Recessed Segmented Switcher ("15m | 1 h | Session") aligned to cy on the right
            let seg_items: [(u64, &str, f32); 3] = [
                (900, "15m", 34.0),
                (3600, "1 h", 30.0),
                (0, "Session", 50.0),
            ];
            let total_seg_w: f32 = seg_items.iter().map(|x| x.2).sum::<f32>() + 4.0 + 4.0;
            let seg_h = 22.0;
            let seg_outer = Rect::from_min_size(
                Pos2::new(hdr_rect.right() - total_seg_w, cy - seg_h * 0.5),
                Vec2::new(total_seg_w, seg_h),
            );
            ui.painter().rect(
                seg_outer,
                Rounding::same(7.0),
                pal.input_ground,
                Stroke::new(1.0_f32, pal.rule),
            );

            let mut sx = seg_outer.left() + 2.0;
            for (secs, label, bw) in seg_items {
                let active = self.frozen_chart.is_none() && self.chart_range_s == secs;
                let btn_rect = Rect::from_min_size(
                    Pos2::new(sx, cy - (seg_h - 4.0) * 0.5),
                    Vec2::new(bw, seg_h - 4.0),
                );
                let resp = ui.interact(
                    btn_rect,
                    egui::Id::new(("chart_seg", secs)),
                    Sense::click(),
                );
                if active || resp.hovered() {
                    ui.painter().rect(
                        btn_rect,
                        Rounding::same(5.0),
                        if active { pal.hover } else { pal.card },
                        if active {
                            Stroke::new(1.0_f32, pal.rule_strong)
                        } else {
                            Stroke::NONE
                        },
                    );
                }
                ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(10.5),
                    if active { pal.ink } else { pal.ink_muted },
                );
                if resp.clicked() {
                    self.frozen_chart = None;
                    self.chart_range_s = secs;
                }
                sx += bw + 2.0;
            }

            ui.add_space(5.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 78.0), Sense::hover());
            self.draw_history_chart(ui.painter(), rect, pal);
        });
    }

    fn draw_history_chart(&self, painter: &egui::Painter, rect: Rect, pal: &ThemePalette) {
        let now = now_ms();
        let (pts, from, to, spikes): (Vec<(u64, f64)>, u64, u64, &[Spike]) =
            if let Some(ref fz) = self.frozen_chart {
                (fz.points.clone(), fz.from, fz.to, &fz.spikes)
            } else if self.chart_range_s == 0 {
                let p: Vec<(u64, f64)> = if self.session_mgr.s.minutes.len() * 30
                    > self.session_mgr.live.len()
                {
                    self.session_mgr
                        .s
                        .minutes
                        .iter()
                        .map(|m| (m.t + 30_000, m.avg_w()))
                        .collect()
                } else {
                    self.session_mgr.live.iter().map(|s| (s.t, s.w)).collect()
                };
                (
                    p,
                    self.session_mgr.s.start,
                    now,
                    &self.session_mgr.s.spikes,
                )
            } else {
                let from_t = now.saturating_sub(self.chart_range_s * 1000);
                let p: Vec<(u64, f64)> = self
                    .session_mgr
                    .live
                    .iter()
                    .filter(|s| s.t >= from_t)
                    .map(|s| (s.t, s.w))
                    .collect();
                (p, from_t, now, &self.session_mgr.s.spikes)
            };

        let gutter = 32.0;
        let plot_rect = Rect::from_min_max(
            Pos2::new(rect.left() + gutter, rect.top() + 4.0),
            Pos2::new(rect.right(), rect.bottom() - 2.0),
        );

        let peak_w = self.session_mgr.s.peak.w;
        let max_scale = (((peak_w * 1.1) / 100.0).ceil() * 100.0).max(400.0);

        let step = if max_scale > 800.0 { 400.0 } else { 200.0 };
        let mut v = 0.0;
        while v <= max_scale {
            let frac = (v / max_scale) as f32;
            let y = plot_rect.bottom() - frac * plot_rect.height();
            painter.line_segment(
                [Pos2::new(plot_rect.left(), y), Pos2::new(plot_rect.right(), y)],
                Stroke::new(1.0_f32, pal.rule),
            );
            painter.text(
                Pos2::new(rect.left(), y),
                egui::Align2::LEFT_CENTER,
                format!("{:.0}", v),
                FontId::proportional(10.0),
                pal.ink_muted,
            );
            v += step;
        }

        let span = (to.saturating_sub(from)).max(1) as f64;

        for sp in spikes {
            if sp.end < from || sp.start > to {
                continue;
            }
            let f0 = ((sp.start.saturating_sub(from)) as f64 / span).clamp(0.0, 1.0) as f32;
            let f1 = ((sp.end.saturating_sub(from)) as f64 / span).clamp(0.0, 1.0) as f32;
            let x0 = plot_rect.left() + f0 * plot_rect.width();
            let x1 = (plot_rect.left() + f1 * plot_rect.width()).max(x0 + 3.0);
            let sp_rect = Rect::from_min_max(
                Pos2::new(x0, plot_rect.top()),
                Pos2::new(x1, plot_rect.bottom()),
            );
            painter.rect_filled(sp_rect, 0.0, load_color(sp.peak).gamma_multiply(0.20));
        }

        let col_w = 4.5f32;
        let nb = (plot_rect.width() / col_w).floor().max(1.0) as usize;
        let mut buckets: Vec<Option<f64>> = vec![None; nb];
        for (t, w) in pts {
            if t < from || t > to {
                continue;
            }
            let idx = (((t - from) as f64 / span) * (nb as f64)).floor() as usize;
            if idx < nb {
                buckets[idx] = Some(buckets[idx].map_or(w, |prev| prev.max(w)));
            }
        }

        for (i, val) in buckets.into_iter().enumerate() {
            if let Some(w_val) = val {
                let frac = (w_val / max_scale).clamp(0.03, 1.0) as f32;
                let bh = frac * plot_rect.height();
                let x = plot_rect.left() + (i as f32) * col_w;
                let bar_r = Rect::from_min_max(
                    Pos2::new(x, plot_rect.bottom() - bh),
                    Pos2::new(x + col_w - 1.2, plot_rect.bottom()),
                );
                draw_gradient_bar(painter, bar_r, load_color(w_val));
            }
        }
    }
}
