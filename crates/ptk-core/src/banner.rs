//! The PORTING TOOLKIT wordmark, shared by the CLI banner and the GUI.
//!
//! An original FIGlet-style block font drawn for this project (generator:
//! `tools/gen_banner.py`): `█` cells are letters, `░` cells their shadow.

/// The art, 12 lines, at most 72 columns.
pub const ART: &str = r"██████      ████    ██████    ██████████  ██████  ██      ██    ██████
██░░░░██  ██ ░░░██  ██░░░░██   ░░░██░░░░░  ░██░░░ ████    ██░ ██ ░░░░░░
██████ ░░ ██░   ██░ ██████ ░░     ██░       ██░   ██░░██  ██░ ██░ ████
██░░░░░   ██░   ██░ ██░░██░       ██░       ██░   ██░  ░████░ ██░  ░██░
██░        ░████ ░░ ██░  ░██      ██░     ██████  ██░    ░██░  ░██████░
 ░░          ░░░░    ░░    ░░      ░░      ░░░░░░  ░░      ░░    ░░░░░░
██████████    ████      ████    ██        ██    ██  ██████  ██████████
 ░░░██░░░░░ ██ ░░░██  ██ ░░░██  ██░       ██░ ██ ░░  ░██░░░  ░░░██░░░░░
    ██░     ██░   ██░ ██░   ██░ ██░       ████ ░░     ██░       ██░
    ██░     ██░   ██░ ██░   ██░ ██░       ██░░██      ██░       ██░
    ██░      ░████ ░░  ░████ ░░ ████████  ██░  ░██  ██████      ██░
     ░░        ░░░░      ░░░░    ░░░░░░░░  ░░    ░░  ░░░░░░      ░░
";

/// Letter color per art line, as xterm 256-color indices: a warm gradient
/// for PORTING, a cool one for TOOLKIT (0 = shadow-only line).
pub const LINE_COLORS: [u8; 12] = [202, 208, 214, 220, 226, 0, 51, 45, 39, 33, 27, 0];

/// Shadow color (xterm 256-color index).
pub const SHADOW_COLOR: u8 = 240;

/// RGB of an xterm 256-color index.
pub fn ansi256_rgb(n: u8) -> [u8; 3] {
    const BASE: [[u8; 3]; 16] = [
        [0, 0, 0],
        [205, 0, 0],
        [0, 205, 0],
        [205, 205, 0],
        [0, 0, 238],
        [205, 0, 205],
        [0, 205, 205],
        [229, 229, 229],
        [127, 127, 127],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [92, 92, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
    ];
    match n {
        0..=15 => BASE[n as usize],
        16..=231 => {
            let n = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            [level(n / 36), level((n / 6) % 6), level(n % 6)]
        }
        _ => {
            let v = 8 + (n - 232) * 10;
            [v, v, v]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette() {
        assert_eq!(ansi256_rgb(208), [255, 135, 0]);
        assert_eq!(ansi256_rgb(51), [0, 255, 255]);
        assert_eq!(ansi256_rgb(240), [88, 88, 88]);
        assert_eq!(ART.lines().count(), LINE_COLORS.len());
    }
}
