// The paper map (crates/game/src/ui/map.rs): the map camera's live picture,
// flattened into a few paper-map colours with ink outlines, paper grain and a
// burnt, ragged edge. Tuning: assets/data/game/map.ron.
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct PaperMap {
    paper: vec4<f32>,
    ink: vec4<f32>,
    // x ink strength, y ink sensitivity, z grain, w brightness bands
    look: vec4<f32>,
    // x palette mix, y edge raggedness, z 1 / picture size (px), w unused
    more: vec4<f32>,
    palette: array<vec4<f32>, 6>,
}

@group(1) @binding(0) var<uniform> map: PaperMap;
@group(1) @binding(1) var view_texture: texture_2d<f32>;
@group(1) @binding(2) var view_sampler: sampler;

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(view_texture, view_sampler, uv, 0.0).rgb;
}

// The palette swatch a (blurred) colour is cut from.
fn swatch(c: vec3<f32>) -> i32 {
    var best = 0;
    var best_d = 1e9;
    for (var k = 0; k < 6; k++) {
        let p = map.palette[k].rgb;
        let d = distance(c, p) + abs(luma(c) - luma(p)) * 0.5;
        if (d < best_d) {
            best_d = d;
            best = k;
        }
    }
    return best;
}

// A soft average round `uv`: the picture is tiny, so this is a blob.
fn soft(uv: vec2<f32>) -> vec3<f32> {
    let px = map.more.z;
    var c = vec3<f32>(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            c += tap(uv + vec2<f32>(f32(i), f32(j)) * px);
        }
    }
    return c / 9.0;
}

// Construction paper: every place is a flat sheet of one of a few colours,
// cut with wobbly scissors (the noise shifts where it samples), laid over the
// sheets under it with a soft shadow, and the paper's fibres show.
@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let px = map.more.z;
    let wob = (vec2<f32>(noise(uv * 55.0), noise(uv * 55.0 + 9.1)) - 0.5) * px * 1.6;
    let k = swatch(soft(uv + wob));
    var col = map.palette[k].rgb;

    // A shadow where a different sheet lies up and to the left (light from
    // the top left): the sheet here sits under its edge.
    let s = 2.2 * px;
    let up = swatch(soft(uv + wob + vec2<f32>(-s, -s)));
    let shade = select(0.0, 1.0, up != k) * map.look.x;
    col = col * (1.0 - 0.28 * shade);

    // Fibres and flecks.
    let fib = noise(uv * vec2<f32>(260.0, 60.0)) * 0.6 + noise(uv * 500.0) * 0.4;
    col = col * (1.0 + (fib - 0.5) * map.look.z * 2.0);
    let fleck = step(0.985, hash(floor(uv * 300.0)));
    col = mix(col, map.paper.rgb, fleck * 0.35);

    // A torn edge round the whole sheet, showing white paper core.
    let d = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let e = d + (noise(uv * 30.0) - 0.5) * 0.025 * map.more.y;
    col = mix(col, vec3<f32>(0.97, 0.95, 0.9), 1.0 - smoothstep(0.006, 0.014, e));
    let alpha = smoothstep(0.0, 0.005, e);
    return vec4<f32>(col, alpha);
}
