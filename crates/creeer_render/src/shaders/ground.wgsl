// The ground's surface (G2.11, ADR-0040), for `terrain.wgsl`: the rock's own colour mottled
// and speckled, the ground's layers (grass, moss, litter: `creeer_art::ground`) laid over it in
// patches, each with a fine grain of its own, and a fine relief for the sun to catch. All in
// the ground's pattern coordinates (body-fixed metres, continuous across chunks: terrain.rs's
// `pattern_offset`), from procedural noise only (art-style.md §6: no images). Detail finer
// than a pixel fades to its average, so nothing shimmers far away.

// Must match `terrain::PATTERN_PERIOD`.
const PATTERN_PERIOD: f32 = 1024.0;
// Features smaller than a metre repeat every this many metres (it divides the pattern's
// period), so their coordinates stay small and exact in f32.
const FINE_PERIOD: f32 = 8.0;

// The noise volume (`renderer/noise.rs`): random values on a tiling lattice of NOISE_CELLS³,
// bound with the atmosphere's tables (`sky_pass.rs`).
@group(2) @binding(3) var ground_noise: texture_3d<f32>;
@group(2) @binding(4) var ground_noise_sampler: sampler;
const NOISE_CELLS: f32 = 32.0;

/// Noise in [0, 1] with features about `size` metres across at pattern point `local`; `salt`
/// sets one noise apart from another of the same size. One linear fetch from the noise volume
/// at its fraction warped by a quintic (Quilez's improved texture interpolation): a smooth value
/// noise, in 3D so it never stretches on slopes. The volume tiles a whole number of times per
/// period, so the noise is continuous where the pattern wraps; `size` is rounded to make it so.
/// `ground_pattern.rs` mirrors it.
fn noise_at(local: vec3<f32>, size: f32, salt: f32) -> f32 {
    var period = PATTERN_PERIOD;
    var p = local;
    if size * NOISE_CELLS <= FINE_PERIOD {
        period = FINE_PERIOD;
        p = local - FINE_PERIOD * floor(local / FINE_PERIOD);
    }
    let tiles = max(round(period / (size * NOISE_CELLS)), 1.0);
    let q = p * (tiles * NOISE_CELLS / period) + vec3<f32>(salt * 7.0, salt * 3.0, salt * 5.0);
    let i = floor(q);
    let f = q - i;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return textureSampleLevel(ground_noise, ground_noise_sampler, (i + u + 0.5) / NOISE_CELLS, 0.0).r;
}

/// How much of a feature `size` metres across survives a pixel covering `footprint` metres
/// (1 sharp, 0 averaged away).
fn kept(size: f32, footprint: f32) -> f32 {
    return clamp(size / footprint - 1.0, 0.0, 1.0);
}

/// Noise in about [−1, 1] of features `size` across, faded to 0 where the pixel is larger.
fn band(local: vec3<f32>, size: f32, salt: f32, footprint: f32) -> f32 {
    let fade = kept(size, footprint);
    if fade <= 0.0 {
        return 0.0;
    }
    return (noise_at(local, size, salt) * 2.0 - 1.0) * fade;
}

/// The ground at one point: its colour (linear) and a height (m) for its fine relief.
struct Ground {
    colour: vec3<f32>,
    height: f32,
};

