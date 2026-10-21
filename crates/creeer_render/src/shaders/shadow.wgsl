// Depth-only pass rendering instanced meshes into one sun shadow cascade.

@group(1) @binding(0) var<uniform> cascade_view_proj: mat4x4<f32>;

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) rotation: vec4<f32>,
    @location(3) position_metal: vec4<f32>,
    @location(4) scale: vec3<f32>,
    @location(5) color: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let world = rotate(rotation, position * scale) + position_metal.xyz;
    return cascade_view_proj * vec4<f32>(world, 1.0);
}
