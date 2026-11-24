//! The power chart: title, range switcher and the bars, with the value under the mouse.

use crate::StromWidgetApp;
use crate::format::fmt_clock;
use crate::i18n::t;
use crate::session::{now_ms, Spike};
use crate::theme::{draw_gradient_bar, faint, load_color, ThemePalette};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

/// Width of one bar column in points.
const COL_W: f32 = 4.5;

impl StromWidgetApp {
    pub(crate) fn render_chart_section(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let hdr_w = ui.available_width();
        let (hdr_rect, _) = ui.allocate_exact_size(Vec2::new(hdr_w, 22.0), Sense::hover());
        let cy = hdr_rect.center().y;

        let title = if let Some(ref fz) = self.frozen_chart {
            fz.label.clone()
        } else {
            t("Leistung", "Power").to_string()
        };
        ui.painter().text(
            Pos2::new(hdr_rect.left(), cy),
            egui::Align2::LEFT_CENTER,
            title,
            FontId::proportional(11.5),
            pal.ink_muted,
        );

        // A frozen session chart has no range to pick.
        if self.frozen_chart.is_none() {
            let seg_items: [(u64, &str); 3] = [(900, "15 min"), (3600, "1 h"), (0, "Session")];
            let galleys: Vec<_> = seg_items
                .iter()
                .map(|(_, l)| ui.painter().layout_no_wrap(l.to_string(), FontId::proportional(10.5), Color32::PLACEHOLDER))
                .collect();
            let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 14.0).collect();
            let seg_h = 22.0;
            let total_w: f32 = widths.iter().sum::<f32>() + 4.0;
            let seg_outer = Rect::from_min_size(
                Pos2::new(hdr_rect.right() - total_w, cy - seg_h * 0.5),
                Vec2::new(total_w, seg_h),
            );
            ui.painter().rect(seg_outer, Rounding::same(7.0), pal.input_ground, Stroke::new(1.0_f32, pal.rule));

            let mut sx = seg_outer.left() + 2.0;
            for (((secs, _), galley), bw) in seg_items.iter().zip(galleys).zip(widths) {
                let active = self.chart_range_s == *secs;
                let btn_rect = Rect::from_min_size(Pos2::new(sx, cy - (seg_h - 4.0) * 0.5), Vec2::new(bw, seg_h - 4.0));
                let resp = ui.interact(btn_rect, egui::Id::new(("chart_seg", *secs)), Sense::click());
                if active {
                    ui.painter().rect_filled(btn_rect, Rounding::same(5.0), pal.hover);
                }
                let gs = galley.size();
                ui.painter().galley(
                    btn_rect.center() - gs * 0.5,
                    galley,
                    if active || resp.hovered() { pal.ink } else { pal.ink_muted },
                );
                if resp.clicked() {
                    self.chart_range_s = *secs;
                }
                sx += bw;
            }
        }

        ui.add_space(6.0);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 104.0), Sense::hover());
        self.draw_history_chart(ui.painter(), rect, resp.hover_pos(), pal);
    }

    fn draw_history_chart(&self, painter: &egui::Painter, rect: Rect, hover: Option<Pos2>, pal: &ThemePalette) {
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
                (p, self.session_mgr.s.start, now, &self.session_mgr.s.spikes)
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

        let gutter = 30.0;
        let plot_rect = Rect::from_min_max(
            Pos2::new(rect.left() + gutter, rect.top() + 4.0),
            Pos2::new(rect.right(), rect.bottom() - 2.0),
        );

        let peak_w = self.session_mgr.s.peak.w;
        let max_scale = (((peak_w * 1.1) / 100.0).ceil() * 100.0).max(400.0);
        let step = if max_scale > 800.0 { 400.0 } else { 200.0 };
        let axis_col = faint(pal);
        let mut v = 0.0;
        while v <= max_scale {
            let y = plot_rect.bottom() - (v / max_scale) as f32 * plot_rect.height();
            painter.line_segment(
                [Pos2::new(plot_rect.left(), y), Pos2::new(plot_rect.right(), y)],
                Stroke::new(1.0_f32, pal.rule),
            );
            painter.text(
                Pos2::new(rect.left(), y),
                egui::Align2::LEFT_CENTER,
                format!("{:.0}", v),
                FontId::proportional(10.0),
                axis_col,
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
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x0, plot_rect.top()), Pos2::new(x1, plot_rect.bottom())),
                0.0,
                load_color(sp.peak).gamma_multiply(0.14),
            );
        }

        let nb = (plot_rect.width() / COL_W).floor().max(1.0) as usize;
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

        for (i, val) in buckets.iter().enumerate() {
            if let Some(w_val) = *val {
                let bh = (w_val / max_scale).clamp(0.03, 1.0) as f32 * plot_rect.height();
                let x = plot_rect.left() + (i as f32) * COL_W;
                draw_gradient_bar(
                    painter,
                    Rect::from_min_max(Pos2::new(x, plot_rect.bottom() - bh), Pos2::new(x + COL_W - 1.2, plot_rect.bottom())),
                    load_color(w_val),
                );
            }
        }

        // Value under the mouse: the nearest filled column within a few columns.
        let Some(p) = hover.filter(|p| plot_rect.contains(*p)) else {
            return;
        };
        let at = ((p.x - plot_rect.left()) / COL_W).floor() as isize;
        let hit = (0..=4)
            .flat_map(|d| [at - d, at + d])
            .filter(|i| *i >= 0 && (*i as usize) < nb)
            .find_map(|i| buckets[i as usize].map(|w| (i as usize, w)));
        let Some((i, w_val)) = hit else {
            return;
        };
        let x = plot_rect.left() + (i as f32 + 0.5) * COL_W;
        let y = plot_rect.bottom() - (w_val / max_scale).clamp(0.03, 1.0) as f32 * plot_rect.height();
        let t_ms = from + (((i as f64) + 0.5) / nb as f64 * span) as u64;
        painter.line_segment(
            [Pos2::new(x, plot_rect.top()), Pos2::new(x, plot_rect.bottom())],
            Stroke::new(1.0_f32, pal.ink_muted.gamma_multiply(0.6)),
        );
        painter.circle(Pos2::new(x, y), 3.2, pal.ground, Stroke::new(1.6_f32, load_color(w_val)));

        let label = painter.layout_no_wrap(
            format!("{} \u{00b7} {:.0} W", fmt_clock(t_ms), w_val),
            FontId::proportional(11.0),
            pal.ink,
        );
        let ls = label.size() + Vec2::new(12.0, 6.0);
        let bx = (x - ls.x * 0.5).clamp(plot_rect.left(), plot_rect.right() - ls.x);
        let by = (y - ls.y - 8.0).max(rect.top());
        let box_rect = Rect::from_min_size(Pos2::new(bx, by), ls);
        painter.rect(box_rect, Rounding::same(5.0), pal.hover, Stroke::new(1.0_f32, pal.rule_strong));
        painter.galley(box_rect.min + Vec2::new(6.0, 3.0), label, pal.ink);
    }
}
