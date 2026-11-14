// Parts' surfaces (G2.11, ADR-0041): a wooden part's pattern as a solid texture, found from
// where a fragment lies in the wood (after Perlin 1985 and Peachey 1985): a stem's growth rings
// are cylinders about its pith, so a pole's side shows its fibres running along it, its cut
// ends the rings, and a sawn board the rings cut across where it was sawn from its log. Bark is
// furrowed along the stem. Every scale is in metres, so a part looks the same at any size, and
// detail finer than a pixel fades to its mean (`fbm`'s footprint), as the models' does.

// How a part's surface follows its wood (vertex.rs, `Grain`).
const GRAIN_ROUND: u32 = 1u;
const GRAIN_SAWN: u32 = 2u;
const GRAIN_BARK: u32 = 3u;
const GRAIN_LEAVES: u32 = 5u;

// Bark's furrows: across the stem (per metre) and along it, and its plates' breaks.
const FURROWS: f32 = 28.0;
const FURROW_ALONG: f32 = 3.0;

/// `local` (m) as its section across axis `axis` (0: x, 1: y, 2: z) and its length along it:
/// (across, across, along).
fn in_wood(local: vec3<f32>, axis: u32) -> vec3<f32> {
    switch axis {
        case 0u: {
            return vec3<f32>(local.y, local.z, local.x);
        }
        case 2u: {
            return local;
        }
        default: {
            return vec3<f32>(local.x, local.z, local.y);
        }
    }
}

/// Where a sawn part's pith lies in its section (m, about its middle), by its `seed`: a board is
/// sawn beside the pith of a log wider than it is, a squared beam about it.
fn pith_of(seed: f32, section: vec2<f32>) -> vec2<f32> {
    let angle = seed * 6.2831853;
    let out = max(section.x, section.y) * (0.15 + 1.4 * fract(seed * 7.31));
    return vec2<f32>(cos(angle), sin(angle)) * out;
}

/// The Wood pattern's brightness at `p` (across, across, along; m) with the pith at `pith`: the
/// models' soft streaks and wandering grain lines, the lines here the rings' crests.
fn solid_wood(p: vec3<f32>, pith: vec2<f32>, footprint: f32, seed: f32) -> f32 {
    let c = p.xy - pith;
    let shift = vec3<f32>(seed * 17.0, seed * 29.0, seed * 7.0);
    let streaks = fbm(vec3<f32>(c * 60.0, p.z * 8.0) + shift, 5u, footprint * 60.0);
    let wander = fbm(vec3<f32>(c * 12.0, p.z * 0.5) + shift + 1.7, 3u, footprint * 12.0)
        * GRAIN_WANDER;
    let crest = sin((length(c) * GRAIN_LINES + wander) * 6.2831853);
    let strength = fbm(vec3<f32>(c * 20.0, p.z) + shift + 3.1, 2u, footprint * 20.0) * 1.6;
    let seen = 1.0 - smoothstep(0.25, 0.5, footprint * GRAIN_LINES);
    let lines = mix(0.07, smoothstep(0.75, 1.0, crest) * strength, seen);
    return 0.92 + 0.16 * streaks - 0.28 * lines;
}

/// Bark's brightness at `p` (across, across, along; m): plates between dark furrows running
/// along the stem, with fine speckle; thin stems' bark is smooth, as young bark is (its furrows
/// are centimetres apart whatever the stem).
fn bark(p: vec3<f32>, footprint: f32, seed: f32) -> f32 {
    let shift = vec3<f32>(seed * 23.0, seed * 11.0, seed * 5.0);
    let n = fbm(vec3<f32>(p.xy * FURROWS, p.z * FURROW_ALONG) + shift, 4u, footprint * FURROWS);
    let crevice = smoothstep(0.0, 0.22, abs(2.0 * n - 1.0));
    let fine = fbm(vec3<f32>(p.xy * 160.0, p.z * 40.0) + shift, 2u, footprint * 160.0);
    let seen = 1.0 - smoothstep(0.25, 0.5, footprint * FURROWS);
    return mix(0.86, 0.62 + 0.38 * crevice, seen) * (0.92 + 0.16 * fine);
}

