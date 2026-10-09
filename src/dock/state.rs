use std::{collections::HashMap, path::PathBuf};

use crownui::{
    ext::ImageId,
    prelude::{Capabilities, Cx, Signal, Size, Store},
};

use super::{
    icons::{self, IconLoads},
    motion::{self, Motion},
    presence::{self, Presence},
    slots::{self, DragState, Slot},
};
use crate::settings::{DockSettings, Metrics};

/// Every piece of the dock's state, as copyable handles into the reactive runtime. All changes
/// go through its methods, which keep the slots, the glass and the reveal in step.
#[derive(Clone, Copy)]
pub struct Dock {
    pub metrics: Metrics,
    pub settings: Signal<DockSettings>,
    pub drag: Signal<DragState>,
    pub slots: Signal<Vec<Slot>>,
    pub icons: Signal<HashMap<PathBuf, ImageId>>,
    /// Each pinned item's app id, read once from its `.desktop` file.
    pub app_ids: Store<HashMap<PathBuf, String>>,
    pub presence: Signal<Presence>,
    /// The layer surface's size.
    pub surface: Signal<Size>,
    /// Device pixels per logical pixel.
    pub scale: Signal<f32>,
    pub capabilities: Signal<Capabilities>,
    /// 0 hidden below the screen edge, 1 fully up.
    pub reveal: Signal<f32>,
    /// The glass's width.
    pub width: Signal<f32>,
    pub motion: Store<Motion>,
    pub loads: Store<IconLoads>,
    /// Bumped on each presence change, so a stale show/hide timer does nothing.
    pub timer: Store<u64>,
}

impl Dock {
    pub fn new(cx: &mut Cx, settings: DockSettings, metrics: Metrics) -> Self {
        let reveal = if settings.autohide { 0.0 } else { 1.0 };
        let width = metrics.pill_width(settings.items.len());
        Self {
            metrics,
            settings: cx.signal(settings),
            drag: cx.signal(DragState::default()),
            slots: cx.signal(Vec::new()),
            icons: cx.signal(HashMap::new()),
            app_ids: cx.store(HashMap::new()),
            presence: cx.signal(Presence::default()),
            surface: cx.signal(Size::ZERO),
            scale: cx.scale_factor(),
            capabilities: cx.platform_capabilities(),
            reveal: cx.signal(reveal),
            width: cx.signal(width),
            motion: cx.store(Motion::new(reveal, width)),
            loads: cx.store(IconLoads::default()),
            timer: cx.store(0),
        }
    }

    /// Edits the pinned items, saves them to `dock.ron` and loads any new icon.
    pub fn update_items(self, cx: &mut Cx, change: impl FnOnce(&mut Vec<PathBuf>)) {
        self.settings
            .update(cx, |settings| change(&mut settings.items));
        let saved = self.settings.with(cx, |settings| {
            crownconfig::save_section(DockSettings::SECTION, settings)
        });
        if let Err(error) = saved {
            log::warn!("save the dock: {error}");
        }
        icons::sync(cx, self);
        self.refresh(cx);
    }

    pub fn update_drag(self, cx: &mut Cx, change: impl FnOnce(&mut DragState)) {
        self.drag.update(cx, change);
        self.refresh(cx);
    }

    pub fn update_presence(self, cx: &mut Cx, change: impl FnOnce(&mut Presence)) {
        let before = self.presence.with(cx, |presence| *presence);
        let mut after = before;
        change(&mut after);
        if after != before {
            self.presence.set(cx, after);
            presence::changed(cx, self);
        }
    }

    /// Lays the row out again from the items, the drag and the icons, and springs the glass to
    /// fit it.
    pub fn refresh(self, cx: &mut Cx) {
        let items = self.settings.with(cx, |settings| settings.items.clone());
        let drag = self.drag.with(cx, Clone::clone);
        let icons = self.icons;
        let arranged = icons.with(cx, |icons| {
            slots::arrange(&items, &drag, |item| icons.get(item).copied())
        });
        let width = self.metrics.pill_width(arranged.len());
        if self.slots.with(cx, |current| *current != arranged) {
            self.slots.set(cx, arranged);
        }
        if self.motion.update(cx, |motion| motion.width.aim(width)) {
            motion::animate(cx, self);
        }
    }

    /// How many pinned items the row shows, the lifted one left out.
    pub fn shown_count(self, cx: &mut Cx) -> usize {
        let drag = self.drag.with(cx, Clone::clone);
        self.settings
            .with(cx, |settings| drag.shown(&settings.items).count())
    }
}
