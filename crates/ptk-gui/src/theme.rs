//! The GUI theme: deep navy surfaces with code-editor accent colors
//! (blue #007ACC / #3794FF, teal, soft yellow, orange, green, red).
//! Layout is the app's own: a header with the project switcher and
//! navigation pills, and centred pages of rounded cards.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals,
};

pub const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

// Surfaces, darkest to lightest.
pub const HEADER: Color32 = rgb(10, 19, 35);
pub const PAGE: Color32 = rgb(14, 26, 46);
pub const INPUT: Color32 = rgb(11, 21, 38);
pub const CARD: Color32 = rgb(21, 38, 64);
pub const SURFACE: Color32 = rgb(28, 49, 80);
pub const HOVER: Color32 = rgb(34, 58, 94);
pub const BORDER: Color32 = rgb(36, 58, 92);
pub const SELECTION: Color32 = rgb(38, 79, 120);

// Text.
pub const TEXT: Color32 = rgb(212, 219, 230);
pub const TEXT_WEAK: Color32 = rgb(139, 155, 180);
pub const TEXT_FAINT: Color32 = rgb(92, 110, 138);

// Accents.
pub const ACCENT: Color32 = rgb(0, 122, 204);
pub const FOCUS: Color32 = rgb(55, 148, 255);
pub const BLUE: Color32 = rgb(86, 156, 214);
pub const LIGHT_BLUE: Color32 = rgb(156, 220, 254);
pub const TEAL: Color32 = rgb(78, 201, 176);
pub const YELLOW: Color32 = rgb(220, 220, 170);
pub const ORANGE: Color32 = rgb(206, 145, 120);
pub const GREEN: Color32 = rgb(137, 209, 133);
pub const WARN: Color32 = rgb(204, 167, 0);
pub const ERROR: Color32 = rgb(244, 135, 113);
pub const PURPLE: Color32 = rgb(197, 134, 192);

/// Loads the system UI font on macOS when present (never bundled), falling
/// back to egui's built-in fonts. `PTK_GUI_FONTS=builtin` skips it.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    if std::env::var("PTK_GUI_FONTS").as_deref() == Ok("builtin") {
        return fonts;
    }
    let candidates: [(&str, &str, FontFamily); 2] = [
        (
            "system-ui",
            "/System/Library/Fonts/SFNS.ttf",
            FontFamily::Proportional,
        ),
        (
            "system-mono",
            "/System/Library/Fonts/SFNSMono.ttf",
            FontFamily::Monospace,
        ),
    ];
    for (name, path, family) in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(apply);
}

pub fn apply(style: &mut egui::Style) {
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(22.)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.)),
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8., 6.);
    style.spacing.button_padding = egui::vec2(14., 5.);
    style.spacing.interact_size.y = 28.;
    style.spacing.combo_width = 140.;
    style.spacing.text_edit_width = 360.;

    let mut v = Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PAGE;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 8.into();
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(170),
    };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = rgb(20, 37, 63);
    v.code_bg_color = INPUT;
    v.hyperlink_color = FOCUS;
    v.warn_fg_color = WARN;
    v.error_fg_color = ERROR;
    v.selection.bg_fill = SELECTION;
    v.selection.stroke = Stroke::new(1.0_f32, LIGHT_BLUE);
    v.text_cursor.stroke = Stroke::new(2.0_f32, FOCUS);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, CARD, BORDER),
        (&mut v.widgets.inactive, SURFACE, rgb(44, 70, 108)),
        (&mut v.widgets.hovered, HOVER, FOCUS),
        (&mut v.widgets.active, ACCENT, FOCUS),
        (&mut v.widgets.open, HOVER, FOCUS),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, stroke);
        w.corner_radius = 6.into();
        w.expansion = 0.;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    style.visuals = v;
}
