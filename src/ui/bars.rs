//! Small chart building blocks for History and Network: a bar chart with an average line and
//! a tooltip under the mouse, a segmented switch, and the weekday x hour heatmap.

use crate::theme::{faint, ThemePalette};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Sense, Shape, Stroke, Vec2};

/// One bar. `label` is drawn under the bar (may be empty), `tip` shows on hover.
pub(crate) struct Bar {
    pub value: f64,
    pub label: String,
    pub tip: String,
    /// The bar of "now" (today, this hour): drawn in full color.
    pub current: bool,
    /// Lies in the future: drawn as an empty slot, no tooltip.
    pub future: bool,
}

/// Rounds a maximum up to a readable axis end (1, 2, 2.5, 5 x 10^n).
pub(crate) fn nice_ceiling(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    for f in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if f * mag >= v {
            return f * mag;
        }
    }
    10.0 * mag
}

/// Draws a bar chart over the full width. `avg` adds a dashed line with an "avg" label;
/// `axis` formats the values on the left axis and on the average line.
pub(crate) fn bar_chart(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    bars: &[Bar],
    avg: Option<f64>,
    height: f32,
    color: Color32,
    axis: &dyn Fn(f64) -> String,
) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
    let painter = ui.painter_at(rect.expand2(Vec2::new(0.0, 40.0)));
    let has_labels = bars.iter().any(|b| !b.label.is_empty());
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 38.0, rect.top() + 6.0),
        Pos2::new(rect.right(), rect.bottom() - if has_labels { 16.0 } else { 2.0 }),
    );
    let raw_max = bars
        .iter()
        .map(|b| b.value)
        .chain(avg)
        .fold(0.0_f64, f64::max);
    let max = nice_ceiling(raw_max * 1.08);
    let y_of = |v: f64| plot.bottom() - (v / max).clamp(0.0, 1.0) as f32 * plot.height();

    let axis_col = faint(pal);
    for f in [0.0, 0.5, 1.0] {
        let y = y_of(max * f);
        painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0_f32, pal.rule));
        painter.text(Pos2::new(rect.left(), y), egui::Align2::LEFT_CENTER, if f == 0.0 { "0".to_string() } else { axis(max * f) }, FontId::proportional(9.5), axis_col);
    }

    let n = bars.len().max(1);
    let slot = plot.width() / n as f32;
    let gap = (slot * 0.25).min(3.0);
    let hovered = resp
        .hover_pos()
        .filter(|p| plot.expand2(Vec2::new(0.0, 4.0)).contains(*p))
        .map(|p| (((p.x - plot.left()) / slot).floor() as usize).min(n - 1));

    for (i, b) in bars.iter().enumerate() {
        let x0 = plot.left() + i as f32 * slot + gap * 0.5;
        let bar_w = (slot - gap).max(1.0);
        if b.future {
            let r = Rect::from_min_max(Pos2::new(x0, plot.bottom() - 2.0), Pos2::new(x0 + bar_w, plot.bottom()));
            painter.rect_filled(r, Rounding::same(1.0), pal.rule);
        } else if b.value > 0.0 {
            let r = Rect::from_min_max(Pos2::new(x0, y_of(b.value).min(plot.bottom() - 1.5)), Pos2::new(x0 + bar_w, plot.bottom()));
            let col = if b.current || hovered == Some(i) { color } else { color.gamma_multiply(0.55) };
            painter.rect_filled(r, Rounding { nw: 2.0, ne: 2.0, sw: 0.0, se: 0.0 }, col);
        }
        if !b.label.is_empty() {
            painter.text(
                Pos2::new(x0 + bar_w * 0.5, rect.bottom() - 6.0),
                egui::Align2::CENTER_CENTER,
                &b.label,
                FontId::proportional(9.5),
                if b.current { pal.ink } else { axis_col },
            );
        }
    }

    if let Some(a) = avg.filter(|a| *a > 0.0) {
        let y = y_of(a);
        painter.extend(Shape::dashed_line(
            &[Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0_f32, pal.ink.gamma_multiply(0.55)),
            3.0,
            3.0,
        ));
        painter.text(
            Pos2::new(plot.right(), y - 3.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("\u{00d8} {}", axis(a)),
            FontId::proportional(10.0),
            pal.ink,
        );
    }

    if let Some(i) = hovered {
        let b = &bars[i];
        if !b.future && !b.tip.is_empty() {
            let galley = painter.layout_no_wrap(b.tip.clone(), FontId::proportional(11.0), pal.ink);
            let size = galley.size() + Vec2::new(12.0, 6.0);
            let cx = plot.left() + (i as f32 + 0.5) * slot;
            let top = y_of(b.value).min(plot.bottom()) - size.y - 6.0;
            let bx = (cx - size.x * 0.5).clamp(rect.left(), rect.right() - size.x);
            let box_rect = Rect::from_min_size(Pos2::new(bx, top.max(rect.top() - 30.0)), size);
            painter.rect(box_rect, Rounding::same(5.0), pal.hover, Stroke::new(1.0_f32, pal.rule_strong));
            painter.galley(box_rect.min + Vec2::new(6.0, 3.0), galley, pal.ink);
        }
    }
}

