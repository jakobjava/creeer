// Terrain chunks (ADR-0006): rock colours per vertex with a soft triplanar detail texture,
// lit by the sun as it reaches each point through the atmosphere (so the night side is dark
// and the terminator glows), by the sky for the sun's elevation there, with sun shadows and
// baked sky occlusion, and seen through the air between it and the camera (aerial
// perspective). Positions are camera-relative.

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(0) var transmittance_lut: texture_2d<f32>;
@group(2) @binding(1) var multiscatter_lut: texture_2d<f32>;
@group(2) @binding(2) var lut_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    // xyz: unit normal, w: openness to the sky (snorm).
    @location(1) normal: vec4<f32>,
    // rgb: sRGB base colour, a: roughness.
    @location(2) color: vec4<f32>,
    // xyz: unit normal of the next coarser level of detail.
    @location(3) coarse_normal: vec4<f32>,
    // Distance along `normal` to the next coarser level's surface (m).
    @location(4) morph: f32,
};

struct InstanceIn {
    // xyz: camera-relative chunk origin, w: chunk size (0: no blending).
    @location(5) origin: vec4<f32>,
    // The body's rotation (body-fixed → world), a quaternion.
    @location(6) rotation: vec4<f32>,
};

struct VertexOut {
    // Invariant: the prepass computes the same depth (`after_prepass_depth_state`).
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Chunk-local position: the detail texture repeats every 32 m, and every chunk origin
    // is a multiple of 32 m, so it is seamless and fixed to the ground.
    @location(2) local: vec3<f32>,
    @location(3) color: vec3<f32>,
    // x: roughness, y: openness.
    @location(4) material: vec2<f32>,
};

// Steps of the aerial-perspective ray march.
const AIR_STEPS: u32 = 10u;
// Period of the detail texture (m).
const DETAIL_PERIOD: f32 = 32.0;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(v: VertexIn, i: InstanceIn) -> VertexOut {
    let m = coarseness(v.position, i.origin, i.rotation);
    let world = rotate(i.rotation, v.position + v.normal.xyz * (v.morph * m)) + i.origin.xyz;
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = rotate(i.rotation, normalize(mix(v.normal.xyz, v.coarse_normal.xyz, m)));
    out.local = v.position;
    out.color = srgb_to_linear(v.color.rgb);
    out.material = vec2<f32>(v.color.a, clamp(v.normal.w, 0.0, 1.0));
    return out;
}

/// Hash of a lattice point, wrapped to `cells` per period, in [0, 1).
fn lattice(p: vec3<i32>, cells: i32) -> f32 {
    let q = vec3<u32>(((p % cells) + cells) % cells);
    var h = (q.x * 0x8da6b343u) ^ (q.y * 0xd8163841u) ^ (q.z * 0xcb1ab31fu);
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h = (h ^ (h >> 12u)) * 0x297a2d39u;
    return f32(h ^ (h >> 15u)) / 4294967296.0;
}

/// Value noise in [0, 1] with `cells` cells per period, smoothly interpolated in 3D: no
/// projection, so it never stretches on slopes.
fn value_noise(p: vec3<f32>, cells: i32) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    let w = f * f * (3.0 - 2.0 * f);
    let c = vec3<i32>(cell);
    let x00 = mix(lattice(c, cells), lattice(c + vec3<i32>(1, 0, 0), cells), w.x);
    let x10 = mix(lattice(c + vec3<i32>(0, 1, 0), cells), lattice(c + vec3<i32>(1, 1, 0), cells), w.x);
    let x01 = mix(lattice(c + vec3<i32>(0, 0, 1), cells), lattice(c + vec3<i32>(1, 0, 1), cells), w.x);
    let x11 = mix(lattice(c + vec3<i32>(0, 1, 1), cells), lattice(c + vec3<i32>(1, 1, 1), cells), w.x);
    return mix(mix(x00, x10, w.y), mix(x01, x11, w.y), w.z);
}

/// Soft detail in about [−1, 1]: three octaves (4 m, 1 m, 25 cm), each faded out where a
/// pixel covers more than half its cells (no shimmer on distant terrain).
fn detail(local: vec3<f32>) -> f32 {
    let p = local / DETAIL_PERIOD;
    let footprint = max(length(fwidth(local)), 1e-4);
    var sum = 0.0;
    var amplitude = 0.5;
    var cells = 8;
    for (var octave = 0; octave < 3; octave++) {
        let size = DETAIL_PERIOD / f32(cells);
        let fade = clamp(size / footprint - 1.0, 0.0, 1.0);
        if fade > 0.0 {
            sum += (value_noise(p * f32(cells), cells) * 2.0 - 1.0) * amplitude * fade;
        }
        amplitude *= 0.6;
        cells *= 4;
    }
    return sum;
}

/// Cosine-weighted sky radiance for a sun cosine `mu_s` against the local vertical
/// (table computed on the CPU from the atmosphere, `globals.sky_irradiance`).
fn sky_radiance(mu_s: f32) -> vec3<f32> {
    let x = clamp((mu_s + 0.2) / 1.2, 0.0, 1.0) * 15.0;
    let i = min(u32(floor(x)), 14u);
    return mix(globals.sky_irradiance[i].rgb, globals.sky_irradiance[i + 1u].rgb, x - f32(i));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = normalize(-in.world);
    let l = globals.sun_direction.xyz;
    let roughness = clamp(in.material.x, 0.045, 1.0);
    let openness = in.material.y;
    let base = in.color * (1.0 + 0.35 * detail(in.local));

    var sun = globals.sun_color.rgb;
    var sky = globals.sky_ambient.rgb;
    var up = globals.up.xyz;
    let sky_on = globals.atmo_geometry.w > 0.5;
    let t = length(in.world);
    let dir = in.world / max(t, 1e-6);
    let h0 = globals.atmo_geometry.z;
    let mu = dot(dir, globals.up.xyz);
    let nu = dot(dir, l);
    let mu_s0 = dot(globals.up.xyz, l);
    if sky_on {
        // Height and sun elevation at this point, cancellation-free (as the sky pass does).
        let r0 = planet_radius() + h0;
        let h = height_at(h0, mu, t);
        let mu_s = (r0 * mu_s0 + t * nu) / (planet_radius() + h);
        sun = globals.sun_irradiance.rgb * transmittance_to_space(max(h, 0.0), mu_s);
        sky = sky_radiance(mu_s);
        up = normalize(globals.up.xyz * r0 + in.world);
    }
    var visibility = 0.0;
    if dot(n, l) > 0.0 {
        visibility = sun_visibility(in.world, n);
    }
    let direct = direct_light(n, v, l, base, 0.0, DIELECTRIC_F0, roughness, sun, visibility);
    // Light bounced from the surroundings: the planet's albedo under sun and sky.
    let bounce = globals.ground_albedo.rgb / PI * (sun * max(dot(up, l), 0.0) + sky * PI);
    let occlusion = ambient_occlusion(in.clip.xy, base, 1.0);
    let ambient = ambient_light(n, v, up, base, 0.0, DIELECTRIC_F0, roughness, sky, bounce) * openness * occlusion;
    var color = direct + ambient;

    if sky_on {
        let span = atmosphere_span(h0, mu);
        if span.y > 0.0 {
            let air = inscatter(h0, mu, mu_s0, nu, span.x, min(span.y, t), AIR_STEPS);
            color = color * air.transmittance + air.radiance;
        }
    }
    return vec4<f32>(color, 1.0);
}
