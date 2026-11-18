//! Everything that is drawn: the collapsed strip, the expanded panel and its tabs.
//!
//! Each file adds its drawing methods to `StromWidgetApp` in an `impl` block of its own.

mod chart;
mod claude_bar;
mod flyout;
mod history;
mod live;
mod settings;
mod spikes;
mod strip;
mod summary;
mod system;

use eframe::egui::{self, Color32, Rect, Rounding, Vec2};

pub(crate) fn draw_meter_bar(
    painter: &egui::Painter,
    rect: Rect,
    share: f32,
    fill_col: Color32,
    bg_col: Color32,
) {
    let r = (rect.height() * 0.5).min(2.0);
    painter.rect_filled(rect, Rounding::same(r), bg_col);
    let s = share.clamp(0.0, 1.0);
    if s > 0.015 {
        let w = (rect.width() * s).max(2.0);
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
        painter.rect_filled(fill_rect, Rounding::same(r), fill_col);
    }
}
