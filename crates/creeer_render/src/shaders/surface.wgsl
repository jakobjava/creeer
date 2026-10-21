// Procedural surfaces: grainy, weathered, used (G1.3c, ADR-0026, docs/art-style.md §6). The
// layers in order, as art/preview.py previews them in Blender: the part's tone, the token's
// pattern, mottling and fine grain, age, grime, rain streaks, edge chips. Noise is value noise
// from a small volume, filtered by its footprint on screen: an octave finer than a pixel fades
// to its mean.

// One palette token (renderer/palette.rs, `TokenRaw`).
struct Token {
    colour: vec3<f32>,
    roughness: f32,
    metalness: f32,
    // 1: each player has their own colour, the instance's.
    per_player: f32,
    wears_to: u32,
    ages_to: u32,
    pattern: u32,
    // The years ageing takes; 0: it does not age.
    weathers_in: f32,
    pad: vec2<u32>,
};
@group(2) @binding(0) var<storage, read> palette: array<Token>;

const PATTERN_WOOD: u32 = 1u;
const PATTERN_PAINT: u32 = 2u;
const PATTERN_METAL: u32 = 3u;
const PATTERN_STONE: u32 = 4u;
const PATTERN_FABRIC: u32 = 5u;
const PATTERN_LEATHER: u32 = 6u;
// Heights are stored over this range (m; art/bake.py), the ground's splash reaches this high.
const HEIGHT_RANGE: f32 = 2.0;
const SPLASH: f32 = 0.35;
// Dirt gathers over about this many years outdoors (art-style.md §6b).
const GRIME_YEARS: f32 = 2.0;
// Grime's colour: dark earthy dirt (linear).
const GRIME: vec3<f32> = vec3<f32>(0.022, 0.018, 0.013);
// Wood: grain lines per metre across the grain, and how far (in lines) each wanders.
const GRAIN_LINES: f32 = 45.0;
const GRAIN_WANDER: f32 = 6.0;

// The noise volume (renderer/noise.rs): random values on a tiling lattice of NOISE_SIZE³.
@group(2) @binding(1) var noise_volume: texture_3d<f32>;
@group(2) @binding(2) var noise_sampler: sampler;
const NOISE_SIZE: f32 = 32.0;

/// Value noise in [0, 1], smooth (quintic) between lattice points: one linear fetch at the
/// fraction warped by the quintic (Quilez's improved texture interpolation).
fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return textureSampleLevel(noise_volume, noise_sampler, (i + u + 0.5) / NOISE_SIZE, 0.0).r;
}

/// Fractal noise of `octaves` octaves in [0, 1]. `footprint` is a pixel's size in `p`'s
/// units: an octave whose cells a pixel half covers fades to its mean, 0.5.
fn fbm(p: vec3<f32>, octaves: u32, footprint: f32) -> f32 {
    var sum = 0.0;
    var total = 0.0;
    var amplitude = 0.5;
    var q = p;
    var fp = footprint;
    for (var k = 0u; k < octaves; k++) {
        let seen = 1.0 - smoothstep(0.25, 0.5, fp);
        // Faded octaves are their mean without a fetch (and every finer one after them).
        var value = 0.5;
        if seen > 0.0 {
            value = mix(0.5, noise(q), seen);
        }
        sum += amplitude * value;
        total += amplitude;
        q = q * 2.03 + vec3<f32>(17.1, 31.7, 7.3);
        fp *= 2.03;
        amplitude *= 0.55;
    }
    return sum / total;
}

/// What a fragment knows about where it is, for the surface.
struct Place {
    // Position and normal in the model's own space (m, +Y up).
    local: vec3<f32>,
    normal: vec3<f32>,
    // Grain coordinates (m): along the part's grain, across it.
    grain: vec2<f32>,
    // A pixel's size: in the model's space (m), and in grain coordinates (u, v).
    footprint: f32,
    grain_footprint: vec2<f32>,
};

/// Noise over the grain coordinates, stretched by `s`.
fn grain_fbm(at: Place, s: vec2<f32>, seed: f32, octaves: u32) -> f32 {
    let p = vec3<f32>(at.grain * s, seed);
    return fbm(p, octaves, max(at.grain_footprint.x * s.x, at.grain_footprint.y * s.y));
}

/// Noise over the model's space at `scale` cells per metre.
fn local_fbm(at: Place, scale: f32, octaves: u32) -> f32 {
    return fbm(at.local * scale, octaves, at.footprint * scale);
}

/// The pattern's brightness factor, about 1.
fn pattern_of(kind: u32, at: Place) -> f32 {
    switch kind {
        case PATTERN_WOOD: {
            // Soft streaks, about 1:8 along the grain, and thin dark grain lines: the crests
            // of a sine across the grain, displaced by slow noise along it.
            let streaks = grain_fbm(at, vec2<f32>(8.0, 60.0), 0.0, 5u);
            let wander = grain_fbm(at, vec2<f32>(0.5, 12.0), 1.7, 3u) * GRAIN_WANDER;
            let crest = sin((at.grain.y * GRAIN_LINES + wander) * 6.2831853);
            let strength = grain_fbm(at, vec2<f32>(1.0, 20.0), 3.1, 2u) * 1.6;
            // Lines finer than a pixel fade to their mean darkening (about 0.07).
            let seen = 1.0 - smoothstep(0.25, 0.5, at.grain_footprint.y * GRAIN_LINES);
            let lines = mix(0.07, smoothstep(0.75, 1.0, crest) * strength, seen);
            return 0.92 + 0.16 * streaks - 0.28 * lines;
        }
        case PATTERN_PAINT: {
            return 0.96 + 0.08 * local_fbm(at, 260.0, 2u);
        }
        case PATTERN_METAL: {
            return 0.93 + 0.14 * grain_fbm(at, vec2<f32>(4.0, 300.0), 5.3, 3u);
        }
        case PATTERN_STONE: {
            return 0.88 + 0.12 * (local_fbm(at, 40.0, 4u) + local_fbm(at, 220.0, 2u));
        }
        case PATTERN_FABRIC: {
            return 0.94 + 0.12 * grain_fbm(at, vec2<f32>(400.0, 400.0), 7.7, 1u);
        }
        case PATTERN_LEATHER: {
            return 0.9 + 0.2 * local_fbm(at, 180.0, 2u);
        }
        default: {
            return 1.0;
        }
    }
}

