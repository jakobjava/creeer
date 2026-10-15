# Creëer: Game Plan (custom Rust engine)

## Context

**Creëer** (Dutch for "create") is a passion and learning project: a 3D sandbox about building and engineering, with singleplayer and multiplayer. Its influences are Minecraft and the Create mod (the main ones), plus KSP, Astroneer, Dream Car Builder, Factorio and Poly Bridge.

**Core pillars, agreed in Q&A:**
- **A realistic physics foundation:** node-beam soft bodies (deformation and fracture from real material properties), rigid bodies, collision, orbital mechanics, electricity, thermodynamics, fluids and gases, aerodynamics, and a physics-based player character.
- **Realistic manufacturing at chemistry level:** real minerals and ore grades; crushing, grinding, flotation, smelting, refining and alloying; real reactions with stoichiometry, energy balance and kinetics; casting, rolling, machining. Material properties feed into the physics.
- **Smooth voxel planets** (no blocks), KSP-like scale (radius roughly 300-600 km), procedurally generated, fully interactive (mining, digging, tree felling), with many resources based on real geology.
- **Free-form, part-based building** with deep customisation: parametric dimensions and materials. Buildings, bases, machines, vehicles and rockets.
- **No artificial tech tree.** Progress is gated by what you can actually manufacture.
- **Light survival:** health, hunger, thirst, oxygen and suit pressure, temperature, fall damage.
- **PvE and PvP:** fauna, environmental hazards, player-built physical weapons and raids.
- **Multiplayer:** server-authoritative. Players can host (2-16 players), or run a headless dedicated server.
- **Art style:** Factorio-like. Stylised, readable, softly textured, strong colours per material. Neither low-poly nor realistic.
- **Data-driven and mod-ready:** all content lives in data files.
- **Tech decision:** a custom engine in Rust instead of Godot. The reasons: full control over performance, threading, the GPU and precision; very little need for an editor because content is procedural or player-built; precedent from Factorio, Teardown and Kitten Space Agency.
- **Working style:** Claude writes most of the code, and the user understands code at a basic level. That makes the process **iterative**, with heavy testing and documented, double-checked decisions.
- **Performance is a top priority.** There are hard, measured budgets from F0 onwards (see §5).
- **Development approach à la Kitten Space Agency:** Phase A builds the technical foundation (engine and every simulation system, validated and fast). Phase B adds the gameplay.
- **Cross-platform:** Windows, Linux (including Steam Deck) and macOS (Apple Silicon). The dedicated server runs on Linux, Windows and macOS, x86-64 and ARM64. Consoles and mobile are out of scope: closed SDKs and too little CPU for this simulation.

Environment: Windows 11, RTX 4070 SUPER, git installed, **Rust not installed yet**. The project folder `C:\Users\jakmo\dev\creëer` is empty.

---

## 1. Architecture principles

1. **Simulation first, engine-agnostic.** All game logic and physics live in crates with no rendering dependency. The same code runs in the client, the headless dedicated server and the test runner.
2. **Server-authoritative from day one.** Singleplayer is an in-process server reached through a local channel transport, which is the same code path as multiplayer. Networking is never retrofitted.
3. **f64 everywhere in the simulation.** Rendering is camera-relative in f32 with reversed-Z depth. Each planet has its own reference frame (rotating near the surface, inertial in space).
4. **Data-oriented hot paths.** Solvers (beams, circuits, pipes, chemistry) use their own struct-of-arrays data. The ECS only handles entity bookkeeping. Parallelism comes from `rayon` and simulation "islands".
5. **Multi-rate fixed tick.** The game ticks at 60 Hz. Physics runs in substeps (count per island, see ADR-0002: low XPBD substep counts proved far too soft for steel). Electrical and fluid systems update every tick; chemistry and thermal at 10-20 Hz; orbits run adaptively.
6. **Simulation LOD:**
   - **Active:** full physics in a radius of about 2.5 km around players.
   - **Abstract:** unloaded factories run as rate equations with buffers, so factories keep producing while you're away.
   - **On rails:** distant vessels are propagated with orbital mechanics only.
7. **SI units internally.** The data loader parses unit strings such as `"7850 kg/m^3"`.
8. **Every important decision gets an ADR** (Architecture Decision Record in `docs/adr/`), backed by a spike or benchmark wherever possible.
9. **Simulation and rendering are decoupled.** The simulation runs on its own threads at a fixed rate. The renderer interpolates between the last two simulation states, so frame rate never depends on simulation load, and a slow tick never causes a stutter.
10. **Portable by construction.** Only pure-Rust or cross-compiling dependencies are used. `wgpu` translates to Vulkan, DX12 and Metal. Nothing platform-specific lives outside a small `platform` module.

