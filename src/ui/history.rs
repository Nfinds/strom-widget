//! History tab: the completed sessions.

use crate::StromWidgetApp;
use crate::format::{fmt_clock, fmt_day, fmt_dur, fmt_euro};
use crate::i18n::t;
use crate::power::class_of;
use crate::session::SessionSummary;
use crate::theme::{class_bg_color, double_bezel_card, draw_class_arrow, ThemePalette};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Vec2};

impl StromWidgetApp {
    // =========================================================================
    // TAB 3: SESSIONS (Verlauf)
    // =========================================================================
    pub(crate) fn render_tab_sessions(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let cur = self.session_mgr.current_summary();
        let total_ct: f64 = self.sessions_cache.iter().map(|s| s.cost).sum::<f64>() + cur.cost;
        let total_wh: f64 = self.sessions_cache.iter().map(|s| s.wh).sum::<f64>() + cur.wh;

        double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
            ui.label(
                RichText::new(tr!(
                    "Gesamtverlauf: {} \u{00b7} {:.2} kWh \u{00b7} {} Sessions", "Total history: {} \u{00b7} {:.2} kWh \u{00b7} {} session(s)",
                    fmt_euro(total_ct),
                    total_wh / 1000.0,
                    self.sessions_cache.len() + 1
                ))
                .font(FontId::proportional(12.0))
                .strong()
                .color(pal.ink),
            );
        });

        ui.add_space(6.0);

        if self.sessions_cache.is_empty() {
            ui.label(
                RichText::new(
                    t("Noch keine abgeschlossene Session. Die erste entsteht mit \"Gute Nacht\" oder beim Herunterfahren.", "No completed session yet. The first one is created with \"Good night\" or when Windows shuts down."),
                )
                .font(FontId::proportional(11.5))
                .color(pal.ink_muted),
            );
            return;
        }

        let mut clicked_summary: Option<SessionSummary> = None;
        for s in &self.sessions_cache {
            double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
                let r = ui
                    .horizontal(|ui| {
                        let cls = class_of(s.avg_w);
                        let (b_rect, _) =
                            ui.allocate_exact_size(Vec2::new(22.0, 15.0), Sense::hover());
                        draw_class_arrow(
                            ui.painter(),
                            b_rect,
                            5.5,
                            class_bg_color(cls),
                            cls,
                            Color32::from_rgb(0x0B, 0x0C, 0x0E),
                            9.5,
                        );

                        ui.label(
                            RichText::new(fmt_day(s.start))
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{}\u{2013}{} \u{00b7} {} \u{00b7} {:.2} kWh",
                                fmt_clock(s.start),
                                fmt_clock(s.end),
                                fmt_dur(s.dur_s),
                                s.wh / 1000.0
                            ))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink_muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(fmt_euro(s.cost))
                                    .font(FontId::proportional(12.5))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                    })
                    .response
                    .interact(Sense::click());

                ui.label(
                    RichText::new(tr!(
                        "{} \u{00b7} \u{00d8} {:.0} W \u{00b7} {} Spikes", "{} \u{00b7} avg {:.0} W \u{00b7} {} spikes",
                        s.reason, s.avg_w, s.spikes
                    ))
                    .font(FontId::proportional(10.5))
                    .color(pal.ink_muted),
                );

                if r.clicked() {
                    clicked_summary = Some(s.clone());
                }
            });
            ui.add_space(4.0);
        }

        if let Some(sum) = clicked_summary {
            self.open_summary_view(Some(sum), "Session", false);
        }
    }
}
