// HDR → display: exposure, Khronos PBR Neutral tonemapping, sRGB encoding (the swapchain is a
// non-sRGB format because egui requires one, so the encoding happens here).

@group(1) @binding(0) var hdr: texture_2d<f32>;
// x: automatic exposure (exposure.wgsl).
@group(1) @binding(1) var<storage, read> exposure: vec4<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the screen: (-1,-1), (3,-1), (-1,3).
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

/// Khronos PBR Neutral (2024): keeps base colours faithful up to ~0.8, then rolls off
/// highlights smoothly towards white.
fn pbr_neutral(input: vec3<f32>) -> vec3<f32> {
    let start_compression = 0.8 - 0.04;
    let desaturation = 0.15;
    let x = min(input.r, min(input.g, input.b));
    let offset = select(0.04, x - 6.25 * x * x, x < 0.08);
    var color = input - offset;
    let peak = max(color.r, max(color.g, color.b));
    if peak < start_compression {
        return color;
    }
    let d = 1.0 - start_compression;
    let new_peak = 1.0 - d * d / (peak + d - start_compression);
    color = color * (new_peak / peak);
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(color, vec3<f32>(new_peak), g);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // Automatic exposure times manual compensation (Lighting::exposure).
    let hdr_color = textureLoad(hdr, vec2<i32>(position.xy), 0).rgb * exposure.x * globals.params.x;
    let mapped = clamp(pbr_neutral(max(hdr_color, vec3<f32>(0.0))), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(linear_to_srgb(mapped), 1.0);
}
