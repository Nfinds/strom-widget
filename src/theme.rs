// Design System & Theme Tokens for Strom-Widget Rust
// Implements the signature EU Energy Label ("Energielabel") A-G color system
// plus all 9 alternate themes, Double-Bezel card geometry, custom Class Arrow painter,
// and custom vector icon primitives (zero font-glyph dependency!).

use crate::power::class_of;
use egui::{Color32, Pos2, Rect, Rounding, Shape, Stroke, Vec2};

pub const THEMES: &[(&str, &str)] = &[
    ("label", "Energielabel (EU A-G)"),
    ("graphite", "Graphit"),
    ("mica", "Windows 11 Mica"),
    ("pump", "Zapfs\u{00e4}ule"),
    ("taxi", "Taxameter"),
    ("ferraris", "Ferraris-Z\u{00e4}hler"),
    ("datamatics", "Datamatics"),
    ("nord", "Nord"),
    ("paper", "Papier"),
    ("receipt", "Kassenbon"),
];

pub const CLASSES: &[(f64, &str)] = &[
    (150.0, "A"),
    (250.0, "B"),
    (350.0, "C"),
    (450.0, "D"),
    (550.0, "E"),
    (650.0, "F"),
    (f64::INFINITY, "G"),
];

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub ground: Color32,
    pub outer_shell: Color32,
    pub card: Color32,
    pub input_ground: Color32,
    pub hover: Color32,
    pub rule: Color32,
    pub rule_strong: Color32,
    pub frame: Color32,
    pub ink: Color32,
    pub ink_muted: Color32,
    pub action: Color32,
    pub action_ink: Color32,
}