## 2. Tech stack (Rust)

Exact versions are pinned in `Cargo.lock` during F0. Each choice gets an ADR and can be revisited.

| Area | Choice |
|---|---|
| Window and input | `winit` |
| Rendering | `wgpu` (Vulkan/DX12), WGSL shaders, compute shaders for meshing |
| Math | `glam` (DVec3 for simulation, Vec3 for rendering) |
| ECS | `bevy_ecs` standalone, pinned (fallback: `hecs`) |
| Parallelism | `rayon` |
| Collision | `parry3d-f64`. Dynamics come from our own unified XPBD solver (fallback: `rapier3d-f64`) |
| Sparse linear algebra | `faer` (electrical MNA, pipe networks, static structural solves) |
| Noise and worldgen | `fastnoise-lite` / own noise implementation |
| UI | `egui` + `egui_plot` with a custom theme |
| Audio | `kira` |
| Networking | `renet` (UDP, reliable/unreliable channels); serialisation with `bitcode`/`postcard` + `zstd` |
| In-game scripting | Luau through `mlua`, sandboxed with an instruction budget |
| Data | `serde` + RON files |
| Profiling and logging | `tracing`, `tracy-client`, `puffin` |
| Testing | `cargo test`, `proptest`, `insta`, `approx`, `criterion` |

## 3. Workspace layout

```
creeer/
  Cargo.toml                 # workspace
  crates/
    creeer_math/             # f64 math, reference frames, units parsing
    creeer_core/             # ECS setup, tick scheduler, ids, events
    creeer_physics/          # unified XPBD: nodes, beams, rigid bodies, contacts, plasticity, fracture
    creeer_orbit/            # Kepler rails for bodies, restricted n-body for vessels, frames, maneuvers
    creeer_terrain/          # planet gen, geology, ore deposits, voxel octree, meshing, edits
    creeer_materials/        # species DB, mixtures, material properties, microstructure
    creeer_chem/             # reactions, kinetics, equilibrium, unit operations
    creeer_mech/             # drivetrain network (shafts, gears, belts) à la Create
    creeer_electric/         # MNA circuit solver (DC, transient, AC phasors)
    creeer_thermal/          # lumped thermal network (conduction, convection, radiation)
    creeer_fluids/           # pipe networks, atmospheres, aerodynamics, buoyancy
    creeer_logic/            # signals, gates, sensors, Luau controllers
    creeer_building/         # parametric parts -> physics structures, blueprints
    creeer_sim/              # composes everything: world, sim LOD, save/load
    creeer_net/              # protocol, replication, interest management
    creeer_render/           # wgpu renderer
    creeer_ui/               # egui panels and overlays
    creeer_audio/
    creeer_client/           # binary
    creeer_server/           # binary (headless)
    creeer_tools/            # data validator, headless scenario runner, benchmarks CLI
  data/                      # species, reactions, minerals, rocks, parts, planets (RON)
  tests/scenarios/           # headless physics scenarios with expected results
  docs/adr/                  # decision records
  docs/physics/              # derivations + validation reports
```

## 4. System designs (summary)

**Physics core (`creeer_physics`):** XPBD for dynamics plus exact quasi-static FEM for structures at rest; the solver for moving stiff structures is chosen by a comparison spike at the start of F2 (see ADR-0002).
- Everything is particles plus constraints. A beam is a stretch constraint plus bending and torsion constraints, with stiffness derived from the material's E and G and the cross-section's A, I and J. Rigid bodies are XPBD bodies in the same solver.
- Plasticity: rest lengths and angles change once stress passes the yield strength. Fracture happens past the ultimate tensile strength (UTS) or a strain limit.
- Contacts and friction use parry. Islands are solved in parallel.
- Static structures such as buildings sleep, and a quasi-static solve runs whenever their load changes. Poly Bridge-style collapse is possible.

**Orbital mechanics (`creeer_orbit`):**
- Celestial bodies move on Kepler rails, which is deterministic and stable.
- Vessels are integrated under the gravity of all bodies (restricted n-body, adaptive high-order integrator). Lagrange points therefore exist naturally.
- Maneuver planning uses numerical propagation. Reference frames switch between planet-rotating and inertial.

