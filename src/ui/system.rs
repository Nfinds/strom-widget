//! System & Ports tab: local web interfaces, listening ports and the biggest RAM users.

use crate::StromWidgetApp;
use crate::format::{fmt_bytes, fmt_rate};
use crate::i18n::t;
use crate::theme::{card, draw_diag_arrow_icon, draw_status_led, pct_color, section_title, ThemePalette};
use crate::ui::draw_meter_bar;
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2,
};

impl StromWidgetApp {
    // =========================================================================
    // TAB 4: SYSTEM & PORTS (Web UIs, All Listening TCP Ports + PID, Top RAM)
    // =========================================================================
    pub(crate) fn render_tab_system(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // CPU, RAM, download and upload: four figures with a thin bar each.
        let figures = [
            ("CPU", format!("{:.0} %", self.sys_view.cpu), (self.sys_view.cpu / 100.0) as f32, pct_color(self.sys_view.cpu)),
            ("RAM", format!("{:.0} %", self.sys_view.ram), (self.sys_view.ram / 100.0) as f32, pct_color(self.sys_view.ram)),
            (
                t("\u{2193} Download", "\u{2193} Download"),
                fmt_rate(self.sys_view.down),
                ((self.sys_view.down / 5_242_880.0) as f32).clamp(0.0, 1.0),
                Color32::from_rgb(0x5A, 0xA9, 0xFF),
            ),
            (
                t("\u{2191} Upload", "\u{2191} Upload"),
                fmt_rate(self.sys_view.up),
                ((self.sys_view.up / 2_097_152.0) as f32).clamp(0.0, 1.0),
                Color32::from_rgb(0xB4, 0x8C, 0xFF),
            ),
        ];
        ui.columns(4, |cols| {
            for (i, (lbl, val, share, col)) in figures.iter().enumerate() {
                let ui = &mut cols[i];
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(*lbl).font(FontId::proportional(10.5)).color(pal.ink_muted));
                ui.label(RichText::new(val).font(FontId::proportional(15.0)).strong().color(pal.ink));
                let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 3.0), Sense::hover());
                draw_meter_bar(ui.painter(), bar, *share, *col, pal.rule);
            }
        });
        ui.add_space(16.0);

        section_title(
            ui,
            pal,
            t("Web-Oberfl\u{00e4}chen", "Web interfaces"),
            &if self.sys_view.web.is_empty() {
                t("keine aktiv", "none active").to_string()
            } else {
                tr!("{} erreichbar", "{} reachable", self.sys_view.web.len())
            },
        );

        if self.sys_view.web.is_empty() {
            card(ui, pal, 10.0, Vec2::new(10.0, 8.0), |ui| {
                ui.label(
                    RichText::new(
                        t("Gerade antwortet kein lokaler Port mit einer Web-Oberfl\u{00e4}che. Sobald ein Server startet, erscheint er hier klickbar.", "No local port is currently answering with a web interface. As soon as a server starts, it shows up here as a link."),
                    )
                    .font(FontId::proportional(11.0))
                    .color(pal.ink_muted),
                );
            });
        } else {
            let row_w = ui.available_width();
            let row_h = 44.0;
            for (idx, w) in self.sys_view.web.iter().enumerate() {
                let (card_rect, _) =
                    ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::hover());
                let cy = card_rect.center().y;

                // Measure port pill first so we can check hover on the pill
                let port_str = format!(":{}", w.port);
                let port_galley = ui.painter().layout_no_wrap(
                    port_str,
                    FontId::proportional(11.5),
                    pal.ink,
                );
                let port_text_w = port_galley.size().x;
                // 11px left pad + text_w + 9px gap + 20px circle + 3px right pad
                let pill_w = (11.0 + port_text_w + 9.0 + 20.0 + 3.0).max(66.0);
                let pill_h = 26.0;
                let pill_rect = Rect::from_min_size(
                    Pos2::new(card_rect.right() - 10.0 - pill_w, cy - pill_h * 0.5),
                    Vec2::new(pill_w, pill_h),
                );
                let pill_resp = ui.interact(
                    pill_rect,
                    egui::Id::new(("web_port_pill", idx, w.port)),
                    Sense::click(),
                );

                // Sleek single-border Obsidian card (avoids 14 stacked double-bezel lines)
                let card_border = if pill_resp.hovered() {
                    pal.rule_strong
                } else {
                    pal.rule
                };
                ui.painter().rect(
                    card_rect,
                    Rounding::same(10.0),
                    pal.card,
                    Stroke::new(1.0_f32, card_border),
                );

                // 1. Glowing Status LED on the left
                let led_center = Pos2::new(card_rect.left() + 16.0, cy);
                draw_status_led(ui.painter(), led_center, w.lan);

                // 2. Title + Process/PID subtitle stacked cleanly around cy (clipped before pill_rect)
                let title = if w.title.is_empty() {
                    &w.name
                } else {
                    &w.title
                };
                let text_left = card_rect.left() + 30.0;
                let text_clip = Rect::from_min_max(
                    Pos2::new(text_left, card_rect.top()),
                    Pos2::new((pill_rect.left() - 8.0).max(text_left + 20.0), card_rect.bottom()),
                );
                let text_painter = ui.painter().with_clip_rect(text_clip);
                text_painter.text(
                    Pos2::new(text_left, cy - 6.8),
                    egui::Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(12.2),
                    pal.ink,
                );
                let sub_str = format!(
                    "{} (PID {}){}",
                    w.name,
                    w.pid,
                    if w.lan { t(" \u{00b7} LAN", " \u{00b7} LAN") } else { t(" \u{00b7} lokal", " \u{00b7} local") }
                );
                text_painter.text(
                    Pos2::new(text_left, cy + 7.2),
                    egui::Align2::LEFT_CENTER,
                    sub_str,
                    FontId::proportional(10.2),
                    pal.ink_muted,
                );

                // 3. Obsidian Linear Button-in-Button Port Pill (":4196 [↗]") with guaranteed 9px gap
                let bg_col = if pill_resp.hovered() {
                    pal.hover
                } else {
                    pal.input_ground
                };
                let border_col = if pill_resp.hovered() {
                    pal.action.gamma_multiply(0.65)
                } else {
                    pal.rule_strong
                };
                ui.painter().rect(
                    pill_rect,
                    Rounding::same(13.0),
                    bg_col,
                    Stroke::new(1.0_f32, border_col),
                );

                ui.painter().galley(
                    Pos2::new(pill_rect.left() + 11.0, cy - port_galley.size().y * 0.5),
                    port_galley,
                    pal.ink,
                );

                let circle_center = Pos2::new(pill_rect.right() - 13.0, cy);
                ui.painter().circle_filled(
                    circle_center,
                    10.0,
                    if pill_resp.hovered() {
                        pal.action
                    } else {
                        pal.action.gamma_multiply(0.20)
                    },
                );
                draw_diag_arrow_icon(
                    ui.painter(),
                    circle_center,
                    if pill_resp.hovered() {
                        pal.action_ink
                    } else {
                        pal.action
                    },
                );

                if pill_resp.on_hover_text(&w.url).clicked() {
                    let _ = open::that(&w.url);
                }

                ui.add_space(5.0);
            }
        }

        ui.add_space(12.0);

        // 2. All Listening Ports & owning processes
        let toggle_txt = if self.show_all_ports {
            t("ausblenden", "hide")
        } else {
            t("anzeigen", "show")
        };
        let toggle = section_title(
            ui,
            pal,
            &tr!("Belegte Ports ({})", "Used ports ({})", self.sys_view.all_ports.len()),
            toggle_txt,
        );
        if toggle.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.show_all_ports = !self.show_all_ports;
        }

        if self.show_all_ports && !self.sys_view.all_ports.is_empty() {
            card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
                for p in &self.sys_view.all_ports {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(Vec2::new(58.0, 17.0), |ui| {
                            ui.label(
                                RichText::new(format!(":{}", p.port))
                                    .font(FontId::proportional(11.5))
                                    .strong()
                                    .color(if p.is_web { pal.action } else { pal.ink }),
                            );
                        });
                        ui.allocate_ui(Vec2::new(165.0, 17.0), |ui| {
                            ui.label(
                                RichText::new(&p.name)
                                    .font(FontId::proportional(11.5))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                        ui.label(
                            RichText::new(format!("PID {} \u{00b7} {}", p.pid, p.addr))
                                .font(FontId::proportional(10.5))
                                .color(pal.ink_muted),
                        );
                        if let Some(ref url) = p.web_url {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .small_button(
                                        RichText::new(t("\u{00d6}ffnen", "Open")).color(pal.action),
                                    )
                                    .clicked()
                                {
                                    let _ = open::that(url);
                                }
                            });
                        }
                    });
                }
            });
            ui.add_space(8.0);
        }

        // 3. Top RAM & CPU Consumers
        ui.add_space(4.0);
        section_title(ui, pal, t("Gr\u{00f6}\u{00df}te Programme", "Biggest programs"), t("RAM \u{00b7} CPU", "RAM \u{00b7} CPU"));

        card(ui, pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
            let max_mem = self
                .sys_view
                .top
                .first()
                .map(|g| g.mem.max(1))
                .unwrap_or(1) as f32;
            // Fixed columns: name | bar | RAM | CPU, so bars and numbers line up row to row.
            let name_w = 150.0;
            let cpu_w = 48.0;
            let mem_w = 64.0;
            for g in &self.sys_view.top {
                let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
                let cy = row.center().y;
                let label = if g.count > 1 {
                    format!("{} ({})", g.title, g.count)
                } else {
                    g.title.clone()
                };
                ui.painter()
                    .with_clip_rect(Rect::from_min_size(row.min, Vec2::new(name_w - 8.0, row.height())))
                    .text(Pos2::new(row.left(), cy), egui::Align2::LEFT_CENTER, label, FontId::proportional(11.5), pal.ink);

                let bar_left = row.left() + name_w;
                let bar_right = row.right() - cpu_w - mem_w - 10.0;
                let bar_rect = Rect::from_min_max(Pos2::new(bar_left, cy - 3.0), Pos2::new(bar_right.max(bar_left + 20.0), cy + 3.0));
                let share = ((g.mem as f32) / max_mem).clamp(0.0, 1.0);
                draw_meter_bar(ui.painter(), bar_rect, share, pal.action, pal.rule);

                ui.painter().text(
                    Pos2::new(row.right() - cpu_w, cy),
                    egui::Align2::RIGHT_CENTER,
                    fmt_bytes(g.mem),
                    FontId::proportional(11.5),
                    pal.ink,
                );
                ui.painter().text(
                    Pos2::new(row.right(), cy),
                    egui::Align2::RIGHT_CENTER,
                    format!("{:.1} %", g.cpu).replace('.', crate::i18n::dec()),
                    FontId::proportional(11.0),
                    pal.ink_muted,
                );
            }
        });

        ui.add_space(6.0);
        let ask_ram = egui::Button::new(
            RichText::new(t("Was kann ich schlie\u{00df}en? Claude fragen", "What can I close? Ask Claude"))
                .font(FontId::proportional(11.5))
                .color(pal.ink),
        )
        .fill(pal.card)
        .stroke(Stroke::new(1.0_f32, pal.rule_strong))
        .rounding(Rounding::same(6.0));
        if ui.add(ask_ram).clicked() {
            self.ask_claude(
                t("Welche Programme belegen gerade am meisten Arbeitsspeicher, brauche ich die gerade, und was kann ich gefahrlos schlie\u{00df}en?", "Which programs are using the most memory right now, do I need them, and what can I safely close?"),
                None,
                t("Was frisst RAM?", "What eats RAM?"),
            );
        }
    }
}
