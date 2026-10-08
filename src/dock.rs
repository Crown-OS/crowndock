//! The dock as crownui views. One layer surface along the bottom edge holds a strip that
//! reveals the dock, the compositor's glass and the row of icons. The root takes no pointer
//! input itself, so the surface's input region is only the strip and the dock.

mod drag;
mod glass;
mod icons;
mod motion;
mod presence;
mod probe;
mod slots;
mod state;
mod sync;
mod tile;

use crownui::prelude::{
    Curve, Listeners, Runtime, Styled, TRANSPARENT, View, bind, component, for_each, hstack, keyed,
    vstack, zstack,
};

use crate::settings::{DockSettings, Metrics, TRIGGER_HEIGHT};
pub use glass::Tint;
use slots::Slot;
use state::Dock;

/// Below this much revealed the dock is gone: nothing drawn, no glass, no input region.
const HIDDEN: f32 = 0.001;

pub fn root(settings: DockSettings, metrics: Metrics, tint: Tint) -> impl View {
    component(move |cx| {
        let dock = Dock::new(cx, settings.clone(), metrics);
        let initial = settings.clone();
        // Watching starts a thread, which must wait until `crownui::run` has set up the GPU
        // driver through the process environment.
        cx.request_animation_frame(move |cx, _| sync::follow(cx, dock, initial));
        dock.refresh(cx);
        zstack((
            probe::size_probe(dock.surface).absolute().inset(0.0),
            icon_loader(dock),
            trigger(dock),
            body(dock, tint),
            row(dock),
        ))
        .pointer_events_none()
        .on_pointer_enter(move |cx, _| dock.update_presence(cx, |presence| presence.pointer = true))
        .on_pointer_leave(move |cx, _| {
            dock.update_presence(cx, |presence| presence.pointer = false)
        })
        .on_drag_over(move |cx, event| drag::over(cx, dock, event))
        .on_drag_leave(move |cx, _| drag::leave(cx, dock))
        .on_drop(move |cx, event| drag::drop(cx, dock, event))
    })
}

/// Decodes the icons once the surface is laid out, and again whenever its scale changes, so
/// they are always exactly as many pixels as they cover.
fn icon_loader(dock: Dock) -> impl View {
    keyed(
        move |runtime| {
            let scale = dock.scale.get(runtime);
            let laid_out = dock.surface.get(runtime).width > 0.0;
            laid_out.then_some(scale.to_bits())
        },
        move |cx, scale| {
            if let Some(scale) = scale {
                icons::reload(cx, dock, f32::from_bits(scale));
            }
            vstack(()).display_none()
        },
    )
}

/// The edge of the screen that brings a hidden dock up.
fn trigger(dock: Dock) -> impl View {
    vstack(())
        .absolute()
        .left(0.0)
        .right(0.0)
        .bottom(0.0)
        .h(TRIGGER_HEIGHT)
        .pointer_events_auto()
        .hidden(bind(move |runtime| {
            !dock.settings.with(runtime, |settings| settings.autohide)
        }))
}

/// How far above the screen edge the dock's foot is: its margin when up, its whole height
/// below the edge when hidden.
fn lift(dock: Dock, runtime: &mut Runtime) -> f32 {
    let metrics = dock.metrics;
    let reveal = dock.reveal.get(runtime);
    metrics.margin - (1.0 - reveal) * (metrics.margin + metrics.pill_height())
}

fn hidden(dock: Dock) -> impl FnMut(&mut Runtime) -> bool {
    move |runtime| dock.reveal.get(runtime) < HIDDEN
}

/// The glass, sprung to the row's width and slid with the reveal on the UI thread, so the
/// compositor's blur under it moves in the same frames.
fn body(dock: Dock, tint: Tint) -> impl View {
    let metrics = dock.metrics;
    let glass = vstack(())
        .w(bind(move |runtime| dock.width.get(runtime)))
        .h(metrics.pill_height())
        .rounded(metrics.radius)
        .material(bind(move |runtime| {
            glass::material(dock.reveal.get(runtime), metrics.radius, tint)
        }))
        .bg(bind(move |runtime| {
            let blurred = dock.capabilities.get(runtime).materials;
            if blurred { TRANSPARENT } else { tint.solid() }
        }))
        .pointer_events_auto()
        .hidden(bind(hidden(dock)));
    hstack(glass)
        .absolute()
        .left(0.0)
        .right(0.0)
        .bottom(bind(move |runtime| lift(dock, runtime)))
        .h(metrics.pill_height())
        .justify_center()
        .pointer_events_none()
}

/// The icons. Moving, appearing and leaving animate on the render thread with the glass's
/// spring; the UI thread only lays them out once per change.
fn row(dock: Dock) -> impl View {
    let metrics = dock.metrics;
    let icons = hstack(for_each(dock.slots, Slot::key, move |cx, slot| {
        tile::tile(cx, dock, slot)
    }))
    .gap(metrics.spacing)
    .items_center()
    .transition(Curve::Spring(motion::spring()))
    .pointer_events_auto();
    hstack(icons)
        .absolute()
        .left(0.0)
        .right(0.0)
        .bottom(bind(move |runtime| lift(dock, runtime) + metrics.padding))
        .h(metrics.icon)
        .justify_center()
        .pointer_events_none()
        .hidden(bind(hidden(dock)))
}
