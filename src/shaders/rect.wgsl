// Dock surface shader.
// Draws a pill-shaped (fully-rounded) outline. The interior is left
// transparent so the compositor-provided background effect (blur via
// ext-background-effect-v1) shows through; only a thin border stroke
// and a very faint inner fill are emitted.

struct Uniforms {
    resolution: vec2<f32>,
    rect_min: vec2<f32>,
    rect_max: vec2<f32>,
    radius: f32,
    border_width: f32,
    border_color: vec4<f32>,
    fill_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) frag: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    // Fullscreen triangle in clip space.
    var clip = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    let p = clip[idx];
    var out: VertexOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    // Map clip-space (-1..1, y up) to framebuffer pixels (y down).
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));
    out.frag = uv * u.resolution;
    return out;
}

fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let center = (u.rect_min + u.rect_max) * 0.5;
    let half_size = (u.rect_max - u.rect_min) * 0.5;
    let p = in.frag - center;
    let d = sd_rounded_box(p, half_size, u.radius);

    // Anti-aliased outer edge coverage.
    let aa = fwidth(d) * 0.7 + 0.0001;
    let outer = clamp(0.5 - d / aa, 0.0, 1.0);
    if (outer <= 0.0) {
        discard;
    }

    // Coverage of the area inside the border stroke.
    let inner = clamp(0.5 - (d + u.border_width) / aa, 0.0, 1.0);
    // Stroke coverage = outer ring minus inner area.
    let stroke = clamp(outer - inner, 0.0, 1.0);

    // Pre-multiplied border stroke.
    let border_pm = vec4<f32>(
        u.border_color.rgb * u.border_color.a * stroke,
        u.border_color.a * stroke,
    );

    // Pre-multiplied faint inner fill (kept very low so the blurred
    // background beneath dominates).
    let fill_pm = vec4<f32>(
        u.fill_color.rgb * u.fill_color.a * inner,
        u.fill_color.a * inner,
    );

    // Composite border over fill (both pre-multiplied).
    let out_rgb = border_pm.rgb + fill_pm.rgb * (1.0 - border_pm.a);
    let out_a = border_pm.a + fill_pm.a * (1.0 - border_pm.a);
    return vec4<f32>(out_rgb, out_a);
}
