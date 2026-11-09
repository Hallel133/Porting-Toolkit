//! Per-user locations of the toolkit.
//!
//! | OS      | Data directory                                   |
//! |---------|--------------------------------------------------|
//! | macOS   | `~/Library/Application Support/PortingToolkit`   |
//! | Linux   | `$XDG_DATA_HOME/porting-toolkit` (or `~/.local/share/porting-toolkit`) |
//! | Windows | `%APPDATA%\PortingToolkit`                       |
//!
//! `PTK_HOME` overrides the data directory (tests use it).

use std::env;
use std::path::PathBuf;

/// The per-user data directory.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = env::var_os("PTK_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    platform_data_dir()
}

/// Managed Python environment for the engine, created by `ptk doctor`.
pub fn engine_env_dir() -> PathBuf {
    data_dir().join("engine-env")
}

/// The Python interpreter inside the managed engine environment.
pub fn engine_env_python() -> PathBuf {
    let env = engine_env_dir();
    if cfg!(windows) {
        env.join("Scripts").join("python.exe")
    } else {
        env.join("bin").join("python")
    }
}

/// Installed plugins (one folder per plugin id).
pub fn plugins_dir() -> PathBuf {
    data_dir().join("plugins")
}

/// Extra target definitions (`*.toml`), for example from an SDK.
pub fn targets_dir() -> PathBuf {
    data_dir().join("targets")
}

fn home_dir() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> PathBuf {
    home_dir().join("Library/Application Support/PortingToolkit")
}

#[cfg(windows)]
fn platform_data_dir() -> PathBuf {
    env::var_os("APPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
        .join("PortingToolkit")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("porting-toolkit")
}
