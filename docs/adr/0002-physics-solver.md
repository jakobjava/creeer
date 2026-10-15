# ADR-0002: Physics solver: XPBD for dynamics, exact FEM for structures

- **Status:** Accepted for F0; the stiff-dynamics solver choice stays open until the F2 comparison spike.
- **Date:** 2026-10-15
- **Evidence:** `crates/creeer_physics/src/spike.rs`, `cargo run --release -p creeer_tools -- bench-spikes`

## Context

The plan (§4) proposed one unified XPBD solver running 8–20 substeps per 60 Hz tick for every structure: buildings, bridges, vehicles and rockets. F0 spike (a) had to check two things: whether that is fast enough (5,000 beams within 4 ms per tick on the minimum-spec CPU), and how accurate it is for real steel.

## Experiment

The test structure is a 48 m simply supported steel box-truss bridge: 24 bays, 2 m deep, 10 cm² members, E = 200 GPa. It has 317 members and 100 nodes, with a pin bearing at one end and a roller bearing at the other. For the speed test, 16 of these bridges run side by side as independent islands (5,072 beams).

Accuracy is measured as the static midspan sag under self-weight, against two references:

1. **Timoshenko beam theory** (bending + diagonal shear): 104.14 mm.
2. **Exact linear FEM of the same discrete truss** (direct stiffness method, Cholesky): **98.83 mm**. This agrees with beam theory within 5.1%, which is expected for 24 discrete bays. That validates the FEM solver, which then serves as ground truth.

Measured on an AMD Ryzen 7 7800X3D (8 cores / 16 threads). The minimum-spec CPU (Ryzen 5 2600 class) is roughly 2× slower per core.

### Speed (ms per 60 Hz tick, 5,072 beams)

| Substeps | Sequential p50 / p99 | Parallel islands p50 / p99 |
|---:|---|---|
| 8 | 0.52 / 0.56 | 0.054 / 0.22 |
| 20 | 1.30 / 1.47 | 0.13 / 0.34 |
| 50 | 3.24 / 4.13 | 0.32 / 0.52 |

Parallel stepping of islands is **bit-identical** to sequential stepping (tested).

### Accuracy (static sag vs. exact FEM)

| Substeps | XPBD sag | XPBD / FEM |
|---:|---:|---:|
| 8 | 853 mm | **8.6× too soft** |
| 20 | 217 mm | 2.2× |
| 50 | 117 mm | 1.19× |
| 100 | 103.5 mm | 1.05× |
| 200 | 100.1 mm | 1.01× |

The spike also caught a modelling mistake: pinning both ends made the bridge act as a tied arch (0.65× of theory). Real bridges have a roller bearing, and so does the spike now. Supports are modelled with a per-axis inverse mass.

## Findings

1. **XPBD is very fast and unconditionally stable**, and it converges to the correct answer.
2. **At 8–20 substeps it is far too soft for steel** (2–9×). For a game whose core promise is realistic structures, that's unacceptable. A bridge that sags 85 cm instead of 10 cm is simply wrong.
3. XPBD needs about **100–200 substeps** for this structure. That costs 12–25× the planned budget, and the required count **grows with structure size**: one Gauss-Seidel pass per substep spreads corrections only a few members per substep. A 5,000-beam single structure (one island, no island parallelism) at 200 substeps would take about 13 ms per tick on this CPU, over budget.
4. An exact sparse linear solve (FEM) is cheap for static structures and needs no tuning.

## Decision

1. **XPBD stays the dynamics core:** contacts, collisions, joints, ropes, soft and loose objects, fast motion, and anything where stability matters more than exact stiffness. Islands step in parallel.
2. **Structures at rest are solved with exact quasi-static FEM:** buildings, bridges, parked vehicles, any sleeping island. K·u = f is solved with a sparse Cholesky factorisation, refactored only when the structure or its supports change. Stresses, yield and failure are evaluated from this exact solution. That's what Poly Bridge-style collapse needs.
3. **Moving stiff structures** (driving vehicles, flying rockets) need a solver that is both accurate and fast. Before writing the production solver, F2 starts with a comparison spike of three candidates against the FEM reference:
   - XPBD with **adaptive substeps per island** (substep count from stiffness, mass ratio and graph diameter);
   - **Vertex Block Descent / Augmented VBD** (Chen et al. 2024/2025): per-vertex Newton steps, graph-coloured, reported to converge much faster for stiff systems;
   - **implicit Euler with a sparse direct solve** (co-rotational, factorisation reused across substeps).

   Pass criteria: ≤ 5% error against FEM in the static case, correct first natural frequency (±5%) in the dynamic case, stable under impacts, and ≤ 4 ms per tick for 5,000 beams **in a single island** on the minimum-spec CPU.
4. Every physics change keeps the FEM reference and the characterisation tests in `spike.rs` (and their F2 successors) green.

## Consequences

- The plan's "XPBD 8–20 substeps" assumption (plan.md §1.5, §4) is replaced by this ADR.
- F2 grows by one spike, and gains a sparse linear-algebra dependency (`faer` is already in the stack).
- The FEM reference doubles as an in-game engineering tool: exact stress and deflection overlays for buildings.
