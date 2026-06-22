use std::collections::HashMap;

#[derive(Default)]
pub struct Icon {}

#[derive(Eq, Hash, PartialEq)]
pub struct IconId(u32);

#[derive(Default)]
pub struct State {
    icons: HashMap<IconId, Icon>,
}

impl State {
    pub fn remove_icon(&mut self, icon_id: &IconId) {
        self.icons.remove(icon_id);
        todo!("Redraw the UI Update");
    }
}
