//! `ptk export` and `ptk serve`, plus the import step of `ptk import`.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::bail;
use ptk_core::convert::{self, Event, ExportOptions, RunResult};
use ptk_core::engine::EngineClient;
use ptk_core::manifest::Manifest;

use crate::style::Style;

/// Prints plugin progress on one line (on a terminal) and logs as they come.
pub struct Reporter<'a> {
    style: &'a Style,
    tty: bool,
    last: String,
}

impl<'a> Reporter<'a> {
    pub fn new(style: &'a Style) -> Self {
        Reporter {
            style,
            tty: std::io::stderr().is_terminal(),
            last: String::new(),
        }
    }

    fn clear(&mut self) {
        if self.tty && !self.last.is_empty() {
            eprint!("\r\x1b[2K");
            self.last.clear();
        }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Progress { fraction, message } => {
                if message.is_empty() {
                    return;
                }
                let pct = fraction
                    .map(|f| format!("{:>3}% ", (f * 100.0).round() as u32))
                    .unwrap_or_default();
                let line = format!("  {pct}{message}");
                if self.tty {
                    let shown: String = line.chars().take(100).collect();
                    eprint!("\r\x1b[2K{}", self.style.dim(&shown));
                    let _ = std::io::stderr().flush();
                    self.last = line;
                } else if line != self.last {
                    eprintln!("{line}");
                    self.last = line;
                }
            }
            Event::Log { warning, message } => {
                self.clear();
                if warning {
                    eprintln!("{} {message}", self.style.warn());
                } else {
                    eprintln!("  {}", self.style.dim(message));
                }
            }
        }
    }

    pub fn done(&mut self) {
        self.clear();
    }
}

fn print_result(style: &Style, r: &RunResult) {
    println!("{} {}", style.ok(), r.summary);
    let source = if r.builtin { "built-in" } else { "installed" };
    println!(
        "  {}",
        style.dim(format!("{} {} ({source})", r.plugin_name, r.plugin_version))
    );
    for w in &r.warnings {
        println!("{} {w}", style.warn());
    }
    for n in &r.notes {
        println!("  {n}");
    }
}

#[allow(clippy::too_many_arguments)]
pub fn export(
    style: &Style,
    project: &Path,
    targets: Vec<String>,
    output: Option<PathBuf>,
    game: Option<PathBuf>,
    plugin: Option<String>,
    set: Vec<String>,
    json: bool,
    open: bool,
) -> anyhow::Result<ExitCode> {
    let manifest = Manifest::load(project)?;
    let targets = if targets.is_empty() {
        manifest.targets.clone()
    } else {
        targets
    };
    if targets.is_empty() {
        bail!("no targets: pass --target ID (see `ptk targets`)");
    }
    let opts = ExportOptions {
        game,
        output,
        plugin,
        settings: convert::parse_settings(&set)?,
    };
    let mut engine = EngineClient::start()?;
    let mut results = Vec::new();
    let mut failed = false;
    for t in &targets {
        if !json {
            println!("{} {}", style.bold("Exporting for"), style.cyan(t));
        }
        let mut rep = Reporter::new(style);
        let r = convert::export(&mut engine, project, t, &opts, &mut |e| {
            if !json {
                rep.event(e)
            }
        });
        rep.done();
        match r {
            Ok(r) => {
                if !json {
                    print_result(style, &r);
                    if let Some(p) = &r.open {
                        println!("  {} {}", style.dim("→"), p.display());
                    }
                    if let Some(dir) = &r.serve {
                        println!(
                            "  {} {}",
                            style.dim("try it:"),
                            style.bold(format!("ptk serve \"{}\"", dir.display()))
                        );
                    }
                    println!();
                }
                if open && let Some(p) = r.open.as_ref().filter(|_| r.serve.is_none()) {
                    open_path(p);
                }
                results.push(r);
            }
            Err(e) => {
                failed = true;
                if json {
                    results.push(RunResult {
                        target: Some(t.clone()),
                        summary: format!("error: {e}"),
                        ..Default::default()
                    });
                } else {
                    eprintln!("{} {t}: {e}\n", style.fail());
                }
            }
        }
    }
    engine.shutdown()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// Runs the import plugin after `ptk import` recorded the game.
pub fn import_step(
    style: &Style,
    engine: &mut EngineClient,
    project: &Path,
    game: &Path,
    file_title: &str,
) {
    let mut rep = Reporter::new(style);
    let r = convert::import(engine, project, game, Some(file_title), &mut |e| {
        rep.event(e)
    });
    rep.done();
    match r {
        Ok(Some(r)) => print_result(style, &r),
        Ok(None) => println!(
            "  {}",
            style.dim("No import plugin for this engine yet (see `ptk market list`).")
        ),
        Err(e) => println!("{} Import plugin: {e}", style.warn()),
    }
}

pub fn open_path(p: &Path) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(p).spawn();
}

pub fn serve(style: &Style, dir: &Path, port: u16, open: bool) -> anyhow::Result<ExitCode> {
    if !dir.join("index.html").is_file() {
        bail!("{} has no index.html", dir.display());
    }
    let server = ptk_core::serve::start(dir, port)?;
    let url = server.url();
    println!(
        "{} Serving {} at {}",
        style.ok(),
        dir.display(),
        style.bold(&url)
    );
    println!(
        "  {}",
        style.dim("Only this computer can connect. Press Ctrl+C to stop.")
    );
    if open {
        open_path(Path::new(&url));
    }
    loop {
        std::thread::park();
    }
}

pub fn art(
    style: &Style,
    project: &Path,
    icon: Option<PathBuf>,
    poster: Option<PathBuf>,
    clear_icon: bool,
    clear_poster: bool,
) -> anyhow::Result<ExitCode> {
    use ptk_core::art::{self, ArtKind};
    for (kind, file, clear) in [
        (ArtKind::Icon, icon, clear_icon),
        (ArtKind::Poster, poster, clear_poster),
    ] {
        if clear {
            art::clear(project, kind)?;
            println!("{} Removed the {}", style.ok(), kind.name());
        }
        if let Some(f) = file {
            let rel = art::set(project, kind, &f)?;
            println!("{} {} → {rel}", style.ok(), kind.name());
        }
    }
    let m = Manifest::load(project)?;
    for kind in [ArtKind::Icon, ArtKind::Poster] {
        let shown = art::resolve(project, &m, kind)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| style.dim("none"));
        println!("  {:<7} {shown}", kind.name());
    }
    Ok(ExitCode::SUCCESS)
}
