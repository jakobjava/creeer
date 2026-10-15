// Debug lines. Positions arrive camera-relative (see creeer_math::render_space); colours are
// authored in sRGB and converted to linear light for the HDR target.

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(position, 1.0);
    out.color = vec4<f32>(srgb_to_linear(color.rgb), color.a);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}