**Terrain (`creeer_terrain`):**
- A cube-sphere planet with a sparse voxel octree near the surface and heightmap LOD far away.
- Each voxel stores density plus a rock or material ID. Meshing uses Surface Nets with seam handling between LOD levels (Dual Contouring is evaluated in an ADR).
- Edits are stored as deltas against the seeded generator.
- Geology: crust layers, igneous, sedimentary and metamorphic rocks, and real deposit models (porphyry Cu-Mo, banded iron, placer gold, laterite bauxite, evaporites, coal seams, oil/gas traps, nickel sulfides, uranium).
- Trees are procedural beam structures that can be felled into logs.

**Materials and chemistry (`creeer_materials`, `creeer_chem`):**
- A species database: elements and compounds, phases, molar mass, ΔHf, S°, Cp (NASA polynomials), and melting and boiling points.
- A mixture is a composition vector plus temperature, pressure and phase.
- Reactors are lumped batch/CSTR models with Arrhenius kinetics, Gibbs equilibrium where appropriate, and full mass and energy balances.
- Unit operations: crushers (Bond work index → energy use), screens, ball mills, flotation (recovery vs. particle size and reagents), thickeners, roasters, blast furnace, BOF/EAF, electrolysis (Faraday), distillation, crystallisation, Czochralski, and more.
- Solid parts carry their material plus microstructure (carbon content, grain size, heat treatment), which determine E, yield strength and UTS in the physics.
- Items come in two kinds: **bulk** (mass + composition) and **discrete** (a part instance with material, dimensions and quality/tolerance).

**Mechanical power (`creeer_mech`):** a 1D rotational network: shafts, gears (module and tooth count), belts, chains, clutches and flywheels, with inertia, torque, friction losses and backlash. On vehicles it couples to the 3D body through reaction torques.

**Electricity (`creeer_electric`):**
- A Modified Nodal Analysis solver covering DC, transients, and AC phasors for power grids.
- Wire resistance comes from material, cross-section and length.
- Components: batteries, DC and induction motors, generators, transformers, solar panels, capacitors, inductors and diodes; transistors come later.
- Joule heating feeds into the thermal network.

**Thermal (`creeer_thermal`):** a lumped network over parts, with conduction, convection and radiation (Stefan-Boltzmann), so radiators matter in space. Covers re-entry heating and engine thermodynamic cycles.

**Fluids (`creeer_fluids`):**
- Pipe network solver: nodal pressures, Darcy-Weisbach friction, compressible gases, and mixtures carrying composition. Pumps, compressors, valves and tanks.
- Each planet has an atmosphere with composition, scale height and temperature profile.
- Panel-based aerodynamics (lift and drag per surface element) and buoyancy in oceans.

**Logic (`creeer_logic`):** signal wires, gates and sensors, plus Luau microcontrollers with a fixed instruction budget per tick. An emulated RISC-V CPU is an optional later extra for real nerds.

**Building (`creeer_building`):**
- A parametric part catalog: structural members (I-beams, tubes, rods, plates with chosen profile, dimensions and material), joints (welded, bolted, hinge, bearing), and mechanical, electrical, fluid, logic and propulsion parts.
- Free placement with optional snapping, grids, symmetry and numeric input. Blueprints.
- A part's physics data (mass, inertia, beam stiffness) is computed from its geometry and material.

**Player:** a physics-based character with real mass and friction, gravity towards the planet's centre, a jetpack or EVA RCS in zero-g, ragdoll on big impacts and impulse-based fall damage. Survival covers health, hunger, thirst, O₂ and suit pressure, temperature and radiation.

**PvE and PvP:**
- Procedural fauna per planet with behaviour-tree AI.
- Hazards: storms, meteor showers, radiation, extreme temperatures, quakes.
- Weapons: ballistics with drag, penetration based on material, and explosives as blast impulses on beam structures.
- Server rules: PvP toggle and raid windows.

**Multiplayer (`creeer_net`):**
- Interest management by spatial region. Players get client-side prediction and reconciliation; other entities use snapshot interpolation.
- Soft bodies replicate their rigid frame plus *plastic deformation deltas*. Elastic jiggle is computed locally.
- Terrain edits are sent as events; factory state syncs at low frequency.
- Time warp is server-wide and only allowed when no player is in an active physics bubble (or by vote).

**Rendering (`creeer_render`):**
- Camera-relative rendering with reversed-Z and an infinite far plane.
- Triplanar terrain materials, atmospheric scattering, sun and stars, cascaded shadow maps, and instancing for thousands of machines.
- The stylised Factorio-like look.
- Engineering overlays: stress heatmap, temperature, voltage, pressure, flow arrows. Plus an orbit map view.