impl ThemePalette {
    pub fn for_id(theme: &str) -> Self {
        match theme {
            "graphite" => Self {
                ground: Color32::from_rgb(0x0E, 0x10, 0x13),
                outer_shell: Color32::from_rgb(0x14, 0x17, 0x1C),
                card: Color32::from_rgb(0x18, 0x1B, 0x20),
                input_ground: Color32::from_rgb(0x09, 0x0A, 0x0C),
                hover: Color32::from_rgb(0x20, 0x24, 0x2B),
                rule: Color32::from_rgb(0x25, 0x29, 0x30),
                rule_strong: Color32::from_rgb(0x38, 0x3E, 0x47),
                frame: Color32::from_rgb(0x2C, 0x31, 0x3A),
                ink: Color32::from_rgb(0xF3, 0xF4, 0xF6),
                ink_muted: Color32::from_rgb(0x9C, 0xA3, 0xAF),
                action: Color32::from_rgb(0x38, 0xBD, 0xF8),
                action_ink: Color32::from_rgb(0x04, 0x13, 0x1D),
            },
            "mica" => Self {
                ground: Color32::from_rgb(0x1C, 0x1F, 0x26),
                outer_shell: Color32::from_rgb(0x22, 0x26, 0x2E),
                card: Color32::from_rgb(0x27, 0x2B, 0x35),
                input_ground: Color32::from_rgb(0x15, 0x17, 0x1D),
                hover: Color32::from_rgb(0x2F, 0x34, 0x40),
                rule: Color32::from_rgb(0x34, 0x39, 0x46),
                rule_strong: Color32::from_rgb(0x49, 0x50, 0x61),
                frame: Color32::from_rgb(0x3B, 0x41, 0x4F),
                ink: Color32::from_rgb(0xF5, 0xF7, 0xFA),
                ink_muted: Color32::from_rgb(0xA6, 0xAE, 0xBC),
                action: Color32::from_rgb(0x60, 0xCD, 0xFF),
                action_ink: Color32::from_rgb(0x05, 0x16, 0x22),
            },
            "nord" => Self {
                ground: Color32::from_rgb(0x24, 0x29, 0x33),
                outer_shell: Color32::from_rgb(0x2B, 0x30, 0x3C),
                card: Color32::from_rgb(0x2E, 0x34, 0x40),
                input_ground: Color32::from_rgb(0x1D, 0x21, 0x2A),
                hover: Color32::from_rgb(0x3B, 0x42, 0x52),
                rule: Color32::from_rgb(0x43, 0x4C, 0x5E),
                rule_strong: Color32::from_rgb(0x4C, 0x56, 0x6A),
                frame: Color32::from_rgb(0x43, 0x4C, 0x5E),
                ink: Color32::from_rgb(0xEC, 0xEF, 0xF4),
                ink_muted: Color32::from_rgb(0xD8, 0xDE, 0xE9),
                action: Color32::from_rgb(0x88, 0xC0, 0xD0),
                action_ink: Color32::from_rgb(0x1D, 0x21, 0x2A),
            },
            "pump" => Self {
                ground: Color32::from_rgb(0x1B, 0x1E, 0x22),
                outer_shell: Color32::from_rgb(0x15, 0x18, 0x1B),
                card: Color32::from_rgb(0x0B, 0x0C, 0x0D),
                input_ground: Color32::from_rgb(0x0F, 0x11, 0x13),
                hover: Color32::from_rgb(0x25, 0x29, 0x30),
                rule: Color32::from_rgb(0x2F, 0x34, 0x3A),
                rule_strong: Color32::from_rgb(0x44, 0x4A, 0x52),
                frame: Color32::from_rgb(0x3A, 0x40, 0x48),
                ink: Color32::from_rgb(0xF1, 0xF1, 0xF1),
                ink_muted: Color32::from_rgb(0xA9, 0xAF, 0xB6),
                action: Color32::from_rgb(0xFF, 0xC6, 0x29),
                action_ink: Color32::from_rgb(0x16, 0x13, 0x0A),
            },
            "taxi" => Self {
                ground: Color32::from_rgb(0x0B, 0x0B, 0x0B),
                outer_shell: Color32::from_rgb(0x10, 0x10, 0x10),
                card: Color32::from_rgb(0x13, 0x13, 0x13),
                input_ground: Color32::from_rgb(0x05, 0x05, 0x05),
                hover: Color32::from_rgb(0x18, 0x18, 0x18),
                rule: Color32::from_rgb(0x26, 0x26, 0x26),
                rule_strong: Color32::from_rgb(0x3A, 0x3A, 0x3A),
                frame: Color32::from_rgb(0x33, 0x33, 0x33),
                ink: Color32::from_rgb(0xED, 0xED, 0xED),
                ink_muted: Color32::from_rgb(0xA3, 0xA3, 0xA3),
                action: Color32::from_rgb(0xFF, 0x3B, 0x2F),
                action_ink: Color32::from_rgb(0x0B, 0x0B, 0x0B),
            },
            "ferraris" => Self {
                ground: Color32::from_rgb(0xE8, 0xE5, 0xDE),
                outer_shell: Color32::from_rgb(0xEF, 0xED, 0xE7),
                card: Color32::from_rgb(0xF3, 0xF1, 0xEC),
                input_ground: Color32::from_rgb(0xF6, 0xF4, 0xEF),
                hover: Color32::from_rgb(0xDE, 0xDA, 0xD2),
                rule: Color32::from_rgb(0xCB, 0xC6, 0xBC),
                rule_strong: Color32::from_rgb(0xA9, 0xA3, 0x98),
                frame: Color32::from_rgb(0xB9, 0xB3, 0xA8),
                ink: Color32::from_rgb(0x1A, 0x1A, 0x1A),
                ink_muted: Color32::from_rgb(0x55, 0x52, 0x4D),
                action: Color32::from_rgb(0xC8, 0x10, 0x2E),
                action_ink: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            },
            "datamatics" => Self {
                ground: Color32::from_rgb(0x00, 0x00, 0x00),
                outer_shell: Color32::from_rgb(0x00, 0x00, 0x00),
                card: Color32::from_rgb(0x05, 0x05, 0x05),
                input_ground: Color32::from_rgb(0x00, 0x00, 0x00),
                hover: Color32::from_rgb(0x1A, 0x1A, 0x1A),
                rule: Color32::from_rgb(0x4A, 0x4A, 0x4A),
                rule_strong: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                frame: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                ink: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                ink_muted: Color32::from_rgb(0xC4, 0xC4, 0xC4),
                action: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                action_ink: Color32::from_rgb(0x00, 0x00, 0x00),
            },
            "paper" => Self {
                ground: Color32::from_rgb(0xF6, 0xF3, 0xEC),
                outer_shell: Color32::from_rgb(0xEC, 0xE7, 0xDD),
                card: Color32::from_rgb(0xFF, 0xFD, 0xF9),
                input_ground: Color32::from_rgb(0xED, 0xE9, 0xE0),
                hover: Color32::from_rgb(0xE6, 0xE1, 0xD6),
                rule: Color32::from_rgb(0xD5, 0xCF, 0xC2),
                rule_strong: Color32::from_rgb(0xAC, 0xA3, 0x93),
                frame: Color32::from_rgb(0xC4, 0xBD, 0xAE),
                ink: Color32::from_rgb(0x18, 0x17, 0x15),
                ink_muted: Color32::from_rgb(0x65, 0x61, 0x59),
                action: Color32::from_rgb(0x00, 0x8A, 0x42),
                action_ink: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            },
            "receipt" => Self {
                ground: Color32::from_rgb(0xFA, 0xFA, 0xF7),
                outer_shell: Color32::from_rgb(0xF1, 0xF1, 0xEC),
                card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                input_ground: Color32::from_rgb(0xEF, 0xEF, 0xEA),
                hover: Color32::from_rgb(0xE8, 0xE8, 0xE2),
                rule: Color32::from_rgb(0xDA, 0xDA, 0xD4),
                rule_strong: Color32::from_rgb(0xB4, 0xB4, 0xAE),
                frame: Color32::from_rgb(0xCF, 0xCF, 0xC9),
                ink: Color32::from_rgb(0x11, 0x11, 0x11),
                ink_muted: Color32::from_rgb(0x5E, 0x5E, 0x5E),
                action: Color32::from_rgb(0x11, 0x11, 0x11),
                action_ink: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            },
            _ => Self {
                // Default "label" (Obsidian Linear + EU Energy Label A-G)
                ground: Color32::from_rgb(0x09, 0x0B, 0x0F),
                outer_shell: Color32::from_rgb(0x0D, 0x10, 0x16),
                card: Color32::from_rgb(0x10, 0x13, 0x1A),
                input_ground: Color32::from_rgb(0x07, 0x09, 0x0D),
                hover: Color32::from_rgb(0x16, 0x1B, 0x25),
                rule: Color32::from_rgb(0x1D, 0x22, 0x2E),
                rule_strong: Color32::from_rgb(0x2A, 0x32, 0x42),
                frame: Color32::from_rgb(0x24, 0x2B, 0x38),
                ink: Color32::from_rgb(0xF2, 0xF5, 0xF9),
                ink_muted: Color32::from_rgb(0x82, 0x8C, 0x9C),
                action: Color32::from_rgb(0x22, 0xC5, 0x5E),
                action_ink: Color32::from_rgb(0x04, 0x13, 0x09),
            },
        }
    }
}

