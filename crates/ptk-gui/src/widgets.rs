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

/// The app mark: a rounded blue square with "PT".
pub fn logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same((size * 0.28) as u8), ACCENT);
    let bar = Rect::from_min_max(
        egui::pos2(rect.min.x + size * 0.24, rect.max.y - size * 0.2),
        egui::pos2(rect.max.x - size * 0.24, rect.max.y - size * 0.13),
    );
    p.rect_filled(bar, CornerRadius::same(2), TEAL);
    p.text(
        rect.center() - vec2(0.0, size * 0.06),
        Align2::CENTER_CENTER,
        "PT",
        FontId::proportional(size * 0.44),
        Color32::WHITE,
    );
}

/// A segmented control; returns true when the value changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &str, Color32)],
) -> bool {
    let mut changed = false;
    let seg_w = 96.0;
    let (rect, _) =
        ui.allocate_exact_size(vec2(seg_w * options.len() as f32, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), INPUT);
    for (i, (v, label, color)) in options.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(seg_w * i as f32, 0.0), vec2(seg_w, 28.0))
            .shrink(2.0);
        let response = ui.interact(
            r,
            ui.id()
                .with(("seg", i, rect.min.x as i32, rect.min.y as i32)),
            Sense::click(),
        );
        let selected = *value == *v;
        if selected {
            ui.painter()
                .rect_filled(r, CornerRadius::same(6), color.gamma_multiply(0.22));
            ui.painter().rect_stroke(
                r,
                CornerRadius::same(6),
                Stroke::new(1.0_f32, color.gamma_multiply(0.7)),
                StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(6), HOVER);
        }
        centered_text(
            ui,
            r,
            label,
            13.0,
            if selected { *color } else { TEXT_WEAK },
        );
        if response.clicked() && !selected {
            *value = *v;
            changed = true;
        }
    }
    changed
}

/// Color of an asset category in charts.
pub fn category_color(id: &str) -> Color32 {
    match id {
        "textures" => FOCUS,
        "models" => PURPLE,
        "audio" => TEAL,
        "video" => ORANGE,
        "shaders" => YELLOW,
        "scripts" => GREEN,
        "data" => LIGHT_BLUE,
        "fonts" => rgb(214, 157, 133),
        "localization" => rgb(184, 215, 163),
        "archives" => BLUE,
        "executables" => ERROR,
        _ => TEXT_FAINT,
    }
}

/// One horizontal bar split into colored segments (`share` sums to ≤ 1).
pub fn stacked_bar(ui: &mut Ui, parts: &[(f64, Color32)]) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), INPUT);
    let mut x = rect.min.x;
    for (share, color) in parts {
        let w =
            (rect.width() * share.clamp(0.0, 1.0) as f32).max(if *share > 0.0 { 3.0 } else { 0.0 });
        let r = Rect::from_min_size(
            egui::pos2(x, rect.min.y),
            vec2(w.min(rect.max.x - x), rect.height()),
        );
        ui.painter().rect_filled(r, CornerRadius::ZERO, *color);
        x += w;
    }
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(2.0_f32, CARD),
        StrokeKind::Outside,
    );
}

/// Page title and subtitle.
pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title)
            .size(26.0)
            .strong()
            .color(Color32::WHITE),
    );
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(14.0).color(TEXT_WEAK));
    }
    ui.add_space(14.0);
}

/// Number of columns that fit, each at least `min` wide.
pub fn columns_for(ui: &Ui, min: f32, max: usize) -> usize {
    ((ui.available_width() / min).floor() as usize).clamp(1, max)
}

/// A panel with a border, for grouping content in the editor area.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Plain top-down layout: columns are justified, cards are not.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add)
                .inner
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
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 9))
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

/// A top-bar tab: highlighted text with a blue underline when selected.
pub fn tab(ui: &mut Ui, icon: &str, label: &str, selected: bool, labels: bool) -> egui::Response {
    let text = if labels {
        format!("{icon}  {label}")
    } else {
        icon.to_owned()
    };
    let color = if selected { FOCUS } else { TEXT_WEAK };
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(14.5), color);
    let size = vec2(galley.size().x + 22.0, 54.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let color = if response.hovered() && !selected {
        TEXT
    } else {
        color
    };
    if response.hovered() && !selected {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(2.0, 11.0)), CornerRadius::same(6), HOVER);
    }
    ui.painter().galley_with_override_text_color(
        rect.center() - galley.size() / 2.0,
        galley,
        color,
    );
    if selected {
        let bar = Rect::from_min_max(
            egui::pos2(rect.min.x + 8.0, rect.max.y - 3.0),
            egui::pos2(rect.max.x - 8.0, rect.max.y),
        );
        ui.painter().rect_filled(bar, CornerRadius::same(2), FOCUS);
    }
    response
}

