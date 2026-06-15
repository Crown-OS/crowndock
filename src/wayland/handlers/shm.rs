use smithay_client_toolkit::{
    delegate_shm,
    shm::{Shm, ShmHandler},
};

use super::super::App;

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_shm!(App);
