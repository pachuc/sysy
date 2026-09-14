//! Desktop design viewer with independently testable scene geometry.

pub mod scene;
mod window;

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Design(#[from] sysy_core::Error),
    #[error("could not open viewer: {0}")]
    Window(String),
}

/// Load a design and show it until the window closes.
///
/// # Errors
/// Returns an error if loading the design or opening the window fails.
pub fn run(path: &Path) -> Result<(), Error> {
    window::run(sysy_core::load(path)?)
}
