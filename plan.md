# Creëer: Game Plan (custom Rust engine)

## Context

**Creëer** (Dutch for "create") is a passion and learning project: a 3D sandbox about building and engineering, with singleplayer and multiplayer. Its influences are Minecraft and the Create mod (the main ones), plus KSP, Astroneer, Dream Car Builder, Factorio and Poly Bridge.

**Core pillars, agreed in Q&A:**
- **A realistic physics foundation:** node-beam soft bodies (deformation and fracture from real material properties), rigid bodies, collision, orbital mechanics, electricity, thermodynamics, fluids and gases, aerodynamics, and a physics-based player character.
- **Realistic manufacturing at chemistry level:** real minerals and ore grades; crushing, grinding, flotation, smelting, refining and alloying; real reactions with stoichiometry, energy balance and kinetics; casting, rolling, machining. Material properties feed into the physics.
- **Smooth voxel planets** (no blocks), KSP-like scale (radius roughly 300-600 km), procedurally generated, fully interactive (mining, digging, tree felling), with many resources based on real geology.
- **Free-form, part-based building** with deep customisation: parametric dimensions and materials. Buildings, bases, machines, vehicles and rockets. There are few designed parts, in the spirit of Dream Car Builder (rods, tubes, plates and panels of any material and size, joined as built), not a catalogue of finished machines as in Satisfactory or Minecraft.
- **No artificial tech tree.** Progress is gated by what you can actually manufacture. **Nothing is given** (the user, 2026-10-22): a survival player starts in the clothes they wear and makes everything else, from stone tools on. The game is hard and expects real knowledge of chemistry, mechanics and physics. It explains (the Codex) but does not hand-hold.
- **Flexible, generic systems with attributes** (the user, 2026-10-22). One system per kind of thing, driven by data: a tool's behaviour comes from the pieces it is made of, and a tree is a species' attributes (form, size, wood, colours) that worldgen's biomes choose from. Tweaking attributes makes content; new content is data, not code.
- **Light survival:** health, hunger, thirst, oxygen and suit pressure, temperature, fall damage.
- **PvE and PvP:** fauna, environmental hazards, player-built physical weapons and raids.
- **Multiplayer:** server-authoritative. Players can host (2-16 players), or run a headless dedicated server.
- **Art style:** Factorio-like. Stylised, readable, softly textured, strong colours per material. Real, smooth shapes, never blocky, voxel-like or low-poly (no Minecraft geometry). Grounded and industrial, never futuristic. Stylised in its surfaces and palette, not photographic. The rules, palette and checks that keep it from drifting: `docs/art-style.md`; models are made in Blender (`docs/blender-mcp.md`).
- **Data-driven and mod-ready:** all content lives in data files.
- **Tech decision:** a custom engine in Rust instead of Godot. The reasons: full control over performance, threading, the GPU and precision; very little need for an editor because content is procedural or player-built; precedent from Factorio, Teardown and Kitten Space Agency.
- **Working style:** Claude writes most of the code, and the user understands code at a basic level. That makes the process **iterative**, with heavy testing and documented, double-checked decisions.
- **Performance and maximum realism, together, are paramount.** Neither is traded for the other: physics is never simplified, nor its tolerances loosened, to gain speed, and slowness is never accepted as the price of realism or left "for later". Where the two seem to conflict, the answer is a formulation that is both exact and fast. There are hard, measured budgets from F0 onwards (see §5), on the worst tick as well as the typical one.
- **Development approach à la Kitten Space Agency:** Phase A builds the technical foundation (engine and every simulation system, validated and fast). Phase B adds the gameplay.
- **Cross-platform:** Windows, Linux (including Steam Deck) and macOS (Apple Silicon). The dedicated server runs on Linux, Windows and macOS, x86-64 and ARM64. Consoles and mobile are out of scope: closed SDKs and too little CPU for this simulation.

Environment: Windows 11, RTX 4070 SUPER, git installed, **Rust not installed yet**. The project folder `C:\Users\jakmo\dev\creëer` is empty.

---

## 1. Architecture principles

1. **Simulation first, engine-agnostic.** All game logic and physics live in crates with no rendering dependency. The same code runs in the client, the headless dedicated server and the test runner.
2. **Server-authoritative from day one.** Singleplayer is an in-process server reached through a local channel transport, which is the same code path as multiplayer. Networking is never retrofitted.
3. **f64 everywhere in the simulation.** Rendering is camera-relative in f32 with reversed-Z depth. Each planet has its own reference frame (rotating near the surface, inertial in space).
4. **Data-oriented hot paths.** Solvers (beams, circuits, pipes, chemistry) use their own struct-of-arrays data. The ECS only handles entity bookkeeping. Parallelism comes from `rayon` and simulation "islands".
5. **Multi-rate fixed tick.** The game ticks at 60 Hz. Structures are stepped implicitly (BDF2, one step per tick, ADR-0004); low XPBD substep counts proved far too soft for steel (ADR-0002). Electrical and fluid systems update every tick; chemistry and thermal at 10-20 Hz; orbits run adaptively.
6. **Simulation LOD:**
   - **Active:** full physics in a radius of about 2.5 km around players.
   - **Abstract:** unloaded factories run as rate equations with buffers, so factories keep producing while you're away.
   - **On rails:** distant vessels are propagated with orbital mechanics only.
7. **SI units internally.** The data loader parses unit strings such as `"7850 kg/m^3"`.
8. **Every important decision gets an ADR** (Architecture Decision Record in `docs/adr/`, conventions in `docs/adr/README.md`), backed by a survey of prior work (principle 12) and by a spike or benchmark wherever possible.
9. **Simulation and rendering are decoupled.** The simulation runs on its own threads at a fixed rate. The renderer interpolates between the last two simulation states, so frame rate never depends on simulation load, and a slow tick never causes a stutter.
10. **Portable by construction.** Only pure-Rust or cross-compiling dependencies are used. `wgpu` translates to Vulkan, DX12 and Metal. Nothing platform-specific lives outside a small `platform` module.
11. **Realistic and fast at once.** Every sandbox scene runs in real time with room to spare: its worst tick is gated by a test (§6), measured with the physics crate's `profile` feature when it is not. A scene that lags is a bug, fixed before new work starts.
12. **Research before building.** Before designing a solver, a system, a performance package or a look, survey how others have solved it: published papers (SIGGRAPH, TOG, arXiv), other engines' documentation (PhysX, MuJoCo, Havok, Jolt, Bullet), open-source code and shipped games. The survey goes into the ADR's **Prior work** section (`docs/adr/README.md`) with its sources, and the design starts from what it found; a method is built from first principles only where the survey shows nothing fits. Skipping it cost weeks in P1 (2026-10-18, ADR-0024): the ragdolls' slowness had a known answer (reduced coordinates, RedMax 2019) that ADR-0022 had rejected without looking, and P1's planned method (a group split across cores) was shown by published results not to pay for groups that small.

