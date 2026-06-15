//! Wayland surface plumbing. Owns the sctk state objects and the calloop event
//! loop; delegates application logic to [`crate::dock::DockState`] and painting
//! to a [`crate::render::Renderer`].

mod handlers;

use std::time::Duration;

use calloop::{EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::CompositorState,
    data_device_manager::DataDeviceManagerState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
    },
    shm::{Shm, slot::SlotPool},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_pointer, wl_seat, wl_shm},
};

use crate::config::Config;
use crate::dnd::PendingDrop;
use crate::dock::DockState;
use crate::error::Result;
use crate::render::{Renderer, Scene, TinySkiaRenderer};
use crate::source;
use crate::watcher;

const DEFAULT_WIDTH: u32 = 1280;

/// Application state held inside the calloop event loop.
pub struct App {
    // sctk state -----------------------------------------------------------
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub shm: Shm,
    pub data_device_manager_state: DataDeviceManagerState,

    // surface lifecycle ----------------------------------------------------
    pub layer: LayerSurface,
    pub pool: SlotPool,
    pub buffer: Option<smithay_client_toolkit::shm::slot::Buffer>,
    pub width: u32,
    pub height: u32,
    pub first_configure: bool,
    pub needs_redraw: bool,
    pub exit: bool,

    // input ----------------------------------------------------------------
    pub pointer: Option<wl_pointer::WlPointer>,
    pub pointer_pos: Option<(f64, f64)>,
    pub seats: Vec<SeatObject>,

    // dock --------------------------------------------------------------
    pub dock: DockState,
    pub renderer: Box<dyn Renderer>,
    pub pending_drops: Vec<PendingDrop>,

    // calloop --------------------------------------------------------------
    pub loop_handle: LoopHandle<'static, App>,
    pub qh: QueueHandle<App>,
}

pub struct SeatObject {
    pub seat: wl_seat::WlSeat,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub data_device: smithay_client_toolkit::data_device_manager::data_device::DataDevice,
}

/// Boot the Wayland connection, run the event loop, return on exit.
pub fn run(config: Config) -> Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| crate::error::AppError::other(format!("compositor: {e}")))?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|e| crate::error::AppError::other(format!("layer-shell: {e}")))?;
    let shm = Shm::bind(&globals, &qh)
        .map_err(|e| crate::error::AppError::other(format!("shm: {e}")))?;
    let data_device_manager_state = DataDeviceManagerState::bind(&globals, &qh)
        .map_err(|e| crate::error::AppError::other(format!("data-device-manager: {e}")))?;

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("crowndock"), None);
    layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_size(0, config.theme.height);
    layer.set_exclusive_zone(config.theme.height as i32);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();

    let pool_size = (config.theme.height as usize).max(1) * 4096 * 4;
    let pool = SlotPool::new(pool_size, &shm)?;

    let mut event_loop: EventLoop<'static, App> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    let watch_rx = watcher::start()?;
    loop_handle.insert_source(watch_rx, |event, _, app| {
        if let calloop::channel::Event::Msg(ev) = event {
            app.handle_watch(ev);
        }
    })?;

    let dock = DockState::new(config.clone(), source::default_registry(&config));

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        data_device_manager_state,

        layer,
        pool,
        buffer: None,
        width: DEFAULT_WIDTH,
        height: config.theme.height,
        first_configure: true,
        needs_redraw: true,
        exit: false,

        pointer: None,
        pointer_pos: None,
        seats: Vec::new(),

        dock,
        renderer: Box::new(TinySkiaRenderer::new()),
        pending_drops: Vec::new(),

        loop_handle: loop_handle.clone(),
        qh: qh.clone(),
    };

    WaylandSource::new(conn, event_queue).insert(loop_handle)?;

    while !app.exit {
        event_loop.dispatch(Duration::from_millis(50), &mut app)?;
        if app.needs_redraw && !app.first_configure {
            app.draw_now();
        }
    }

    Ok(())
}

impl App {
    /// Redraw using the current `QueueHandle`. Convenience wrapper for the main
    /// loop, which doesn't have a `qh: &QueueHandle<Self>` parameter the way
    /// the sctk handler callbacks do.
    pub fn draw_now(&mut self) {
        let qh = self.qh.clone();
        self.draw(&qh);
    }

    pub fn relayout(&mut self) {
        self.dock.relayout(self.width, self.height);
        self.needs_redraw = true;
    }

    pub fn reload_items(&mut self) {
        self.dock.reload_items();
        self.relayout();
    }

    pub fn handle_watch(&mut self, event: crate::watcher::WatchEvent) {
        match event {
            crate::watcher::WatchEvent::ConfigChanged => {
                self.dock.apply_config(Config::load());
                self.relayout();
            }
            crate::watcher::WatchEvent::PinnedChanged => {
                self.reload_items();
            }
        }
    }

    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let stride = width as i32 * 4;

        if self.buffer.is_none() {
            match self.pool.create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            ) {
                Ok((buf, _)) => self.buffer = Some(buf),
                Err(err) => {
                    log::error!("create_buffer failed: {err}");
                    return;
                }
            }
        }

        let buffer = self.buffer.as_mut().expect("buffer present");
        let canvas = match self.pool.canvas(buffer) {
            Some(c) => c,
            None => {
                // Compositor still holds the previous buffer; allocate a second.
                match self.pool.create_buffer(
                    width as i32,
                    height as i32,
                    stride,
                    wl_shm::Format::Argb8888,
                ) {
                    Ok((new_buf, canvas)) => {
                        *buffer = new_buf;
                        canvas
                    }
                    Err(err) => {
                        log::error!("create_buffer (double) failed: {err}");
                        return;
                    }
                }
            }
        };

        canvas.fill(0);
        let scene = Scene::from_state(&self.dock, width, height);
        self.renderer.paint(&scene, canvas);

        let surface = self.layer.wl_surface();
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.frame(qh, surface.clone());
        buffer.attach_to(surface).expect("buffer attach");
        self.layer.commit();
        self.needs_redraw = false;
    }
}
