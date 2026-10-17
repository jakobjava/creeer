# ADR-0001: Custom engine in Rust

- **Status:** Accepted (2026-10-15). Confirmed by the F0 spikes (see below).
- **Deciders:** project owner + Claude

## Context

Creëer needs a realistic simulation core: node-beam soft bodies, orbital mechanics at planetary scale, chemistry, circuits and fluids. It also needs smooth voxel planets roughly 300–600 km in radius, multiplayer with a dedicated server, and cross-platform support (Windows, Linux, macOS). Performance is a top priority. Content is mostly procedural or built by players, so a level editor adds little value.

## Options considered

| Option | For | Against |
|---|---|---|
| Godot 4 + C++ GDExtension | Editor, renderer, UI and export out of the box | Single precision unless the engine is rebuilt; overhead and friction at the GDExtension boundary; limited control over threading and the renderer; we would bypass its physics anyway |
| BRUTAL (C#, used by Kitten Space Agency) | Built for scale; Vulkan, Jolt and job system bindings | Young repo (no source build yet); no renderer or editor; licence asks for a splash screen; C# interop for a native core |
| Unreal Engine 5 | Best renderer, native large worlds | Heavy; slow iteration; Chaos physics unsuitable; dynamic voxel and beam work fights the engine |
| Custom engine in C++ | Largest ecosystem (Jolt, Vulkan, ImGui) | Memory-safety and data-race bugs in a heavily parallel simulation; CMake/vcpkg friction |
| **Custom engine in Rust** | Memory safety and no data races in parallel simulation code; `cargo`; f64 collision (`parry3d-f64`); `wgpu` covers Vulkan, DX12 and Metal | Smaller ecosystem; no native Jolt; we write the renderer ourselves |

## Decision

A custom engine in Rust, assembled from libraries: `winit`, `wgpu`, `glam` (f64), our own XPBD solver plus `parry3d-f64`, `bevy_ecs`, `rayon`, `egui`, `renet`, `kira` and `mlua` (Luau; changed in F6 to standard Lua 5.4, ADR-0015).

The simulation crates have no rendering dependency, so the same code runs in the client, the dedicated server and the headless tests.

Precedents: Factorio and Teardown (custom engines), and Kitten Space Agency (a custom framework). Most code is written by Claude for an owner who is not an experienced programmer, so the compiler catching memory and threading bugs counts heavily in Rust's favour.

## Consequences

- Several months of renderer work before the game looks like a game. That's acceptable under the foundation-first roadmap (plan.md §7).
- Cross-platform determinism is enforced from day one: `libm` through `creeer_math::det`, glam's `libm` feature, and a clippy `disallowed-methods` guard (verified to fire).
- Revisit if the F0 spikes miss their budgets.

## Spike results (F0)

Measured on a Ryzen 7 7800X3D. The projection to the minimum-spec CPU assumes half the per-core speed. Reproduce with `cargo xtask perf`.

| Spike | Result | Budget | Verdict |
|---|---|---|---|
| (a) XPBD, 5,072 beams, 8 substeps | 0.52 ms/tick on one core (about 1.0 ms projected to min-spec); 0.05 ms with parallel islands | ≤ 4 ms | Speed OK. **Accuracy is not**: too soft for steel at low substep counts, see ADR-0002 |
| (b) Voxel meshing, 32³ chunks, 500 km planet | 3.1 ms generation + 0.4 ms Surface Nets per chunk on one thread; 2,230 chunks/s on 16 threads; about 850 chunks/s projected to 6 min-spec cores | ≥ 200 chunks/s | OK (4× headroom); noise generation dominates |
| (c) Camera-relative precision | error < 1 mm at 500 km offset (test); naive f32 > 1 mm (test); no visible jitter in the sandbox | < 1 mm | OK |

Also verified in F0:
- Render/simulation decoupling: `latest()` stays under 5 ms while ticks take 40 ms (test), and the sandbox stays smooth.
- Determinism: golden bit patterns (libm) and a golden state hash (truss simulation), checked on Windows, Linux and macOS in CI.

None of the spikes found a reason to revisit the engine or language decision.