## 2. Tech stack (Rust)

Exact versions are pinned in `Cargo.lock` during F0. Each choice gets an ADR and can be revisited.

| Area | Choice |
|---|---|
| Window and input | `winit` |
| Rendering | `wgpu` (Vulkan/DX12), WGSL shaders, compute shaders for meshing |
| Math | `glam` (DVec3 for simulation, Vec3 for rendering) |
| ECS | `bevy_ecs` standalone, pinned (fallback: `hecs`) |
| Parallelism | `rayon` |
| Collision | Own narrow phase for spheres and boxes (SAT with clipping), sort-and-sweep broad phase (ADR-0005); `parry3d-f64` when convex hulls and meshes are needed. Dynamics come from our own solvers: implicit Newmark/BDF2 for structures (ADR-0004), contacts, impacts and plasticity per ADR-0005 |
| Sparse linear algebra | Own deterministic sparse block Cholesky for structures (ADR-0004; bit-identical across platforms). `faer` remains an option for non-deterministic tools |
| Noise and worldgen | `fastnoise-lite` / own noise implementation |
| UI | `egui` + `egui_plot` with a custom theme |
| Audio | `kira` |
| Networking | `renet` (UDP, reliable/unreliable channels); serialisation with `bitcode`/`postcard` + `zstd` |
| In-game scripting | Lua 5.4 (built from source, deterministic) through `mlua`, sandboxed with an instruction budget |
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
    creeer_physics/          # structures (implicit), rigid bodies, contacts, plasticity, fracture
    creeer_orbit/            # n-body bodies (IAS15), ephemerides, rails for ships and small bodies, generation, analysis
    creeer_terrain/          # planet gen, geology, ore deposits, voxel octree, meshing, edits
    creeer_materials/        # species DB, mixtures, material properties, microstructure
    creeer_chem/             # reactions, kinetics, equilibrium, unit operations
    creeer_mech/             # drivetrain network (shafts, gears, belts) à la Create
    creeer_electric/         # MNA circuit solver (DC, transient, AC phasors)
    creeer_thermal/          # lumped thermal network (conduction, convection, radiation)
    creeer_fluids/           # pipe networks, atmospheres, aerodynamics, buoyancy
    creeer_logic/            # signals, gates, sensors, Lua controllers
    creeer_building/         # parts and fasteners -> physics structures; design files later
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

**Physics core (`creeer_physics`):** structures (anything built from members) are stepped implicitly (Newmark for free structures, BDF2 for groups in contact, ADR-0004): an energy minimisation with line-searched Newton steps on a prefactorised sparse direct solve in the body frame (ADR-0004). Structures at rest use the exact quasi-static FEM (ADR-0002). Contacts, joints and loose objects follow ADR-0005.
- Structures are nodes plus beam elements (stretch, bending and torsion; 6 DOF per node), with stiffness derived from the material's E and G and the cross-section's A, I and J.
- Plasticity: rest lengths and angles change once stress passes the yield strength. Fracture happens past the ultimate tensile strength (UTS) or a strain limit.
- Contacts and friction use parry. Islands are solved in parallel.
- Static structures such as buildings sleep, and a quasi-static solve runs whenever their load changes. Poly Bridge-style collapse is possible.

**Orbital mechanics (`creeer_orbit`, `creeer_sim::universe`, ADR-0007):**
- **Everything pulls on everything that matters.** Stars, planets, moons and the few asteroids heavy enough to matter are integrated together under their mutual gravity (no Kepler rails): orbits precess and resonate, and **pushes** (engines, mass drivers, what stands on a body) change them, so machines can one day move asteroids, moons and planets.
- Ships and small bodies are test particles on rails under the gravity of all bodies (restricted n-body); ships towing an asteroid by gravity are integrated together with it. Lagrange points exist naturally (Trojans stay at L4/L5 only below Routh's mass ratio, as tested).
- One integrator for every trajectory: IAS15 (15th order, adaptive, errors below f64 rounding, close encounters resolved), with compensated sums. The bodies' trajectories are stored as an ephemeris (truth up to now, forecast after; a push rewrites the forecast).
- The physics bubble turns with the body near the focus (static terrain) or free-falls with a floating origin in space; gravity in it includes every other body's tides. What stands or hovers on a body pushes it back (Newton's third law through the ground and gravity).
- Procedural systems from data, checked for stability by integrating them for centuries. Predicted paths are computed and drawn in a plotting frame (centred on a body, or turning with a pair). Maneuver nodes come with G8.

**Terrain (`creeer_terrain`, ADR-0006):**
- Heights live on cube-sphere tiles (one quadtree per face); the 3D terrain is a planet-centred octree of voxel chunks (32³ cells, 1 m at full detail) at every level of detail, from the ground to the whole planet. Sea level is the equipotential of the planet's gravity and rotation.
- The density is computed from the heights and the edits; the rock comes from the geology. Meshing uses Surface Nets; each chunk is clipped exactly at its box, with caps over the rock its faces cut where a neighbour has another level, and vertices blend into the next coarser level towards a level border (geomorphing).
- Edits are an ordered log of dig/fill brushes: deltas against the seeded generator, saved bit-exactly.
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

**Logic (`creeer_logic`):** signal wires, gates and sensors, plus Lua 5.4 microcontrollers with a fixed instruction budget per tick. An emulated RISC-V CPU is an optional later extra for real nerds.

**Building (`creeer_building`):**
- A parametric part catalog: structural members (I-beams, tubes, rods, plates with chosen profile, dimensions and material), joints (welded, bolted, hinge, bearing), and mechanical, electrical, fluid, logic and propulsion parts.
- Building by hand: parts are held, put in place and fastened with tools. Precision (measuring, drawings, CAD and design files) comes from tools the player has made (Phase B, principle 13).
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
- Mods are data packs that override or extend `data/`. A Lua scripting API comes later.

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
- Prefactorised sparse direct solves (nested-dissection ordering) for structures; parallel islands.
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

