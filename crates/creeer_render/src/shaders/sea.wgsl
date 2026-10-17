// Sea pass (F6.6): a planet's ocean at sea level, the datum, taken as the ellipsoid of its
// polar and equatorial radii (the equipotential differs by metres). Each pixel's view ray
// meets it exactly (camera-relative, the camera's height over it from f64 so the solution
// does not cancel); the depth written there lets land above sea level hide it. Lit by the
// sun through the air and the sky's radiance, Fresnel-reflecting the sky, the sun's glint
// on a gently rough sea, seen through the air between (aerial perspective).

@group(1) @binding(0) var transmittance_lut: texture_2d<f32>;
@group(1) @binding(1) var multiscatter_lut: texture_2d<f32>;
@group(1) @binding(2) var lut_sampler: sampler;

struct SeaOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

struct SeaFragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

const AIR_STEPS: u32 = 8u;
// Water's reflectance at normal incidence (n = 1.33), and the sea's roughness (a light swell).
const F0: f32 = 0.02;
const ROUGHNESS: f32 = 0.08;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> SeaOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    let ndc = uv * 2.0 - 1.0;
    var out: SeaOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

/// Cosine-weighted sky radiance for a sun cosine `mu_s` (as the terrain has it).
fn sea_sky(mu_s: f32) -> vec3<f32> {
    let x = clamp((mu_s + 0.2) / 1.2, 0.0, 1.0) * 15.0;
    let i = min(u32(floor(x)), 14u);
    return mix(globals.sky_irradiance[i].rgb, globals.sky_irradiance[i + 1u].rgb, x - f32(i));
}

@fragment
fn fs_main(in: SeaOut) -> SeaFragment {
    let polar = globals.sea.y;
    let equator = globals.sea.z;
    let height = globals.sea.w;
    if globals.sea.x < 0.5 || height <= 0.0 {
        discard;
    }
    let near = globals.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let d = normalize(near.xyz / near.w);
    // Stretched along the axis by equator/polar the ellipsoid is a sphere of the equatorial
    // radius: |q' + t·d'|² = a².
    let k = globals.sea_axis.xyz;
    let stretch = equator / polar - 1.0;
    let q = -globals.sea_center.xyz;
    let qs = q + k * (stretch * dot(q, k));
    let ds = d + k * (stretch * dot(d, k));
    let a = dot(ds, ds);
    let b = dot(qs, ds);
    let c = height * (2.0 * equator + height);
    let disc = b * b - a * c;
    if disc < 0.0 || b >= 0.0 {
        discard;
    }
    // The nearer root, without cancellation.
    let t = c / (-b + sqrt(disc));
    let p = d * t;
    let clip = globals.view_proj * vec4<f32>(p, 1.0);
    let hit = q + p;
    let squash = (equator / polar) * (equator / polar) - 1.0;
    let n = normalize(hit + k * (squash * dot(hit, k)));
    let v = -d;
    let l = globals.sun_direction.xyz;

    var sun = globals.sun_color.rgb;
    var sky = globals.sky_ambient.rgb;
    let sky_on = globals.atmo_geometry.w > 0.5;
    let mu_s = dot(n, l);
    if sky_on {
        sun = globals.sun_irradiance.rgb * transmittance_to_space(0.0, mu_s);
        sky = sea_sky(mu_s);
    }
    let cos_v = max(dot(n, v), 1e-4);
    let fresnel = F0 + (1.0 - F0) * pow(1.0 - cos_v, 5.0);
    // Light from within the water: its colour under the sun and the sky, what leaves it.
    let body = globals.sea_color.rgb / PI * (sun * max(mu_s, 0.0) + sky * PI) * (1.0 - fresnel);
    // The sky in it, and the sun's glint (GGX, Schlick–Smith).
    var glint = vec3<f32>(0.0);
    let n_l = dot(n, l);
    if n_l > 0.0 {
        let h = normalize(l + v);
        let n_h = max(dot(n, h), 0.0);
        let alpha = ROUGHNESS * ROUGHNESS;
        let denom = n_h * n_h * (alpha * alpha - 1.0) + 1.0;
        let distribution = alpha * alpha / (PI * denom * denom);
        let kk = alpha / 2.0;
        let geometry = n_l / (n_l * (1.0 - kk) + kk) * cos_v / (cos_v * (1.0 - kk) + kk);
        let f_h = F0 + (1.0 - F0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
        glint = sun * distribution * geometry * f_h / (4.0 * cos_v);
    }
    var color = body + sky * fresnel + glint;
    if sky_on {
        let h0 = globals.atmo_geometry.z;
        let mu = dot(d, globals.up.xyz);
        let span = atmosphere_span(h0, mu);
        if span.y > 0.0 {
            let air = inscatter(h0, mu, dot(globals.up.xyz, l), dot(d, l), span.x, min(span.y, t), AIR_STEPS);
            color = color * air.transmittance + air.radiance;
        }
    }
    var out: SeaFragment;
    out.color = vec4<f32>(color, 1.0);
    out.depth = clip.z / clip.w;
    return out;
}
