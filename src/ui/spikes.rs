//! Spike list: the load peaks of the running session and their cause. Reached from the
//! "Spikes" section of the "Now" tab.

use crate::{ActiveTab, StromWidgetApp};
use crate::format::{fmt_clock, fmt_dur, fmt_money_ct};
use crate::i18n::t;
use crate::power::class_of;
use crate::session::Spike;
use crate::theme::{card, class_bg_color, draw_class_arrow, number_color, ThemePalette};
use eframe::egui::{self, Align, Color32, CursorIcon, FontId, Layout, RichText, Rounding, Sense, Stroke, Vec2};

/// How many spikes the "Now" tab shows before "see all".
pub(crate) const OVERVIEW_SPIKES: usize = 3;

impl StromWidgetApp {
    /// Which spikes the "Now" tab shows, as indices into `session_mgr.s.spikes`, in display
    /// order (top row first). At most `OVERVIEW_SPIKES`.
    pub(crate) fn overview_spikes(&self) -> Vec<usize> {
        let spikes = &self.session_mgr.s.spikes;
        // The newest three, newest first: "Now" answers "what just happened"; the full list
        // (with costs) is one click away.
        (0..spikes.len()).rev().take(OVERVIEW_SPIKES).collect()
    }

    pub(crate) fn render_tab_spikes(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let back = ui.add(
            egui::Label::new(
                RichText::new(t("\u{2190} Jetzt", "\u{2190} Now"))
                    .font(FontId::proportional(11.5))
                    .color(pal.ink_muted),
            )
            .sense(Sense::click()),
        );
        if back.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.active_tab = ActiveTab::Live;
        }
        ui.add_space(8.0);

        if self.session_mgr.s.spikes.is_empty() {
            card(ui, pal, 10.0, Vec2::new(12.0, 12.0), |ui| {
                ui.label(
                    RichText::new(
                        t("Noch keine Spikes in dieser Session.\nAls Spike z\u{00e4}hlt, wenn die Leistung mindestens 60 W und 35 % \u{00fc}ber der Grundlast der letzten zwei Minuten liegt.", "No spikes in this session yet.\nA spike is counted when the power is at least 60 W and 35 % above the base load of the last two minutes."),
                    )
                    .font(FontId::proportional(11.5))
                    .color(pal.ink_muted),
                );
            });
            return;
        }

        let spikes_snapshot: Vec<(usize, Spike)> = self
            .session_mgr
            .s
            .spikes
            .iter()
            .cloned()
            .enumerate()
            .rev()
            .collect();

        let mut ask_spike_req: Option<(usize, String)> = None;

        for (idx, sp) in spikes_snapshot {
            let is_open = self.open_spike_idx == Some(idx);
            let secs = ((sp.end.saturating_sub(sp.start)) as f64 / 1000.0).max(2.0);
            let dur_txt = if secs < 60.0 {
                format!("{:.0} s", secs)
            } else {
                fmt_dur(secs)
            };

            card(ui, pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
                let resp = ui
                    .horizontal(|ui| {
                        let cls = class_of(sp.peak);
                        let (badge_rect, _) =
                            ui.allocate_exact_size(Vec2::new(22.0, 15.0), Sense::hover());
                        draw_class_arrow(
                            ui.painter(),
                            badge_rect,
                            5.5,
                            class_bg_color(cls),
                            cls,
                            Color32::from_rgb(0x0B, 0x0C, 0x0E),
                            9.5,
                        );

                        ui.label(
                            RichText::new(fmt_clock(sp.start))
                                .font(FontId::proportional(12.0))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(tr!("bis {:.0} W \u{00b7} {}", "up to {:.0} W \u{00b7} {}", sp.peak, dur_txt))
                                .font(FontId::proportional(11.5))
                                .color(pal.ink_muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("+{}", fmt_money_ct(sp.extra_ct)))
                                    .font(FontId::proportional(12.5))
                                    .strong()
                                    .color(number_color(sp.peak)),
                            );
                        });
                    })
                    .response
                    .interact(Sense::click());

                ui.label(
                    RichText::new(sp.cause_line())
                        .font(FontId::proportional(11.0))
                        .color(pal.ink_muted),
                );

                if resp.clicked() {
                    self.open_spike_idx = if is_open { None } else { Some(idx) };
                }

                if is_open {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                    let procs_txt = if sp.procs.is_empty() {
                        String::new()
                    } else {
                        tr!(
                            "\nProzesse: {}", "\nProcesses: {}",
                            sp.procs
                                .iter()
                                .map(|p| format!("{} {:.0} %", p.n, p.c))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let fg_txt = sp
                        .fg
                        .as_ref()
                        .map(|f| tr!("\nIm Vordergrund: {}", "\nIn foreground: {}", f))
                        .unwrap_or_default();

                    ui.label(
                        RichText::new(tr!(
                            "Grundlast vorher {:.0} W, Spitze {:.0} W um {}. Mehrverbrauch {:.1} Wh.\nProzessor +{:.0} W, Grafikkarte +{:.0} W.{}{}", "Base load before {:.0} W, peak {:.0} W at {}. Extra use {:.1} Wh.\nProcessor +{:.0} W, graphics card +{:.0} W.{}{}",
                            sp.base,
                            sp.peak,
                            fmt_clock(sp.peak_t),
                            sp.extra_wh,
                            sp.cpu_rise,
                            sp.gpu_rise,
                            procs_txt,
                            fg_txt
                        ))
                        .font(FontId::proportional(11.0))
                        .color(pal.ink),
                    );

                    ui.add_space(4.0);
                    let ask_btn = egui::Button::new(
                        RichText::new(t("Claude zu diesem Spike fragen", "Ask Claude about this spike"))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink),
                    )
                    .fill(pal.hover)
                    .stroke(Stroke::new(1.0_f32, pal.rule_strong))
                    .rounding(Rounding::same(5.0));
                    if ui.add(ask_btn).clicked() {
                        ask_spike_req = Some((idx, tr!("Spike um {}", "Spike at {}", fmt_clock(sp.start))));
                    }
                }
            });
            ui.add_space(5.0);
        }

        if let Some((idx, lbl)) = ask_spike_req {
            self.ask_claude(
                t("Was hat diesen Spike verursacht, war er normal, und was hat er gekostet?", "What caused this spike, was it normal, and what did it cost?"),
                Some(idx),
                &lbl,
            );
        }
    }
}
