//! What the row of icons shows: the pinned items, minus one being dragged away, plus a gap where
//! a drag hovering over the dock would land.

use std::path::{Path, PathBuf};

use crownui::ext::ImageId;

use crate::settings::Metrics;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SlotKey {
    Item(PathBuf),
    Gap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub key: SlotKey,
    pub image: Option<ImageId>,
}

impl Slot {
    pub fn key(slot: &Slot) -> SlotKey {
        slot.key.clone()
    }
}

/// A drag the dock takes part in: one of its own items lifted out, a drag hovering over it, or
/// both at once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DragState {
    /// The pinned item being dragged out of the dock.
    pub lifted: Option<PathBuf>,
    /// Where a drop would land, among the items still shown.
    pub gap: Option<usize>,
}

impl DragState {
    pub fn shown<'a>(&'a self, items: &'a [PathBuf]) -> impl Iterator<Item = &'a PathBuf> {
        items
            .iter()
            .filter(move |item| self.lifted.as_ref() != Some(*item))
    }
}

pub fn arrange(
    items: &[PathBuf],
    drag: &DragState,
    image: impl Fn(&Path) -> Option<ImageId>,
) -> Vec<Slot> {
    let mut slots: Vec<Slot> = drag
        .shown(items)
        .map(|item| Slot {
            key: SlotKey::Item(item.clone()),
            image: image(item),
        })
        .collect();
    if let Some(gap) = drag.gap {
        let at = gap.min(slots.len());
        slots.insert(
            at,
            Slot {
                key: SlotKey::Gap,
                image: None,
            },
        );
    }
    slots
}

/// The gap a drag at `x` opens among `shown` icons, in a row centred on `center`.
pub fn gap_index(x: f32, center: f32, shown: usize, metrics: &Metrics) -> usize {
    let left = center - metrics.row_width(shown + 1) / 2.0;
    ((x - left) / metrics.step())
        .floor()
        .clamp(0.0, shown as f32) as usize
}

/// Puts `dropped` at `index` among the items other than `lifted`; an item already pinned moves
/// there instead of appearing twice.
pub fn place(items: &mut Vec<PathBuf>, lifted: Option<&Path>, dropped: &[PathBuf], index: usize) {
    items.retain(|item| Some(item.as_path()) != lifted);
    let mut at = index.min(items.len());
    for path in dropped {
        if let Some(existing) = items.iter().position(|item| item == path) {
            items.remove(existing);
            at -= usize::from(existing < at);
        }
        items.insert(at, path.clone());
        at += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DockSettings;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_lifted_item_leaves_a_gap_where_it_was() {
        let items = paths(&["a", "b", "c"]);
        let drag = DragState {
            lifted: Some("b".into()),
            gap: Some(1),
        };
        let keys: Vec<SlotKey> = arrange(&items, &drag, |_| None)
            .iter()
            .map(Slot::key)
            .collect();
        assert_eq!(
            keys,
            [
                SlotKey::Item("a".into()),
                SlotKey::Gap,
                SlotKey::Item("c".into())
            ]
        );
    }

    #[test]
    fn the_gap_follows_the_pointer_across_the_row() {
        let metrics = Metrics::new(&DockSettings::default());
        let step = metrics.step();
        let left = 500.0 - (3.0 * step - metrics.spacing) / 2.0;
        assert_eq!(gap_index(0.0, 500.0, 2, &metrics), 0);
        assert_eq!(gap_index(left + step * 1.5, 500.0, 2, &metrics), 1);
        assert_eq!(gap_index(2000.0, 500.0, 2, &metrics), 2);
    }

    #[test]
    fn dropping_a_lifted_item_moves_it() {
        let mut items = paths(&["a", "b", "c"]);
        place(&mut items, Some(Path::new("a")), &paths(&["a"]), 2);
        assert_eq!(items, paths(&["b", "c", "a"]));
    }

    #[test]
    fn dropping_a_pinned_item_from_elsewhere_moves_it_without_a_duplicate() {
        let mut items = paths(&["a", "b", "c"]);
        place(&mut items, None, &paths(&["a", "d"]), 3);
        assert_eq!(items, paths(&["b", "c", "a", "d"]));
    }
}
