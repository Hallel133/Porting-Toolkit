//! Porting Toolkit desktop GUI: placeholder for phase 2.
//!
//! Plan (see ARCHITECTURE.md): an egui app on top of `ptk-core`, sharing the
//! CLI's commands, optionally reusing the GTA V-style theme and OFL fonts
//! from Grand-V's launcher (`LauncherApp/rust/src/ui/theme.rs`).

fn main() {
    println!(
        "Porting Toolkit GUI {} is planned for phase 2.\nUse the `ptk` command line for now (run `ptk` for help).",
        ptk_core::VERSION
    );
}