/// A surface as lit: colour, roughness, metalness.
struct Surface {
    colour: vec3<f32>,
    roughness: f32,
    metalness: f32,
};

/// The surface of a model's vertex of `token`, with its baked `occlusion`, `wear`, part `tone`
/// and `height`, on an object `exposure` years weathered with `used` wear from use; `accent` is
/// its instance's colour (a player's, for per-player tokens).
fn model_surface(
    token_id: u32, accent: vec3<f32>, occlusion: f32, wear: f32, tone: f32, height: f32,
    exposure: f32, used: f32, at: Place,
) -> Surface {
    let token = palette[token_id];
    var colour = select(token.colour, accent, token.per_player > 0.5);
    let pattern = pattern_of(token.pattern, at);
    // 1-3: the part's tone (±18 %), its pattern, mottling (±15 %) and fine grain (±10 %).
    colour *= (0.82 + 0.36 * tone) * pattern;
    colour *= 0.85 + 0.30 * local_fbm(at, 3.5, 3u);
    colour *= 0.90 + 0.20 * local_fbm(at, 420.0, 1u);

    // 5 (first: ageing follows the dirt): the ground's splash up the sides, none on what faces
    // up, all over what faces down; dirt in creases; both gathering as the object ages.
    let object_age = clamp(exposure / GRIME_YEARS, 0.0, 1.0);
    let rise = height * HEIGHT_RANGE;
    var splash = pow(1.0 - smoothstep(0.0, SPLASH, rise), 1.5) * (1.0 - max(at.normal.y, 0.0));
    splash = max(splash, smoothstep(0.4, 0.6, -at.normal.y));
    var grime = max(splash, 1.0 - occlusion) * (local_fbm(at, 9.0, 4u) + 0.5);
    grime = clamp(grime * (0.1 + 0.9 * object_age), 0.0, 1.0);

    // 4: age, at the token's own rate; each part a little older or younger (its tone, ±0.3).
    // Wood greys all over and more in patches; metal rusts in patches, where dirt holds water.
    if token.weathers_in > 0.0 {
        let age = clamp(exposure / token.weathers_in + (tone - 0.5) * 0.6, 0.0, 1.0);
        let patches = local_fbm(at, 2.5, 5u) * 0.7 + grime * 0.5;
        let aged = smoothstep(-0.08, 0.08, patches - (1.25 - age));
        var amount = aged * 0.9;
        if token.pattern == PATTERN_WOOD {
            amount = max(age * 0.95, aged);
        }
        colour = mix(colour, palette[token.ages_to].colour * pattern, amount);
    }
    colour = mix(colour, GRIME, grime * 0.9);
    // A film of dirt over what is old, and rain streaks down the sides.
    let film_age = clamp(object_age + (tone - 0.5) * 0.6, 0.0, 1.0);
    colour *= 1.0 - 0.3 * film_age;
    let streak = smoothstep(0.5, 0.72, fbm(at.local * vec3<f32>(50.0, 1.5, 50.0), 3u, at.footprint * 50.0));
    colour *= 1.0 - streak * (1.0 - abs(at.normal.y)) * film_age * 0.3;

    // 6: wear along edges, in patches a few centimetres long (never a continuous line,
    // art-style.md §5), growing with age and use. It reveals the token beneath, with its own
    // finish (painted steel's bare metal, timber's cut wood). What is revealed weathers and
    // dirties too, lagging the surface: fresh on a new object, mostly its surface on an old one.
    let worn = clamp(max(object_age, used) * 1.2 + 0.1, 0.0, 1.0);
    let chips = smoothstep(0.58, 0.72, wear * local_fbm(at, 9.0, 3u)) * worn;
    let fresh = chips * (0.7 - 0.55 * film_age);
    let beneath = palette[token.wears_to];
    colour = mix(colour, beneath.colour * pattern, fresh);

    let roughness = mix(token.roughness + grime * 0.15, beneath.roughness, fresh);
    return Surface(colour, clamp(roughness, 0.0, 1.0), mix(token.metalness, beneath.metalness, fresh));
}

/// A procedural mesh's surface: its instance's material with mottling and fine grain only (it
/// carries no bake).
fn plain_surface(colour: vec3<f32>, roughness: f32, metalness: f32, at: Place) -> Surface {
    var c = colour * (0.92 + 0.16 * local_fbm(at, 3.5, 3u));
    c *= 0.95 + 0.10 * local_fbm(at, 420.0, 1u);
    return Surface(c, roughness, metalness);
}