**Nerd UI (`creeer_ui`):** production statistics graphs, a part inspector, reactor composition, a circuit "oscilloscope", a Δv and staging calculator, a maneuver planner, and a **Codex** that explains the physics and chemistry with the actual formulas.

**Persistence and modding:**
- Region files hold zstd-compressed voxel deltas plus versioned entity snapshots, with migration tests.
- Mods are data packs that override or extend `data/`. A Luau scripting API comes later.

## 5. Performance and platforms

**Target hardware and budgets.** These are checked on every milestone, on all three operating systems.

| Tier | Hardware | Target |
|---|---|---|
| Minimum | 6-core CPU (around Ryzen 5 2600), GTX 1660 / RX 5600-class GPU, 16 GB RAM, 4 GB VRAM | 1080p, 60 FPS on low/medium |
| Steam Deck / Apple M1 | | 720p-1080p, 30-40 FPS on low |
| Recommended | 8-core CPU, RTX 3060-class GPU | 1440p, 60+ FPS on high; 144 FPS possible |
| Simulation tick | Minimum CPU | 60 Hz tick ≤ 10 ms with the reference load: 20k active beams, 1,000 machines, 2 km² of loaded terrain, 8 players |

**CPU techniques**
- Struct-of-arrays data, no allocations in hot loops (arenas and pools), and cache-friendly node ordering.
- Graph colouring so XPBD constraints solve in parallel.
- SIMD through `glam` and `wide`.
- `rayon` work-stealing across simulation islands.
- Sleeping bodies and islands, adaptive substeps, quasi-static solves for buildings.
- Simulation LOD (active, abstract, on rails).
- Chunk generation, meshing and saving run on background workers and never block the frame.

**GPU techniques**
- Instancing for all repeated parts, and merged meshes for static builds.
- Frustum culling plus Hi-Z occlusion culling on the GPU.
- Terrain and part LODs, texture arrays, a minimal number of draw calls.
- Cached shadow cascades for static geometry.
- Quality presets and a render-scale slider (temporal upscaling later).
- Streaming within a 4 GB VRAM budget.

**Network:** about 256 kbit/s or less per client in typical play, with delta compression, quantisation and interest management.

**Build settings:**
- Shipping builds use the `dist` profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, stripped. Day-to-day `release` builds use thin LTO and keep line tables, so they compile faster and work with profilers.
- The CPU baseline is x86-64-v2, with runtime dispatch (`multiversion`) to AVX2 for hot kernels. ARM64 uses NEON.
- PGO comes later.
- Dev builds use opt-level 1 for our own crates and 3 for dependencies, so debug runs stay playable.

**Performance gates:**
- `criterion` benchmarks plus headless **stress scenarios**: a large factory, 50k beams, 10 cars crashing, a planet flyover. They log frame-time percentiles (p50, p99, max).
- CI fails on a regression of more than 10% against the stored baseline.
- An in-game profiler overlay (`puffin`), plus Tracy for deep dives.

**Cross-platform measures:**
- A CI matrix on Windows, Linux and macOS (build, test, run the stress scenarios headless).
- Shaders are written in WGSL and translated by naga to SPIR-V, HLSL and MSL. Only wgpu features that exist on all three backends are used (no Vulkan-only extensions).
- Simulation-critical transcendental functions (sin, exp, …) go through the `libm` crate, so replays, tests and golden hashes are bit-identical across platforms. FMA is never relied on implicitly.
- OS paths through `directories`, and audio through `kira` (cross-platform).
- Packaging: Windows installer/zip; Linux AppImage plus a Steam runtime build; a signed, notarised macOS `.app` (needs an Apple developer account when releasing); and a Docker image for the dedicated server.

## 6. Quality and verification strategy (essential)

- **Unit tests** in every crate. CI runs `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test`, through `cargo xtask ci` locally (and GitHub Actions if a remote is added).
- **Physics validation suite** against analytic results, each with a documented tolerance:
  - Mechanics: pendulum period; projectile flight with and without drag; terminal velocity; cantilever deflection δ = FL³/3EI; Euler buckling load; a beam's natural frequency; momentum and energy conservation in collisions; friction on a ramp (μ).
  - Orbits: energy and angular-momentum drift over 1000 orbits; Hohmann transfer Δv; escape velocity.
  - Circuits and heat: RC time constant, voltage divider, RLC resonance, 1D heat conduction.
  - Fluids: pipe flow against Darcy-Weisbach.
