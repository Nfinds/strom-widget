//! Asking Claude from the panel: the question with its measurement context, the answer
//! box, the quick questions and the input line.

use crate::StromWidgetApp;
use crate::claude::{ask_async, ClaudeStatus};
use crate::i18n::t;
use crate::session::{compact_minute, CompactMinute, Spike};
use crate::theme::{card, draw_close_icon, draw_send_icon, ThemePalette};
use eframe::egui::{
    self, Align, Color32, FontId, Key, Layout, Margin, RichText, Rounding, Sense, Stroke, Vec2,
};
use std::sync::Arc;

impl StromWidgetApp {
    // =========================================================================
    // CLAUDE ANSWER DRAWER & PROMPT BAR
    // =========================================================================
    pub(crate) fn render_claude_answer_box(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // A poisoned lock (a panic in the asking thread) counts as "nothing to show", never as a crash.
        let st_snapshot = self
            .claude_status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(ClaudeStatus::Idle);
        if matches!(st_snapshot, ClaudeStatus::Idle) {
            return;
        }

        card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
            ui.horizontal(|ui| {
                let lbl = match &st_snapshot {
                    ClaudeStatus::Asking { label, .. } => tr!("Claude \u{00b7} {}", "Claude \u{00b7} {}", label),
                    ClaudeStatus::Done { label, .. } => tr!("Claude \u{00b7} {}", "Claude \u{00b7} {}", label),
                    ClaudeStatus::Error { label, .. } => tr!("Claude \u{00b7} {}", "Claude \u{00b7} {}", label),
                    ClaudeStatus::Idle => String::new(),
                };
                ui.label(
                    RichText::new(lbl)
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(pal.action),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (x_rect, x_resp) =
                        ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::click());
                    draw_close_icon(ui.painter(), x_rect.center(), 7.0, pal.ink_muted);
                    if x_resp.clicked() {
                        if let Ok(mut st) = self.claude_status.lock() {
                            *st = ClaudeStatus::Idle;
                        }
                    }
                });
            });

            match st_snapshot {
                ClaudeStatus::Asking { started_at, .. } => {
                    ui.label(
                        RichText::new(tr!(
                            "Claude liest die Messdaten \u{00b7} {} s ...", "Claude is reading the measurements \u{00b7} {} s ...",
                            started_at.elapsed().as_secs()
                        ))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink_muted),
                    );
                }
                ClaudeStatus::Done { answer, .. } => {
                    egui::ScrollArea::vertical()
                        .max_height(55.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(answer)
                                    .font(FontId::proportional(11.5))
                                    .color(pal.ink),
                            );
                        });
                }
                ClaudeStatus::Error { error, .. } => {
                    ui.label(
                        RichText::new(tr!("Keine Antwort: {}", "No answer: {}", error))
                            .font(FontId::proportional(11.0))
                            .color(Color32::from_rgb(0xFF, 0x8A, 0x4A)),
                    );
                }
                ClaudeStatus::Idle => {}
            }
        });
        ui.add_space(5.0);
    }

    pub(crate) fn render_claude_prompt_bar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let chips = [
                (
                    t("Zusammenfassung", "Summary"),
                    t("Fass diese Session kurz zusammen: Kosten, was hat am meisten Strom gezogen, und was war auff\u{00e4}llig?", "Summarize this session briefly: cost, what drew the most power, and what stood out?"),
                ),
                (
                    t("Gr\u{00f6}\u{00df}ter Spike", "Biggest spike"),
                    t("Was war der gr\u{00f6}\u{00df}te Spike, was hat ihn verursacht und was hat er gekostet?", "What was the biggest spike, what caused it, and what did it cost?"),
                ),
                (
                    t("Sparpotenzial", "Savings potential"),
                    t("Wo k\u{00f6}nnte ich bei diesem Nutzungsmuster konkret Strom sparen, und wie viel w\u{00e4}re das ungef\u{00e4}hr pro Monat?", "Where could I concretely save power with this usage pattern, and roughly how much would that be per month?"),
                ),
            ];
            for (lbl, q) in chips {
                let chip = egui::Button::new(
                    RichText::new(lbl)
                        .font(FontId::proportional(10.5))
                        .color(pal.ink_muted),
                )
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule))
                .rounding(Rounding::same(10.0));
                if ui.add(chip).clicked() {
                    self.ask_claude(q, None, lbl);
                }
            }
        });

        ui.add_space(4.0);
        egui::Frame::none()
            .fill(pal.input_ground)
            .stroke(Stroke::new(1.0_f32, pal.rule))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(8.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.claude_input)
                        .frame(false)
                        .hint_text(t("Claude zu dieser Session fragen ...", "Ask Claude about this session ..."))
                        .desired_width(ui.available_width() - 32.0);
                    let resp = ui.add(edit);
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                    // Custom Vector Send Button aligned inside the input pill
                    let (send_rect, send_resp) =
                        ui.allocate_exact_size(Vec2::new(26.0, 20.0), Sense::click());
                    ui.painter().rect_filled(
                        send_rect,
                        Rounding::same(5.0),
                        if send_resp.hovered() {
                            Color32::from_rgb(0x16, 0xD0, 0x6A)
                        } else {
                            pal.action
                        },
                    );
                    draw_send_icon(ui.painter(), send_rect.center(), pal.action_ink);

                    if (send_resp.on_hover_text(t("Fragen", "Ask")).clicked() || enter)
                        && !self.claude_input.trim().is_empty()
                    {
                        let q = self.claude_input.trim().to_string();
                        self.claude_input.clear();
                        self.ask_claude(&q, None, &q);
                    }
                });
            });
    }

    pub(crate) fn ask_claude(&mut self, question: &str, spike_idx: Option<usize>, label: &str) {
        let cur_sum = self.session_mgr.current_summary();
        let mins: Vec<CompactMinute> = self
            .session_mgr
            .s
            .minutes
            .iter()
            .rev()
            .take(180)
            .rev()
            .map(compact_minute)
            .collect();
        let spikes: Vec<&Spike> = self.session_mgr.s.spikes.iter().rev().take(25).collect();
        let asked_spike = spike_idx.and_then(|i| self.session_mgr.s.spikes.get(i));

        let ctx_val = serde_json::json!({
            "jetzt": {
                "wattGesamt": self.reading.watts.round(),
                "wattGemessen": self.reading.measured_w.round(),
                "wattGeschaetzt": self.reading.estimated_w.round(),
                "cpuW": self.reading.parts.cpu.w.round(),
                "gpuW": self.reading.parts.gpu.w.round(),
                "cpuAuslastung": self.reading.util,
                "gpuAuslastung": self.reading.gpu_util,
                "gpuTemp": self.reading.gpu_temp,
                "vordergrund": self.reading.fg,
                "topProzesse": self.reading.top,
            },
            "preisCtProKWh": self.settings.price_ct,
            "waehrung": self.settings.currency,
            "modus": self.settings.mode,
            "session": cur_sum,
            "minuten": mins,
            "spikes": spikes,
            "frageZuSpike": asked_spike,
            "system": if self.settings.sys_addon { Some(&self.sys_view) } else { None },
        });

        let hist = self
            .claude_history
            .lock()
            .map(|h| h.iter().rev().take(3).rev().cloned().collect())
            .unwrap_or_default();

        ask_async(
            question.to_string(),
            label.to_string(),
            ctx_val.to_string(),
            hist,
            Arc::clone(&self.claude_status),
            Arc::clone(&self.claude_history),
        );
    }
}