/// How much of a birch's dark marks cover its bark at `p` (0–1): short dashes across the stem,
/// and larger dark patches; finer than a pixel, their share (a birch reads grey-white with
/// dark bands from afar, never as a white line).
fn marks_at(p: vec3<f32>, footprint: f32, seed: f32) -> f32 {
    let dashes = fbm(vec3<f32>(p.xy * 7.0, p.z * 45.0) + seed * 41.0, 3u, footprint * 45.0);
    let patches = fbm(vec3<f32>(p.xy * 3.0, p.z * 4.0) + seed * 13.0, 2u, footprint * 4.0);
    let seen = 1.0 - smoothstep(0.25, 0.5, footprint * 45.0);
    let marked = max(smoothstep(0.56, 0.62, dashes), smoothstep(0.62, 0.7, patches));
    return mix(max(0.28, smoothstep(0.62, 0.7, patches)), marked, seen);
}

/// A part's pattern and marks at `local`: `part` is its grain, axis, seed and marks token (as
/// `vs_main` gives it), `scale` its instance's size along its axes.
fn part_pattern(token: Token, part: vec4<f32>, scale: vec3<f32>, at: Place) -> vec2<f32> {
    let grain = u32(part.x);
    if token.pattern != PATTERN_WOOD {
        return vec2<f32>(pattern_of(token.pattern, at), 0.0);
    }
    let axis = u32(part.y);
    let p = in_wood(at.local, axis);
    switch grain {
        case GRAIN_BARK: {
            let marked = select(0.0, marks_at(p, at.footprint, part.z), part.w >= 0.0);
            return vec2<f32>(bark(p, at.footprint, part.z), marked);
        }
        case GRAIN_SAWN: {
            let section = in_wood(scale, axis).xy;
            return vec2<f32>(solid_wood(p, pith_of(part.z, section), at.footprint, part.z), 0.0);
        }
        default: {
            return vec2<f32>(solid_wood(p, vec2<f32>(0.0), at.footprint, part.z), 0.0);
        }
    }
}

/// Foliage's surface (`creeer_render::foliage`): its token's colour in its lump's or tip's
/// `tone` (±18 %), mottled at 30 cm and dappled at a few centimetres, as leaves and needles
/// catch the light; its creases and underside darkened by its baked occlusion (`lit`). Leaves do
/// not weather, gather grime or wear at their edges: none of those layers is paid for.
fn leaves_surface(token: Token, tone: f32, at: Place) -> Surface {
    var colour = token.colour * (0.82 + 0.36 * tone);
    colour *= 0.85 + 0.3 * local_fbm(at, 3.5, 2u);
    colour *= 0.88 + 0.24 * local_fbm(at, 40.0, 1u);
    return Surface(colour, token.roughness, token.metalness, 0.0);
}

/// A part's edge wear from its unit normal `n` (0 on its faces, 1 on its rounded edges): a box's
/// edges and corners, a cylinder's rims.
fn part_wear(n: vec3<f32>, grain: u32) -> f32 {
    var flat = max(abs(n.y), length(n.xz));
    if grain == GRAIN_SAWN {
        flat = max(abs(n.x), max(abs(n.y), abs(n.z)));
    }
    return smoothstep(0.03, 0.25, 1.0 - flat);
}

/// How high `local` (an instance's, turned by `rotation`, of `scale`) is above the instance's
/// lowest point along the world's up, over the models' height range (0–1).
fn part_height(local: vec3<f32>, rotation: vec4<f32>, scale: vec3<f32>) -> f32 {
    let up = globals.up.xyz;
    let lowest = abs(dot(rotate(rotation, vec3<f32>(1.0, 0.0, 0.0)), up)) * scale.x
        + abs(dot(rotate(rotation, vec3<f32>(0.0, 1.0, 0.0)), up)) * scale.y
        + abs(dot(rotate(rotation, vec3<f32>(0.0, 0.0, 1.0)), up)) * scale.z;
    return clamp((dot(rotate(rotation, local), up) + 0.5 * lowest) / HEIGHT_RANGE, 0.0, 1.0);
}

/// The axis a part's grain runs along: a cylinder's Y, a box's longest side.
fn part_axis(grain: u32, scale: vec3<f32>) -> u32 {
    if grain != GRAIN_SAWN {
        return 1u;
    }
    if scale.x >= scale.y && scale.x >= scale.z {
        return 0u;
    }
    return select(2u, 1u, scale.y >= scale.z);
}

/// A tapered part's normal (in its instance's space, scaled: `n / scale`): its side leans back
/// as its radius shrinks along Y.
fn tapered(n: vec3<f32>, scale: vec3<f32>, taper: f32) -> vec3<f32> {
    let slope = 0.5 * scale.x * (1.0 - taper) / max(scale.y, 1e-6);
    return vec3<f32>(n.x, n.y + length(n.xz) * slope, n.z);
}
