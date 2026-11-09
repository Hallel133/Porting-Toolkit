//! Core library of Porting Toolkit.
//!
//! It holds everything the `ptk` CLI and the future GUI share:
//!
//! - [`manifest`]: the `ptk.json` project manifest, with a schema version.
//! - [`targets`]: the registry of platforms a project can be ported to.
//! - [`pack`]: `.ptk` community project files (pack, unpack) and [`verify`]
//!   for checking a user's own game files against recorded hashes.
//! - [`hash`]: SHA-256 helpers.
//! - [`engine`]: the JSON-RPC client that talks to the Python engine.
//!
//! The toolkit is a tool, not a distribution channel for games: a `.ptk` file
//! carries recipes, settings, community patches and hashes, never original
//! game files. See `docs/legal.md`.

pub mod category;
pub mod engine;
pub mod glob;
pub mod hash;
pub mod manifest;
pub mod pack;
pub mod paths;
pub mod targets;
pub mod verify;

/// Version of the toolkit, shared by every crate in the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Public repository of the project.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