- **Unit tests** in every crate. CI runs a file-length check, `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test`, through `cargo xtask ci` locally and in GitHub Actions.
- **Small files:** at most **400 lines per source file** (`.rs` and `.wgsl`, tests included), enforced by `cargo xtask ci`. Code is split by responsibility (for example, one module per render pass; large test blocks in a sibling `tests.rs`), so every file stays readable for the project owner.
- **Physics validation suite** against analytic results, each with a documented tolerance:
  - Mechanics: pendulum period; projectile flight with and without drag; terminal velocity; cantilever deflection δ = FL³/3EI; Euler buckling load; a beam's natural frequency; momentum and energy conservation in collisions; friction on a ramp (μ).
  - Orbits: energy and angular-momentum drift over 1000 orbits (also at e = 0.99); Hohmann transfer Δv; escape velocity; Kepler as the two-body oracle; Lagrange points as equilibria and Routh's Trojan criterion; rotating vs inertial frames; bubble vs rails; momentum through the ground (ADR-0007).
  - Circuits and heat: RC time constant, voltage divider, RLC resonance, 1D heat conduction.
  - Fluids: pipe flow against Darcy-Weisbach.
- **Data integrity tests** run automatically over the whole database: every reaction conserves elements and charge; ΔG signs match known spontaneous and non-spontaneous reactions; all unit strings parse; no recipe cycle produces mass from nothing.
- **Property tests** (`proptest`): conservation laws under randomised inputs.
- **Determinism tests:** the same seed and inputs give the same state hash; world generation has golden hashes.
- **Snapshot tests** (`insta`) for data loading and world-gen outputs.
- **Benchmarks** (`criterion`) with a performance budget per milestone, tracked for regressions.
- **Real-time gates:** every sandbox scene's worst tick (not only its typical one) is tested against a budget of a few milliseconds on the development machine. Speed-ups are checked against the physics validation suite, and a change of rounding is proved as such before golden hashes are regenerated.
- **Headless scenario runner:** scripted scenarios (dropping a car from 10 m, a bridge under load, a rocket to orbit) with checked outcomes.
- **Process:** a survey of prior work before each design (§1, principle 12), an ADR before each major decision (with that survey as its Prior work section), validation reports in `docs/physics/`, and at the end of each milestone a demo for the user plus a review before the next one starts.

## 7. Roadmap

This follows the **Kitten Space Agency approach**: first build a solid, validated technical foundation (engine, renderer, physics, planets, orbits, networking and every simulation domain), and only then add the fun gameplay layers on top. This works because realism and performance can't be retrofitted, while gameplay can always be added on a sound base.

There are no deadlines. Every milestone ends with a visible demo, a green test suite and its performance budgets met.

### Phase A: Technical foundation

There are no gameplay loops in this phase. Each milestone delivers a **tech sandbox**: a free-fly camera with debug UI where you can spawn objects, poke at the simulation and watch the numbers. Like KSA's early builds, the sandbox is already fun to explore.

| # | Milestone | Exit criteria |
|---|---|---|
| **F0** | Toolchain & engine skeleton | Toolchain, workspace, CI script + 3-OS CI matrix, decoupled simulation/render threads, window + free camera + debug lines + egui FPS/frame-time overlay, headless runner, perf-gate harness. Three spikes measured: (a) XPBD with 5k beams at ≥ 8 substeps × 60 Hz within 4 ms on the minimum CPU class; (b) voxel chunk meshing throughput (target ≥ 200 chunks/s on 6 cores); (c) camera-relative rendering at 500 km radius with no jitter. ADR-0001 (engine/language) and ADR-0002 (solver) written |
| F1 | Renderer core | wgpu renderer: PBR-lite stylised materials, sun lighting + cascaded shadows, instancing, GPU culling, LODs, reversed-Z at planetary scale, atmosphere scattering, stars, engineering overlay system, egui integration. Budgets: 10k instanced parts at 60 FPS on minimum spec |
| F2 | Physics core | **Done** (ADR-0004, ADR-0005). Solver comparison spike (ADR-0004: implicit BDF2 + sparse direct solve). Structures (beams with stretch/bend/torsion, damping, rigid bodies, contacts, friction, plasticity, fracture, sleeping, parallel islands) + the full mechanics validation suite green. Sandbox: bridge under load with stress overlay, crash test, stacking |
| F3 | Planets & terrain | **Done** (ADR-0006). Cube-sphere planet, geology + ore deposit generator (data-driven), voxel octree + LOD meshing, dig/fill edits, terrain collision with the physics world, radial gravity, day/night, edits saved as deltas. Sandbox: fly from orbit down to the surface without hitches, and dig tunnels |
| F4 | Orbital mechanics & solar system | **Done** (ADR-0007). Procedural solar system (stability-checked), **full n-body for the bodies** with pushes (instead of Kepler rails), restricted n-body rails for ships and small bodies, Lagrange points, reference-frame switching (rotating ↔ free fall), floating origin, time warp (governed), map view with predicted paths and a co-rotating frame. Orbit validation suite green. Sandbox: put a ship in orbit, fly to the moon, push bodies |
| F5 | Networking & persistence | **Done** (ADR-0008; ADR-0005 amended). Client/server split, in-process server for singleplayer, interest management, snapshot interpolation, prediction for a test avatar, replication of soft bodies (plastic deltas) and terrain edits, dedicated server binary, versioned saves + migration tests. Bandwidth budgets met (typical play: 157 kbit/s to a client, 46 from it; budget 256). Sandbox: the planet sandbox is a client of a server it hosts or of a dedicated one (`--connect`); walk a test avatar (V), drop crates, balls and steel frames, dig; save and load the world (F3 edit files load too). **Also in F5, finishing F2–F3 work:** seamless terrain levels of detail (done: geomorphing, clipped meshes with caps instead of skirts, crease-free elevation; ADR-0006 amended) and more realistic contacts (done: contact laws from the materials, Hertz points and face patches in series with the resolvable stiffness, which stiffens with the load a contact carries at rest, critically damped on the gap's own rate; strain hardening and Cowper–Symonds rate-dependent plasticity; rigid bodies solved jointly with the structures they rest on; slender members that buckle, split as they are compressed and given EN 1090-2's crookedness; convex hulls as collision shapes (rocks, wedges, any convex part); ADR-0005 amended; then the contact law revised: the forces of a face's lifted corners reach the bodies, impacts carry friction, normals turn with their bodies and balls are measured to a box's or hull's closest point, faces on curved ground press with the ground's depth measured at their triangles' corners and as a punch of the area they press with, and bodies dropped on bumps come to rest; ADR-0005's last amendment. Still soft: sharp corners on flat faces) |
| F6 | Simulation domains | **Done** (ADR-0009–0016; ADR-0004 and ADR-0005 amended; fast ticks after it: ADR-0017; `docs/physics/f6-validation.md`; plan: `docs/f6-plan.md`). The engines for each domain, each validated against analytic or reference results, each with a sandbox scene: **materials & chemistry** (species DB, mixtures, kinetics, equilibrium, mass/energy balance), **thermal** network, **electrical** MNA (DC/transient/AC), **fluids** (pipes, atmospheres, aero panels, buoyancy), **mechanical** drivetrain, **logic** (signals + sandboxed Lua VM) |
| F7 | Scale & hardening | **Done** (ADR-0018–0021; `docs/physics/f7-validation.md`; plan: `docs/f7-plan.md`). Data packs with a validator and hot-reload; simulation LOD (resting islands parked bit for bit beyond 3 km); abstract machines (rates measured from their own full simulation by probes, the rate solver exact between events; every stock within 1 % of full); stress scenarios whose hashes are identical on Windows, Linux and macOS; a profiler of our own and the pass it found (the reference tick within 10 ms projected). Exit criteria: Simulation LOD (active / abstract / on rails), the abstract-mode rate solver, stress scenarios on all three operating systems, data/mod pipeline (data packs, validator, hot-reload), profiling pass. **Foundation gate:** every validation suite green and every budget met on Windows, Linux and macOS before Phase B starts |