/// Official EU Energy Label badge background color for class A..G
pub fn class_bg_color(cls: &str) -> Color32 {
    match cls {
        "A" => Color32::from_rgb(0x00, 0xA6, 0x51),
        "B" => Color32::from_rgb(0x55, 0xD9, 0x34),
        "C" => Color32::from_rgb(0xBF, 0xD7, 0x30),
        "D" => Color32::from_rgb(0xFF, 0xF2, 0x00),
        "E" => Color32::from_rgb(0xFD, 0xB9, 0x13),
        "F" => Color32::from_rgb(0xF3, 0x70, 0x21),
        _ => Color32::from_rgb(0xED, 0x1C, 0x24), // G
    }
}

/// Lifted number tint for readable figures on dark background
pub fn class_number_color(cls: &str) -> Color32 {
    match cls {
        "A" => Color32::from_rgb(0x26, 0xD0, 0x7C),
        "B" => Color32::from_rgb(0x55, 0xD9, 0x34),
        "C" => Color32::from_rgb(0xBF, 0xD7, 0x30),
        "D" => Color32::from_rgb(0xFF, 0xF2, 0x00),
        "E" => Color32::from_rgb(0xFD, 0xB9, 0x13),
        "F" => Color32::from_rgb(0xFF, 0x8A, 0x4A),
        _ => Color32::from_rgb(0xFF, 0x4D, 0x4D), // G
    }
}

pub fn load_color(watts: f64) -> Color32 {
    class_bg_color(class_of(watts))
}

pub fn number_color(watts: f64) -> Color32 {
    class_number_color(class_of(watts))
}

/// CPU/RAM percentage color in 15% steps (A..G)
pub fn pct_color(pct: f64) -> Color32 {
    let idx = ((pct / 15.0).floor() as usize).min(6);
    let cls = ["A", "B", "C", "D", "E", "F", "G"][idx];
    class_number_color(cls)
}

