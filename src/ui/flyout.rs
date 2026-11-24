//! The expanded panel: one header line with the live figures, the tab line, the tab content
//! and the footer.

use crate::{ActiveTab, StromWidgetApp};
use crate::claude::ClaudeStatus;
use crate::format::{fmt_dur, fmt_euro, fmt_money_ct};
use crate::i18n::t;
use crate::power::class_of;
use crate::theme::{
    class_bg_color, draw_class_arrow, draw_close_icon, draw_gear_icon, draw_moon_icon,
    draw_pin_icon, number_color, ThemePalette,
};
use eframe::egui::{self, Color32, FontId, Margin, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

/// Height of the footer row plus its spacing.
const FOOTER_H: f32 = 40.0;
/// Quick questions and input line of the Claude bar.
const CLAUDE_BAR_H: f32 = 70.0;
/// Answer box of the Claude bar.
const CLAUDE_ANSWER_H: f32 = 96.0;

/// A square icon button: no fill at rest, a hairline when hovered or switched on.
fn icon_button(ui: &mut egui::Ui, rect: Rect, id: &str, pal: &ThemePalette, on: bool) -> egui::Response {
    let resp = ui.interact(rect, egui::Id::new(id), Sense::click());
    if on || resp.hovered() {
        ui.painter().rect(
            rect,
            Rounding::same(7.0),
            if on { pal.action.gamma_multiply(0.14) } else { pal.hover },
            Stroke::new(1.0_f32, if on { pal.action.gamma_multiply(0.6) } else { pal.rule_strong }),
        );
    }
    resp
}

impl StromWidgetApp {
    pub(crate) fn render_expanded_flyout(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        let claude_active = self
            .claude_status
            .lock()
            .map(|s| !matches!(*s, ClaudeStatus::Idle))
            .unwrap_or(false);

        egui::Frame::none()
            .inner_margin(Margin { left: 14.0, right: 14.0, top: 10.0, bottom: 8.0 })
            .show(ui, |ui| {
                self.render_expanded_header(ui, ctx, pal);
                ui.add_space(4.0);
                self.render_navigation_tabs(ui, pal);
                ui.add_space(12.0);

                let mut bottom_reserve = FOOTER_H;
                if self.claude_bar_open {
                    bottom_reserve += CLAUDE_BAR_H;
                }
                if claude_active {
                    bottom_reserve += CLAUDE_ANSWER_H;
                }
                let content_h = (ui.available_height() - bottom_reserve).max(160.0);

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
                if claude_active {
                    self.render_claude_answer_box(ui, pal);
                }
                if self.claude_bar_open {
                    self.render_claude_prompt_bar(ui, pal);
                    ui.add_space(6.0);
                }
                self.render_footer(ui, ctx, pal);
            });
    }

    /// One line: watts, energy class, cost per hour and session, then pin and close.
    fn render_expanded_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        let watts = self.reading.watts;
        let cls = class_of(watts);
        let num_col = number_color(watts);

        let avail_w = ui.available_width();
        let (hero_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 42.0), Sense::hover());
        let cy = hero_rect.center().y;
        let mut x = hero_rect.left();

        let w_galley = ui.painter().layout_no_wrap(
            format!("{:.0}", watts),
            FontId::proportional(34.0),
            num_col,
        );
        let w_w = w_galley.size().x;
        let w_h = w_galley.size().y;
        ui.painter().galley(Pos2::new(x, cy - w_h * 0.5), w_galley, num_col);
        x += w_w + 3.0;

        let u_galley = ui.painter().layout_no_wrap("W".to_string(), FontId::proportional(14.0), pal.ink_muted);
        let u_h = u_galley.size().y;
        ui.painter().galley(Pos2::new(x, cy + w_h * 0.5 - u_h - 4.0), u_galley.clone(), pal.ink_muted);
        x += u_galley.size().x + 10.0;

        // Energy class: the arrow alone; the A-G scale is in its tooltip.
        let arrow_rect = Rect::from_min_size(Pos2::new(x, cy - 10.0), Vec2::new(30.0, 20.0));
        draw_class_arrow(
            ui.painter(),
            arrow_rect,
            7.0,
            class_bg_color(cls),
            cls,
            Color32::from_rgb(0x07, 0x09, 0x0C),
            12.0,
        );
        ui.interact(arrow_rect, egui::Id::new("hdr_class"), Sense::hover()).on_hover_text(t(
            "EU-Energieklasse nach Leistung\nA unter 150 W, je Klasse +100 W, G ab 650 W",
            "EU energy class by power\nA below 150 W, +100 W per class, G from 650 W",
        ));
        x += 30.0 + 12.0;

        let ct_per_h = (watts / 1000.0) * self.settings.price_ct;
        ui.painter().text(
            Pos2::new(x, cy - 7.5),
            egui::Align2::LEFT_CENTER,
            format!("{}/h", fmt_money_ct(ct_per_h)),
            FontId::proportional(14.0),
            pal.ink,
        );
        ui.painter().text(
            Pos2::new(x, cy + 8.5),
            egui::Align2::LEFT_CENTER,
            tr!(
                "Session {} \u{00b7} {}", "Session {} \u{00b7} {}",
                fmt_euro(self.session_mgr.s.cost),
                fmt_dur(self.session_mgr.dur_s())
            ),
            FontId::proportional(11.5),
            pal.ink_muted,
        );

        let btn = 28.0;
        let close_rect = Rect::from_min_size(Pos2::new(hero_rect.right() - btn, cy - btn * 0.5), Vec2::splat(btn));
        let close_resp = icon_button(ui, close_rect, "hdr_close_btn", pal, false);
        draw_close_icon(
            ui.painter(),
            close_rect.center(),
            9.0,
            if close_resp.hovered() { pal.ink } else { pal.ink_muted },
        );
        if close_resp.on_hover_text(t("Zuklappen (Esc)", "Collapse (Esc)")).clicked() {
            self.toggle_expand(ctx, Some(false));
        }

        let pin_rect = close_rect.translate(Vec2::new(-btn - 4.0, 0.0));
        let pinned = self.settings.pinned;
        let pin_resp = icon_button(ui, pin_rect, "hdr_pin_btn", pal, pinned);
        draw_pin_icon(
            ui.painter(),
            pin_rect.center(),
            pinned,
            if pinned { pal.action } else { pal.ink_muted },
        );
        if pin_resp
            .on_hover_text(t("Immer im Vordergrund umschalten", "Toggle always on top"))
            .clicked()
        {
            self.settings.pinned = !self.settings.pinned;
            self.settings.save();
            self.sync_window_geometry(ctx, true);
        }
    }

    /// Tab line with an underline under the open tab; settings as a gear on the right.
    fn render_navigation_tabs(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let line_w = ui.available_width();
        let line_h = 32.0;
        let (line_rect, _) = ui.allocate_exact_size(Vec2::new(line_w, line_h), Sense::hover());
        let cy = line_rect.center().y - 1.0;
        ui.painter().line_segment(
            [line_rect.left_bottom(), line_rect.right_bottom()],
            Stroke::new(1.0_f32, pal.rule),
        );

        let unseen = self.session_mgr.unseen();
        let port_count = self.sys_view.all_ports.len();
        let mut tabs: Vec<(ActiveTab, &str, Option<usize>)> = vec![
            (ActiveTab::Live, t("Jetzt", "Now"), (unseen > 0).then_some(unseen)),
            (ActiveTab::Sessions, t("Verlauf", "History"), None),
        ];
        if self.settings.sys_addon {
            tabs.push((ActiveTab::System, "System", (port_count > 0).then_some(port_count)));
        }

        // Spikes belong to "Now", a receipt to "History".
        let shown = match self.active_tab {
            ActiveTab::Spikes => ActiveTab::Live,
            ActiveTab::Summary => ActiveTab::Sessions,
            other => other,
        };

        let mut tx = line_rect.left();
        for (tab_id, title, badge) in tabs {
            let is_on = shown == tab_id;
            let title_galley = ui.painter().layout_no_wrap(
                title.to_string(),
                FontId::proportional(12.5),
                Color32::PLACEHOLDER, // color follows hover, set when painting
            );
            let title_w = title_galley.size().x;
            let badge_galley = badge.map(|n| {
                ui.painter().layout_no_wrap(n.to_string(), FontId::proportional(10.5), pal.action)
            });
            let badge_w = badge_galley.as_ref().map(|g| g.size().x + 4.0).unwrap_or(0.0);
            let tab_rect = Rect::from_min_size(
                Pos2::new(tx, line_rect.top()),
                Vec2::new(title_w + badge_w + 20.0, line_h),
            );
            let resp = ui.interact(tab_rect, egui::Id::new(("nav_tab", title)), Sense::click());
            let col = if is_on || resp.hovered() { pal.ink } else { pal.ink_muted };
            let text_x = tab_rect.left() + 10.0;
            ui.painter().galley(
                Pos2::new(text_x, cy - title_galley.size().y * 0.5),
                title_galley,
                col,
            );
            if let Some(g) = badge_galley {
                let gh = g.size().y;
                ui.painter().galley(Pos2::new(text_x + title_w + 4.0, cy - gh * 0.5 - 0.5), g, pal.action);
            }
            if is_on {
                let ul = Rect::from_min_max(
                    Pos2::new(text_x, line_rect.bottom() - 2.0),
                    Pos2::new(text_x + title_w + badge_w, line_rect.bottom()),
                );
                ui.painter().rect_filled(ul, Rounding::same(1.0), pal.action);
            }

            if resp.clicked() {
                if self.active_tab == ActiveTab::Summary && tab_id != ActiveTab::Summary {
                    self.frozen_chart = None;
                    self.summary_fresh = false;
                }
                self.active_tab = tab_id;
                if tab_id == ActiveTab::Sessions {
                    self.sessions_cache = self.session_mgr.list_sessions(60);
                }
            }
            tx += tab_rect.width();
        }

        let gear_on = self.active_tab == ActiveTab::Settings;
        let gear_rect = Rect::from_min_size(
            Pos2::new(line_rect.right() - 28.0, cy - 13.0),
            Vec2::splat(26.0),
        );
        let gear_resp = icon_button(ui, gear_rect, "nav_gear_btn", pal, false);
        draw_gear_icon(
            ui.painter(),
            gear_rect.center(),
            if gear_on { pal.action } else if gear_resp.hovered() { pal.ink } else { pal.ink_muted },
        );
        if gear_resp.on_hover_text(t("Einstellungen", "Settings")).clicked() {
            self.active_tab = ActiveTab::Settings;
        }
    }

    /// "Good night" on the left, the Claude switch next to it, "Quit" on the right.
    fn render_footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, pal: &ThemePalette) {
        let foot_w = ui.available_width();
        let (foot_rect, _) = ui.allocate_exact_size(Vec2::new(foot_w, 30.0), Sense::hover());
        ui.painter().line_segment(
            [
                Pos2::new(foot_rect.left() - 14.0, foot_rect.top() - 2.0),
                Pos2::new(foot_rect.right() + 14.0, foot_rect.top() - 2.0),
            ],
            Stroke::new(1.0_f32, pal.rule),
        );
        let cy = foot_rect.center().y + 2.0;

        let night_label = t("Gute Nacht", "Good night");
        let night_galley = ui.painter().layout_no_wrap(
            night_label.to_string(),
            FontId::proportional(12.0),
            pal.action_ink,
        );
        let btn_rect = Rect::from_min_size(
            Pos2::new(foot_rect.left(), cy - 12.0),
            Vec2::new(night_galley.size().x + 36.0, 24.0),
        );
        let btn_resp = ui.interact(btn_rect, egui::Id::new("footer_goodnight"), Sense::click());
        let fill_c = if btn_resp.hovered() { pal.action.gamma_multiply(0.85) } else { pal.action };
        ui.painter().rect_filled(btn_rect, Rounding::same(12.0), fill_c);
        let moon_center = Pos2::new(btn_rect.left() + 14.0, cy);
        draw_moon_icon(ui.painter(), moon_center, pal.action_ink, fill_c);
        let ng_h = night_galley.size().y;
        ui.painter().galley(Pos2::new(btn_rect.left() + 26.0, cy - ng_h * 0.5), night_galley, pal.action_ink);
        if btn_resp
            .on_hover_text(t("Schlie\u{00df}t die Session ab und zeigt den Beleg", "Closes the session and shows the receipt"))
            .clicked()
        {
            self.trigger_goodnight(ctx);
        }

        let claude_label = t("Claude fragen", "Ask Claude");
        let cl_galley = ui.painter().layout_no_wrap(claude_label.to_string(), FontId::proportional(11.5), Color32::PLACEHOLDER);
        let cl_rect = Rect::from_min_size(
            Pos2::new(btn_rect.right() + 8.0, cy - 12.0),
            Vec2::new(cl_galley.size().x + 22.0, 24.0),
        );
        let cl_resp = ui.interact(cl_rect, egui::Id::new("footer_claude"), Sense::click());
        let cl_on = self.claude_bar_open;
        ui.painter().rect(
            cl_rect,
            Rounding::same(12.0),
            if cl_on { pal.action.gamma_multiply(0.12) } else if cl_resp.hovered() { pal.hover } else { Color32::TRANSPARENT },
            Stroke::new(1.0_f32, if cl_on { pal.action.gamma_multiply(0.6) } else { pal.rule_strong }),
        );
        let cl_h = cl_galley.size().y;
        ui.painter().galley(
            Pos2::new(cl_rect.left() + 11.0, cy - cl_h * 0.5),
            cl_galley,
            if cl_on || cl_resp.hovered() { pal.ink } else { pal.ink_muted },
        );
        if cl_resp.clicked() {
            self.claude_bar_open = !self.claude_bar_open;
        }

        let quit_rect = Rect::from_min_size(
            Pos2::new(foot_rect.right() - 56.0, cy - 11.0),
            Vec2::new(56.0, 22.0),
        );
        let quit_resp = ui.interact(quit_rect, egui::Id::new("footer_quit"), Sense::click());
        ui.painter().text(
            Pos2::new(foot_rect.right() - 2.0, cy),
            egui::Align2::RIGHT_CENTER,
            t("Beenden", "Quit"),
            FontId::proportional(11.5),
            if quit_resp.hovered() { pal.ink } else { pal.ink_muted },
        );
        if quit_resp.clicked() {
            self.session_mgr.save();
            std::process::exit(0);
        }
    }
}
