use std::path::Path;

use tiny_skia::Pixmap;

/// Load and rasterize an icon at `size` px square, returning a premultiplied
/// RGBA [`Pixmap`]. Format is inferred from the extension.
pub fn load(path: &Path, size: u32) -> Option<Pixmap> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "svg" | "svgz" => render_svg(path, size),
        "xpm" => None,
        _ => render_raster(path, size),
    }
}

fn render_raster(path: &Path, size: u32) -> Option<Pixmap> {
    let img = image::open(path).ok()?.to_rgba8();
    let resized = image::imageops::resize(&img, size, size, image::imageops::FilterType::Lanczos3);

    let mut pixmap = Pixmap::new(size, size)?;
    let dst = pixmap.data_mut();
    for (i, px) in resized.pixels().enumerate() {
        let [r, g, b, a] = [px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32];
        let o = i * 4;
        // tiny_skia expects premultiplied RGBA.
        dst[o] = ((r * a) / 255) as u8;
        dst[o + 1] = ((g * a) / 255) as u8;
        dst[o + 2] = ((b * a) / 255) as u8;
        dst[o + 3] = a as u8;
    }
    Some(pixmap)
}

fn render_svg(path: &Path, size: u32) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;

    let mut pixmap = Pixmap::new(size, size)?;
    let tree_size = tree.size();
    let scale = (size as f32 / tree_size.width()).min(size as f32 / tree_size.height());
    let tx = (size as f32 - tree_size.width() * scale) / 2.0;
    let ty = (size as f32 - tree_size.height() * scale) / 2.0;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}
