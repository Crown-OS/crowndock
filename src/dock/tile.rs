use crownui::{
    ext::ImageId,
    prelude::{
        AnyView, Curve, Cx, Listeners, Signal, Size, Styled, View, bind, cover_image, keyed, rgba,
        vstack, zstack,
    },
};

use super::{
    app_icons,
    apps::AppKey,
    drag,
    glass::Tint,
    indicator, motion,
    slots::{Slot, SlotKey},
    state::Dock,
};

const PRESSED_SCALE: f32 = 0.9;
/// A landed icon shows again at once: the compositor draws its copy until then, so a fade would
/// show both.
const LANDING_MS: u64 = 0;

/// One place in the row: an app, or the gap a drag would land in.
pub fn tile(cx: &mut Cx, dock: Dock, slot: Signal<Slot>, tint: Tint) -> AnyView {
    let size = dock.metrics.icon;
    match slot.with(cx, |slot| slot.key.clone()) {
        SlotKey::Gap => vstack(()).size(size).boxed(),
        SlotKey::Item(key) => app(cx, dock, slot, key, tint).boxed(),
    }
}

/// Grows from its foot when hovered, sinks when pressed, opens the app on click and, when
/// pinned, lifts out of the dock when dragged. Its window marks sit below it. Appearing and
/// leaving, it scales from and to nothing on the render thread; while the compositor carries
/// it, it keeps its place undrawn.
fn app(cx: &mut Cx, dock: Dock, slot: Signal<Slot>, key: AppKey, tint: Tint) -> impl View {
    let metrics = dock.metrics;
    let clicked = key.clone();
    let icon = keyed(
        move |runtime| slot.with(runtime, |slot| slot.image),
        move |_, image| art(image, metrics.icon, metrics.icon_radius()),
    )
    .size(metrics.icon)
    .origin(0.5, 1.0)
    .hover(move |style| style.scale(metrics.hover_scale))
    .active(|style| style.scale(PRESSED_SCALE))
    .transition(Curve::Spring(motion::spring()))
    .on_click(move |cx, _| app_icons::open(cx, dock, &clicked));
    let icon = match key {
        AppKey::Pinned(path) => {
            icon.on_drag_start(move |cx, event| drag::lift(cx, dock, &path, event.position))
        }
        AppKey::Running(_) => icon,
    };
    let shown = zstack(icon)
        .size(metrics.icon)
        .opacity(bind(move |runtime| {
            let carried = slot.with(runtime, |slot| slot.carried);
            if carried { 0.0 } else { 1.0 }
        }))
        .transition(Curve::linear(LANDING_MS));
    zstack((shown, indicator::indicator(cx, slot, metrics, tint)))
        .size(metrics.icon)
        .enter(|style| style.scale(0.0).opacity(0.0))
        .exit(|style| style.scale(0.0).opacity(0.0))
}

fn art(image: Option<ImageId>, size: f32, radius: f32) -> AnyView {
    match image {
        Some(image) => cover_image(image, Size::new(size, size), 0.0)
            .size(size)
            .boxed(),
        None => vstack(())
            .size(size)
            .rounded(radius)
            .bg(rgba(0xFF_FF_FF_2E))
            .boxed(),
    }
}

#[cfg(test)]
mod tests {
    use crate::dock::testing::{hidden_icons, laid_out_icons, start, windows};

    #[test]
    fn a_carried_icon_keeps_its_place_undrawn_until_it_lands() {
        let mut app = start(&["org.example.A", "org.example.B"]);
        app.app_windows(windows("org.example.A", 1, false)).settle();
        let resting = laid_out_icons(&mut app);
        app.lift_app_icon("org.example.A").settle();
        assert_eq!(hidden_icons(&mut app), 1);
        assert_eq!(laid_out_icons(&mut app), resting);
        app.land_app_icon("org.example.A");
        assert_eq!(hidden_icons(&mut app), 0);
    }

    #[test]
    fn an_unpinned_app_can_be_carried_too() {
        let mut app = start(&[]);
        app.app_windows(windows("org.example.Unpinned", 1, false))
            .settle();
        app.lift_app_icon("org.example.Unpinned");
        assert_eq!(hidden_icons(&mut app), 1);
    }
}
