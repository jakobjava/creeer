// Skinning (G1.3e, ADR-0026 decision 6): each character's rest vertices, at the level it is
// drawn, moved by its bones (the ragdoll's segments) with dual quaternions (Kavan et al. 2007),
// into a buffer the mesh, prepass and shadow pipelines draw as a plain mesh. One dispatch a
// frame: x over a character's vertices, y over the characters. `crate::skin` is the same
// arithmetic on the CPU, which the tests hold this to.

// A rest vertex, 8 words (`skin_layout.rs`): position (3), normal (16-bit, 2), material, its
// four bones (a byte each), their weights (8-bit, summing to 255).
const REST_WORDS: u32 = 8u;
// A skinned vertex, 6 words: position (3), normal (16-bit, 2), material. What does not move
// (tone, height, grain) is drawn from the rest data.
const OUT_WORDS: u32 = 6u;
// A per-player token's slot in `slots`: none.
const NO_SLOT: u32 = 255u;

struct Job {
    // The character's level's first rest vertex, and how many.
    rest_first: u32,
    count: u32,
    // Its first skinned vertex, and its first bone in `bones`.
    out_first: u32,
    bone_first: u32,
    // Its own palette entries for the per-player tokens: the jacket's colour, its skin and its
    // hair, from here on.
    slot_base: u32,
    // Three words, not a vec3 (aligned to 16, it would make the struct 48 bytes, not 32).
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<storage, read> rest: array<u32>;
// Two a bone: its rotation (x, y, z, w), then its dual part; camera-relative.
@group(0) @binding(1) var<storage, read> bones: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> jobs: array<Job>;
// For each palette token: which per-player slot it takes (0–2), or NO_SLOT.
@group(0) @binding(3) var<storage, read> slots: array<u32>;
@group(0) @binding(4) var<storage, read_write> skinned: array<u32>;

fn word(at: u32) -> f32 {
    return bitcast<f32>(rest[at]);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let job = jobs[id.y];
    if id.x >= job.count {
        return;
    }
    let r = (job.rest_first + id.x) * REST_WORDS;
    let p = vec3<f32>(word(r), word(r + 1u), word(r + 2u));
    let n = vec3<f32>(unpack2x16snorm(rest[r + 3u]), unpack2x16snorm(rest[r + 4u]).x);
    let packed = rest[r + 6u];
    let weights = unpack4x8unorm(rest[r + 7u]);

    // The bones' dual quaternions blended, each on the first one's side (q and −q are one
    // rotation), then made unit.
    let first = job.bone_first + (packed & 0xffu);
    let pivot = bones[2u * first];
    var real = vec4<f32>(0.0);
    var dual = vec4<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        let w = weights[k];
        if w == 0.0 {
            continue;
        }
        let bone = job.bone_first + ((packed >> (8u * k)) & 0xffu);
        let b_real = bones[2u * bone];
        let b_dual = bones[2u * bone + 1u];
        let signed = select(w, -w, dot(b_real, pivot) < 0.0);
        real += b_real * signed;
        dual += b_dual * signed;
    }
    let norm = length(real);
    real /= norm;
    dual /= norm;
    let rv = real.xyz;
    let moved = p + 2.0 * cross(rv, cross(rv, p) + real.w * p)
        + 2.0 * (real.w * dual.xyz - dual.w * rv + cross(rv, dual.xyz));
    let turned = n + 2.0 * cross(rv, cross(rv, n) + real.w * n);

    var material = rest[r + 5u];
    let token = material & 0xffffu;
    if token < arrayLength(&slots) {
        let slot = slots[token];
        if slot != NO_SLOT {
            material = (material & 0xffff0000u) | (job.slot_base + slot);
        }
    }

    let o = (job.out_first + id.x) * OUT_WORDS;
    skinned[o] = bitcast<u32>(moved.x);
    skinned[o + 1u] = bitcast<u32>(moved.y);
    skinned[o + 2u] = bitcast<u32>(moved.z);
    skinned[o + 3u] = pack2x16snorm(turned.xy);
    skinned[o + 4u] = pack2x16snorm(vec2<f32>(turned.z, 0.0));
    skinned[o + 5u] = material;
}
