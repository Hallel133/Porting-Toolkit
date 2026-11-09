//! The PORTING TOOLKIT banner.
//!
//! The art lives in `ptk_core::banner` (shared with the GUI); this module
//! colors it with 256-color ANSI codes.

use crate::style::Style;

pub use ptk_core::banner::ART;
use ptk_core::banner::{LINE_COLORS, SHADOW_COLOR};

/// The art, colored when `color` is on.
pub fn render_art(color: bool) -> String {
    if !color {
        return ART.to_owned();
    }
    let mut out = String::new();
    for (i, line) in ART.lines().enumerate() {
        let mut current: Option<u8> = None;
        for ch in line.chars() {
            let want = match ch {
                '█' => Some(LINE_COLORS[i]),
                '░' => Some(SHADOW_COLOR),
                _ => None,
            };
            if want != current {
                match want {
                    Some(c) => out.push_str(&format!("\x1b[38;5;{c}m")),
                    None => out.push_str("\x1b[0m"),
                }
                current = want;
            }
            out.push(ch);
        }
        if current.is_some() {
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

/// One line under the art.
pub fn tagline(style: &Style) -> String {
    format!(
        "{}  {}\n{}\n",
        style.accent(format!("Porting Toolkit {}", ptk_core::VERSION)),
        style.dim("manage, convert, export and import game projects between platforms"),
        style.dim("Bring your own game: ptk works on files you own and never ships them."),
    )
}

/// Art plus tagline.
pub fn banner(style: &Style) -> String {
    format!("{}\n{}", render_art(style.color), tagline(style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::strip_ansi;

    const SNAPSHOT: &str = include_str!("../tests/snapshots/banner.txt");

    #[test]
    fn plain_banner_matches_snapshot() {
        assert_eq!(render_art(false), SNAPSHOT);
        assert!(ART.lines().all(|l| l.chars().count() <= 72));
        assert_eq!(ART.lines().count(), 12);
    }

    #[test]
    fn colored_banner_is_the_same_text() {
        let colored = render_art(true);
        assert!(colored.contains("\x1b[38;5;208m"));
        assert_eq!(strip_ansi(&colored), SNAPSHOT);
        let style = Style { color: false };
        assert!(!banner(&style).contains('\x1b'));
    }
}