### Phase B: Gameplay (the fun part)

Revised 2026-10-18, before G1, with the user's answers: the physics' deferred speed work comes **just in time** (in the milestone whose gameplay first makes it matter), gameplay scenes are gated at a **worst tick ≤ 5 ms** on the development machine (aim 2–3 ms), the first loop is **Create-style** (mechanical power before the production chains), and each milestone is planned in sub-milestones here, with a fuller `docs/gN-plan.md` written when it starts.

#### Principles of Phase B

1. **The physics is the game.** No rule is faked where the simulation can answer it. A crusher's throughput comes from its shaft power and the ore's Bond work index, not from a recipe time; a building stands or falls by its members' stresses; a fall hurts by the deceleration the body takes, not by the height fallen.
2. **Vertical slices.** Each milestone ends playable as one loop, in singleplayer (in-process server) and multiplayer (hosted and dedicated). Saves carry the new state (save version raised, migration tests for the old ones), and everything new replicates within the bandwidth budget (§5: ≤ 256 kbit/s per client).
3. **Data first.** Every part, machine, tool, creature and process parameter lives in data packs (ADR-0018) and is checked by `creeer_tools validate`. Nothing gameplay-related is hard-coded where a modder would want to change it.
4. **Play scenes, gated.** Each milestone defines **play scenes**: headless, scripted recordings of what players will actually do, such as felling a forest or collapsing a tower (list per milestone below). Their gates:
   - **Worst tick ≤ 5 ms** on the development machine (≈ 10 ms projected to min-spec, so a 60 Hz server never falls behind), aiming at 2–3 ms; the p99 ≤ 2 ms.
   - Their Newton iterations are gated in CI (work) and their times in `cargo xtask perf`, as for the sandbox scenes (ADR-0017).
   - The reference tick (§5) stays ≤ 10 ms projected, and grows with the game: from G2 it includes a building, from G6 vehicles.
   
   A milestone is done only when its play scenes meet their gates. If a scene misses for a reason a later performance package (below) addresses, that package moves earlier; the gate does not move.
5. **Realism debts are paid when gameplay reaches them.** Each milestone lists the items of `docs/future-improvements.md` its gameplay would expose, and lifts them. One that is not lifted is recorded there again, with the user's decision.
6. **Playtest at every milestone's end.** The user plays the milestone's demo (and from G4 with others, on a rented server). The feedback is taken in before the next milestone starts.
7. **Process as in Phase A.** Branch `gN`, small commits per sub-milestone passing `cargo xtask ci`, a squash-merge at the end; ADRs for decisions; a validation report in `docs/physics/` where physics changed.
8. **Research first, in every milestone and package** (§1, principle 12). Each `gN-plan.md` and each performance package starts with a survey of prior work (papers, engines, games) for what it builds; its findings decide the method, and it is recorded in the ADR's Prior work section. A sub-milestone's design is not started before its survey is written. The survey is focused on what the step builds: enough to choose the method well.
9. **Nothing given** (the user, 2026-10-22). Survival gives the clothes worn and nothing else: no landing pod, no kit, no free refills, no endless sources. Handouts belong to creative and test servers.
10. **Fundamentals first, and development time is limited** (the user, 2026-10-22: "basic and fundamental gameplay mechanics and systems and not spend hours on little polish things"; "i DO NOT have unlimited usage and want to get to the fun part of designing production, buildings, vehicles, etc. soon"). Each milestone builds the smallest version of its loop that is real and fast, tested where its physics and gates need it, and the user plays it as soon as it is playable. Story, staging, decoratives, UI themes, golden images of the UI and other polish go to G11 through `docs/future-improvements.md`. Performance is a fundamental, never polish. In the user's words: "good quality but focused and research driven; no faffing about; the game can always be fleshed out later to make it look more flashy".
11. **Generic systems, few designed parts** (pillars above). A milestone builds the system with its attributes and a few data entries that prove it is generic, not a catalogue of hand-made content.
12. **Foundations now, refinement later** (the user, 2026-10-22: "you (opus 5.5) lay the foundations of the game and make the actual important design and architecture decisions; and then later one can refine it easily because everything is flexible and data driven (so i can use cheaper models for that)"). Each milestone builds the architecture and the generic systems, with a few data entries that prove them. More content, tuning and polish come later and are data work. Every data file says in its header how to add to it, the validator catches bad entries, and the tests check every entry generically.
13. **Precision is technology** (the user, 2026-10-23: "one should only be able to build precise things if one has the technology to do so, visually it should not really be more than what you see in real life"; "proper things like vehicles and rockets will need a pen and paper, tape measure, cad-software etc"). The build interface shows only what a person on site would see: no ghost previews, snapping, grids, symmetry, numeric input, undo or "will it hold?" preview. A part goes where hands put it and is as straight as the eye judges it. Precision comes from tools the player has made, on a ladder: **by eye** (G2: cut where you point, copy a length by marking against another part) → **measuring and marking tools** (G3: tape measure, steel rule, square, spirit level, jigs and templates, with the lathe's tolerances) → **paper drawings** with dimensions, followed with those tools (before G6's vehicles) → **CAD, CNC and design files** shared between players (computers, G7/G10). Each tool's accuracy is data. Debug views such as the stress overlay exist on creative and test servers as information, not as a survival tool; creative gives free materials, not conveniences.

