use smithay_client_toolkit::{
    data_device_manager::data_offer::{DataOfferHandler, DragOffer},
    reexports::client::{protocol::wl_data_device_manager::DndAction, Connection, QueueHandle},
};

use crate::window::Window;

const ACCEPTED_DND_ACTIONS: DndAction = DndAction::Copy;

impl DataOfferHandler for Window {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        offer: &mut DragOffer,
        actions: DndAction,
    ) {
        let preferred = if actions.contains(DndAction::Copy) {
            DndAction::Copy
        } else {
            DndAction::empty()
        };
        offer.set_actions(ACCEPTED_DND_ACTIONS, preferred);
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
