mod config;
mod persistence;
mod renderer;
mod ui;
mod wayland;
mod window;

use anyhow::{Result, anyhow};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

use renderer::Renderer;
use ui::Ui;
use window::Window;

pub fn app() -> Result<()> {
    let mut event_loop: EventLoop<'static, Window> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    let (connection, mut event_queue, mut window) = Window::new(loop_handle.clone())?;

    while window.first_configure {
        event_queue.blocking_dispatch(&mut window)?;
    }

    let renderer = Renderer::new(&connection, &window)?;
    window.ui = Some(Ui::new(renderer)?);
    // First paint at the hidden offset so we never flash the dock at rest.
    window.paint();

    WaylandSource::new(connection, event_queue)
        .insert(loop_handle.clone())
        .map_err(|e| anyhow!("register wayland source: {}", e.error))?;

    let signal = event_loop.get_signal();
    event_loop.run(None, &mut window, move |window| {
        if window.exit {
            signal.stop();
        }
    })?;

    Ok(())
}
