// Ambient occlusion as the scene's meshes and terrain read it (G1.3d, `ao_pass.rs`).

@group(3) @binding(0) var ao_map: texture_2d<f32>;
@group(3) @binding(1) var ao_linear: sampler;
// x, y: one over the screen's size (pixels); z: 1 when occlusion is on; w: the floor, the
// darkest it may make the ambient light (`art-style.md` §5).
@group(3) @binding(2) var<uniform> ao_view: vec4<f32>;

/// GTAO's multi-bounce fit (Jimenez et al. 2016, eq. 10): light bounced within a crease
/// lightens it more on bright surfaces, so white paint is not greyed like dark rock.
fn multi_bounce(v: f32, albedo: vec3<f32>) -> vec3<f32> {
    let a = 2.0404 * albedo - 0.3324;
    let b = -4.7951 * albedo + 0.6417;
    let c = 2.7552 * albedo + 0.6903;
    return max(vec3<f32>(v), ((v * a + b) * v + c) * v);
}

/// The ambient light's occlusion at the pixel `frag` (its position in pixels) of a surface
/// of `albedo`: the screen's GTAO with its bounces, no darker than the floor, or `baked`
/// where that is darker (the lesser of the two, as Filament combines a material's occlusion
/// with the screen's).
fn ambient_occlusion(frag: vec2<f32>, albedo: vec3<f32>, baked: f32) -> vec3<f32> {
    var screen = vec3<f32>(1.0);
    if ao_view.z > 0.5 {
        let v = textureSampleLevel(ao_map, ao_linear, frag * ao_view.xy, 0.0).r;
        screen = max(multi_bounce(v, albedo), vec3<f32>(ao_view.w));
    }
    return min(screen, vec3<f32>(baked));
}
