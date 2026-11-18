//! The expanded panel: header with the live figures, tab bar, tab content and footer.

use crate::{ActiveTab, StromWidgetApp};
use crate::claude::ClaudeStatus;
use crate::format::{fmt_dur, fmt_euro, fmt_money_ct, fmt_rate};
use crate::i18n::t;
use crate::power::class_of;
use crate::theme::{
    class_bg_color, double_bezel_card, draw_class_arrow, draw_close_icon, draw_gear_icon,
    draw_moon_icon, draw_net_arrow, draw_pin_icon, number_color, pct_color, ThemePalette,
};
use crate::ui::draw_meter_bar;
use eframe::egui::{self, Color32, FontId, Margin, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

impl StromWidgetApp {
    // =========================================================================
    // EXPANDED FLYOUT PANEL (456 x 800 — Obsidian Linear Architecture)
    // =========================================================================
    pub(crate) fn render_expanded_flyout(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        egui::Frame::none()
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                // 1. Double-Bezel Hero Card + 4-Tile Telemetry Micro-Grid
                self.render_expanded_header(ui, ctx, pal);
                ui.add_space(7.0);

                // 2. Interactive History & Spike Chart with Peak Badge & Segmented Switcher
                self.render_chart_section(ui, pal);
                ui.add_space(7.0);

                // 3. Unified Segmented Pill Navigation Dock
                self.render_navigation_tabs(ui, pal);
                ui.add_space(7.0);

                // 4. Scrollable Main Tab Content (leaves space at bottom for Claude bar + Footer)
                let claude_open =
                    !matches!(*self.claude_status.lock().unwrap(), ClaudeStatus::Idle);
                let bottom_reserve = if claude_open { 172.0 } else { 96.0 };
                let content_h = (ui.available_height() - bottom_reserve).max(200.0);

                let mut scroll = egui::ScrollArea::vertical();
                if let Some(y) = self.self_test_scroll {
                    scroll = scroll.vertical_scroll_offset(y);
                }
                scroll
                    .max_height(content_h)
                    .min_scrolled_height(content_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.active_tab {
                        ActiveTab::Live => self.render_tab_live(ui, pal),
                        ActiveTab::Spikes => self.render_tab_spikes(ui, pal),
                        ActiveTab::Sessions => self.render_tab_sessions(ui, pal),
                        ActiveTab::System => self.render_tab_system(ui, pal),
                        ActiveTab::Settings => self.render_tab_settings(ui, ctx, pal),
                        ActiveTab::Summary => self.render_tab_summary(ui, pal),
                    });

                ui.add_space(6.0);

                // 5. Claude Answer Drawer (when active)
                self.render_claude_answer_box(ui, pal);

                // 6. Claude Quick-Chips & Input Bar
                self.render_claude_prompt_bar(ui, pal);
                ui.add_space(6.0);

                // 7. Footer ("Gute Nacht" primary pill button + Beenden)
                self.render_footer(ui, ctx, pal);
            });
    }

    fn render_expanded_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        double_bezel_card(ui, pal, 14.0, Vec2::new(12.0, 10.0), |ui| {
            let watts = self.reading.watts;
            let cls = class_of(watts);
            let num_col = number_color(watts);

            // Top Hero Row: Allocate a single 32px tall rect so Watt, W, Class Arrow, Divider, Cost/h, Pin, and Zuklappen share an exact vertical center
            let avail_w = ui.available_width();
            let (hero_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 32.0), Sense::hover());
            let cy = hero_rect.center().y;
            let mut x = hero_rect.left();

            // 1. Live Watt Figure + "W"
            let w_galley = ui.painter().layout_no_wrap(
                format!("{:.0}", watts),
                FontId::proportional(30.0),
                num_col,
            );
            let w_w = w_galley.size().x;
            ui.painter().galley(
                Pos2::new(x, cy - w_galley.size().y * 0.5),
                w_galley,
                num_col,
            );
            x += w_w + 4.0;

            let u_galley = ui.painter().layout_no_wrap(
                "W".to_string(),
                FontId::proportional(13.5),
                pal.ink_muted,
            );
            let u_w = u_galley.size().x;
            ui.painter().galley(
                Pos2::new(x, cy - u_galley.size().y * 0.5 + 3.0),
                u_galley,
                pal.ink_muted,
            );
            x += u_w + 8.0;

            // 2. EU Energy Label Pointed Class Arrow (A-G)
            let arrow_rect =
                Rect::from_min_size(Pos2::new(x, cy - 10.5), Vec2::new(30.0, 21.0));
            draw_class_arrow(
                ui.painter(),
                arrow_rect,
                7.5,
                class_bg_color(cls),
                cls,
                Color32::from_rgb(0x07, 0x09, 0x0C),
                12.0,
            );
            x += 30.0 + 10.0;

            // 3. Vertical Hairline Divider
            ui.painter().line_segment(
                [Pos2::new(x, cy - 12.0), Pos2::new(x, cy + 12.0)],
                Stroke::new(1.0_f32, pal.rule_strong),
            );
            x += 10.0;

            // 4. Cost per hour + running session cost/duration
            let ct_per_h = (watts / 1000.0) * self.settings.price_ct;
            let cur_sess = self.session_mgr.current_summary();
            ui.painter().text(
                Pos2::new(x, cy - 6.8),
                egui::Align2::LEFT_CENTER,
                format!("{}/h", fmt_money_ct(ct_per_h)),
                FontId::proportional(13.8),
                pal.ink,
            );
            ui.painter().text(
                Pos2::new(x, cy + 7.2),
                egui::Align2::LEFT_CENTER,
                format!("{} in {}", fmt_euro(cur_sess.cost), fmt_dur(cur_sess.dur_s)),
                FontId::proportional(11.0),
                pal.ink_muted,
            );

            // 5. Right-aligned Pill Buttons: Pin + "✕ Zuklappen"
            let close_w = 88.0;
            let close_rect = Rect::from_min_size(
                Pos2::new(hero_rect.right() - close_w, cy - 13.0),
                Vec2::new(close_w, 26.0),
            );
            let close_resp = ui.interact(close_rect, egui::Id::new("hdr_close_btn"), Sense::click());
            ui.painter().rect(
                close_rect,
                Rounding::same(13.0),
                if close_resp.hovered() {
                    pal.rule
                } else {
                    pal.hover
                },
                Stroke::new(1.0_f32, pal.rule_strong),
            );
            draw_close_icon(
                ui.painter(),
                Pos2::new(close_rect.left() + 13.0, close_rect.center().y),
                6.5,
                pal.ink,
            );
            ui.painter().text(
                Pos2::new(close_rect.left() + 24.0, close_rect.center().y),
                egui::Align2::LEFT_CENTER,
                t("Zuklappen", "Collapse"),
                FontId::proportional(11.5),
                pal.ink,
            );
            if close_resp.clicked() {
                self.toggle_expand(ctx, Some(false));
            }

            let pin_w = 28.0;
            let pin_rect = Rect::from_min_size(
                Pos2::new(close_rect.left() - 6.0 - pin_w, cy - 13.0),
                Vec2::new(pin_w, 26.0),
            );
            let pin_resp = ui.interact(pin_rect, egui::Id::new("hdr_pin_btn"), Sense::click());
            let pin_bg = if self.settings.pinned {
                pal.action.gamma_multiply(0.18)
            } else if pin_resp.hovered() {
                pal.rule
            } else {
                pal.hover
            };
            let pin_stroke = if self.settings.pinned {
                pal.action
            } else {
                pal.rule_strong
            };
            ui.painter().rect(
                pin_rect,
                Rounding::same(13.0),
                pin_bg,
                Stroke::new(1.0_f32, pin_stroke),
            );
            draw_pin_icon(
                ui.painter(),
                pin_rect.center(),
                self.settings.pinned,
                if self.settings.pinned {
                    pal.action
                } else {
                    pal.ink_muted
                },
            );
            if pin_resp
                .on_hover_text(t("Immer im Vordergrund umschalten", "Toggle always on top"))
                .clicked()
            {
                self.settings.pinned = !self.settings.pinned;
                self.settings.save();
                self.sync_window_geometry(ctx, true);
            }

            // Obsidian Linear 4-Tile Telemetry Micro-Grid (CPU, RAM, DOWN, UP) — Direct Mathematical Painter Grid
            if self.settings.sys_addon {
                ui.add_space(9.0);
                let cpu_pct = self.sys_view.cpu;
                let ram_pct = self.sys_view.ram;
                let cpu_w = self.reading.parts.cpu.w;
                let down_txt = fmt_rate(self.sys_view.down);
                let up_txt = fmt_rate(self.sys_view.up);
                let down_share = ((self.sys_view.down / 5_242_880.0) as f32).clamp(0.06, 1.0);
                let up_share = ((self.sys_view.up / 2_097_152.0) as f32).clamp(0.06, 1.0);

                let grid_w = ui.available_width();
                let tile_gap = 6.0;
                let tile_w = (grid_w - tile_gap * 3.0) / 4.0;
                let tile_h = 48.0;
                let (grid_rect, _) =
                    ui.allocate_exact_size(Vec2::new(grid_w, tile_h), Sense::hover());

                let tiles = [
                    (
                        "CPU",
                        Some(format!("{:.0} W", cpu_w)),
                        pct_color(cpu_pct),
                        format!("{:.0} %", cpu_pct),
                        pal.ink,
                        (cpu_pct / 100.0) as f32,
                        pct_color(cpu_pct),
                        None,
                    ),
                    (
                        "RAM",
                        Some(t("Belegt", "Used").to_string()),
                        pal.ink_muted,
                        format!("{:.0} %", ram_pct),
                        pct_color(ram_pct),
                        (ram_pct / 100.0) as f32,
                        pct_color(ram_pct),
                        None,
                    ),
                    (
                        "DOWN",
                        None,
                        pal.action,
                        down_txt,
                        pal.ink,
                        down_share,
                        pal.action,
                        Some(true),
                    ),
                    (
                        "UP",
                        None,
                        pal.ink_muted,
                        up_txt,
                        pal.ink_muted,
                        up_share,
                        pal.ink_muted,
                        Some(false),
                    ),
                ];

                for (i, (lbl, right_sub, sub_col, main_val, val_col, bar_share, bar_col, arrow_down)) in
                    tiles.iter().enumerate()
                {
                    let tx = grid_rect.left() + (i as f32) * (tile_w + tile_gap);
                    let t_rect =
                        Rect::from_min_size(Pos2::new(tx, grid_rect.top()), Vec2::new(tile_w, tile_h));

                    ui.painter().rect(
                        t_rect,
                        Rounding::same(8.0),
                        pal.input_ground,
                        Stroke::new(1.0_f32, pal.rule),
                    );

                    // Top row inside tile: Label on left, sub-value or arrow on right
                    let top_y = t_rect.top() + 11.0;
                    ui.painter().text(
                        Pos2::new(t_rect.left() + 8.5, top_y),
                        egui::Align2::LEFT_CENTER,
                        *lbl,
                        FontId::proportional(9.5),
                        pal.ink_muted,
                    );

                    if let Some(ref sub_txt) = right_sub {
                        ui.painter().text(
                            Pos2::new(t_rect.right() - 8.5, top_y),
                            egui::Align2::RIGHT_CENTER,
                            sub_txt,
                            FontId::proportional(9.5),
                            *sub_col,
                        );
                    } else if let Some(is_down) = arrow_down {
                        draw_net_arrow(
                            ui.painter(),
                            Pos2::new(t_rect.right() - 11.5, top_y),
                            *is_down,
                            *sub_col,
                        );
                    }

                    // Middle row inside tile: Main Telemetry Value (clean proportional font, zero wide gaps)
                    let mid_y = t_rect.top() + 27.5;
                    ui.painter().text(
                        Pos2::new(t_rect.left() + 8.5, mid_y),
                        egui::Align2::LEFT_CENTER,
                        main_val,
                        FontId::proportional(13.0),
                        *val_col,
                    );

                    // Bottom row inside tile: Clean 3px Progress Bar
                    let bar_rect = Rect::from_min_max(
                        Pos2::new(t_rect.left() + 8.5, t_rect.bottom() - 8.5),
                        Pos2::new(t_rect.right() - 8.5, t_rect.bottom() - 5.5),
                    );
                    draw_meter_bar(ui.painter(), bar_rect, *bar_share, *bar_col, pal.rule);
                }
            }
        });
    }

    fn render_navigation_tabs(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // Unified Segmented Pill Navigation Dock — Direct Painter Layout so all tabs, badges, and gear share an exact centerline
        let dock_w = ui.available_width();
        let dock_h = 32.0;
        let (dock_rect, _) = ui.allocate_exact_size(Vec2::new(dock_w, dock_h), Sense::hover());
        let cy = dock_rect.center().y;

        ui.painter().rect(
            dock_rect,
            Rounding::same(10.0),
            pal.input_ground,
            Stroke::new(1.0_f32, pal.rule),
        );

        let spike_count = self.session_mgr.s.spikes.len();
        let port_count = self.sys_view.all_ports.len();

        let mut tabs: Vec<(ActiveTab, &str, Option<usize>)> = vec![
            (ActiveTab::Live, "Live", None),
            (
                ActiveTab::Spikes,
                "Spikes",
                if spike_count > 0 {
                    Some(spike_count)
                } else {
                    None
                },
            ),
            (ActiveTab::Sessions, t("Verlauf", "History"), None),
        ];
        if self.settings.sys_addon {
            tabs.push((
                ActiveTab::System,
                t("System & Ports", "System & Ports"),
                if port_count > 0 {
                    Some(port_count)
                } else {
                    None
                },
            ));
        }

        let pill_h = 26.0;
        let mut tx = dock_rect.left() + 3.0;

        for (tab_id, title, badge_cnt) in tabs {
            let is_on = self.active_tab == tab_id;
            let title_galley = ui.painter().layout_no_wrap(
                title.to_string(),
                FontId::proportional(11.5),
                if is_on { pal.ink } else { pal.ink_muted },
            );
            let title_w = title_galley.size().x;

            let badge_info = badge_cnt.map(|cnt| {
                let b_galley = ui.painter().layout_no_wrap(
                    format!("{}", cnt),
                    FontId::proportional(9.8),
                    pal.action,
                );
                let bw = (b_galley.size().x + 10.0).max(18.0);
                (b_galley, bw)
            });

            let badge_extra = badge_info.as_ref().map(|(_, bw)| 5.0 + *bw).unwrap_or(0.0);
            let pill_w = 18.0 + title_w + badge_extra;
            let pill_rect = Rect::from_min_size(
                Pos2::new(tx, cy - pill_h * 0.5),
                Vec2::new(pill_w, pill_h),
            );
            let resp = ui.interact(
                pill_rect,
                egui::Id::new(("nav_tab", title)),
                Sense::click(),
            );

            if is_on || resp.hovered() {
                ui.painter().rect(
                    pill_rect,
                    Rounding::same(7.0),
                    if is_on { pal.hover } else { pal.card },
                    if is_on {
                        Stroke::new(1.0_f32, pal.rule_strong)
                    } else {
                        Stroke::NONE
                    },
                );
            }

            let text_x = pill_rect.left() + 9.0;
            ui.painter().galley(
                Pos2::new(text_x, cy - title_galley.size().y * 0.5),
                title_galley,
                if is_on { pal.ink } else { pal.ink_muted },
            );

            if let Some((b_galley, bw)) = badge_info {
                let bh = 16.0;
                let b_rect = Rect::from_min_size(
                    Pos2::new(text_x + title_w + 5.0, cy - bh * 0.5),
                    Vec2::new(bw, bh),
                );
                ui.painter().rect_filled(
                    b_rect,
                    Rounding::same(8.0),
                    pal.action.gamma_multiply(0.18),
                );
                ui.painter().galley(
                    Pos2::new(
                        b_rect.center().x - b_galley.size().x * 0.5,
                        cy - b_galley.size().y * 0.5,
                    ),
                    b_galley,
                    pal.action,
                );
            }

            if resp.clicked() {
                if self.active_tab == ActiveTab::Summary && tab_id != ActiveTab::Summary {
                    self.frozen_chart = None;
                    self.summary_fresh = false;
                }
                self.active_tab = tab_id;
                if tab_id == ActiveTab::Spikes {
                    self.session_mgr.mark_seen();
                }
                if tab_id == ActiveTab::Sessions {
                    self.sessions_cache = self.session_mgr.list_sessions(60);
                }
            }

            tx += pill_w + 3.0;
        }

        // Right-aligned Vector Gear Button inside the Segmented Dock on the exact same cy
        let gear_on = self.active_tab == ActiveTab::Settings;
        let gear_w = 28.0;
        let gear_rect = Rect::from_min_size(
            Pos2::new(dock_rect.right() - 3.0 - gear_w, cy - pill_h * 0.5),
            Vec2::new(gear_w, pill_h),
        );
        let gear_resp = ui.interact(gear_rect, egui::Id::new("nav_gear_btn"), Sense::click());
        if gear_on || gear_resp.hovered() {
            ui.painter().rect(
                gear_rect,
                Rounding::same(7.0),
                if gear_on { pal.hover } else { pal.card },
                if gear_on {
                    Stroke::new(1.0_f32, pal.rule_strong)
                } else {
                    Stroke::NONE
                },
            );
        }
        draw_gear_icon(
            ui.painter(),
            gear_rect.center(),
            if gear_on { pal.action } else { pal.ink_muted },
        );
        if gear_resp.on_hover_text(t("Einstellungen", "Settings")).clicked() {
            self.active_tab = ActiveTab::Settings;
        }
    }

    fn render_footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, pal: &ThemePalette) {
        let foot_w = ui.available_width();
        let (foot_rect, _) = ui.allocate_exact_size(Vec2::new(foot_w, 26.0), Sense::hover());
        let cy = foot_rect.center().y;

        // 1. Obsidian Linear Pill-shaped "Gute Nacht" Button with nested dark circle around Moon icon
        let btn_rect = Rect::from_min_size(
            Pos2::new(foot_rect.left(), cy - 13.0),
            Vec2::new(116.0, 26.0),
        );
        let btn_resp = ui.interact(btn_rect, egui::Id::new("footer_goodnight"), Sense::click());
        let fill_c = if btn_resp.hovered() {
            Color32::from_rgb(0x16, 0xD0, 0x6A)
        } else {
            pal.action
        };
        ui.painter()
            .rect_filled(btn_rect, Rounding::same(13.0), fill_c);

        let moon_center = Pos2::new(btn_rect.left() + 13.0, cy);
        let moon_bg = Color32::from_rgb(0x06, 0x28, 0x14);
        ui.painter().circle_filled(moon_center, 9.0, moon_bg);
        draw_moon_icon(
            ui.painter(),
            moon_center,
            Color32::from_rgb(0xE2, 0xF8, 0xEC),
            moon_bg,
        );
        ui.painter().text(
            Pos2::new(btn_rect.left() + 27.0, cy),
            egui::Align2::LEFT_CENTER,
            t("Gute Nacht", "Good night"),
            FontId::proportional(12.0),
            pal.action_ink,
        );
        if btn_resp.clicked() {
            self.trigger_goodnight(ctx);
        }

        // 2. Subtitle aligned on the exact same centerline cy
        ui.painter().text(
            Pos2::new(btn_rect.right() + 10.0, cy),
            egui::Align2::LEFT_CENTER,
            t("schlie\u{00df}t die Session mit Beleg ab", "closes the session with a receipt"),
            FontId::proportional(11.0),
            pal.ink_muted,
        );

        // 3. Right-aligned "Beenden" button on the exact same centerline cy
        let quit_rect = Rect::from_min_size(
            Pos2::new(foot_rect.right() - 52.0, cy - 11.0),
            Vec2::new(52.0, 22.0),
        );
        let quit_resp = ui.interact(quit_rect, egui::Id::new("footer_quit"), Sense::click());
        ui.painter().text(
            Pos2::new(foot_rect.right() - 2.0, cy),
            egui::Align2::RIGHT_CENTER,
            t("Beenden", "Quit"),
            FontId::proportional(11.0),
            if quit_resp.hovered() {
                pal.ink
            } else {
                pal.ink_muted
            },
        );
        if quit_resp.clicked() {
            self.session_mgr.save();
            std::process::exit(0);
        }
    }
}
