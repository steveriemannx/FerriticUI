struct SolidVertex {
    pos: vec2<f32>,
    pixel: vec2<f32>,
    center: vec2<f32>,
    half_size: vec2<f32>,
    radii: vec4<f32>,
    fill: vec4<f32>,
    border_width: f32,
    border_color: vec4<f32>,
    blur: f32,
    mode: f32,
}

struct TextureVertex {
    pos: vec2<f32>,
    uv: vec2<f32>,
    color: vec4<f32>,
}

struct SolidOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel: vec2<f32>,
    @location(1) center: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) fill: vec4<f32>,
    @location(5) border_width: f32,
    @location(6) border_color: vec4<f32>,
    @location(7) blur: f32,
    @location(8) mode: f32,
}

struct TextureOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_color(input: SolidVertex) -> SolidOutput {
    var out: SolidOutput;
    out.position = vec4<f32>(input.pos, 0.0, 1.0);
    out.pixel = input.pixel;
    out.center = input.center;
    out.half_size = input.half_size;
    out.radii = input.radii;
    out.fill = input.fill;
    out.border_width = input.border_width;
    out.border_color = input.border_color;
    out.blur = input.blur;
    out.mode = input.mode;
    return out;
}

@fragment
fn fs_color(input: SolidOutput) -> @location(0) vec4<f32> {
    let p = input.pixel - input.center;
    var radius = input.radii.x;
    if (p.x > 0.0) {
        radius = input.radii.y;
    }
    if (p.y > 0.0) {
        radius = input.radii.w;
    }
    if (p.x > 0.0 && p.y > 0.0) {
        radius = input.radii.z;
    }
    radius = clamp(radius, 0.0, min(input.half_size.x, input.half_size.y));

    let q = abs(p) - input.half_size + vec2<f32>(radius);
    let d = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
    if (input.mode > 0.5) {
        // Soft shadow: the quad is the shape inflated by `blur`; fade out
        // smoothly over the blur radius.
        let t = clamp(d / max(input.blur, 0.5), 0.0, 1.0);
        let falloff = 1.0 - t * t * (3.0 - 2.0 * t);
        let alpha = input.fill.a * falloff;
        if (alpha <= 0.0) {
            return vec4<f32>(0.0);
        }
        return vec4<f32>(input.fill.rgb, alpha);
    }

    let aa = max(fwidth(d), 0.0001);
    let inside = clamp(0.5 - d / aa, 0.0, 1.0);

    var border = 0.0;
    if (input.border_width > 0.0) {
        let inner = clamp(0.5 - (d + input.border_width) / aa, 0.0, 1.0);
        border = max(inside - inner, 0.0);
    }

    let fill_coverage = inside - border;
    let alpha = input.fill.a * fill_coverage + input.border_color.a * border;
    if (alpha <= 0.0) {
        return vec4<f32>(0.0);
    }
    let premultiplied =
        input.fill.rgb * (input.fill.a * fill_coverage) +
        input.border_color.rgb * (input.border_color.a * border);
    return vec4<f32>(premultiplied / alpha, alpha);
}

var r_texture: texture_2d<f32>;
var r_sampler: sampler;

@vertex
fn vs_texture(input: TextureVertex) -> TextureOutput {
    var out: TextureOutput;
    out.position = vec4<f32>(input.pos, 0.0, 1.0);
    out.color = input.color;
    out.uv = input.uv;
    return out;
}

@fragment
fn fs_texture(input: TextureOutput) -> @location(0) vec4<f32> {
    let mask = textureSample(r_texture, r_sampler, input.uv).r;
    return vec4<f32>(input.color.rgb, input.color.a * mask);
}
