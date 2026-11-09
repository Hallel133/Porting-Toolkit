//! Terminal colors and small formatting helpers.
//!
//! Color is on only when stdout is a terminal, `NO_COLOR` is unset (or
//! empty), `TERM` is not `dumb` and `--no-color` was not given.
//! `FORCE_COLOR` / `CLICOLOR_FORCE` turn it on for a pipe (screenshots),
//! but `NO_COLOR` and `--no-color` always win.

use std::ffi::OsString;
use std::fmt::Display;
use std::io::IsTerminal;

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub color: bool,
}

fn set(v: &Option<OsString>) -> bool {
    v.as_ref().is_some_and(|v| !v.is_empty() && v != "0")
}

/// The color decision, as a pure function for tests.
pub fn decide(
    no_color_flag: bool,
    no_color_env: Option<OsString>,
    force_env: Option<OsString>,
    term: Option<OsString>,
    is_tty: bool,
) -> bool {
    if no_color_flag || no_color_env.as_ref().is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if set(&force_env) {
        return true;
    }
    is_tty && term.as_deref() != Some("dumb".as_ref())
}

impl Style {
    pub fn detect(no_color_flag: bool) -> Self {
        let force = std::env::var_os("FORCE_COLOR").or_else(|| std::env::var_os("CLICOLOR_FORCE"));
        Style {
            color: decide(
                no_color_flag,
                std::env::var_os("NO_COLOR"),
                force,
                std::env::var_os("TERM"),
                std::io::stdout().is_terminal(),
            ),
        }
    }

    pub fn paint(&self, text: impl Display, code: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, t: impl Display) -> String {
        self.paint(t, "1")
    }
    pub fn dim(&self, t: impl Display) -> String {
        self.paint(t, "2")
    }
    pub fn red(&self, t: impl Display) -> String {
        self.paint(t, "1;31")
    }
    pub fn green(&self, t: impl Display) -> String {
        self.paint(t, "32")
    }
    pub fn yellow(&self, t: impl Display) -> String {
        self.paint(t, "33")
    }
    pub fn cyan(&self, t: impl Display) -> String {
        self.paint(t, "36")
    }
    /// The banner's orange, for headings.
    pub fn accent(&self, t: impl Display) -> String {
        self.paint(t, "1;38;5;208")
    }

    pub fn ok(&self) -> String {
        self.green("✔")
    }
    pub fn warn(&self) -> String {
        self.yellow("▲")
    }
    pub fn fail(&self) -> String {
        self.red("✖")
    }

    /// A section heading with a rule under it.
    pub fn heading(&self, title: &str) -> String {
        format!(
            "\n{}\n{}",
            self.accent(title),
            self.dim("─".repeat(title.chars().count().max(40)))
        )
    }
}

/// Visible width of a string that may contain ANSI escapes.
pub fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

/// Removes ANSI escapes.
#[cfg(test)]
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `1.5 GB`, `320 KB`, `12 B` (powers of 1000, like Finder).
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

/// A simple left/right aligned text table.
pub struct Table {
    headers: Vec<String>,
    right: Vec<bool>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// Headers starting with `>` are right-aligned.
    pub fn new(headers: &[&str]) -> Self {
        Table {
            headers: headers
                .iter()
                .map(|h| h.trim_start_matches('>').to_owned())
                .collect(),
            right: headers.iter().map(|h| h.starts_with('>')).collect(),
            rows: Vec::new(),
        }
    }

    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    pub fn render(&self, style: &Style) -> String {
        let cols = self.headers.len();
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.chars().count()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate().take(cols) {
                widths[i] = widths[i].max(visible_width(cell));
            }
        }
        let line = |cells: &[String], header: bool| {
            let mut out = String::from("  ");
            for (i, cell) in cells.iter().enumerate().take(cols) {
                let pad = widths[i].saturating_sub(visible_width(cell));
                let text = if header {
                    style.dim(cell)
                } else {
                    cell.clone()
                };
                let last = i + 1 == cols;
                if self.right[i] {
                    out.push_str(&" ".repeat(pad));
                    out.push_str(&text);
                } else {
                    out.push_str(&text);
                    if !last {
                        out.push_str(&" ".repeat(pad));
                    }
                }
                if !last {
                    out.push_str("  ");
                }
            }
            out.trim_end().to_owned()
        };
        let mut out = vec![line(&self.headers, true)];
        out.extend(self.rows.iter().map(|r| line(r, false)));
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_decision() {
        let s = |v: &str| Some(OsString::from(v));
        assert!(decide(false, None, None, s("xterm"), true));
        assert!(
            !decide(false, None, None, s("xterm"), false),
            "pipes get no color"
        );
        assert!(!decide(false, s("1"), None, s("xterm"), true), "NO_COLOR");
        assert!(
            decide(false, s(""), None, s("xterm"), true),
            "empty NO_COLOR is unset"
        );
        assert!(!decide(true, None, None, s("xterm"), true), "--no-color");
        assert!(!decide(false, None, None, s("dumb"), true));
        assert!(decide(false, None, s("1"), None, false), "FORCE_COLOR");
        assert!(
            !decide(false, s("1"), s("1"), None, true),
            "NO_COLOR beats FORCE_COLOR"
        );
    }

    #[test]
    fn sizes_and_widths() {
        assert_eq!(human_size(12), "12 B");
        assert_eq!(human_size(1_500), "1.5 KB");
        assert_eq!(human_size(250_000_000), "250 MB");
        assert_eq!(visible_width("\x1b[1;31mab\x1b[0m"), 2);
        assert_eq!(strip_ansi("\x1b[32m✔\x1b[0m ok"), "✔ ok");
    }

    #[test]
    fn table() {
        let mut t = Table::new(&["NAME", ">SIZE"]);
        t.row(vec!["a".into(), "1 B".into()]);
        t.row(vec!["longer".into(), "10 KB".into()]);
        let out = t.render(&Style { color: false });
        assert_eq!(out, "  NAME     SIZE\n  a         1 B\n  longer  10 KB");
    }
}
