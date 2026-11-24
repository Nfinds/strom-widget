//! Session summary: the receipt shown after "Good night" or for a session from the
//! history.

use crate::{ActiveTab, FrozenChart, StromWidgetApp, CREATE_NO_WINDOW};
use crate::format::{fmt_clock, fmt_day, fmt_dur, fmt_euro};
use crate::i18n::t;
use crate::power::class_of;
use crate::session::SessionSummary;
use crate::theme::{card, class_number_color, ThemePalette};
use crate::ui::draw_meter_bar;
use eframe::egui::{self, FontId, RichText, Rounding, Sense, Stroke, Vec2};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

impl StromWidgetApp {
    // =========================================================================
    // TAB 6: SESSION SUMMARY ("Gute Nacht" Receipt)
    // =========================================================================
    pub(crate) fn render_tab_summary(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let Some(sum) = self.active_summary.clone() else {
            card(ui, pal, 10.0, Vec2::new(12.0, 12.0), |ui| {
                ui.label(
                    RichText::new(t("Session zu kurz", "Session too short"))
                        .font(FontId::proportional(15.0))
                        .strong()
                        .color(pal.ink),
                );
                ui.label(
                    RichText::new(
                        t("Unter 5 Sekunden, daher keine Zusammenfassung gespeichert. Eine neue Session l\u{00e4}uft bereits.", "Under 5 seconds, so no summary was saved. A new session is already running."),
                    )
                    .font(FontId::proportional(11.5))
                    .color(pal.ink_muted),
                );
                ui.add_space(8.0);
                if ui.button(t("Weiter zu Live", "Continue to Live")).clicked() {
                    self.active_tab = ActiveTab::Live;
                }
            });
            return;
        };

        if self.frozen_chart.is_some() {
            self.render_chart_section(ui, pal);
            ui.add_space(14.0);
        }
        ui.vertical(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} \u{00b7} {} {}\u{2013}{}",
                    self.summary_title,
                    fmt_day(sum.start),
                    fmt_clock(sum.start),
                    fmt_clock(sum.end)
                ))
                .font(FontId::proportional(13.0))
                .strong()
                .color(pal.ink),
            );
            ui.add_space(8.0);

            let monthly_ct = (sum.cost / sum.dur_s.max(1.0)) * 3600.0 * 4.0 * 30.0;
            ui.columns(2, |cols| {
                card(&mut cols[0], pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
                        ui.label(
                            RichText::new(fmt_euro(sum.cost))
                                .font(FontId::proportional(24.0))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(tr!(
                                "{:.2} kWh in {}", "{:.2} kWh in {}",
                                sum.wh / 1000.0,
                                fmt_dur(sum.dur_s)
                            ))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink_muted),
                        );
                    });

                card(&mut cols[1], pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
                        ui.label(
                            RichText::new(fmt_euro(monthly_ct))
                                .font(FontId::proportional(24.0))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(t("pro Monat bei 4 h t\u{00e4}glich", "per month at 4 h a day"))
                                .font(FontId::proportional(11.0))
                                .color(pal.ink_muted),
                        );
                    });
            });

            ui.add_space(8.0);
            let cls = class_of(sum.avg_w);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.label(
                    RichText::new(tr!("\u{00d8} {:.0} W (Klasse {})", "avg {:.0} W (class {})", sum.avg_w, cls))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(class_number_color(cls)),
                );
                ui.label(
                    RichText::new(tr!("Spitze {:.0} W", "Peak {:.0} W", sum.peak.w))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink),
                );
                ui.label(
                    RichText::new(tr!("{} Spikes", "{} spikes", sum.spikes))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink_muted),
                );
            });

            if !sum.top.is_empty() {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t("CPU-Last nach Programm", "CPU load by program"))
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(pal.ink_muted),
                );
                let tot_share: f64 = sum.top.iter().map(|p| p.share).sum::<f64>().max(1.0);
                for p in &sum.top {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(Vec2::new(150.0, 16.0), |ui| {
                            ui.label(
                                RichText::new(&p.n)
                                    .font(FontId::proportional(11.5))
                                    .color(pal.ink),
                            );
                        });
                        let (r_bar, _) =
                            ui.allocate_exact_size(Vec2::new(140.0, 7.0), Sense::hover());
                        draw_meter_bar(
                            ui.painter(),
                            r_bar,
                            (p.share / tot_share) as f32,
                            pal.action,
                            pal.rule,
                        );
                        ui.label(
                            RichText::new(format!("{:.0} %", p.share))
                                .font(FontId::proportional(11.0))
                                .color(pal.ink_muted),
                        );
                    });
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if self.summary_fresh {
                    if !self.shutdown_scheduled {
                        let is_armed = self
                            .shutdown_armed_until
                            .map(|t| Instant::now() < t)
                            .unwrap_or(false);
                        let btn_txt = if is_armed {
                            t("Zum Best\u{00e4}tigen nochmal klicken", "Click again to confirm")
                        } else {
                            t("PC herunterfahren", "Shut down PC")
                        };
                        let sh_btn = egui::Button::new(
                            RichText::new(btn_txt)
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(pal.action_ink),
                        )
                        .fill(pal.action)
                        .rounding(Rounding::same(6.0));
                        if ui.add(sh_btn).clicked() {
                            if is_armed {
                                self.shutdown_armed_until = None;
                                self.shutdown_scheduled = true;
                                let _ = Command::new("shutdown")
                                    .creation_flags(CREATE_NO_WINDOW)
                                    .args([
                                        "/s",
                                        "/t",
                                        "10",
                                        "/c",
                                        t("Gute Nacht. Strom-Widget f\u{00e4}hrt den PC herunter.", "Good night. Strom-Widget is shutting the PC down."),
                                    ])
                                    .spawn();
                            } else {
                                self.shutdown_armed_until =
                                    Some(Instant::now() + Duration::from_secs(4));
                            }
                        }
                    } else {
                        let abort_btn = egui::Button::new(
                            RichText::new(t("F\u{00e4}hrt in 10 s herunter \u{00b7} Abbrechen", "Shutting down in 10 s \u{00b7} Cancel"))
                                .font(FontId::proportional(11.5))
                                .color(pal.ink),
                        )
                        .fill(pal.hover)
                        .stroke(Stroke::new(1.0_f32, pal.rule_strong))
                        .rounding(Rounding::same(6.0));
                        if ui.add(abort_btn).clicked() {
                            self.shutdown_scheduled = false;
                            let _ = Command::new("shutdown")
                                .creation_flags(CREATE_NO_WINDOW)
                                .arg("/a")
                                .spawn();
                        }
                    }
                }

                let claude_btn = egui::Button::new(
                    RichText::new(t("Fazit von Claude", "Summary from Claude"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                )
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule_strong))
                .rounding(Rounding::same(6.0));
                if ui.add(claude_btn).clicked() {
                    let q = tr!(
                        "Schreib ein kurzes Fazit zu dieser abgeschlossenen Session ({}\u{2013}{}): Kosten {}, {:.2} kWh, \u{00d8} {:.0} W, Spitze {:.0} W, {} Spikes.", "Write a short summary of this completed session ({}\u{2013}{}): cost {}, {:.2} kWh, avg {:.0} W, peak {:.0} W, {} spikes.",
                        fmt_clock(sum.start),
                        fmt_clock(sum.end),
                        fmt_euro(sum.cost),
                        sum.wh / 1000.0,
                        sum.avg_w,
                        sum.peak.w,
                        sum.spikes
                    );
                    self.ask_claude(&q, None, t("Fazit", "Summary"));
                }

                let close_btn = egui::Button::new(
                    RichText::new(t("Weiter", "Next"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                )
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule))
                .rounding(Rounding::same(6.0));
                if ui.add(close_btn).clicked() {
                    self.frozen_chart = None;
                    self.summary_fresh = false;
                    self.active_tab = ActiveTab::Live;
                }
            });
        });
    }

    pub(crate) fn open_summary_view(&mut self, sum: Option<SessionSummary>, title: &str, fresh: bool) {
        self.summary_title = title.to_string();
        self.summary_fresh = fresh;
        if let Some(ref s) = sum {
            if let Some(detail) = self.session_mgr.load_session_detail(&s.id) {
                let pts: Vec<(u64, f64)> = detail
                    .minutes
                    .iter()
                    .map(|m| (m.t + 30_000, m.avg))
                    .collect();
                self.frozen_chart = Some(FrozenChart {
                    points: pts,
                    from: s.start,
                    to: s.end,
                    spikes: detail.spikes,
                    label: tr!(
                        "Session {}\u{2013}{}", "Session {}\u{2013}{}",
                        fmt_clock(s.start),
                        fmt_clock(s.end)
                    ),
                });
            }
        }
        self.active_summary = sum;
        self.active_tab = ActiveTab::Summary;
    }
}
