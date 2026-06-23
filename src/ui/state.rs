use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Icon {
    pub path: PathBuf,
}

impl Icon {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Deterministic color derived from the desktop file path so each icon
    /// is visually distinct until real artwork is loaded.
    pub fn color(&self) -> [f32; 4] {
        let bytes = self.path.as_os_str().to_string_lossy().into_owned().into_bytes();
        let mut hash: u32 = 0x811c_9dc5;
        for b in bytes {
            hash ^= b as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        let h = (hash & 0xFF) as f32 / 255.0;
        let s = 0.55;
        let v = 0.95;
        let (r, g, b) = hsv_to_rgb(h, s, v);
        [r, g, b, 1.0]
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

#[derive(Default)]
pub struct State {
    pub icons: Vec<Icon>,
}

impl State {
    pub fn add_icon(&mut self, path: PathBuf) {
        if self.icons.iter().any(|i| i.path == path) {
            return;
        }
        self.icons.push(Icon::new(path));
    }
}
