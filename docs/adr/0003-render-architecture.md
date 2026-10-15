# ADR-0003: Render architecture

- **Status:** Accepted (2026-10-15)
- **Context:** F1 (renderer core). Requirements: a stylised Factorio/Astroneer look, planetary scale without jitter, 10,000+ instanced parts at 60 FPS on a GTX 1660-class GPU, and portability across Vulkan, DX12 and Metal (plan.md §5).

## Decision

**Forward rendering with MSAA into an HDR target, followed by tonemapping.**

```
[shadow cascades] → [scene pass: HDR Rgba16Float, 4× MSAA, reversed-Z depth]
                  → [tonemap: Khronos PBR Neutral → sRGB, into the swapchain]
                  → [UI: egui]
```

- **Forward, not deferred.** The scene has one sun plus few local lights. Forward rendering keeps MSAA cheap and correct, which gives crisp edges for the stylised look and the thin beams of structures. It also handles transparency (glass, overlays) without a separate path. Deferred rendering only pays off with many lights; we can move to clustered forward later if needed.
- **HDR + tonemapping.** Sunlight in space, a dark cave and a glowing furnace all need range beyond 0–1. The *Khronos PBR Neutral* tonemapper keeps base colours close to what they are authored as, which suits a colour language where every material has a recognisable colour.
- **Camera-relative everything** (ADR-0001): instance transforms are built on the CPU in f64, then converted to f32 relative to the camera. Reversed-Z with an infinite far plane.
- **One mesh arena.** All meshes share one vertex buffer and one index buffer; a draw is (mesh range, instance range). Instances are uploaded per frame, grouped by mesh: 10,000 instances × 64 bytes = 640 kB per frame, trivial for PCIe. Static geometry may move to persistent GPU buffers later if profiling asks for it.
- **"PBR-lite" materials:** base colour, roughness and metalness per instance. GGX specular plus Lambert diffuse, hemispheric sky/ground ambient, and sun shadows. No textures yet; the style comes from shape and colour.
- **Culling:** CPU frustum culling per instance first (cheap at 10,000 instances, measured). GPU culling with `draw_indirect` follows when instance counts demand it. `multi_draw_indirect` is avoided because of portability.
- **Verification:** `creeer_client --screenshot <png>` renders a fixed scene and saves it. Claude reviews the images during development; automated image tests follow once CI has a software GPU.

## Consequences

- Debug lines draw into the HDR target too, so their sRGB colours are converted to linear in the shader.
- MSAA 4× on Rgba16Float costs memory (1080p is about 32 MB for colour + depth); a quality setting can lower it.

## Addendum (F1): shadows and atmosphere

**Sun shadows:** 4 cascades with practical split distances (λ = 0.8) up to 250 m. Resolution is a setting (`ShadowQuality`): High 4096² per cascade (default; texels 1 / 2.4 / 5.6 / 17.5 cm; 256 MB), Medium 2048² (64 MB, for 4 GB graphics cards), Low 1024².
- Each cascade fits a bounding sphere around its frustum slice, so rotating the camera never changes the cascade size.
- Each cascade centre is snapped to a world-fixed texel grid **in f64**, so shadows don't shimmer when the camera moves (tested).
- Shadow casters are culled per cascade, including objects outside the camera view.
- Filtering uses 4 bilinear comparison taps at ±½ texel, a 3×3-texel tent filter, with a normal offset of 1 texel plus slope-scaled bias against acne. The last 15 % of each cascade blends into the next, so resolution changes are gradual instead of lines sweeping along with the camera (the visible "flicker" reported in testing). Casters smaller than one texel are skipped in that cascade.
- Measured in testing: λ = 0.85 sharpened the near range but made shadows 70–100 m away *coarser*. Crop comparisons (`creeer_tools image-crop`) now back every shadow change.
- Measured cost: High quality costs no more than the earlier 2048² setup (1.67 ms per full frame), because the 4-tap filter is cheaper than the old 9-tap one.

**Atmosphere:** physically based, following Hillaire 2020 (the Unreal Engine sky model).
- Single Rayleigh + Mie scattering along each view ray, plus multiple scattering from a 32² table (Ψ = L₂ / (1 − f_ms)). Transmittance comes from a 256×64 table with Bruneton's horizon-aware parameterisation.
- **Both tables are built on the CPU in f64** (deterministic, unit-tested against direct integration) and uploaded as Rgba16Float.
- The same CPU model computes the object lighting (sun colour after extinction, sky and ground ambient), so objects and sky can't disagree.
- All geometry uses the height above ground instead of radii, because the textbook ray–sphere formulas cancel catastrophically on a 500 km planet.
- Measured cost: about 0.26 ms at 1080p.

Findings during implementation:
- Single scattering alone makes a hazy horizon *darker* than the zenith. Multiple scattering is required for the bright, white horizon of real skies (now tested).
- Commonly quoted aerosol coefficients (optical depth ≈ 0.005) describe exceptionally clean air and make the sky several times too dark. We use optical depth 0.1 at 550 nm (clean continental air).
- With a fixed exposure, sunsets are physically correct but very dark, because illuminance drops 10–100×. **Automatic exposure (eye adaptation)** is required and is added to F1.

Not yet done: aerial perspective on geometry (haze over distant terrain) arrives with planetary terrain in F3. A sky-view table would add speed if the per-pixel ray march ever becomes a bottleneck.

## Addendum (F1): automatic exposure and engineering overlays

**Automatic exposure (eye adaptation):** two compute passes over the resolved HDR image.
- A 256-bin log-luminance histogram, collected in workgroup memory and then merged.
- A reduction that produces the log-average luminance.
- Exposure = 1.5 · (0.12 / L_avg)^0.75, clamped to [0.25, 8], and approached smoothly: τ = 0.5 s when the scene gets brighter, 1.5 s when it gets darker.
- Screenshots and the first frame jump straight to the target.

Findings:
- Full adaptation (exponent 1) makes a sunset exactly as bright as noon, which reads wrong. The exponent of 0.75 models incomplete adaptation, so dusk stays recognisably darker. A GPU test checks that the sunset gets brighter but stays darker than day.
- Black pixels (space) are excluded from the average; otherwise a planet seen from orbit would be wildly overexposed.

**Engineering overlays (`Heatmap`):** each instance can carry a measured value, and the frame can carry a scale plus palette.
- **Viridis** (sequential, monotonic brightness, readable for colour-blind people) and **cool-warm** (diverging, neutral at zero).
- With an overlay active, a part's colour comes from its value and the surface turns matte, so reflections can't be mistaken for data. Parts without data recede to a dim matte grey, and shading and shadows stay intact so shapes remain readable.
- The same palette formulas run in Rust (for the UI legend and tests) and in WGSL. A GPU test checks the colours on screen.

Measured cost of the complete F1 frame (10,000 parts, four 4096² shadow cascades, sky, exposure, tonemap): **1.82 ms at 1080p** on an RTX 4070 SUPER (p99 2.19 ms).
