use smithay_client_toolkit::{
    delegate_pointer,
    seat::pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler},
    shell::WaylandSurface,
};
use wayland_client::{Connection, QueueHandle, protocol::wl_pointer};

use crate::launcher;

use super::super::App;

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.pointer_pos = Some(event.position);
                    let new_hover = self.dock.cell_at(event.position.0, event.position.1);
                    if new_hover != self.dock.hovered {
                        self.dock.hovered = new_hover;
                        self.draw(qh);
                    }
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_pos = None;
                    if self.dock.hovered.is_some() {
                        self.dock.hovered = None;
                        self.draw(qh);
                    }
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    if let Some((px, py)) = self.pointer_pos
                        && let Some(idx) = self.dock.cell_at(px, py)
                        && let Some(item) = self.dock.items.get(idx)
                    {
                        launcher::activate(&item.action);
                    }
                }
                _ => {}
            }
        }
    }
}

delegate_pointer!(App);
