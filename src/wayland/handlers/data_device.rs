use std::fs;
use std::io::BufRead;

use calloop::PostAction;
use smithay_client_toolkit::{
    data_device_manager::{
        WritePipe,
        data_device::DataDeviceHandler,
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
    },
    delegate_data_device,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_surface,
    },
};

use crate::config;
use crate::dnd::{self, PendingDrop};

use super::super::App;

impl DataDeviceHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl_data_device: &WlDataDevice,
        _: f64,
        _: f64,
        _: &wl_surface::WlSurface,
    ) {
        let Some(drag) = drag_offer_for(self, wl_data_device) else {
            return;
        };
        let chosen = drag.with_mime_types(|m| dnd::pick_uri_mime(m));
        let serial = drag.serial;
        if let Some(mime) = chosen {
            drag.accept_mime_type(serial, Some(mime));
            drag.set_actions(DndAction::Copy, DndAction::Copy);
            self.dock.drop_active = true;
            self.needs_redraw = true;
        } else {
            drag.accept_mime_type(serial, None);
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if self.dock.drop_active {
            self.dock.drop_active = false;
            self.needs_redraw = true;
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        wl_data_device: &WlDataDevice,
    ) {
        let Some(drag) = drag_offer_for(self, wl_data_device) else {
            return;
        };
        let Some(mime) = drag.with_mime_types(|m| dnd::pick_uri_mime(m)) else {
            drag.finish();
            self.dock.drop_active = false;
            self.needs_redraw = true;
            return;
        };
        let pipe = match drag.receive(mime.clone()) {
            Ok(p) => p,
            Err(err) => {
                log::warn!("drop receive failed: {err}");
                drag.finish();
                self.dock.drop_active = false;
                return;
            }
        };
        drag.accept_mime_type(drag.serial, Some(mime));
        drag.set_actions(DndAction::Copy, DndAction::Copy);

        self.pending_drops.push(PendingDrop::new(drag.clone()));
        let drag_for_match = drag.clone();
        let insert = self
            .loop_handle
            .insert_source(pipe, move |_, f, app| read_drop_pipe(app, &drag_for_match, f));

        match insert {
            Ok(token) => {
                if let Some(last) = self.pending_drops.last_mut() {
                    last.token = Some(token);
                }
            }
            Err(err) => {
                log::warn!("insert_source for drop pipe failed: {err}");
                if let Some(last) = self.pending_drops.pop() {
                    last.offer.finish();
                }
            }
        }
    }
}

fn drag_offer_for(app: &App, wl_data_device: &WlDataDevice) -> Option<DragOffer> {
    app.seats
        .iter()
        .find(|s| s.data_device.inner() == wl_data_device)
        .and_then(|s| s.data_device.data().drag_offer())
}

fn read_drop_pipe(
    app: &mut App,
    drag: &DragOffer,
    file: &mut calloop::generic::NoIoDrop<fs::File>,
) -> PostAction {
    let Some(pos) = app.pending_drops.iter().position(|p| p.offer == *drag) else {
        return PostAction::Remove;
    };
    let mut pending = app.pending_drops.remove(pos);
    let Some(token) = pending.token.take() else {
        return PostAction::Remove;
    };

    // SAFETY: the FD is only used for the duration of this callback.
    let f: &mut fs::File = unsafe { file.get_mut() };
    let mut reader = std::io::BufReader::new(f);
    let consumed = match reader.fill_buf() {
        Ok(buf) if buf.is_empty() => {
            // EOF — payload complete.
            process_drop_payload(app, &pending.buffer);
            pending.offer.finish();
            pending.offer.destroy();
            app.dock.drop_active = false;
            app.needs_redraw = true;
            return PostAction::Remove;
        }
        Ok(buf) => {
            pending.buffer.extend_from_slice(buf);
            let len = buf.len();
            pending.token = Some(token);
            app.pending_drops.push(pending);
            len
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
            pending.token = Some(token);
            app.pending_drops.push(pending);
            return PostAction::Continue;
        }
        Err(err) => {
            log::warn!("drop read failed: {err}");
            pending.offer.finish();
            pending.offer.destroy();
            app.dock.drop_active = false;
            return PostAction::Remove;
        }
    };
    reader.consume(consumed);
    PostAction::Continue
}

fn process_drop_payload(app: &mut App, data: &[u8]) {
    let mut pinned_any = false;
    for path in dnd::parse_uri_list(data) {
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        if !path.exists() {
            log::warn!("dropped path does not exist: {}", path.display());
            continue;
        }
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let dest = config::pinned_dir().join(file_name);
        if let Err(err) = fs::copy(&path, &dest) {
            log::warn!("copy to {} failed: {err}", dest.display());
            continue;
        }
        log::info!("pinned {}", dest.display());
        pinned_any = true;
    }
    // The watcher will eventually reload, but rebuild now for immediate
    // feedback regardless of the filesystem's notification cadence.
    if pinned_any {
        app.reload_items();
    }
}

impl DataOfferHandler for App {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _: DndAction,
    ) {
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl DataSourceHandler for App {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: String,
        _: WritePipe,
    ) {
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        source.destroy();
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        source.destroy();
    }
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

delegate_data_device!(App);
