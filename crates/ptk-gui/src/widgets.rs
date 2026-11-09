//! Small drawing helpers shared by the views.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, vec2,
};
use ptk_core::banner::ART;

use crate::theme::*;

/// Paints the PORTING TOOLKIT wordmark from the shared banner art, one
/// rectangle per cell, in editor blues (PORTING) and teals (TOOLKIT).
pub fn wordmark(ui: &mut Ui, cell: f32) -> egui::Response {
    let lines: Vec<Vec<char>> = ART.lines().map(|l| l.chars().collect()).collect();
    let cols = lines.iter().map(Vec::len).max().unwrap_or(0);
    let size = vec2(cols as f32 * cell, lines.len() as f32 * cell * 2.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let warm = [
        rgb(55, 148, 255),
        rgb(79, 162, 255),
        rgb(104, 177, 255),
        rgb(129, 192, 255),
        rgb(156, 208, 255),
    ];
    let cool = [
        rgb(78, 201, 176),
        rgb(70, 190, 190),
        rgb(62, 178, 204),
        rgb(55, 165, 218),
        rgb(48, 152, 230),
    ];
    let shadow = rgb(30, 52, 86);
    for (row, line) in lines.iter().enumerate() {
        let color = match row {
            0..=4 => warm[row],
            6..=10 => cool[row - 6],
            _ => shadow,
        };
        for (col, ch) in line.iter().enumerate() {
            let fill = match ch {
                '█' => color,
                '░' => shadow,
                _ => continue,
            };
            let min = rect.min + vec2(col as f32 * cell, row as f32 * cell * 2.0);
            painter.rect_filled(
                Rect::from_min_size(min, vec2(cell + 0.3, cell * 2.0 + 0.3)),
                0.0,
                fill,
            );
        }
    }
    response
}

/// An upper-case side bar section title.
pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(title.to_uppercase())
            .size(11.0)
            .color(TEXT_WEAK)
            .strong(),
    );
    ui.add_space(2.0);
}

/// A panel with a border, for grouping content in the editor area.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Title of a card.
pub fn card_title(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .size(15.0)
            .strong()
            .color(Color32::WHITE),
    );
    ui.add_space(4.0);
}

/// Weak explanatory text.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(TEXT_WEAK).size(12.5));
}

/// A rounded badge such as `active` or `high`.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(11.0), color);
    let size = galley.size() + vec2(12.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), color.gamma_multiply(0.18));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, color.gamma_multiply(0.6)),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
}

/// A horizontal share bar, `share` in 0..=1.
pub fn share_bar(ui: &mut Ui, share: f64, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), INPUT);
    let w = (rect.width() * share.clamp(0.0, 1.0) as f32).max(if share > 0.0 { 2.0 } else { 0.0 });
    let filled = Rect::from_min_size(rect.min, vec2(w, rect.height()));
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), FOCUS);
}

/// A primary (filled blue) button.
pub fn primary(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(ACCENT),
    )
}

/// A text field for a path with a Browse button that opens a native picker.
/// Returns true when the path changed.
pub fn path_field(ui: &mut Ui, value: &mut String, hint_text: &str, pick: Pick) -> bool {
    let mut changed = false;
    // Button first (right to left), so the field takes exactly what is left
    // and never pushes a side panel wider frame after frame.
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if ui.button("Browse…").clicked()
                && let Some(p) = pick.run()
            {
                *value = p.display().to_string();
                changed = true;
            }
            changed |= ui
                .add(
                    egui::TextEdit::singleline(value)
                        .hint_text(hint_text)
                        .desired_width(ui.available_width()),
                )
                .changed();
        },
    );
    changed
}

/// Which native dialog [`path_field`] opens.
#[derive(Clone, Copy)]
pub enum Pick {
    Folder,
    PackFile,
    SavePack,
}

impl Pick {
    fn run(self) -> Option<std::path::PathBuf> {
        match self {
            Pick::Folder => rfd::FileDialog::new().pick_folder(),
            Pick::PackFile => rfd::FileDialog::new()
                .add_filter("Porting Toolkit project", &["ptk"])
                .pick_file(),
            Pick::SavePack => rfd::FileDialog::new()
                .add_filter("Porting Toolkit project", &["ptk"])
                .save_file(),
        }
    }
}

/// A status icon: ✔ ▲ ✖ in green, yellow, red.
pub fn status_icon(ui: &mut Ui, status: ptk_core::doctor::Status) {
    use ptk_core::doctor::Status;
    let (text, color) = match status {
        Status::Ok => ("✔", GREEN),
        Status::Warn => ("▲", WARN),
        Status::Fail => ("✖", ERROR),
    };
    ui.label(RichText::new(text).color(color).strong());
}

/// A message box tinted by severity.
pub fn message(ui: &mut Ui, color: Color32, text: &str) {
    Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(TEXT));
        });
}

/// A key/value row in a two-column grid.
pub fn kv(ui: &mut Ui, key: &str, value: impl Into<egui::WidgetText>) {
    ui.label(RichText::new(key).color(TEXT_WEAK));
    ui.label(value);
    ui.end_row();
}

/// Paints text centred in a rect (for icons).
pub fn centered_text(ui: &Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        color,
    );
}

/// `1.5 GB`, `320 KB` (powers of 1000, like Finder).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
