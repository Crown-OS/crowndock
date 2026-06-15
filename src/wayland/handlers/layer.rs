use smithay_client_toolkit::{
    delegate_layer,
    shell::wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use wayland_client::{Connection, QueueHandle};

use super::super::App;

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let (w, h) = configure.new_size;
        if w != 0 {
            self.width = w;
        }
        if h != 0 {
            self.height = h;
        }
        if self.width == 0 {
            self.width = super::super::DEFAULT_WIDTH;
        }
        if self.height == 0 {
            self.height = self.dock.theme().height;
        }
        self.buffer = None;
        self.relayout();
        self.first_configure = false;
        self.draw(qh);
    }
}

delegate_layer!(App);