/// A segmented switch drawn right-aligned in `row`; returns the clicked index, if any.
pub(crate) fn segmented(ui: &mut egui::Ui, pal: &ThemePalette, row: Rect, id: &str, items: &[&str], selected: usize) -> Option<usize> {
    let galleys: Vec<_> = items
        .iter()
        .map(|l| ui.painter().layout_no_wrap(l.to_string(), FontId::proportional(10.5), Color32::PLACEHOLDER))
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 14.0).collect();
    let h = 22.0;
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let outer = Rect::from_min_size(Pos2::new(row.right() - total, row.center().y - h * 0.5), Vec2::new(total, h));
    ui.painter().rect(outer, Rounding::same(7.0), pal.input_ground, Stroke::new(1.0_f32, pal.rule));
    let mut x = outer.left() + 2.0;
    let mut clicked = None;
    for (i, (g, bw)) in galleys.into_iter().zip(widths).enumerate() {
        let r = Rect::from_min_size(Pos2::new(x, outer.top() + 2.0), Vec2::new(bw, h - 4.0));
        let resp = ui.interact(r, egui::Id::new((id, i)), Sense::click());
        if i == selected {
            ui.painter().rect_filled(r, Rounding::same(5.0), pal.hover);
        }
        let gs = g.size();
        ui.painter().galley(r.center() - gs * 0.5, g, if i == selected || resp.hovered() { pal.ink } else { pal.ink_muted });
        if resp.clicked() {
            clicked = Some(i);
        }
        x += bw;
    }
    clicked
}

/// Weekday x hour grid. `cells[weekday][hour]` is an average in W (weekday 0 = Monday).
pub(crate) fn heatmap(ui: &mut egui::Ui, pal: &ThemePalette, cells: &[[f64; 24]; 7], day_names: &[&str; 7], color: Color32) {
    let w = ui.available_width();
    let label_w = 22.0;
    let gap = 2.0;
    let cell = ((w - label_w) / 24.0 - gap).max(4.0);
    let height = 7.0 * (cell + gap) + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
    let painter = ui.painter_at(rect.expand2(Vec2::new(0.0, 30.0)));
    let max = cells.iter().flatten().fold(0.0_f64, |a, b| a.max(*b)).max(1.0);
    let axis_col = faint(pal);
    let mut hover: Option<(usize, usize)> = None;

    for (d, row) in cells.iter().enumerate() {
        let y = rect.top() + d as f32 * (cell + gap);
        painter.text(Pos2::new(rect.left(), y + cell * 0.5), egui::Align2::LEFT_CENTER, day_names[d], FontId::proportional(9.5), axis_col);
        for (h, v) in row.iter().enumerate() {
            let x = rect.left() + label_w + h as f32 * (cell + gap);
            let r = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(cell));
            let a = (*v / max) as f32;
            let fill = if *v <= 0.5 { pal.rule.gamma_multiply(0.6) } else { color.gamma_multiply(0.15 + 0.85 * a) };
            painter.rect_filled(r, Rounding::same(2.0), fill);
            if resp.hover_pos().is_some_and(|p| r.contains(p)) {
                hover = Some((d, h));
                painter.rect_stroke(r, Rounding::same(2.0), Stroke::new(1.0_f32, pal.ink));
            }
        }
    }
    for h in [0usize, 6, 12, 18] {
        let x = rect.left() + label_w + h as f32 * (cell + gap);
        painter.text(Pos2::new(x, rect.bottom() - 6.0), egui::Align2::LEFT_CENTER, h.to_string(), FontId::proportional(9.5), axis_col);
    }
    if let Some((d, h)) = hover {
        let txt = format!("{} {:02}:00 \u{00b7} \u{00d8} {:.0} W", day_names[d], h, cells[d][h]);
        let galley = painter.layout_no_wrap(txt, FontId::proportional(11.0), pal.ink);
        let size = galley.size() + Vec2::new(12.0, 6.0);
        let x = rect.left() + label_w + h as f32 * (cell + gap);
        let bx = (x - size.x * 0.5).clamp(rect.left(), rect.right() - size.x);
        let by = rect.top() + d as f32 * (cell + gap) - size.y - 4.0;
        let box_rect = Rect::from_min_size(Pos2::new(bx, by), size);
        painter.rect(box_rect, Rounding::same(5.0), pal.hover, Stroke::new(1.0_f32, pal.rule_strong));
        painter.galley(box_rect.min + Vec2::new(6.0, 3.0), galley, pal.ink);
    }
}
