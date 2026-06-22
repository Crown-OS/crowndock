mod config;
mod renderer;
mod ui;
mod wayland;
mod window;

use anyhow::Result;

use ui::Ui;
use window::Window;

use crate::renderer::Renderer;

pub fn app() -> Result<()> {
    let (connection, mut event_queue, mut state) = Window::new()?;

    while state.first_configure {
        event_queue.blocking_dispatch(&mut state)?;
    }

    let renderer = Renderer::new(&connection, &state)?;
    let mut ui = Ui::new(renderer)?;
    ui.render()?;

    while !state.exit {
        event_queue.blocking_dispatch(&mut state)?;
    }

    Ok(())
}