#### When the physics gets faster: the performance track

The levers within the solvers' present design were measured and taken or rejected in ADR-0017 and ADR-0021. What remains needs different methods (weeks each). Each package is scheduled where gameplay first produces its load, so it is built and measured against real play rather than synthetic scenes:

| Package | What | Lands in | Why then | Target (development machine) |
|---|---|---|---|---|
| **P1: piles** | First pass (brought forward after G1.2d) done 2026-10-18: the legs' struts and about 8 % off a ragdoll's step; what follows stays here, starting from its measurements (ADR-0024, "Outcome of the first pass"). Revised after its survey of prior work (ADR-0024, 2026-10-18): jointed bodies (ragdolls) stepped as one articulated body in reduced coordinates within the implicit step, joints in maximal coordinates kept for loops and grabs (RedMax's hybrid); friction updated within Newton's iterations; a large group's contact solve split across cores by consensus ADMM where groups have hundreds of bodies (piles). With it: friction on fresh normal forces (no contact's friction on a normal force more than 5 % off its own), edge contacts and face patches made consistent (no rattling rocks kept awake), per-group turn substeps, cheaper faces over curved ground | **G1** | Felled logs, mined boulders and dropped items pile up on terrain | Rock pile worst 18.2 → ≤ 5 ms, p99 14.2 → ≤ 2; 50 crates on terrain no slower than on a plane ×1.2 |
| **P2: the structure solve** | SIMD kernels with runtime AVX2/NEON dispatch, fewer passes over the elements, less memory traffic when many structures step at once | **G2** | Buildings put the reference load's 20,000 beams into real play | Reference tick ≤ 8 ms projected (today 9.1–9.7, a thin margin); eight 5,000-beam bridges at once ≤ 1.3× one alone (today 1.6×) |
| **P3: struck structures** | Subcycling the struck region: its joints in short steps, the rest of the structure in the tick's, energy exact where they meet | **G2** | Collapsing buildings and loads dropped on structures are G2's core | Bridge struck 32.8 → ≤ 5 ms; crash 19.0 → ≤ 5; every crush, crash and impact test within its present tolerance |
| **P4: networks beside the tick** | A network's ordering, analysis and first factorisation built on worker threads and swapped in at a fixed tick; the same for an edit of an island's topology | **G3** (drivetrains), **G4** (grids, heat) | Players build and edit networks while they run | No tick > 5 ms when a 20,000-node network is built, loaded or edited (today 20.7 ms) |
| **P5: meshed pipe networks** | Supernodal Cholesky (SIMD kernels from P2), PCG for large meshed thermal and electrical islands | **G5** | Refineries mesh their pipes | A 2,500-junction meshed network at 60 Hz within its share of the tick (today it is stepped at 20/10 Hz) |
| **P6: many vehicles** | Islands on their own thread groups; impacts of many crashing soft bodies (with P3) | **G6** | Eight players each driving a soft-body vehicle | Ten cars crashing in pairs at 15 m/s: worst tick 91 → ≤ 5 ms |
| **P7: servers on VMs and real hardware** | Parallel grain chosen from the wake-up cost measured at start; the first measurement on a real min-spec CPU | **G4** (first playtest on a rented Linux server) | Dedicated servers run on VMs, and min-spec is only projected so far | Reference tick in a Linux VM within 1.25× of native (today 28.8 ms projected against 9.2); min-spec projection confirmed or the budgets corrected |
| **P8: final pass** | PGO, `dist` builds measured, every play scene on real min-spec hardware on all three operating systems and the Steam Deck | **G11** | Headroom; the hardware matrix | Every gate met on real min-spec hardware |

#### Milestones

| # | Milestone | The loop it makes playable | Performance |
|---|---|---|---|
| G1 | Player & interaction | Start with nothing but clothes, make stone tools, fell a tree, dig ore, carry it home, with friends | P1 |
| G2 | Building system | Build a bridge or a tower from parts, load it, watch it hold or collapse | P2, P3 |
| G3 | Mechanical power & first production chains | Water wheel or windmill → shafts → crusher, mill, furnace → your own steel, made into your own beams | P4 (drivetrains) |
| G4 | Heat engines & electricity | Coal → boiler → steam engine → generator → grid → lights, motors, aluminium | P4 (grids), P7 |
| G5 | Chemistry & fluids | Crude oil → fuels and plastics; air → oxygen; ammonia → fertiliser | P5 |
| G6 | Vehicles | Build a truck, boat or plane from your own parts and fuel; crash it | P6 |
| G7 | Automation & logic | An ore-to-steel line that runs itself under Lua controllers | — |
| G8 | Rockets & space | A rocket from your own aluminium and liquid oxygen, to orbit and the moon | — |
| G9 | Survival, PvE, PvP | Needs, food, fauna, hazards, weapons and raids | P3 rechecked (explosions) |
| G10 | Advanced tech | Chips from quartz, fission, superalloys, composites | — |
| G11 | Polish | The game as others will first meet it | P8 |

#### G1: Player & interaction

Detailed plan: `docs/g1-plan.md` (its sub-milestone numbering supersedes the outline below; it adds joints, a play-scene harness and the art direction with an asset pipeline). **Re-planned 2026-10-22, before G1.5:** there is no landing pod and no kit. The player starts in their clothes, gathers stones and wood, and makes tools with one generic tool system (a head, a haft and a joint, with properties from their materials). Trees are one generic system of species' attributes. Only the fundamentals are built, so that G1 is playable soon.

