use std::{
    fs,
    io::{BufRead, BufReader},
};

use calloop::PostAction;
use smithay_client_toolkit::{
    data_device_manager::data_device::DataDeviceHandler, delegate_data_device,
    reexports::client::{
        protocol::{
            wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
            wl_surface::WlSurface,
        },
        Connection, QueueHandle,
    },
};

use crate::window::{DragOfferRead, Window};

const URI_LIST_MIME: &str = "text/uri-list";

impl DataDeviceHandler for Window {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _x: f64,
        _y: f64,
        _: &WlSurface,
    ) {
        let Some(dd) = self.data_device.as_ref() else {
            return;
        };
        let Some(drag_offer) = dd.data().drag_offer() else {
            return;
        };

        let supported = drag_offer.with_mime_types(|mimes| {
            mimes.iter().find(|m| m.as_str() == URI_LIST_MIME).cloned()
        });
        if let Some(mime) = supported {
            self.accept_counter += 1;
            drag_offer.accept_mime_type(self.accept_counter, Some(mime));
            drag_offer.set_actions(DndAction::Copy, DndAction::Copy);
        } else {
            drag_offer.accept_mime_type(0, None);
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _x: f64,
        _y: f64,
    ) {
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        let Some(dd) = self.data_device.as_ref() else {
            return;
        };
        let Some(offer) = dd.data().drag_offer() else {
            return;
        };

        let has_uri_list = offer.with_mime_types(|mimes| {
            mimes.iter().any(|m| m.as_str() == URI_LIST_MIME)
        });
        if !has_uri_list {
            offer.finish();
            offer.destroy();
            return;
        }

        let read_pipe = match offer.receive(URI_LIST_MIME.to_string()) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("failed to open DnD pipe: {e}");
                offer.finish();
                offer.destroy();
                return;
            }
        };

        self.accept_counter += 1;
        offer.accept_mime_type(self.accept_counter, Some(URI_LIST_MIME.to_string()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);

        self.dnd_offers.push(DragOfferRead {
            offer: offer.clone(),
            data: Vec::new(),
            token: None,
        });
        let key = offer.clone();
        let insert = self
            .loop_handle
            .insert_source(read_pipe, move |_, f, window: &mut Window| {
                let Some(idx) = window.dnd_offers.iter().position(|o| o.offer == key) else {
                    return PostAction::Continue;
                };
                // SAFETY: we only borrow the file via BufReader; we never close it.
                let file: &mut fs::File = unsafe { f.get_mut() };
                let mut reader = BufReader::new(file);
                match reader.fill_buf() {
                    Ok(buf) if buf.is_empty() => {
                        let entry = window.dnd_offers.remove(idx);
                        entry.offer.finish();
                        entry.offer.destroy();
                        ingest_uri_list(window, &entry.data);
                        PostAction::Remove
                    }
                    Ok(buf) => {
                        let len = buf.len();
                        window.dnd_offers[idx].data.extend_from_slice(buf);
                        reader.consume(len);
                        PostAction::Continue
                    }
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::Interrupted) => {
                        PostAction::Continue
                    }
                    Err(e) => {
                        log::warn!("DnD read error: {e}");
                        if let Some(entry) = window.dnd_offers.get(idx) {
                            entry.offer.finish();
                            entry.offer.destroy();
                        }
                        window.dnd_offers.remove(idx);
                        PostAction::Remove
                    }
                }
            });
        match insert {
            Ok(token) => {
                if let Some(last) = self.dnd_offers.last_mut() {
                    last.token = Some(token);
                }
            }
            Err(e) => {
                log::warn!("failed to schedule DnD reader: {e}");
                offer.finish();
                offer.destroy();
                self.dnd_offers.pop();
            }
        }
    }
}

fn ingest_uri_list(window: &mut Window, bytes: &[u8]) {
    let Ok(text) = std::str::from_utf8(bytes) else {
        log::warn!("DnD payload is not valid utf-8");
        return;
    };
    let mut added = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(path) = uri_to_path(line) else {
            continue;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        window.state.add_icon(path);
        added = true;
    }
    if added {
        window.request_frame();
    }
}

fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = percent_decode(rest);
    Some(std::path::PathBuf::from(path))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

delegate_data_device!(Window);
