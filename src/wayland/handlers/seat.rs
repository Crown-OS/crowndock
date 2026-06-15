use smithay_client_toolkit::{
    delegate_seat,
    seat::{Capability, SeatHandler, SeatState},
};
use wayland_client::{Connection, QueueHandle, protocol::wl_seat};

use super::super::{App, SeatObject};

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let seat_obj = match self.seats.iter().position(|s| s.seat == seat) {
            Some(i) => &mut self.seats[i],
            None => {
                let data_device = self.data_device_manager_state.get_data_device(qh, &seat);
                self.seats.push(SeatObject {
                    seat: seat.clone(),
                    pointer: None,
                    data_device,
                });
                self.seats.last_mut().expect("just pushed")
            }
        };

        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("get_pointer failed");
            self.pointer = Some(pointer.clone());
            seat_obj.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(p) = self.pointer.take()
        {
            p.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seats.retain(|s| s.seat != seat);
    }
}

delegate_seat!(App);
