//! Settings page.

use crate::{autostart, i18n, theme, StromWidgetApp};
use crate::i18n::t;
use crate::power::parse_price;
use crate::theme::{double_bezel_card, ThemePalette, THEMES};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};

impl StromWidgetApp {
    // =========================================================================
    // TAB 5: SETTINGS (Einstellungen)
    // =========================================================================
    pub(crate) fn render_tab_settings(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        double_bezel_card(ui, pal, 10.0, Vec2::new(12.0, 10.0), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Position", "Placement"))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [
                        ("br", t("Unten re.", "Bottom right")),
                        ("tc", t("Oben Mitte", "Top center")),
                        ("tr", t("Oben re.", "Top right")),
                        ("taskbar", t("Taskleiste (Dock)", "Taskbar (dock)")),
                    ] {
                        let on = self.settings.placement == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .stroke(Stroke::new(1.0_f32, if on { pal.action } else { pal.rule }))
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.placement = code.to_string();
                            self.settings.save();
                            self.sync_window_geometry(ctx, true);
                        }
                    }
                });
            });

            if self.settings.placement == "taskbar" {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("Taskleiste auf", "Taskbar on"))
                            .font(FontId::proportional(11.5))
                            .color(pal.ink_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for (code, lbl) in [
                            ("secondary", t("Zweitbildschirm", "Second screen")),
                            ("primary", t("Hauptbildschirm", "Main screen")),
                        ] {
                            let on = self.settings.taskbar_display == code;
                            let btn = egui::Button::new(
                                RichText::new(lbl)
                                    .font(FontId::proportional(11.0))
                                    .color(if on { pal.action_ink } else { pal.ink }),
                            )
                            .fill(if on { pal.action } else { pal.input_ground })
                            .stroke(Stroke::new(1.0_f32, if on { pal.action } else { pal.rule }))
                            .rounding(Rounding::same(5.0));
                            if ui.add(btn).clicked() {
                                self.settings.taskbar_display = code.to_string();
                                self.settings.save();
                                self.sync_window_geometry(ctx, true);
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("Versatz (oder Streifen ziehen)", "Offset (or drag the strip)"))
                            .font(FontId::proportional(11.5))
                            .color(pal.ink_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ofs = self.settings.taskbar_offset_x;
                        let small = |s: &str| {
                            egui::Button::new(RichText::new(s).font(FontId::proportional(11.0)).color(pal.ink))
                                .fill(pal.input_ground)
                                .stroke(Stroke::new(1.0_f32, pal.rule))
                                .rounding(Rounding::same(5.0))
                        };
                        if ui.add(small(t("Zur\u{00fc}cksetzen", "Reset"))).clicked() {
                            self.nudge_taskbar_offset(ctx, 0);
                        }
                        if ui.add(small("10 \u{25b6}")).on_hover_text(t("10 px nach rechts", "10 px to the right")).clicked() {
                            self.nudge_taskbar_offset(ctx, ofs + 10);
                        }
                        ui.label(
                            RichText::new(format!("{} px", ofs))
                                .font(FontId::monospace(11.0))
                                .color(pal.ink),
                        );
                        if ui.add(small("\u{25c0} 10")).on_hover_text(t("10 px nach links", "10 px to the left")).clicked() {
                            self.nudge_taskbar_offset(ctx, ofs - 10);
                        }
                    });
                });
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // 2. Strompreis
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr!("Strompreis ({}/kWh)", "Electricity price ({}/kWh)", i18n::cent_unit()))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("= {:.1}", self.settings.price_ct).replace('.', crate::i18n::dec()) + &format!(" {}/kWh", i18n::cent_unit()))
                            .font(FontId::proportional(11.0))
                            .color(if self.price_input_valid {
                                pal.action
                            } else {
                                Color32::from_rgb(0xFF, 0x4D, 0x4D)
                            }),
                    );
                    let resp = ui.add_sized(
                        [85.0, 22.0],
                        egui::TextEdit::singleline(&mut self.price_input_buf),
                    );
                    if resp.changed() {
                        if let Some(p) = parse_price(&self.price_input_buf) {
                            self.price_input_valid = true;
                            self.settings.price_ct = (p * 100.0).round() / 100.0;
                            self.settings.save();
                        } else {
                            self.price_input_valid = false;
                        }
                    }
                });
            });

            ui.add_space(6.0);

            // 3. Messmodus
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Messung", "Measurement"))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [
                        ("estimated", t("Gesch\u{00e4}tzt", "Estimated")),
                        ("measured", t("Gemessen (RAPL+NVML)", "Measured (RAPL+NVML)")),
                    ] {
                        let on = self.settings.mode == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .stroke(Stroke::new(1.0_f32, if on { pal.action } else { pal.rule }))
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.mode = code.to_string();
                            self.settings.save();
                        }
                    }
                });
            });

            ui.add_space(6.0);

            // 4. Monitors
            for i in 0..self.settings.monitors.len() {
                if self.monitor_w_bufs.len() <= i {
                    self.monitor_w_bufs
                        .push(format!("{:.0}", self.settings.monitors[i].w));
                }
                let m_name = self.settings.monitors[i].name.clone();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr!("Monitor: {}", "Monitor: {}", m_name))
                            .font(FontId::proportional(11.5))
                            .color(pal.ink),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("W").color(pal.ink_muted));
                        if ui
                            .add_sized(
                                [60.0, 20.0],
                                egui::TextEdit::singleline(&mut self.monitor_w_bufs[i]),
                            )
                            .changed()
                        {
                            if let Ok(v) = self.monitor_w_bufs[i].parse::<f64>() {
                                self.settings.monitors[i].w = v.clamp(0.0, 400.0);
                                self.settings.save();
                            }
                        }
                    });
                });
            }

            ui.add_space(4.0);

            // 5. Restsystem & Netzteil
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Rest-System (Mainboard, RAM, NVMe)", "Rest of system (mainboard, RAM, NVMe)"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("W").color(pal.ink_muted));
                    if ui
                        .add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.rest_w_buf))
                        .changed()
                    {
                        if let Ok(v) = self.rest_w_buf.parse::<f64>() {
                            self.settings.rest_w = v.clamp(0.0, 300.0);
                            self.settings.save();
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Netzteil-Nennleistung (80+ Gold)", "PSU rating (80+ Gold)"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("W").color(pal.ink_muted));
                    if ui
                        .add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.psu_w_buf))
                        .changed()
                    {
                        if let Ok(v) = self.psu_w_buf.parse::<f64>() {
                            self.settings.psu_w = v.clamp(200.0, 2000.0);
                            self.settings.save();
                        }
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // 6. Collapsed size and theme
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Gr\u{00f6}\u{00df}e zugeklappt", "Collapsed size"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [("m", t("M (Breiter)", "M (wider)")), ("xs", t("XS (Kompakt)", "XS (compact)"))] {
                        let on = self.settings.size == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.size = code.to_string();
                            self.settings.save();
                            self.sync_window_geometry(ctx, true);
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Sprache", "Language"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [("en", "English"), ("de", "Deutsch")] {
                        let on = self.settings.lang == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.lang = code.to_string();
                            i18n::set_en(code == "en");
                            self.settings.save();
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("W\u{00e4}hrung", "Currency"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [("USD", "$ Dollar"), ("EUR", "\u{20ac} Euro")] {
                        let on = self.settings.currency == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.currency = code.to_string();
                            i18n::set_usd(code == "USD");
                            self.settings.save();
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Design / Farbschema", "Design / color scheme"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let current_name = THEMES
                        .iter()
                        .find(|&&(id, _)| id == self.settings.theme)
                        .map(|&(id, _)| theme::theme_label(id))
                        .unwrap_or("Energielabel");
                    egui::ComboBox::from_id_salt("theme_combo")
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for &(id, _) in THEMES {
                                let name = theme::theme_label(id);
                                if ui
                                    .selectable_label(self.settings.theme == id, name)
                                    .clicked()
                                {
                                    self.settings.theme = id.to_string();
                                    self.settings.save();
                                }
                            }
                        });
                });
            });

            ui.add_space(4.0);

            // 7. Checkboxes & Opacity
            if ui
                .checkbox(
                    &mut self.settings.monitors_off_after_idle,
                    t("Monitore bei Leerlauf (>10 min) herausrechnen", "Exclude monitors when idle (>10 min)"),
                )
                .changed()
            {
                self.settings.save();
            }
            if ui
                .checkbox(
                    &mut self.settings.sys_addon,
                    t("System-Addon (CPU, RAM, Netzwerk, Ports & Web-UIs)", "System add-on (CPU, RAM, network, ports & web UIs)"),
                )
                .changed()
            {
                self.settings.save();
                self.sync_window_geometry(ctx, true);
            }
            if ui
                .checkbox(&mut self.settings.pinned, t("Immer im Vordergrund halten", "Keep always on top"))
                .changed()
            {
                self.settings.save();
                self.sync_window_geometry(ctx, true);
            }
            if ui
                .checkbox(&mut self.settings.autostart, t("Mit Windows starten", "Start with Windows"))
                .changed()
            {
                let want = self.settings.autostart;
                if !autostart::set(want) {
                    self.settings.autostart = autostart::is_enabled();
                }
                self.settings.save();
            }

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Deckkraft zugeklappt", "Collapsed opacity"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut op = self.settings.opacity as u32;
                    if ui
                        .add(egui::Slider::new(&mut op, 35..=100).suffix(" %"))
                        .changed()
                    {
                        self.settings.opacity = op as u8;
                        self.settings.save();
                    }
                });
            });
        });
    }
}
