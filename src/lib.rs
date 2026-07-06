mod config;
mod dock_handler;
mod persistence;
mod ui;

use anyhow::Result;
use crownshell::{Anchor, KeyboardInteractivity, Layer, WindowConfig};

use config::{WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH};
use dock_handler::DockHandler;

pub fn app() -> Result<()> {
    crownshell::run(|app| {
        let config = WindowConfig {
            namespace: WINDOW_TITLE.to_string(),
            layer: Layer::Overlay,
            anchor: Anchor::BOTTOM,
            size: (WINDOW_WIDTH, WINDOW_HEIGHT),
            exclusive_zone: 0,
            keyboard_interactivity: KeyboardInteractivity::None,
            blur: true,
            ..Default::default()
        };
        app.create_window(config, DockHandler::new());
        Ok(())
    })
}
