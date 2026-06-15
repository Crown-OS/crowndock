use crate::config::Config;
use crate::item::DockItem;
use crate::source::SourceRegistry;
use crate::theme::Theme;

/// Axis-aligned cell rectangle in surface coordinates.
#[derive(Debug, Clone, Copy)]
pub struct CellRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CellRect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        let px = px as f32;
        let py = py as f32;
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Owns the dock's logical state — the items it shows, the cells they occupy,
/// and any interaction flags ([`hovered`], [`drop_active`]). The renderer reads
/// from this; Wayland handlers mutate it.
pub struct DockState {
    pub config: Config,
    pub sources: SourceRegistry,
    pub items: Vec<DockItem>,
    pub cells: Vec<CellRect>,
    pub hovered: Option<usize>,
    pub drop_active: bool,
}

impl DockState {
    pub fn new(config: Config, sources: SourceRegistry) -> Self {
        let items = sources.collect(config.theme.icon_size);
        Self {
            config,
            sources,
            items,
            cells: Vec::new(),
            hovered: None,
            drop_active: false,
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.config.theme
    }

    /// Reload from sources after config or filesystem changes.
    pub fn reload_items(&mut self) {
        self.items = self.sources.collect(self.config.theme.icon_size);
        if let Some(h) = self.hovered
            && h >= self.items.len()
        {
            self.hovered = None;
        }
    }

    pub fn apply_config(&mut self, config: Config) {
        // Rebuild sources from the new config so PinnedConfigSource sees the
        // updated `pinned` list.
        self.sources = crate::source::default_registry(&config);
        self.config = config;
        self.reload_items();
    }

    /// Compute cell positions for the given surface size.
    pub fn relayout(&mut self, surface_w: u32, surface_h: u32) {
        self.cells = layout(surface_w, surface_h, self.items.len() as u32, self.theme());
    }

    /// Cell index at (px, py), if any.
    pub fn cell_at(&self, px: f64, py: f64) -> Option<usize> {
        self.cells.iter().position(|c| c.contains(px, py))
    }
}

/// Pure layout: centered row of `n` square cells separated by `theme.icon_gap`.
pub fn layout(surface_w: u32, surface_h: u32, n: u32, theme: &Theme) -> Vec<CellRect> {
    if n == 0 {
        return Vec::new();
    }
    let cell = theme.cell_size();
    let gap = theme.icon_gap;
    let total_w = n * cell + n.saturating_sub(1) * gap;
    let start_x = (surface_w.saturating_sub(total_w)) as f32 / 2.0;
    let cy = (surface_h as f32 - cell as f32) / 2.0;
    (0..n)
        .map(|i| CellRect {
            x: start_x + i as f32 * (cell + gap) as f32,
            y: cy,
            w: cell as f32,
            h: cell as f32,
        })
        .collect()
}
