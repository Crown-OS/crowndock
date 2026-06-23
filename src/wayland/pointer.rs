use smithay_client_toolkit::{
    delegate_pointer,
    seat::pointer::{PointerEvent, PointerEventKind, PointerHandler},
    shell::WaylandSurface,
};
use wayland_client::{Connection, QueueHandle, protocol::wl_pointer};

use crate::window::Window;

impl PointerHandler for Window {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let dock_surface = self.layer.wl_surface().clone();
        for event in events {
            if event.surface != dock_surface {
                continue;
            }
            match event.kind {
                PointerEventKind::Enter { .. } => self.on_pointer_enter(),
                PointerEventKind::Leave { .. } => self.on_pointer_leave(),
                _ => {}
            }
        }
    }
}

delegate_pointer!(Window);
