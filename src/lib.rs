mod desktop_entry;
mod dock;
mod icon;
mod launcher;
pub mod settings;
mod surfaces;
mod util;

use anyhow::{Context, Result};

use surfaces::DockSurfaces;

pub fn app() -> Result<()> {
    crownui::run_surfaces(DockSurfaces).context("the dock stopped")
}
