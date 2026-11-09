//! One error type for the higher-level operations shared by the CLI and GUI.

use std::io;

use thiserror::Error;

use crate::engine::EngineError;
use crate::manifest::ManifestError;
use crate::pack::PackError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Pack(#[from] PackError),
    #[error("{context}: {source}")]
    Io { context: String, source: io::Error },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Wraps an I/O error with what was being done.
pub(crate) fn io_ctx(context: impl Into<String>) -> impl FnOnce(io::Error) -> Error {
    let context = context.into();
    move |source| Error::Io { context, source }
}