/// Draws the circular Efficiency Ring Badge (from Concept 3) for the left side of the taskbar strip.
/// Uses the live EU Energy Label color for both the glowing ring border, soft translucent fill, and letter.
pub fn draw_class_ring_badge(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    cls: &str,
    font_size: f32,
) {
    let col = class_number_color(cls);
    // Soft ambient outer halo
    painter.circle_filled(center, radius + 1.5, col.gamma_multiply(0.10));
    // Translucent tinted inner disc
    painter.circle_filled(center, radius, col.gamma_multiply(0.16));
    // Crisp 2px ring border in the live class color
    painter.circle_stroke(center, radius, Stroke::new(1.8_f32, col));
    // Centered bold class letter (A..G)
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        cls,
        egui::FontId::proportional(font_size),
        col,
    );
}

/// Draws the signature EU Energy Label pointed arrow (pentagon pointing right)
pub fn draw_class_arrow(
    painter: &egui::Painter,
    rect: Rect,
    tip_width: f32,
    fill: Color32,
    text: &str,
    text_color: Color32,
    font_size: f32,
) {
    let left = rect.left();
    let right = rect.right();
    let top = rect.top();
    let bottom = rect.bottom();
    let mid_y = rect.center().y;
    let body_right = (right - tip_width).max(left + 2.0);

    let pts = vec![
        Pos2::new(left, top),
        Pos2::new(body_right, top),
        Pos2::new(right, mid_y),
        Pos2::new(body_right, bottom),
        Pos2::new(left, bottom),
    ];

    painter.add(Shape::convex_polygon(pts, fill, Stroke::NONE));

    let text_center = Pos2::new((left + body_right) * 0.5 + 1.0, mid_y);
    painter.text(
        text_center,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(font_size),
        text_color,
    );
}

/// Draws a vertical bar with a smooth top-to-bottom alpha fade (Obsidian Linear chart style)
pub fn draw_gradient_bar(painter: &egui::Painter, rect: Rect, top_col: Color32) {
    let bot_col = top_col.gamma_multiply(0.18);
    let mut mesh = egui::epaint::Mesh::default();
    let i0 = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), top_col);
    mesh.colored_vertex(rect.right_top(), top_col);
    mesh.colored_vertex(rect.right_bottom(), bot_col);
    mesh.colored_vertex(rect.left_bottom(), bot_col);
    mesh.add_triangle(i0, i0 + 1, i0 + 2);
    mesh.add_triangle(i0, i0 + 2, i0 + 3);
    painter.add(Shape::Mesh(mesh));
}

/// Draws a glowing status LED dot (emerald for local, sky-blue for LAN)
pub fn draw_status_led(painter: &egui::Painter, center: Pos2, is_lan: bool) {
    let col = if is_lan {
        Color32::from_rgb(0x38, 0xBD, 0xF8)
    } else {
        Color32::from_rgb(0x26, 0xD0, 0x7C)
    };
    painter.circle_filled(center, 6.0, col.gamma_multiply(0.20));
    painter.circle_filled(center, 3.2, col);
}

/// Draws a crisp diagonal north-east arrow (↗) inside a circular Button-in-Button wrapper
pub fn draw_diag_arrow_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5_f32, color);
    let p_from = Pos2::new(center.x - 2.6, center.y + 2.6);
    let p_to = Pos2::new(center.x + 2.6, center.y - 2.6);
    painter.line_segment([p_from, p_to], stroke);
    painter.line_segment(
        [Pos2::new(p_to.x - 3.6, p_to.y), p_to],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(p_to.x, p_to.y + 3.6), p_to],
        stroke,
    );
}

/// Crisp vector chevron icon (up or down)
pub fn draw_chevron_icon(painter: &egui::Painter, center: Pos2, up: bool, color: Color32) {
    let w = 4.2;
    let h = 2.4;
    let stroke = Stroke::new(1.6_f32, color);
    if up {
        let p1 = Pos2::new(center.x - w, center.y + h);
        let p2 = Pos2::new(center.x, center.y - h);
        let p3 = Pos2::new(center.x + w, center.y + h);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
    } else {
        let p1 = Pos2::new(center.x - w, center.y - h);
        let p2 = Pos2::new(center.x, center.y + h);
        let p3 = Pos2::new(center.x + w, center.y - h);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], stroke);
    }
}

/// Crisp vector close 'X' icon
pub fn draw_close_icon(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.5;
    let stroke = Stroke::new(1.5_f32, color);
    painter.line_segment(
        [Pos2::new(center.x - r, center.y - r), Pos2::new(center.x + r, center.y + r)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(center.x + r, center.y - r), Pos2::new(center.x - r, center.y + r)],
        stroke,
    );
}

