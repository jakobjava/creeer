// Ground-truth ambient occlusion (GTAO; Jimenez et al. 2016), ported from Intel's XeGTAO
// (MIT, 2021; ADR-0026): what the three compute passes share. View space here is XeGTAO's:
// x right, y up, z the distance along the view (positive), in metres.

struct Ao {
    // The grid the occlusion is computed on (pixels), and one over it.
    size: vec2<f32>,
    pixel: vec2<f32>,
    // Screen position (0..1, y down) → view-space x and y at depth 1.
    ndc_to_view_mul: vec2<f32>,
    ndc_to_view_add: vec2<f32>,
    // ndc_to_view_mul × pixel: a pixel's size in view space at depth 1.
    mul_x_pixel: vec2<f32>,
    // The near plane (m): reversed-Z infinite depth d is near / z.
    near: f32,
    // The effect radius (m), XeGTAO's radius multiplier included.
    radius: f32,
    // The part of the radius over which samples fade out.
    falloff: f32,
    // Power on the final value (XeGTAO's fit to ray-traced references; 1: the integral).
    power: f32,
    // Power on the samples' distances (small crevices weigh more).
    distribution: f32,
    // Depth MIP chosen at log2(sample distance) minus this.
    mip_offset: f32,
    slices: f32,
    steps: f32,
    // MIP of the depth chain that is the grid's own (0 full, 1 half resolution).
    base_mip: f32,
    // 1: the 4×4 spatial filter on; 0: off.
    denoise_on: f32,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> ao: Ao;

const PI: f32 = 3.14159265;
// Depths beyond this (m) are the sky.
const FAR_DEPTH: f32 = 100000.0;
const DEPTH_MIPS: i32 = 5;

fn view_depth(ndc: f32) -> f32 {
    return min(ao.near / max(ndc, 1e-30), FAR_DEPTH);
}

fn view_position(screen: vec2<f32>, z: f32) -> vec3<f32> {
    return vec3<f32>((ao.ndc_to_view_mul * screen + ao.ndc_to_view_add) * z, z);
}
