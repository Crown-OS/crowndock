use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use freedesktop_entry_parser::parse_entry;
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

/// Decode size for raster art / SVG target rasterization. Vello downsamples
/// at render time, so over-decoding here just costs a bit of memory in
/// exchange for crisp icons regardless of final dock size.
const DECODE_SIZE: u32 = 256;

pub fn load_from_desktop(desktop_path: &Path) -> Result<ImageData> {
    let entry = parse_entry(desktop_path)?;
    let icon_name = entry
        .section("Desktop Entry")
        .attr("Icon")
        .ok_or_else(|| anyhow!("no Icon= in {}", desktop_path.display()))?
        .to_owned();

    let icon_path = resolve_icon(&icon_name)
        .ok_or_else(|| anyhow!("could not resolve icon `{}`", icon_name))?;
    decode_image(&icon_path)
}

fn resolve_icon(name: &str) -> Option<PathBuf> {
    if Path::new(name).is_absolute() {
        let p = PathBuf::from(name);
        if p.exists() {
            return Some(p);
        }
    }
    freedesktop_icons::lookup(name)
        .with_size(DECODE_SIZE as u16)
        .with_cache()
        .find()
}

fn decode_image(path: &Path) -> Result<ImageData> {
    let bytes = std::fs::read(path)?;
    let is_svg = matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("svg") | Some("svgz")
    );
    let decoded = if is_svg {
        rasterize_svg(&bytes)?
    } else {
        decode_raster(&bytes)?
    };
    Ok(ImageData {
        data: Blob::from(decoded.data),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: decoded.width,
        height: decoded.height,
    })
}

struct Decoded {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

fn decode_raster(bytes: &[u8]) -> Result<Decoded> {
    let img = image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = img.dimensions();
    let mut data = img.into_raw();
    // Vello expects premultiplied alpha; image gives us straight alpha.
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 255 {
            continue;
        }
        px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
        px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
        px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
    }
    Ok(Decoded {
        data,
        width,
        height,
    })
}

fn rasterize_svg(bytes: &[u8]) -> Result<Decoded> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &opt)?;
    let svg_size = tree.size();
    let longest = svg_size.width().max(svg_size.height()).max(1.0);
    let scale = DECODE_SIZE as f32 / longest;
    let width = (svg_size.width() * scale).round().max(1.0) as u32;
    let height = (svg_size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow!("svg pixmap allocation failed at {}x{}", width, height))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny_skia stores premultiplied RGBA already.
    Ok(Decoded {
        data: pixmap.data().to_vec(),
        width,
        height,
    })
}