/// A full-width button of an action column: icon, label, left aligned.
pub fn action_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let response = if enabled {
        response
    } else {
        ui.interact(rect, response.id, Sense::hover())
    };
    let fill = if !enabled {
        CARD.gamma_multiply(0.6)
    } else if response.hovered() {
        HOVER
    } else {
        SURFACE
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    let color = if enabled { TEXT } else { TEXT_FAINT };
    ui.painter().text(
        rect.left_center() + vec2(14.0, 0.0),
        Align2::LEFT_CENTER,
        icon,
        FontId::proportional(14.0),
        if enabled { LIGHT_BLUE } else { TEXT_FAINT },
    );
    ui.painter().text(
        rect.left_center() + vec2(40.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        color,
    );
    response
}

/// Color and letter of an engine's project icon.
/// Paints a texture into `rect` (cropped to fill it), with rounded corners.
pub fn image_in(ui: &Ui, rect: Rect, tex: &egui::TextureHandle, radius: u8) {
    let [w, h] = tex.size().map(|v| v.max(1) as f32);
    let (tw, th) = (rect.width(), rect.height());
    let scale = (tw / w).max(th / h);
    let (cw, ch) = (tw / (w * scale), th / (h * scale));
    let uv = Rect::from_center_size(egui::pos2(0.5, 0.5), egui::vec2(cw, ch));
    egui::Image::new((tex.id(), rect.size()))
        .uv(uv)
        .corner_radius(radius)
        .paint_at(ui, rect);
}

pub fn engine_badge(engine: Option<&str>) -> (Color32, &'static str) {
    match engine {
        Some("godot") => (rgb(71, 140, 191), "G"),
        Some("unity") => (rgb(110, 118, 135), "U"),
        Some("unreal") => (PURPLE, "UE"),
        Some("rage") => (ORANGE, "R"),
        Some("source") => (rgb(196, 154, 60), "S"),
        Some("gamemaker") => (GREEN, "GM"),
        Some("renpy") => (rgb(230, 120, 120), "R'"),
        Some("rpgmaker") => (TEAL, "RM"),
        Some("macos") => (LIGHT_BLUE, ""),
        _ => (SURFACE, "?"),
    }
}

/// A rounded square with the engine's letter.
pub fn project_icon(ui: &Ui, rect: Rect, engine: Option<&str>, dim: bool) {
    let (color, letter) = engine_badge(engine);
    let color = if dim {
        color.gamma_multiply(0.35)
    } else {
        color
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), color);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional(rect.height() * 0.42),
        Color32::WHITE.gamma_multiply(if dim { 0.5 } else { 1.0 }),
    );
}

/// A path with the home folder shown as `~`.
pub fn display_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// Shortens text to `max` characters by cutting out the middle.
pub fn elide_middle(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max || max < 5 {
        return text.to_owned();
    }
    let keep = max - 1;
    let head = keep / 3;
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// `just now`, `5 min ago`, `3 h ago`, `2 days ago`, or a date.
pub fn relative_time(t: std::time::SystemTime) -> String {
    let Ok(age) = std::time::SystemTime::now().duration_since(t) else {
        return "just now".into();
    };
    let s = age.as_secs();
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} h ago", s / 3600),
        86400..=2_591_999 => format!("{} days ago", s / 86400),
        _ => {
            // Civil date from days since the epoch (UTC).
            let days = t
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() / 86400)
                .unwrap_or(0) as i64;
            let z = days + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
            format!("{y:04}-{m:02}-{d:02}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(elide_middle("abcdefghij", 20), "abcdefghij");
        let e = elide_middle("/Users/someone/projects/very/long/path/my-port", 20);
        assert_eq!(e.chars().count(), 20);
        assert!(e.ends_with("my-port") && e.contains('…'));
    }

    #[test]
    fn relative_times() {
        use std::time::{Duration, SystemTime, UNIX_EPOCH};
        assert_eq!(relative_time(SystemTime::now()), "just now");
        assert_eq!(
            relative_time(SystemTime::now() - Duration::from_secs(7200)),
            "2 h ago"
        );
        assert_eq!(
            relative_time(UNIX_EPOCH + Duration::from_secs(1_762_646_400)),
            "2025-11-09"
        );
    }
}