- **Data integrity tests** run automatically over the whole database: every reaction conserves elements and charge; ΔG signs match known spontaneous and non-spontaneous reactions; all unit strings parse; no recipe cycle produces mass from nothing.
- **Property tests** (`proptest`): conservation laws under randomised inputs.
- **Determinism tests:** the same seed and inputs give the same state hash; world generation has golden hashes.
- **Snapshot tests** (`insta`) for data loading and world-gen outputs.
- **Benchmarks** (`criterion`) with a performance budget per milestone, tracked for regressions.
- **Headless scenario runner:** scripted scenarios (dropping a car from 10 m, a bridge under load, a rocket to orbit) with checked outcomes.
- **Process:** an ADR before each major decision, validation reports in `docs/physics/`, and at the end of each milestone a demo for the user plus a review before the next one starts.

## 7. Roadmap

This follows the **Kitten Space Agency approach**: first build a solid, validated technical foundation (engine, renderer, physics, planets, orbits, networking and every simulation domain), and only then add the fun gameplay layers on top. This works because realism and performance can't be retrofitted, while gameplay can always be added on a sound base.

There are no deadlines. Every milestone ends with a visible demo, a green test suite and its performance budgets met.

### Phase A: Technical foundation

There are no gameplay loops in this phase. Each milestone delivers a **tech sandbox**: a free-fly camera with debug UI where you can spawn objects, poke at the simulation and watch the numbers. Like KSA's early builds, the sandbox is already fun to explore.

| # | Milestone | Exit criteria |
|---|---|---|
| **F0** | Toolchain & engine skeleton | Toolchain, workspace, CI script + 3-OS CI matrix, decoupled simulation/render threads, window + free camera + debug lines + egui FPS/frame-time overlay, headless runner, perf-gate harness. Three spikes measured: (a) XPBD with 5k beams at ≥ 8 substeps × 60 Hz within 4 ms on the minimum CPU class; (b) voxel chunk meshing throughput (target ≥ 200 chunks/s on 6 cores); (c) camera-relative rendering at 500 km radius with no jitter. ADR-0001 (engine/language) and ADR-0002 (solver) written |
| F1 | Renderer core | wgpu renderer: PBR-lite stylised materials, sun lighting + cascaded shadows, instancing, GPU culling, LODs, reversed-Z at planetary scale, atmosphere scattering, stars, engineering overlay system, egui integration. Budgets: 10k instanced parts at 60 FPS on minimum spec |
| F2 | Physics core | Unified XPBD (particles, beams with stretch/bend/torsion, rigid bodies, contacts, friction, plasticity, fracture, sleeping, parallel islands) + the full mechanics validation suite green. Sandbox: bridge under load with stress overlay, crash test, stacking |
| F3 | Planets & terrain | Cube-sphere planet, geology + ore deposit generator (data-driven), voxel octree + LOD meshing, dig/fill edits, terrain collision with the XPBD world, radial gravity, day/night, edits saved as deltas. Sandbox: fly from orbit down to the surface without hitches, and dig tunnels |
| F4 | Orbital mechanics & solar system | Procedural solar system, Kepler rails, restricted n-body propagation, reference-frame switching (rotating ↔ inertial), floating origin, time warp, map view with orbits. Orbit validation suite green. Sandbox: place a test mass in orbit and fly to the moon |
| F5 | Networking & persistence | Client/server split, in-process server for singleplayer, interest management, snapshot interpolation, prediction for a test avatar, replication of soft bodies (plastic deltas) and terrain edits, dedicated server binary, versioned saves + migration tests. Bandwidth budgets met |
| F6 | Simulation domains | The engines for each domain, each validated against analytic or reference results, each with a sandbox scene: **materials & chemistry** (species DB, mixtures, kinetics, equilibrium, mass/energy balance), **thermal** network, **electrical** MNA (DC/transient/AC), **fluids** (pipes, atmospheres, aero panels, buoyancy), **mechanical** drivetrain, **logic** (signals + sandboxed Luau VM) |
| F7 | Scale & hardening | Simulation LOD (active / abstract / on rails), the abstract-mode rate solver, stress scenarios on all three operating systems, data/mod pipeline (data packs, validator, hot-reload), profiling pass. **Foundation gate:** every validation suite green and every budget met on Windows, Linux and macOS before Phase B starts |

