// GTAO, pass 1 (XeGTAO's PrefilterDepths16x16): the scene's depth, sample 0 of each pixel,
// as view-space depth in five MIPs, each a weighted mean of its four texels that leaves out
// what lies far behind the nearest (so a thin rod in front stays in the coarse MIPs).

@group(0) @binding(1) var depth: texture_depth_multisampled_2d;
@group(0) @binding(2) var mip0: texture_storage_2d<r32float, write>;
@group(0) @binding(3) var mip1: texture_storage_2d<r32float, write>;
@group(0) @binding(4) var mip2: texture_storage_2d<r32float, write>;
@group(0) @binding(5) var mip3: texture_storage_2d<r32float, write>;
// WebGPU allows four storage textures a stage: the last MIP is a small pass of its own.
@group(0) @binding(6) var mip3_read: texture_2d<f32>;
@group(0) @binding(7) var mip4: texture_storage_2d<r32float, write>;

var<workgroup> scratch: array<array<f32, 8>, 8>;

fn mip_filter(d0: f32, d1: f32, d2: f32, d3: f32) -> f32 {
    let deepest = max(max(d0, d1), max(d2, d3));
    // XeGTAO: 0.75 of the radius, found empirically.
    let radius = 0.75 * ao.radius;
    let range = ao.falloff * radius;
    let fade_from = radius * (1.0 - ao.falloff);
    let mul = -1.0 / range;
    let add = fade_from / range + 1.0;
    let w = clamp((vec4<f32>(deepest) - vec4<f32>(d0, d1, d2, d3)) * mul + add, vec4<f32>(0.0), vec4<f32>(1.0));
    return dot(w, vec4<f32>(d0, d1, d2, d3)) / (w.x + w.y + w.z + w.w);
}

fn load(p: vec2<i32>) -> f32 {
    let last = vec2<i32>(textureDimensions(depth)) - 1;
    return view_depth(textureLoad(depth, clamp(p, vec2<i32>(0), last), 0));
}

@compute @workgroup_size(8, 8)
fn prefilter(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    let base = vec2<i32>(id.xy);
    let p = base * 2;
    let d0 = load(p);
    let d1 = load(p + vec2<i32>(1, 0));
    let d2 = load(p + vec2<i32>(0, 1));
    let d3 = load(p + vec2<i32>(1, 1));
    textureStore(mip0, p, vec4<f32>(d0));
    textureStore(mip0, p + vec2<i32>(1, 0), vec4<f32>(d1));
    textureStore(mip0, p + vec2<i32>(0, 1), vec4<f32>(d2));
    textureStore(mip0, p + vec2<i32>(1, 1), vec4<f32>(d3));

    let m1 = mip_filter(d0, d1, d2, d3);
    textureStore(mip1, base, vec4<f32>(m1));
    scratch[local.x][local.y] = m1;
    workgroupBarrier();

    let t = local.xy;
    if all(t % 2u == vec2<u32>(0u)) {
        let m2 = mip_filter(
            scratch[t.x][t.y], scratch[t.x + 1u][t.y], scratch[t.x][t.y + 1u], scratch[t.x + 1u][t.y + 1u],
        );
        textureStore(mip2, base / 2, vec4<f32>(m2));
        scratch[t.x][t.y] = m2;
    }
    workgroupBarrier();

    if all(t % 4u == vec2<u32>(0u)) {
        let m3 = mip_filter(
            scratch[t.x][t.y], scratch[t.x + 2u][t.y], scratch[t.x][t.y + 2u], scratch[t.x + 2u][t.y + 2u],
        );
        textureStore(mip3, base / 4, vec4<f32>(m3));
    }
}

/// MIP 4 from MIP 3, one texel a thread.
@compute @workgroup_size(8, 8)
fn last_mip(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy) * 2;
    let last = vec2<i32>(textureDimensions(mip3_read)) - 1;
    let d0 = textureLoad(mip3_read, min(p, last), 0).r;
    let d1 = textureLoad(mip3_read, min(p + vec2<i32>(1, 0), last), 0).r;
    let d2 = textureLoad(mip3_read, min(p + vec2<i32>(0, 1), last), 0).r;
    let d3 = textureLoad(mip3_read, min(p + vec2<i32>(1, 1), last), 0).r;
    textureStore(mip4, vec2<i32>(id.xy), vec4<f32>(mip_filter(d0, d1, d2, d3)));
}
