use std::path::PathBuf;

use crownui::{
    ext::ImageId,
    prelude::{
        AnyView, Cx, Listeners, Signal, Size, Styled, View, cover_image, keyed, rgba, vstack,
    },
};

use super::{
    app_icons, drag,
    slots::{Slot, SlotKey},
    state::Dock,
};

const PRESSED_SCALE: f32 = 0.9;
/// An icon's corner radius as a share of its edge.
pub const PLACEHOLDER_CORNER: f32 = 0.22;

/// One place in the row: a pinned app, or the gap a drag would land in.
pub fn tile(cx: &mut Cx, dock: Dock, slot: Signal<Slot>) -> AnyView {
    let size = dock.metrics.icon;
    match slot.with(cx, |slot| slot.key.clone()) {
        SlotKey::Gap => vstack(()).size(size).boxed(),
        SlotKey::Item(path) => app(dock, slot, path).boxed(),
    }
}

/// Grows from its foot when hovered, sinks when pressed, opens the app on click and lifts out of
/// the dock when dragged. Appearing and leaving, it scales from and to nothing on the render
/// thread.
fn app(dock: Dock, slot: Signal<Slot>, path: PathBuf) -> impl View {
    let metrics = dock.metrics;
    let clicked = path.clone();
    keyed(
        move |runtime| slot.with(runtime, |slot| slot.image),
        move |_, image| art(image, metrics.icon),
    )
    .size(metrics.icon)
    .origin(0.5, 1.0)
    .hover(move |style| style.scale(metrics.hover_scale))
    .active(|style| style.scale(PRESSED_SCALE))
    .enter(|style| style.scale(0.0).opacity(0.0))
    .exit(|style| style.scale(0.0).opacity(0.0))
    .on_click(move |cx, _| app_icons::open(cx, dock, &clicked))
    .on_drag_start(move |cx, event| drag::lift(cx, dock, &path, event.position))
}

fn art(image: Option<ImageId>, size: f32) -> AnyView {
    match image {
        Some(image) => cover_image(image, Size::new(size, size), 0.0)
            .size(size)
            .boxed(),
        None => vstack(())
            .size(size)
            .rounded(size * PLACEHOLDER_CORNER)
            .bg(rgba(0xFF_FF_FF_2E))
            .boxed(),
    }
}
