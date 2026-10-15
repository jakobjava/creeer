// Automatic exposure (eye adaptation), two compute passes over the resolved HDR image:
// 1. `histogram`: count pixels per log2-luminance bin (shared memory, then global atomics).
// 2. `adapt`: average the histogram, derive the target exposure and move the current
//    exposure towards it smoothly; clear the histogram for the next frame.

@group(1) @binding(0) var hdr: texture_2d<f32>;
@group(1) @binding(1) var<storage, read_write> histogram: array<atomic<u32>, 256>;
// x: current exposure, y: average luminance, z: target exposure.
@group(1) @binding(2) var<storage, read_write> exposure: vec4<f32>;

// Luminance range covered by the histogram (log2): about 1e-4 … 64.
const MIN_LOG2: f32 = -13.0;
const LOG2_RANGE: f32 = 19.0;
// Exposure at the reference luminance (a clear noon scene in renderer units).
const REFERENCE_LUMINANCE: f32 = 0.12;
const REFERENCE_EXPOSURE: f32 = 1.5;
// Partial adaptation: darker scenes are brightened, but less than fully (exponent < 1), so
// dusk still looks like dusk. Mimics incomplete, roughly logarithmic eye adaptation.
const ADAPTATION_STRENGTH: f32 = 0.75;
const MIN_EXPOSURE: f32 = 0.25;
const MAX_EXPOSURE: f32 = 8.0;
// Adaptation time constants (s): getting brighter is fast, adjusting to the dark slower.
const TAU_BRIGHTER_SCENE: f32 = 0.5;
const TAU_DARKER_SCENE: f32 = 1.5;

var<workgroup> local_bins: array<atomic<u32>, 256>;

fn luminance_bin(color: vec3<f32>) -> u32 {
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    if luminance < 1e-5 {
        return 0u;
    }
    let t = clamp((log2(luminance) - MIN_LOG2) / LOG2_RANGE, 0.0, 1.0);
    return u32(t * 254.0 + 1.0);
}

@compute @workgroup_size(16, 16)
fn histogram_main(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
) {
    atomicStore(&local_bins[local], 0u);
    workgroupBarrier();
    let size = textureDimensions(hdr);
    if id.x < size.x && id.y < size.y {
        let color = textureLoad(hdr, vec2<i32>(id.xy), 0).rgb;
        atomicAdd(&local_bins[luminance_bin(color)], 1u);
    }
    workgroupBarrier();
    atomicAdd(&histogram[local], atomicLoad(&local_bins[local]));
}

var<workgroup> weighted: array<f32, 256>;
var<workgroup> counts: array<f32, 256>;

@compute @workgroup_size(256)
fn adapt_main(@builtin(local_invocation_index) bin: u32) {
    let count = f32(atomicLoad(&histogram[bin]));
    // Bin 0 holds black pixels; they would drag the average to −∞ in log space.
    let valid = select(count, 0.0, bin == 0u);
    let log2_luminance = (f32(bin) - 1.0) / 254.0 * LOG2_RANGE + MIN_LOG2;
    weighted[bin] = valid * log2_luminance;
    counts[bin] = valid;
    atomicStore(&histogram[bin], 0u);
    workgroupBarrier();
    // Tree reduction over the 256 bins.
    for (var stride = 128u; stride > 0u; stride >>= 1u) {
        if bin < stride {
            weighted[bin] += weighted[bin + stride];
            counts[bin] += counts[bin + stride];
        }
        workgroupBarrier();
    }
    if bin == 0u {
        let average = exp2(weighted[0] / max(counts[0], 1.0));
        let relative = REFERENCE_LUMINANCE / max(average, 1e-6);
        let target_exposure = clamp(
            REFERENCE_EXPOSURE * pow(relative, ADAPTATION_STRENGTH),
            MIN_EXPOSURE,
            MAX_EXPOSURE,
        );
        let dt = globals.params.z;
        var current = exposure.x;
        // params.w = 1: no history (first frame, screenshots) → jump straight to the target.
        if globals.params.w > 0.5 || current <= 0.0 {
            current = target_exposure;
        } else {
            let tau = select(TAU_DARKER_SCENE, TAU_BRIGHTER_SCENE, target_exposure < current);
            current = mix(current, target_exposure, 1.0 - exp(-dt / tau));
        }
        exposure = vec4<f32>(current, average, target_exposure, 0.0);
    }
}
