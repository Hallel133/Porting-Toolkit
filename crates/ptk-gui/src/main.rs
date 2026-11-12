//! Porting Toolkit desktop GUI, built with egui on `ptk-core`.
//!
//! ```text
//! ptk-gui [--view NAME] [--project DIR] [--game DIR] [--run]
//!         [--screenshot FILE.png] [--scroll-bottom] [--smoke] [--size WxH]
//! ```
//!
//! `--run` starts the view's job right away (scan, doctor, plugin list,
//! verify); `--screenshot` saves the window once that work is done and
//! closes; `--smoke` opens the window for three seconds.

mod app;
mod menu;
mod theme;
mod views;
mod widgets;

use std::path::PathBuf;

use eframe::egui;

use app::{App, Config, View};

const USAGE: &str = "usage: ptk-gui [--view home|project|scan|options|share|plugins|targets|doctor|about] [--project DIR] [--game DIR] [--run] [--screenshot FILE.png] [--scroll-bottom] [--smoke] [--size WxH]";

fn parse_args(args: impl Iterator<Item = String>) -> Result<(Config, [f32; 2]), String> {
    let mut config = Config::default();
    let mut size = [1280.0, 820.0];
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--view" => {
                let v = value("--view")?;
                config.view = Some(View::parse(&v).ok_or(format!("unknown view {v:?}"))?);
            }
            "--project" => config.project = Some(PathBuf::from(value("--project")?)),
            "--game" => config.game = Some(PathBuf::from(value("--game")?)),
            "--screenshot" => config.screenshot = Some(PathBuf::from(value("--screenshot")?)),
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v.split_once('x').ok_or("--size is WIDTHxHEIGHT")?;
                size = [
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                ];
            }
            "--run" => config.auto_run = true,
            "--scroll-bottom" => config.scroll_bottom = true,
            "--smoke" => config.smoke = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    Ok((config, size))
}

fn main() -> std::process::ExitCode {
    let (config, size) = match parse_args(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    let screenshot = config.screenshot.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Porting Toolkit")
            .with_app_id("io.github.hallel133.porting-toolkit")
            .with_inner_size(size)
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Porting Toolkit",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            let config = Config {
                native_menu: true,
                ..config
            };
            Ok(Box::new(App::new(config, &cc.egui_ctx)))
        }),
    );
    if let Err(e) = result {
        eprintln!("ptk-gui: {e}");
        return std::process::ExitCode::FAILURE;
    }
    if let Some(path) = screenshot
        && !path.is_file()
    {
        eprintln!("ptk-gui: the screenshot was not saved");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> impl Iterator<Item = String> {
        s.split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn arguments() {
        let (c, size) = parse_args(args("--view scan --game /g --run --size 1000x700")).unwrap();
        assert_eq!(c.view, Some(View::Scan));
        assert_eq!(c.game, Some(PathBuf::from("/g")));
        assert!(c.auto_run);
        assert_eq!(size, [1000.0, 700.0]);
        assert!(parse_args(args("--view nope")).is_err());
        assert!(parse_args(args("--bogus")).is_err());
        assert!(parse_args(args("--project")).is_err());
    }
}
