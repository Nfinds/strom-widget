//! The collapsed strip: docked into the taskbar or in a screen corner.

use crate::{taskbar, StromWidgetApp};
use crate::format::{fmt_dur, fmt_euro, fmt_money_ct, fmt_rate, fmt_rate_short};
use crate::power::class_of;
use crate::theme::{
    draw_chevron_icon, draw_class_ring_badge, draw_net_arrow, load_color, number_color, pct_color,
    ThemePalette,
};
use eframe::egui::{self, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

impl StromWidgetApp {
    // =========================================================================
    // COMPACT STRIP (Taskbar-Docked Overlay OR Desktop Corner Strip)
    // =========================================================================
    pub(crate) fn render_compact_strip(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, pal: &ThemePalette) {
        let avail = ui.available_rect_before_wrap();
        let in_taskbar = self.settings.placement == "taskbar";

        let ticker_h = if in_taskbar { 3.0 } else { 5.0 };
        let sys_h = if self.settings.sys_addon && !in_taskbar {
            16.0
        } else {
            0.0
        };
        let ticker_rect = Rect::from_min_max(
            Pos2::new(avail.left() + 6.0, avail.bottom() - ticker_h - sys_h - 1.5),
            Pos2::new(avail.right() - 6.0, avail.bottom() - sys_h - 1.5),
        );
        self.draw_mini_ticker(ui.painter(), ticker_rect);

        // Shared vertical centerline above the mini ticker bar
        let content_top = avail.top() + 1.0;
        let content_bot = ticker_rect.top() - 1.0;
        let cy = (content_top + content_bot) * 0.5;
        let row1_y = cy - 6.2;
        let row2_y = cy + 6.2;

        let watts = self.reading.watts;
        let cls = class_of(watts);
        let num_col = number_color(watts);

        let painter = ui.painter();
        let mut x = avail.left() + if in_taskbar { 9.0 } else { 11.0 };

        // 1. Leftmost: Circular Efficiency Ring Badge (A-G) in live EU Energy Label color
        let ring_r = if in_taskbar { 11.0 } else { 12.5 };
        let ring_center = Pos2::new(x + ring_r, cy);
        draw_class_ring_badge(
            painter,
            ring_center,
            ring_r,
            cls,
            if in_taskbar { 11.5 } else { 13.0 },
        );
        x += ring_r * 2.0 + 7.0;

        // 2. Live Watt Figure + "W" unit on the shared centerline
        let w_font_sz = if in_taskbar {
            19.5
        } else if self.settings.size == "m" {
            25.0
        } else {
            21.0
        };
        let w_str = format!("{:.0}", watts);
        let w_galley = painter.layout_no_wrap(w_str, FontId::proportional(w_font_sz), num_col);
        let w_w = w_galley.size().x;
        painter.galley(
            Pos2::new(x, cy - w_galley.size().y * 0.5),
            w_galley,
            num_col,
        );
        x += w_w + 2.5;

        let u_galley = painter.layout_no_wrap(
            "W".to_string(),
            FontId::proportional(if in_taskbar { 10.5 } else { 12.0 }),
            pal.ink_muted,
        );
        let u_w = u_galley.size().x;
        painter.galley(
            Pos2::new(x, cy - u_galley.size().y * 0.5 + 1.5),
            u_galley,
            pal.ink_muted,
        );
        x += u_w + 7.5;

        // Crisp 1px vertical hairline divider
        painter.line_segment(
            [Pos2::new(x, cy - 10.5), Pos2::new(x, cy + 10.5)],
            Stroke::new(1.0_f32, pal.rule_strong),
        );
        x += 7.5;

        // 3. Cost per hour + running session cost/duration on exact row1_y / row2_y
        let ct_per_h = (watts / 1000.0) * self.settings.price_ct;
        let cur_sess = self.session_mgr.current_summary();
        let ct_str = format!("{}/h", fmt_money_ct(ct_per_h));
        let sess_str = format!("{} \u{00b7} {}", fmt_euro(cur_sess.cost), fmt_dur(cur_sess.dur_s));

        let ct_galley = painter.layout_no_wrap(
            ct_str,
            FontId::proportional(if in_taskbar { 11.2 } else { 12.5 }),
            pal.ink,
        );
        let sess_galley = painter.layout_no_wrap(
            sess_str,
            FontId::proportional(if in_taskbar { 9.5 } else { 10.5 }),
            pal.ink_muted,
        );
        let col3_w = ct_galley.size().x.max(sess_galley.size().x);
        painter.galley(
            Pos2::new(x, row1_y - ct_galley.size().y * 0.5),
            ct_galley,
            pal.ink,
        );
        painter.galley(
            Pos2::new(x, row2_y - sess_galley.size().y * 0.5),
            sess_galley,
            pal.ink_muted,
        );
        x += col3_w + 8.0;

        // 4. In Taskbar Mode with System-Addon: Clean aligned CPU/RAM & Down/Up columns
        if in_taskbar && self.settings.sys_addon {
            painter.line_segment(
                [Pos2::new(x, cy - 10.5), Pos2::new(x, cy + 10.5)],
                Stroke::new(1.0_f32, pal.rule_strong),
            );
            x += 7.5;

            // Sub-column A: CPU % & RAM %
            let cpu_lbl = painter.layout_no_wrap(
                "CPU".to_string(),
                FontId::proportional(9.5),
                pal.ink_muted,
            );
            let ram_lbl = painter.layout_no_wrap(
                "RAM".to_string(),
                FontId::proportional(9.5),
                pal.ink_muted,
            );
            let lbl_w = cpu_lbl.size().x.max(ram_lbl.size().x);

            painter.galley(
                Pos2::new(x, row1_y - cpu_lbl.size().y * 0.5),
                cpu_lbl,
                pal.ink_muted,
            );
            painter.galley(
                Pos2::new(x, row2_y - ram_lbl.size().y * 0.5),
                ram_lbl,
                pal.ink_muted,
            );

            let val_x = x + lbl_w + 4.0;
            let cpu_col = pct_color(self.sys_view.cpu);
            let ram_col = pct_color(self.sys_view.ram);
            let cpu_val = painter.layout_no_wrap(
                format!("{:.0}%", self.sys_view.cpu),
                FontId::proportional(10.2),
                cpu_col,
            );
            let ram_val = painter.layout_no_wrap(
                format!("{:.0}%", self.sys_view.ram),
                FontId::proportional(10.2),
                ram_col,
            );
            let val_w = cpu_val.size().x.max(ram_val.size().x).max(22.0);

            painter.galley(
                Pos2::new(val_x, row1_y - cpu_val.size().y * 0.5),
                cpu_val,
                cpu_col,
            );
            painter.galley(
                Pos2::new(val_x, row2_y - ram_val.size().y * 0.5),
                ram_val,
                ram_col,
            );
            x = val_x + val_w + 7.5;

            // Sub-column B: Down (↓) & Up (↑)
            draw_net_arrow(painter, Pos2::new(x + 3.0, row1_y), true, pal.action);
            draw_net_arrow(painter, Pos2::new(x + 3.0, row2_y), false, pal.ink_muted);

            let net_x = x + 9.0;
            let down_galley = painter.layout_no_wrap(
                fmt_rate(self.sys_view.down),
                FontId::proportional(10.2),
                pal.ink,
            );
            let up_galley = painter.layout_no_wrap(
                fmt_rate(self.sys_view.up),
                FontId::proportional(9.8),
                pal.ink_muted,
            );
            painter.galley(
                Pos2::new(net_x, row1_y - down_galley.size().y * 0.5),
                down_galley,
                pal.ink,
            );
            painter.galley(
                Pos2::new(net_x, row2_y - up_galley.size().y * 0.5),
                up_galley,
                pal.ink_muted,
            );
        }

        // 5. Rightmost Vector Chevron (snug at the right edge on the shared centerline cy)
        let chevron_col_w = 18.0;
        let chevron_rect = Rect::from_min_max(
            Pos2::new(avail.right() - chevron_col_w, content_top),
            Pos2::new(avail.right() - 3.0, content_bot),
        );
        let chev_resp = ui.interact(chevron_rect, egui::Id::new("strip_chevron"), Sense::click());
        let chev_col = if chev_resp.hovered() {
            pal.ink
        } else {
            pal.ink_muted
        };
        draw_chevron_icon(
            ui.painter(),
            Pos2::new(chevron_rect.center().x, cy),
            self.grows_up,
            chev_col,
        );

        // Desktop compact sysline (when not in taskbar)
        if !in_taskbar && self.settings.sys_addon {
            let sys_rect = Rect::from_min_max(
                Pos2::new(avail.left() + 10.0, avail.bottom() - sys_h),
                Pos2::new(avail.right() - 10.0, avail.bottom() - 2.0),
            );
            ui.allocate_ui_at_rect(sys_rect, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(
                        RichText::new(format!("CPU {:.0}%", self.sys_view.cpu))
                            .font(FontId::proportional(10.0))
                            .color(pct_color(self.sys_view.cpu)),
                    );
                    ui.label(
                        RichText::new(format!("RAM {:.0}%", self.sys_view.ram))
                            .font(FontId::proportional(10.0))
                            .color(pct_color(self.sys_view.ram)),
                    );
                    let (d_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 10.0), Sense::hover());
                    draw_net_arrow(ui.painter(), d_rect.center(), true, pal.ink_muted);
                    ui.label(
                        RichText::new(fmt_rate_short(self.sys_view.down))
                            .font(FontId::proportional(10.0))
                            .color(pal.ink_muted),
                    );
                    let (u_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 10.0), Sense::hover());
                    draw_net_arrow(ui.painter(), u_rect.center(), false, pal.ink_muted);
                    ui.label(
                        RichText::new(fmt_rate_short(self.sys_view.up))
                            .font(FontId::proportional(10.0))
                            .color(pal.ink_muted),
                    );
                });
            });
        }

        let sense = if in_taskbar { Sense::click_and_drag() } else { Sense::click() };
        // No hover tooltips here: egui draws them inside this 42 px window, covering the strip.
        let bg_interact = ui.interact(avail, egui::Id::new("strip_bg_click"), sense);
        if in_taskbar {
            self.handle_strip_drag(ctx, &bg_interact);
        }
        if chev_resp.clicked() || bg_interact.clicked() {
            self.toggle_expand(ctx, Some(true));
        }
    }

    /// Moves the taskbar strip with the mouse. Uses the absolute cursor position, because the
    /// window itself moves while dragging (window-relative deltas would feed back and jitter).
    fn handle_strip_drag(&mut self, ctx: &egui::Context, resp: &egui::Response) {
        let scale = ctx.pixels_per_point().max(1.0);
        if resp.drag_started() {
            if let Some(cx) = taskbar::cursor_x() {
                self.strip_drag = Some((cx, self.settings.taskbar_offset_x));
            }
        }
        if let Some((start_x, start_ofs)) = self.strip_drag {
            if resp.dragged() {
                if let Some(cx) = taskbar::cursor_x() {
                    let delta_pts = ((cx - start_x) as f32 / scale).round() as i32;
                    let (cw, _) = self.compact_dimensions();
                    let next = taskbar::clamp_taskbar_offset(
                        start_ofs + delta_pts,
                        cw,
                        scale,
                        self.settings.taskbar_display == "secondary",
                    );
                    if next != self.settings.taskbar_offset_x {
                        self.settings.taskbar_offset_x = next;
                        self.sync_window_geometry(ctx, true);
                    }
                }
            }
            if resp.drag_stopped() {
                self.strip_drag = None;
                self.settings.save();
            }
        }
    }

    fn draw_mini_ticker(&self, painter: &egui::Painter, rect: Rect) {
        let n = 60usize;
        let len = self.session_mgr.live.len();
        if len == 0 {
            return;
        }
        let slice = &self.session_mgr.live[len.saturating_sub(n)..];
        let min_w = slice
            .iter()
            .map(|s| s.w)
            .fold(f64::INFINITY, f64::min)
            * 0.6;
        let max_w = slice
            .iter()
            .map(|s| s.w)
            .fold(min_w + 150.0, f64::max);

        let slot = rect.width() / (n as f32);
        let offset = n.saturating_sub(slice.len());
        for (i, s) in slice.iter().enumerate() {
            let x = rect.left() + ((offset + i) as f32) * slot;
            let frac = ((s.w - min_w) / (max_w - min_w)).clamp(0.18, 1.0) as f32;
            let bh = (rect.height() * frac).max(1.5);
            let bar_rect = Rect::from_min_max(
                Pos2::new(x, rect.bottom() - bh),
                Pos2::new(x + (slot - 1.0).max(1.0), rect.bottom()),
            );
            painter.rect_filled(bar_rect, 0.0, load_color(s.w));
        }
    }
}