/// Crisp vector pushpin icon
pub fn draw_pin_icon(painter: &egui::Painter, center: Pos2, active: bool, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    // Pin head
    painter.circle_stroke(Pos2::new(center.x, center.y - 2.2), 2.8, stroke);
    if active {
        painter.circle_filled(Pos2::new(center.x, center.y - 2.2), 1.4, color);
    }
    // Pin needle
    painter.line_segment(
        [Pos2::new(center.x, center.y + 0.6), Pos2::new(center.x, center.y + 5.0)],
        stroke,
    );
}

/// Crisp vector gear icon for Settings tab
pub fn draw_gear_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5_f32, color);
    painter.circle_stroke(center, 3.6, stroke);
    for i in 0..6 {
        let angle = (i as f32) * std::f32::consts::TAU / 6.0;
        let p1 = Pos2::new(center.x + angle.cos() * 3.8, center.y + angle.sin() * 3.8);
        let p2 = Pos2::new(center.x + angle.cos() * 5.6, center.y + angle.sin() * 5.6);
        painter.line_segment([p1, p2], stroke);
    }
}

/// Crisp vector moon icon for "Gute Nacht"
pub fn draw_moon_icon(painter: &egui::Painter, center: Pos2, color: Color32, bg: Color32) {
    painter.circle_filled(center, 4.6, color);
    painter.circle_filled(Pos2::new(center.x + 2.1, center.y - 1.5), 3.8, bg);
}

/// Crisp vector send arrow icon (right-pointing paper plane / arrow)
pub fn draw_send_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.6_f32, color);
    painter.line_segment(
        [Pos2::new(center.x - 4.5, center.y), Pos2::new(center.x + 4.0, center.y)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(center.x + 1.0, center.y - 3.2), Pos2::new(center.x + 4.2, center.y)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(center.x + 1.0, center.y + 3.2), Pos2::new(center.x + 4.2, center.y)],
        stroke,
    );
}

/// Crisp vector vertical arrow (up or down) for network speeds
pub fn draw_net_arrow(painter: &egui::Painter, center: Pos2, down: bool, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    painter.line_segment(
        [Pos2::new(center.x, center.y - 4.0), Pos2::new(center.x, center.y + 4.0)],
        stroke,
    );
    if down {
        painter.line_segment(
            [Pos2::new(center.x - 2.6, center.y + 1.4), Pos2::new(center.x, center.y + 4.0)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(center.x + 2.6, center.y + 1.4), Pos2::new(center.x, center.y + 4.0)],
            stroke,
        );
    } else {
        painter.line_segment(
            [Pos2::new(center.x - 2.6, center.y - 1.4), Pos2::new(center.x, center.y - 4.0)],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(center.x + 2.6, center.y - 1.4), Pos2::new(center.x, center.y - 4.0)],
            stroke,
        );
    }
}

/// Double-Bezel Card wrapper (Outer Shell + Inner Core with concentric radii)
pub fn double_bezel_card<R>(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    outer_rounding: f32,
    inner_padding: Vec2,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::none()
        .fill(pal.outer_shell)
        .stroke(Stroke::new(1.0_f32, pal.rule))
        .rounding(Rounding::same(outer_rounding))
        .inner_margin(egui::Margin::same(3.0))
        .show(ui, |ui| {
            egui::Frame::none()
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule_strong.gamma_multiply(0.55)))
                .rounding(Rounding::same((outer_rounding - 3.0).max(5.0)))
                .inner_margin(egui::Margin {
                    left: inner_padding.x,
                    right: inner_padding.x,
                    top: inner_padding.y,
                    bottom: inner_padding.y,
                })
                .show(ui, add_contents)
                .inner
        })
        .inner
}

/// Display name of a theme in the current language.
pub fn theme_label(id: &str) -> &'static str {
    use crate::i18n::t;
    match id {
        "label" => t("Energielabel (EU A-G)", "Energy label (EU A-G)"),
        "graphite" => t("Graphit", "Graphite"),
        "mica" => "Windows 11 Mica",
        "pump" => t("Zapfs\u{00e4}ule", "Gas pump"),
        "taxi" => t("Taxameter", "Taximeter"),
        "ferraris" => t("Ferraris-Z\u{00e4}hler", "Ferraris meter"),
        "datamatics" => "Datamatics",
        "nord" => "Nord",
        "paper" => t("Papier", "Paper"),
        "receipt" => t("Kassenbon", "Receipt"),
        _ => "Theme",
    }
}