/// The ground at pattern point `local`, of rock colour `rock` (linear), with the layers'
/// `cover` (the share of the ground each covers here), seen by a pixel `footprint` metres
/// across.
fn ground_at(local: vec3<f32>, rock: vec3<f32>, cover: vec4<f32>, footprint: f32) -> Ground {
    // The rock: broad tones (±6 % at 40 m, so distant land is not one flat colour), mottling
    // (±12 % at 5 m and 1.2 m) and speckle at two scales (±9 %: crumbs, grains).
    let broad = band(local, 40.0, 0.0, footprint);
    let mottle = 0.6 * band(local, 5.0, 1.0, footprint) + 0.4 * band(local, 1.2, 2.0, footprint);
    let speckle = 0.6 * band(local, 0.07, 3.0, footprint) + 0.4 * band(local, 0.018, 4.0, footprint);
    var out: Ground;
    out.colour = rock * (1.0 + 0.06 * broad + 0.12 * mottle + 0.09 * speckle);
    out.height = 0.004 * speckle + 0.01 * mottle;
    for (var k = 0u; k < 4u; k++) {
        let amount = cover[k];
        if amount < 0.004 {
            continue;
        }
        let layer = globals.ground_layers[k];
        let size = layer.w;
        let grain = globals.ground_grain[k];
        let salt = 10.0 + f32(k) * 3.0;
        // Patches: where the noise rises over a threshold set by the share covered (the noise
        // is about 0.5 ± 0.15, so `amount` of the ground lies above it), with edges a little
        // soft, and wider where a pixel spans more.
        // Three octaves, the finest breaking the patches' edges into tufts and cushions.
        let n = 0.55 * noise_at(local, size, salt) + 0.3 * noise_at(local, 0.31 * size, salt + 1.0)
            + 0.15 * (band(local, 0.07 * size, salt + 5.0, footprint) * 0.5 + 0.5);
        let threshold = 0.5 + (0.5 - amount) * 0.6;
        let soft = 0.03 + footprint / size;
        var mask = smoothstep(threshold - soft, threshold + soft, n) * smoothstep(0.0, 0.1, amount);
        // Patches smaller than a pixel: their share instead.
        mask = mix(amount, mask, kept(size, footprint)) * globals.ground_density[k];
        // Its grain: blades, cushions, needles (±22 %), and its own patchy tone (±10 %).
        let fine = 0.6 * band(local, grain, salt + 2.0, footprint) + 0.4 * band(local, 3.0 * grain, salt + 3.0, footprint);
        let tone = band(local, 0.5 * size, salt + 4.0, footprint);
        let colour = layer.rgb * (1.0 + 0.22 * fine + 0.1 * tone);
        out.colour = mix(out.colour, colour, mask);
        out.height += mask * (0.006 + 0.003 * fine);
    }
    return out;
}

// A closed forest's share of the ground under crowns, seen from above.
const CROWN_COVER: f32 = 0.85;
// A canopy seen from afar is darker than its leaves: the crowns shade one another and the
// gaps between them (conifer forests' albedo is about half their needles' reflectance).
const CANOPY_SHADE: f32 = 0.55;

/// The ground `g` at pattern point `local`, `distance` metres away, where a forest grows
/// `forest` fully: beyond where trees are drawn (`globals.ground_canopy.w`), it shows their
/// crowns as one sees a forest from afar, crown by crown (5 m) and stand by stand (60 m).
fn canopy_over(g: Ground, local: vec3<f32>, forest: f32, distance: f32, footprint: f32) -> Ground {
    let start = globals.ground_canopy.w;
    let k = forest * CROWN_COVER * smoothstep(0.65 * start, 1.05 * start, distance);
    if start <= 0.0 || k <= 0.0 {
        return g;
    }
    let crowns = band(local, 5.0, 20.0, footprint);
    let stands = band(local, 60.0, 21.0, footprint);
    let canopy = globals.ground_canopy.rgb * CANOPY_SHADE * (1.0 + 0.3 * crowns + 0.12 * stands);
    var out = g;
    out.colour = mix(g.colour, canopy, k);
    out.height = mix(g.height, 0.0, k);
    return out;
}

/// The normal `n` turned by the relief `height` (m) across the pixel (Mikkelsen 2010, "Bump
/// mapping unparametrized surfaces on the GPU": the surface gradient from screen derivatives).
/// Called in uniform control flow.
fn relief(n: vec3<f32>, world: vec3<f32>, height: f32) -> vec3<f32> {
    let dpx = dpdx(world);
    let dpy = dpdy(world);
    let dhx = dpdx(height);
    let dhy = dpdy(height);
    let r1 = cross(dpy, n);
    let r2 = cross(n, dpx);
    let det = dot(dpx, r1);
    let gradient = sign(det) * (dhx * r1 + dhy * r2);
    let turned = abs(det) * n - gradient;
    let len = length(turned);
    return select(n, turned / len, len > 1e-12);
}