- **G1.1 The character.** The ball avatar (ADR-0008) becomes a physics character:
  - real mass (about 80 kg plus suit and load); walking as forces through the feet, so slopes are climbable by friction (μ from the materials underfoot) and the ground is pushed back (a raft moves when walked on);
  - jumping, crouching, climbing (ladders and handholds as constraints with a grip-force limit), swimming and drowning (lifts *avatars in water*: buoyancy and drag on the body);
  - a jetpack and EVA thrusters with finite propellant (Δv = Isp·g₀·ln(m₀/m₁));
  - a ragdoll (articulated rigid bodies with joint limits) on big impacts;
  - **injury** from the body's peak deceleration and impulse, not from height.
  
  Lifts *predicting bumps into things*: the client's prediction world holds the bodies near its player, so walking into a crate is predicted rather than corrected.
- **G1.2 Interaction.** Tools (hands, pick, shovel, axe, saw; powered drills with G3–G4) used on what the crosshair points at, within 2.5 m of the hands. Carrying and throwing bodies through a grab constraint whose force limit is the character's strength (a 500 kg crate cannot be lifted, only dragged if friction allows). The time a dig takes comes from the tool's power and the rock's strength in the geology data. Lifts *small edits vanish at a distance*: a dug hole stays visible at every level of detail.
- **G1.3 Mining with real composition.** A dug volume yields its rock's mass and mineral composition at that point (ore grade from the deposit models, ADR-0006) as bulk items. Breaking larger pieces makes boulders (hull bodies). Prospecting: rock colours per material, and an assay tool that reads a sample's composition.
- **G1.4 Trees.** Procedural trees as beam structures from wood species data (density, stiffness and strength along the grain), placed by biome. Felling by cutting members: the tree falls and breaks under its own physics. It is bucked into logs (structure bodies) and wood items.
- **G1.5 Inventory.** Bulk items (mass, composition, temperature) and discrete items (a part instance with material, dimensions and quality). Capacity by mass and volume, not slots. Dropped items are physical bodies; containers hold items. Every transfer is a server-authoritative transaction, saved.
- **G1.6 HUD and first vitals.** Health; suit readouts measured from the surroundings (pressure, temperature, O₂; consumption and needs come in G9); interaction prompts; the inventory window (egui, the game's theme) showing mass and composition.
- **P1: piles** (performance track).
- **Play scenes:** *lumberjack* (20 trees felled on a slope, logs rolling and piling); *quarry* (a 10 m pit dug, 200 boulders and dropped bags); *crowd* (8 players walking, jumping and carrying in one place, ≤ 256 kbit/s each).
- **Demo:** spawn with nothing but clothes, make a stone axe and a pick, fell a tree, dig ore, carry both back, save, load; then the same with a second player.

#### G2: Building system

Detailed plan: `docs/g2-plan.md`. Re-planned 2026-10-23, before G2, with the user (principle 13): building is by hand, as on a real site. Parts are carried, put in place and fastened; nothing snaps, previews or undoes. Blueprints and precise placement move to the tools that make them possible (G3, G6, G7/G10).

- **G2.1 Parts and fasteners** (`creeer_building`), as one generic attribute system:
  - parts are the discrete items the player has (G1.5), with the material and dimensions they were made with: logs, sawn beams and planks, rods and bars, panels and plates. A part's mass, inertia and section properties are computed from its shape and material (lifts *the sandbox's built-in S235*), tested against section tables. Rolled sections (I, H, channels, angles, tubes) are the same system's data once G3's rolling mill makes them;
  - fasteners are data with physical attributes: a joint's stiffness, slip and strength come from its fastener's count, size and material and the parts it joins (Eurocode 5's dowel-type rules for nails and pegs; friction and rope strength for lashings). G2 proves the system with **lashings, wooden pegs and nails**. **Welds and bolts** must later be data entries and one joint law each, with no change to the system. Nails need iron, so survival players get them with G3; creative servers have them from the start.
- **G2.2 Building by hand.** A part is held by the grab constraint (G1.2), so its weight is real: a heavy or overhead beam needs props, rope, scaffolding or a second player, and a brace holds a part still against the body. It is put against another part by hand, as straight as the player judges it, and fastened with a tool action that takes time (lash, drill and peg, hammer a nail). The joint forms where the parts touch. Cutting to length is a saw cut where the player points, or along a mark made against another part to copy its length. Fastening takes the fastener items from the inventory; creative servers give materials freely. Taking a structure apart means pulling the nails, cutting the lashing or sawing the member.
- **G2.3 Structure → physics.** Fastened parts are a structure (ADR-0004): resting on the ground by friction, or anchored by a post dug in and backfilled (its pull-out and lateral strength from the soil). It is quasi-static at rest and collapses when overloaded (yield, buckling, fracture, joints that slip and pull out). Lifts *lateral-torsional buckling* (beams in buildings are bent about their strong axis) and *members colliding with each other* (a collapse must not fold through itself). In survival a structure shows only what a real one does: sag, sway, cracks and failure. The stress overlay is a debug view on creative and test servers.
- **G2.4 Damage and deconstruction.** Cutting members (saw now, torch later); salvage returns the material less the cut's waste; bent parts keep their plastic deformation; pulled fasteners can be reused when undamaged.
- **P2: the structure solve, P3: struck structures** (performance track).
- **Play scenes:** *tower* (a 20-storey steel frame of about 20,000 beams under wind, at rest); *collapse* (the tower loses a ground-floor column); *drop* (the sandbox's 10 t block on a 40 m bridge); *builders* (8 players building by hand at once). The tower and the bridge are assembled by script, as play scenes are.
- **Demo:** in survival, build a log bridge across a gorge by hand with lashings and pegs, load it with crates, watch it sag, overload it until it collapses; on a creative server, the same with nailed timber and the stress overlay.

#### G3: Mechanical power & first production chains

Re-planned 2026-10-22 in outline (the full plan comes at G3's start): the player arrives with stone tools only, so the route to steel is history's, kept short. Machines are built from generic parts wherever their function can be computed from them: a crusher is jaws, a frame and a drive; a mill is a drum, its charge and a drive. A machine is a single designed part only where the physics cannot yet be composed, and that is recorded as a limit.

- **G3.0 From stone to metal.** Fire by friction (a bow drill: heat from the friction's power, ignition from the wood's data); charcoal; clay dug and fired in a pit kiln (ceramics as materials); copper from malachite or native copper in a charcoal-fired pit furnace, cast in clay or stone moulds; bronze with tin from cassiterite. The generic tool (G1.6d) takes metal heads. Each step is one generic process (heating, reduction, casting) from `creeer_thermal` and `creeer_chem`, not a recipe.
- **G3.1 Power sources.** A hand crank; a windmill (aero panels on its blades, lifts *a wind field*: a height profile and gusts); a water wheel fed from a reservoir through a sluice (pipe network). Rivers are not part of the terrain yet: decided at G3's start whether they come here.
- **G3.2 Drivetrains as gameplay** (`creeer_mech`, ADR-0012). Shafts, gears (module, tooth count), belts, chains, clutches and flywheels, as parts on structures (reaction torques). Lifts *gear-mesh and belt compliance*, *belt creep*, *Stribeck friction*, *gearboxes' churning losses* and *two machines on one drivetrain coupled within a step*: chained machines expose all of them.
- **G3.3 First machines and bulk handling.** A jaw crusher and a ball mill (energy from the Bond work index, the product's size distribution), screens, a sawmill (logs → planks and timber members), a trip hammer. Conveyors driven by shafts, chutes, hoppers and bins; material on a belt is a bulk flow, not bodies. A simple arm with taught positions (programmed in G7).
- **G3.4 Concentration.** Gravity concentration (jigs, shaking tables) and magnetic separation first; froth flotation with a collector from the chemistry (recovery against particle size and reagent).
- **G3.5 Heat and smelting.** Charcoal from wood (pyrolysis), coal; a bloomery, crucible furnaces and a blast furnace, with temperatures from the thermal network and reactions from `creeer_chem` (carbothermic reduction of iron oxides; roasting and smelting copper sulfides to matte and copper). Lifts *activity models* as far as smelting needs: a slag model (FeO–SiO₂–CaO) for slags and the metal they leave.
- **G3.6 Shaping.** Casting (solidification time from the heat flow), forging, a rolling mill, wire drawing, a lathe and a mill with tolerances; measuring and marking tools (tape measure, steel rule, square, spirit level, jigs and templates), the second step of principle 13's ladder. A part's microstructure (carbon content, grain size, heat treatment) sets its E, yield strength and UTS, so **a beam rolled from your steel has your steel's strength**.
- **G3.7 Steel.** Pig iron fined to steel with air (Bessemer; oxygen comes in G5), carbon controlled by the chemistry.
- **G3.8 Progression and statistics.** No tech tree: an engineering notebook records what the player has identified and made (minerals, reactions, alloys), and the Codex opens by observation, not by unlocks. Production statistics per machine and network, from their ledgers. The player's machines go abstract when nobody is near (ADR-0020); lifts *machines sharing stocks*.
- **P4: networks beside the tick**, for drivetrains (performance track).
- **Play scenes:** *mill* (a water wheel driving 30 machines through 200 shafts and gears, conveyors carrying ore); *smelter* (a blast furnace and five furnaces, thermal and chemistry); *away* (the factory abstract while its player is away and back, every stock within 1 % of full simulation).
- **Demo:** from an ore outcrop to a steel I-beam rolled from your own steel, built into a structure, its properties shown in the inspector.

#### G4: Heat engines & electricity

- **G4.1 Steam.** Boilers burning fuel by real combustion chemistry, a steam engine (pistons, a real Rankine cycle, efficiency from the states) and a steam turbine, condensers, safety valves. Lifts *two-phase heat transfer* (boiling and condensing on surfaces) and *two-phase pipe flow* (steam lines). A boiler can burst: pressure in its wall's stresses, so a vessel fails as its structure does.
- **G4.2 Generators and motors as gameplay.** Dynamos, alternators that synchronise to a grid, induction motors; wires as parts (material and cross-section → resistance and heating; insulation that melts); switches, fuses and breakers.
- **G4.3 Grids.** AC grids, transformers, transmission lines. Lifts *unbalanced three-phase grids* and *mixed AC/DC* (rectifiers and inverters, as average models: batteries meet grids here).
- **G4.4 Storage.** Lead-acid batteries from their real chemistry, flywheels, pumped hydro.
- **G4.5 Electrochemistry.** Hall-Héroult aluminium (bauxite → Bayer alumina → cryolite-bath electrolysis, Faraday's law), copper electrorefining, an electric arc furnace for steel.
- **P4: networks beside the tick**, for grids and heat; **P7: servers on VMs and real hardware** (performance track).
- **First multiplayer playtest** on a rented Linux server.
- **Play scenes:** *powerplant* (three boilers, two engines, a grid of 20,000 nodes lighting a base); *grid edit* (players connecting and cutting parts of a running grid); *potline* (an aluminium smelter's cells).
- **Demo:** a coal-fired steam plant that lights a base and runs an aluminium smelter.

#### G5: Chemistry & fluids

- **G5.1 Pipes and tanks as gameplay.** Pipes as parts (diameter, material and wall → burst pressure), pumps, valves, tanks, gauges, heat exchangers; leaks and bursts. Lifts *compressors in gas networks* and *heat exchange of pipes with their surroundings*, and *water hammer and cavitation* if pumps and valves show the need.
- **G5.2 Reactors.** Batch, stirred and plug-flow reactors as parts with real kinetics, heating and cooling jackets; runaway reactions as a hazard (heat and kinetics coupled).
- **G5.3 Separation.** Distillation columns, absorbers, crystallisers, filters, dryers. Lifts *distillation-grade vapour pressures* and *activity models* for azeotropes (ethanol and water).
- **G5.4 Chains.** Air separation (oxygen, nitrogen, argon; lifts *cryogenic liquids*, which G8 needs; the jetpack's nitrogen comes from here, or its compressed air from G4's compressors, and nothing refills it for free), Haber-Bosch ammonia → nitric acid → fertiliser, the contact process (sulfuric acid), crude distillation → fuels, cracking → ethylene → polyethylene (plastics as materials with properties), cement and concrete (a structural material for G2's catalog), glass. Oxygen steelmaking replaces Bessemer.
- **P5: meshed pipe networks** (performance track).
- **Play scenes:** *refinery* (a meshed network of 2,500 junctions with columns, at 60 Hz); *chemical plant* (100 reactors).
- **Demo:** crude oil to petrol and plastic; air to liquid oxygen; ammonia to fertiliser.

#### G6: Vehicles

- **G6.1 Wheels.** A tyre model (Pacejka or a brush model fitted to the rubber's properties: load sensitivity, slip, heat); springs and dampers as parts; steering geometry.
- **G6.2 Engines and drivetrains.** Combustion engines (Otto and Diesel cycles from G5's fuels: torque, consumption, exhaust composition), clutches, gearboxes and differentials; electric drive from G4. Lifts *a drivetrain feeling its body's linear acceleration*.
- **G6.3 Building and driving vehicles.** Vehicles are free structures built with G2's tools, made precise with G3's measuring tools and paper drawings (principle 13: paper from wood pulp, a drawing board, dimensions written on the drawing and followed with the tape measure); they deform and tear in crashes. Seats and controls: the character drives.
- **G6.4 Water.** Boats and propellers. Lifts *waves* (a sea state, roll damping by radiation), *water over the deck*, *sloshing in hulls*, *added mass as a tensor* and *hull drag*.
- **G6.5 Air.** Aircraft from aero panels, lifts *aero interference* (downwash on tailplanes, panels' pitching moments).
- **G6.6 Vehicles in multiplayer.** A driver predicts their own vehicle; others are interpolated; plastic deltas of crashed vehicles within the bandwidth budget.
- **P6: many vehicles** (performance track).
- **Play scenes:** *crash* (ten cars crashing in pairs at 15 m/s); *convoy* (8 players driving trucks over terrain); *harbour* (five boats in waves); *airfield* (three planes).
- **Demo:** a truck with a diesel engine built from your own steel hauling ore; a crash test; a boat; a small plane.

#### G7: Automation & logic

- **G7.1 Sensors and actuators.** Position, speed, pressure, temperature, level, composition (a spectrometer) and item sensors; valves, motors, pistons, arms as actuators.
- **G7.2 Signals.** Gates and wires as parts; lifts *messages between controllers* beyond wires (buses, radio).
- **G7.3 Lua controllers in the game** (ADR-0015). A code editor window, the instruction budget shown, prints and stepping for debugging. *Saving a script's functions and coroutines* is decided here (lift or keep restarting from source).
- **G7.4 Logistics.** Programmable arms, sorting by composition, routing items between machines; carts on rails.
- **Play scenes:** *automated factory* (500 controllers at their budgets, 1,000 sensors).
- **Demo:** the G3 ore-to-steel line running by itself under Lua control.

#### G8: Rockets & space

- **G8.1 Rocket engines.** Isp from the propellants' chemistry (combustion equilibrium → chamber temperature, γ, molar mass → c*, nozzle expansion), pressure-fed and pump-fed cycles, gimbals; cryogenic tanks that boil off (thermal).
- **G8.2 Staging.** Decouplers, launch clamps, the Δv and staging calculator.
- **G8.3 Flight.** Maneuver nodes on the map (lifts *maneuver nodes*); lifts *the transonic gap*; re-entry heating with heat-shield and ablative materials.
- **G8.4 Other worlds.** Landing on and building on moons and planets: lifts *other bodies' terrain* (more than the nearest body streamed, eclipses) and *the sea in the solar-system view*; *J2* if sun-synchronous orbits are wanted. Life support for bases is basic here, full in G9.
- **G8.5 Moving bodies.** Engines and mass drivers on asteroids push them (ADR-0007); lifts *spins never change* (pushes torque bodies).
- **Play scenes:** *launch* (a 2,000-part rocket at lift-off, its structure flexing under thrust and aero); *station* (two players assembling a station in orbit); *landing* (on the moon).
- **Demo:** a rocket of your own aluminium on liquid oxygen and kerosene, to orbit and down on the moon.

#### G9: Survival, PvE, PvP

- **G9.1 Needs.** Hunger, thirst (water quality from its chemistry), O₂ and CO₂ (a suit and base life support with real gas exchange), body temperature (the body as a thermal node with metabolism), radiation dose. Food: crops that need soil, water, light and G5's fertiliser; cooking.
- **G9.2 Fauna.** Procedural creatures per planet: articulated physics bodies, behaviour-tree AI, hunting.
- **G9.3 Hazards.** Storms (the wind field), meteor showers, quakes (ground motion into structures), extreme temperatures, radiation belts. Lifts *terrain edits far from players*: a landslide or crater wakes what stands there first.
- **G9.4 Weapons.** Ballistics with drag, penetration from the target's material, explosives as blast pressure on structures; weapons are manufactured parts.
- **G9.5 PvP rules.** A PvP toggle, raid windows, land claims. Lifts *secure connections* (connect tokens) and *signed packs* for public servers.
- **P3 rechecked** with many struck regions at once (explosions).
- **Play scenes:** *raid* (8 players with explosives against a fortified base); *storm* (a base in a storm); *herd* (50 creatures).
- **Demo:** survive a night on a new planet: food, air, a storm, a raid.

#### G10: Advanced tech

- **G10.1 Semiconductors.** Quartz → metallurgical silicon → the Siemens process → Czochralski → wafers → lithography → chips. Lifts *transistors* and electronic components, and chips feed G7's controllers (the optional RISC-V CPU is decided here). Computers run CAD and drive CNC machines, the last step of principle 13's ladder: design files (data files validated like a pack) are shared between players, and machines and robot arms build from them exactly. Whether a simpler CAD comes earlier on G7's controllers is decided at G7.
- **G10.2 Fission.** Uranium deposits (ADR-0006) → enrichment → reactors with kinetics and decay heat, shielding, radiation.
- **G10.3 Materials.** Stainless steels, superalloys, titanium (Kroll), carbon-fibre composites. Lifts *local buckling of thin walls* (shell elements) if thin aerospace structures need it.
- **Demo:** a chip made from sand running a controller; a fission plant on the grid.

#### G11: Polish

- **G11.1 Look and sound.** The art pass (the Factorio-like style everywhere), audio (`kira`). With it comes the polish deferred from earlier milestones (principle 10, `docs/future-improvements.md`): decoratives, the UI's theme and font, icons in the inventory, golden images of the UI, a first-person camera, and the ragdoll's off-centre collision boxes.
- **G11.2 Learning and the start.** The complete Codex with its formulas, and onboarding for the first hour. The arrival's staging and story (a landing pod or another start), which never gives the player more than the clothes they wear.
- **G11.3 Settings.** Quality presets, the render-scale slider, temporal upscaling, MSAA settings (lifts *MSAA memory*), accessibility, key bindings.
- **G11.4 Modding.** Modding docs and the Lua scripting API for mods.
- **G11.5 Shipping.** Packaging for all three operating systems and the Steam Deck (§5).
- **P8: final pass** (performance track).

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
