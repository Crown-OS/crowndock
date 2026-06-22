use smithay_client_toolkit::{
    delegate_layer,
    shell::wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use wayland_client::{Connection, QueueHandle};

use crate::{config, window::Window};

impl LayerShellHandler for Window {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if configure.new_size.0 != 0 {
            self.width = configure.new_size.0;
        }
        if configure.new_size.1 != 0 {
            self.height = configure.new_size.1;
        }
        self.first_configure = false;

        // Surface-local pill bounds match the inset used by the dock shader.
        let inset_x = config::DOCK_INSET_X.round() as i32;
        let inset_y = config::DOCK_INSET_Y.round() as i32;
        let w = (self.width as i32 - 2 * inset_x).max(0);
        let h = (self.height as i32 - 2 * inset_y).max(0);
        self.update_blur_region((inset_x, inset_y, w, h));
    }
}

delegate_layer!(Window);
