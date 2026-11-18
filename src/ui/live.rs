//! Live tab: energy label scale, breakdown by component and the session facts.

use crate::{i18n, StromWidgetApp};
use crate::format::{fmt_clock, fmt_dur, fmt_euro};
use crate::i18n::t;
use crate::power::class_of;
use crate::theme::{
    class_bg_color, class_number_color, double_bezel_card, draw_class_arrow, load_color,
    number_color, ThemePalette, CLASSES,
};
use crate::ui::draw_meter_bar;
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Margin, Pos2, Rect, RichText, Rounding, Sense, Stroke,
    Vec2,
};

impl StromWidgetApp {
    // =========================================================================
    // TAB 1: LIVE (EU Energy Label A-G Scale + Parts Breakdown + Fact Cards)
    // =========================================================================
    pub(crate) fn render_tab_live(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // 1. Signature EU Energy Label A-G Scale with Pointer
        double_bezel_card(ui, pal, 10.0, Vec2::new(10.0, 8.0), |ui| {
            let cur_cls = class_of(self.reading.watts);
            let avail_w = ui.available_width();
            let gap = 4.0;
            let arrow_w = (avail_w - gap * 6.0) / 7.0;
            let row_h = 26.0;

            let (scale_rect, _) =
                ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::hover());

            for (i, &(_, k)) in CLASSES.iter().enumerate() {
                let is_cur = k == cur_cls;
                let h = if is_cur { 25.0 } else { 18.0 };
                let y0 = scale_rect.top() + (row_h - h) * 0.5;
                let x0 = scale_rect.left() + (i as f32) * (arrow_w + gap);
                let r = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(arrow_w, h));
                draw_class_arrow(
                    ui.painter(),
                    r,
                    7.0,
                    class_bg_color(k),
                    k,
                    Color32::from_rgb(0x0B, 0x0C, 0x0E),
                    if is_cur { 13.5 } else { 11.0 },
                );
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Draw small vector right triangle pointer
                let (p_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 12.0), Sense::hover());
                let c = p_rect.center();
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(c.x - 3.0, c.y - 4.0),
                        Pos2::new(c.x + 3.0, c.y),
                        Pos2::new(c.x - 3.0, c.y + 4.0),
                    ],
                    class_number_color(cur_cls),
                    Stroke::NONE,
                ));

                ui.label(
                    RichText::new(tr!("Klasse {}", "Class {}", cur_cls))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(class_number_color(cur_cls)),
                );
                ui.label(
                    RichText::new(tr!("jetzt {:.0} W", "now {:.0} W", self.reading.watts))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(t("A <150 W \u{00b7} +100 W/Klasse \u{00b7} G \u{2265}650 W", "A <150 W \u{00b7} +100 W/class \u{00b7} G \u{2265}650 W"))
                            .font(FontId::proportional(10.0))
                            .color(pal.ink_muted),
                    );
                });
            });
        });

        ui.add_space(7.0);

        // 2. Parts Breakdown Table (CPU, GPU, Restsystem, Netzteilverlust, Monitore, Steckdose)
        double_bezel_card(ui, pal, 10.0, Vec2::new(11.0, 8.0), |ui| {
            let total_w = self.reading.watts.max(1.0);
            let rows = [
                (
                    t("Prozessor (CPU)", "Processor (CPU)"),
                    self.reading.parts.cpu.w,
                    self.reading.parts.cpu.src.as_str(),
                    false,
                ),
                (
                    t("Grafikkarte (GPU)", "Graphics card (GPU)"),
                    self.reading.parts.gpu.w,
                    self.reading.parts.gpu.src.as_str(),
                    false,
                ),
                (
                    t("Restsystem", "Rest of system"),
                    self.reading.parts.rest.w,
                    self.reading.parts.rest.src.as_str(),
                    false,
                ),
                (
                    t("Netzteilverlust", "PSU loss"),
                    self.reading.parts.psu.w,
                    self.reading.parts.psu.src.as_str(),
                    false,
                ),
                (
                    t("Monitore", "Monitors"),
                    self.reading.parts.mon.w,
                    self.reading.parts.mon.src.as_str(),
                    false,
                ),
                (
                    t("Steckdose (Gesamt)", "Wall socket (total)"),
                    self.reading.watts,
                    if self.settings.mode == "measured" {
                        "Summe"
                    } else {
                        t("Sch\u{00e4}tzung", "Estimate")
                    },
                    true,
                ),
            ];

            for (label, w_val, src, is_total) in rows {
                if is_total {
                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);
                }
                ui.horizontal(|ui| {
                    ui.allocate_ui(Vec2::new(132.0, 17.0), |ui| {
                        ui.label(
                            RichText::new(label)
                                .font(FontId::proportional(11.5))
                                .color(if is_total { pal.ink } else { pal.ink_muted })
                                .strong(),
                        );
                    });
                    ui.allocate_ui(Vec2::new(54.0, 17.0), |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.0} W", w_val))
                                    .font(FontId::proportional(12.0))
                                    .strong()
                                    .color(if is_total {
                                        number_color(w_val)
                                    } else {
                                        pal.ink
                                    }),
                            );
                        });
                    });
                    ui.allocate_ui(Vec2::new(66.0, 17.0), |ui| {
                        let src_full = match src {
                            "gem." => t("gemessen", "measured"),
                            "aus" => t("aus", "off"),
                            "Summe" => t("Summe", "Sum"),
                            _ => t("gesch\u{00e4}tzt", "estimated"),
                        };
                        ui.label(
                            RichText::new(src_full)
                                .font(FontId::proportional(10.5))
                                .color(if src == "gem." {
                                    pal.action
                                } else {
                                    pal.ink_muted
                                }),
                        );
                    });

                    let bar_w = ui.available_width().max(40.0);
                    let (bar_rect, _) =
                        ui.allocate_exact_size(Vec2::new(bar_w, 7.5), Sense::hover());
                    let share = (w_val / total_w).clamp(0.0, 1.0) as f32;
                    let fill_col = if is_total {
                        load_color(w_val)
                    } else {
                        pal.action
                    };
                    draw_meter_bar(ui.painter(), bar_rect, share, fill_col, pal.rule);
                });
            }

            ui.add_space(5.0);
            let diff_pct = if self.reading.measured_w > 0.0 {
                ((self.reading.estimated_w - self.reading.measured_w) / self.reading.measured_w)
                    * 100.0
            } else {
                0.0
            };
            let sign = if diff_pct >= 0.0 { "+" } else { "\u{2212}" };
            let gpu_str = self
                .reading
                .gpu_util
                .map(|u| format!("{:.0} %", u))
                .unwrap_or_else(|| "\u{2013}".to_string());
            let temp_str = self
                .reading
                .gpu_temp
                .map(|t| format!(" \u{00b7} {:.0} \u{00b0}C", t))
                .unwrap_or_default();
            let fg_str = self
                .reading
                .fg
                .as_ref()
                .map(|f| tr!(" \u{00b7} Fokus: {}", " \u{00b7} Focus: {}", f))
                .unwrap_or_default();

            ui.label(
                RichText::new(tr!(
                    "Gemessen {:.0} W \u{00b7} Gesch\u{00e4}tzt {:.0} W \u{00b7} Abweichung {}{:.0} %\nCPU {:.0} % \u{00b7} GPU {}{}{}", "Measured {:.0} W \u{00b7} Estimated {:.0} W \u{00b7} Deviation {}{:.0} %\nCPU {:.0} % \u{00b7} GPU {}{}{}",
                    self.reading.measured_w,
                    self.reading.estimated_w,
                    sign,
                    diff_pct.abs(),
                    self.reading.util,
                    gpu_str,
                    temp_str,
                    fg_str
                ))
                .font(FontId::proportional(11.0))
                .color(pal.ink_muted),
            );
        });

        ui.add_space(7.0);

        // 3. 6 Fact Cards in a clean 2-row x 3-column Equal-Width Layout
        let s = self.session_mgr.current_summary();
        let facts = [
            (t("Seit", "Since"), fmt_clock(s.start)),
            (t("Dauer", "Duration"), fmt_dur(s.dur_s)),
            (t("Kosten", "Cost"), fmt_euro(s.cost)),
            (t("Energie", "Energy"), format!("{:.2} kWh", s.wh / 1000.0).replace('.', crate::i18n::dec())),
            (t("Durchschnitt", "Average"), format!("{:.0} W", s.avg_w)),
            (t("Spitze", "Peak"), format!("{:.0} W", s.peak.w)),
        ];

        for row_pair in facts.chunks(3) {
            ui.columns(3, |cols| {
                for (c_idx, (k, v)) in row_pair.iter().enumerate() {
                    egui::Frame::none()
                        .fill(pal.card)
                        .stroke(Stroke::new(1.0_f32, pal.rule))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::symmetric(10.0, 6.0))
                        .show(&mut cols[c_idx], |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(*k)
                                    .font(FontId::proportional(10.5))
                                    .color(pal.ink_muted),
                            );
                            ui.label(
                                RichText::new(v)
                                    .font(FontId::proportional(14.0))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                }
            });
            ui.add_space(5.0);
        }

        ui.label(
            RichText::new(tr!(
                "Nativ in Rust gemessen: CPU-Paket (RAPL PDH), Grafikkarte (NVML). Wirkungsgrad {:.0} % \u{00b7} Tarif {:.1} {}/kWh.", "Measured natively in Rust: CPU package (RAPL PDH), graphics card (NVML). Efficiency {:.0} % \u{00b7} Tariff {:.1} {}/kWh.",
                self.reading.eff * 100.0,
                self.settings.price_ct,
                i18n::cent_unit()
            ))
            .font(FontId::proportional(10.5))
            .color(pal.ink_muted),
        );
    }
}