### Phase B: Gameplay (the fun part)

| # | Milestone | Exit criteria |
|---|---|---|
| G1 | Player & interaction | Physics character (walk, climb, jetpack, EVA, ragdoll), mining with real composition, tree felling, inventory (bulk + discrete), HUD |
| G2 | Building system | Parametric part catalog, placement/snapping/symmetry/numeric input, welding/bolting, structure → physics, blueprints, collapsible buildings |
| G3 | First production chains | Copper & iron → steel: crushing, grinding, flotation, smelting, casting, rolling, machining; conveyors, chutes, arms; production statistics. Knowledge-gated progression |
| G4 | Power | Gameplay for shafts, gears and belts (Create-style); a steam engine with a real cycle; generators, motors, grids, batteries; Hall-Héroult aluminium |
| G5 | Chemistry & fluids | Reactors, distillation, petrochemicals, plastics, fertiliser, gases; pipe and tank gameplay |
| G6 | Vehicles | Pacejka tyres, suspension, combustion engines (fuel chemistry), gearboxes, aero, boats |
| G7 | Automation & logic | Sensors, gates, Luau microcontrollers, programmable arms, logistics automation |
| G8 | Rockets & space | Rocket engines (Isp from propellant chemistry), staging, Δv tools, maneuver nodes, re-entry heating, landing on and colonising other planets |
| G9 | Survival, PvE, PvP | Health, hunger, thirst, O₂, temperature, radiation; food; fauna AI; hazards; ballistics, penetration, explosives; PvP server rules |
| G10 | Advanced tech | Semiconductor chain (quartz → Si → Siemens → Czochralski → lithography → chips), fission, advanced alloys, composites |
| G11 | Polish | Art and audio pass, Codex, onboarding, settings, modding docs, final performance pass |

## 8. Immediate next steps (F0, after approval)

1. **The user installs the Rust toolchain:** `rustup` (stable, MSVC target) plus the Visual Studio Build Tools (C++ workload). Then I check that `rustc` and `cargo` work.
2. `git init`, a `.gitignore`, and the Cargo workspace with the crate skeletons from §3 (empty crates with one smoke test each). Set up the release and dev profiles from §5.
3. `creeer_math`: f64 frames, the camera-relative transform and `libm`-based deterministic functions, with tests.
4. `creeer_client`: a winit + wgpu window, free-fly camera, reversed-Z, grid and debug-line renderer, egui overlay (FPS, frame-time graph), and a simulation thread with render interpolation.
5. `creeer_tools`: a headless scenario runner skeleton, `cargo xtask ci`, and `cargo xtask perf` (stress scenarios + regression check against a baseline).
6. A GitHub Actions matrix for Windows, Linux and macOS. This needs a GitHub remote; I'll ask before pushing anything. Until then, `xtask ci` runs locally.
7. Spikes a/b/c with `criterion` benchmarks; results go in `docs/adr/0001-engine-and-language.md` and `docs/adr/0002-physics-solver.md`.
8. Demo and review with the user, then on to F1.

## 9. Verification for F0

- `cargo xtask ci` passes: fmt, clippy with no warnings, all tests.
- `cargo run -p creeer_client --release` opens a window with a flyable camera, grid, debug lines and an FPS/frame-time overlay. It runs at the monitor's refresh rate, and frame time stays stable even with an artificially slowed simulation tick, which proves the decoupling works.
- `cargo xtask perf` prints the spike numbers and p50/p99 frame times against their budgets.
- A test at a 500 km offset shows camera-relative rendering error below 1 mm.
- A cross-platform determinism test: the same scenario gives the same state hash on Windows, Linux and macOS in CI.

## 10. Risks and mitigations

- **Scope:** this is a multi-year project. Vertical slices keep each step playable; abstraction mode and LOD keep simulation cost bounded.
- **Soft-body performance:** spike in F0, simulation LOD, sleeping, parallel islands; GPU compute is an option later.
- **Physics sync in multiplayer:** server authority, plastic deltas only, local elastic visuals.
- **Chemistry data authoring:** automated integrity tests and validator tooling; start with a small, correct set and grow it.
- **Custom renderer effort:** keep the art style simple; build only the features the look actually needs.
- **macOS/Metal differences:** use only features available on every wgpu backend, and the CI matrix catches breakage early. There is no Mac on hand, so CI is the main safety net there.
- **Performance drifting over time:** perf gates in CI and per-milestone budgets. A milestone counts as done only when it stays within budget.
