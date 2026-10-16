// Depth-only pass rendering terrain chunks into one sun shadow cascade.

@group(1) @binding(0) var<uniform> cascade_view_proj: mat4x4<f32>;

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) origin: vec4<f32>,
    @location(4) rotation: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    return cascade_view_proj * vec4<f32>(rotate(rotation, position) + origin.xyz, 1.0);
}
