//! Desktop design viewer with independently testable scene geometry.

pub mod interaction;
pub mod scene;
mod window;

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Design(#[from] sysy_core::Error),
    #[error("could not watch design file: {0}")]
    Watch(#[from] notify::Error),
    #[error("could not open viewer: {0}")]
    Window(String),
}

/// Load a design and show it until the window closes.
///
/// # Errors
/// Returns an error if loading the design, watching its directory, or opening the window fails.
pub fn run(path: &Path) -> Result<(), Error> {
    let document = interaction::LiveDesign::open(path)?;
    #[cfg(target_os = "linux")]
    if ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .all(|name| std::env::var_os(name).is_none_or(|value| value.is_empty()))
    {
        return Err(Error::Window(
            "no desktop display; run sysy ui in an X11 or Wayland session".into(),
        ));
    }
    window::run(document)
}
