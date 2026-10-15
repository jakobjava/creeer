// Engineering overlay palettes; must match heatmap.rs (which also draws the legend).

/// Polynomial fit of matplotlib's viridis (sRGB).
fn viridis(t: f32) -> vec3<f32> {
    let c0 = vec3<f32>(0.27772733, 0.0054073445, 0.3340998);
    let c1 = vec3<f32>(0.10509304, 1.4046135, 1.3845902);
    let c2 = vec3<f32>(-0.33086183, 0.21484756, 0.09509516);
    let c3 = vec3<f32>(-4.6342306, -5.799101, -19.332441);
    let c4 = vec3<f32>(6.22827, 14.179933, 56.69055);
    let c5 = vec3<f32>(4.776385, -13.745145, -65.35303);
    let c6 = vec3<f32>(-5.435456, 4.6458526, 26.312435);
    let c = c0 + t * (c1 + t * (c2 + t * (c3 + t * (c4 + t * (c5 + t * c6)))));
    return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
}

/// Moreland's cool-warm with a neutral grey centre (sRGB).
fn cool_warm(t: f32) -> vec3<f32> {
    let cool = vec3<f32>(0.230, 0.299, 0.754);
    let neutral = vec3<f32>(0.865, 0.865, 0.865);
    let warm = vec3<f32>(0.706, 0.016, 0.150);
    if t < 0.5 {
        return mix(cool, neutral, t * 2.0);
    }
    return mix(neutral, warm, (t - 0.5) * 2.0);
}

fn heatmap_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

/// Linear overlay colour for a measured value (globals.heatmap: on, min, max, palette).
fn heatmap_color(value: f32) -> vec3<f32> {
    let span = max(globals.heatmap.z - globals.heatmap.y, 1e-6);
    let t = clamp((value - globals.heatmap.y) / span, 0.0, 1.0);
    let srgb = select(viridis(t), cool_warm(t), globals.heatmap.w > 0.5);
    return heatmap_srgb_to_linear(srgb);
}
