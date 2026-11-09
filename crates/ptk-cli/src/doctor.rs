//! `ptk doctor`: prints `ptk_core::doctor` checks.

use std::process::ExitCode;

use ptk_core::doctor::{self, Status};

use crate::style::Style;

pub fn run(style: &Style, no_install: bool) -> anyhow::Result<ExitCode> {
    println!("{}", style.heading("ptk doctor"));
    let checks = doctor::run(!no_install);
    for c in &checks {
        let mark = match c.status {
            Status::Ok => style.ok(),
            Status::Warn => style.warn(),
            Status::Fail => style.fail(),
        };
        println!("  {mark} {:<22} {}", c.name, c.detail);
        if let Some(h) = &c.hint {
            println!("    {:<22} {}", "", style.dim(format!("→ {h}")));
        }
    }
    let problems = checks.iter().filter(|c| c.status == Status::Fail).count();
    let warnings = checks.iter().filter(|c| c.status == Status::Warn).count();
    println!();
    if problems == 0 {
        let extra = if warnings > 0 {
            format!(" ({warnings} warnings)")
        } else {
            String::new()
        };
        println!("  {} ptk is ready{extra}", style.ok());
        Ok(ExitCode::SUCCESS)
    } else {
        println!("  {} {problems} problem(s) found", style.fail());
        Ok(ExitCode::FAILURE)
    }
}
