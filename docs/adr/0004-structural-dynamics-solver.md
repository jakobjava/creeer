# ADR-0004: Structural dynamics: implicit BDF2 with a prefactorised sparse direct solve

- **Status:** Accepted
- **Date:** 2026-10-15
- **Supersedes:** the open "moving stiff structures" question of ADR-0002 (decision point 3)
- **Evidence:** `crates/creeer_physics/src/spike/` (`implicit.rs`, `implicit/newton.rs`, `vbd.rs`, `dynamics.rs`, `compare_tests.rs`), `crates/creeer_physics/src/sparse/`, `crates/creeer_physics/src/rotation.rs`, `cargo run --release -p creeer_tools -- bench-spikes`

## Context

ADR-0002 found that XPBD at 8–20 substeps makes steel 2–9× too soft, and needs 100–200 substeps to be accurate. That is too slow for one large structure (an island cannot be split over threads). It asked for a comparison of three candidates for moving stiff structures (vehicles, rockets, cranes, anything built from steel that moves), with these pass criteria:

1. static deflection within 5% of the exact FEM;
2. first natural frequency within ±5%;
3. stable under impacts;
4. at most 4 ms per 60 Hz tick for 5,000 beams **in a single island** on the minimum-spec CPU (Ryzen 5 2600 class, taken as 2× slower per core than the 7800X3D the measurements ran on).

## References used for validation

- **Static:** the exact linear FEM from ADR-0002 (98.83 mm midspan sag of the 48 m reference bridge). The new sparse solver reproduces the dense one to 1e-10.
- **Dynamic:** the bridge starts undeformed and gravity is switched on. It swings about its static sag, mostly in its first vertical bending mode. Two independent references for that mode agree to 0.12%:
  - Rayleigh's method on the exact static shape: 1.774 Hz;
  - a brute-force explicit simulation at h = 1/60,000 s: 1.772 Hz, no damping.
- **Stability:**
  - the midspan of the bridge kicked down at 30 m/s;
  - a free steel block spinning at about 3 rad/s for 10 s.

## Candidates and results

Measured on the 7800X3D. Tick cost is for a 4,946-beam space-frame roof (2D lattice of cubes, the harder case for a direct solver), one island, one core.

| Candidate | Static | Frequency | Damping ζ | Tick p50 / p99 |
|---|---|---|---|---|
| XPBD, 20 substeps | 2.2× too soft | −32% | 0.2% | ≈1.6 ms |
| XPBD, 100 substeps | 1.05× | −2.2% | 0.1% | 7.8 / 8.1 ms ✗ |
| VBD, 50 iterations | does not settle | −51% | unstable (−4%) | ≈5 ms |
| VBD, 200 iterations | 1.002× | −22% ✗ | 2.6% | 21 / 22 ms ✗ |
| Implicit backward Euler, 1 step/tick | exact | −1.1% | **9.1%** ✗ | ≈0.5 ms |
| **Implicit BDF2, 1 step/tick** | **exact** | **−1.1%** | **0.14%** | **0.52 / 0.72 ms** |
| Implicit BDF2, 2 steps/tick | exact | −0.3% | 0.02% | 0.70 / 1.06 ms |

Projected to the minimum-spec CPU, BDF2 at 1 step per tick costs about 1.4 ms at p99, against a 4 ms budget.

### What the spike found along the way

1. **VBD is correct but converges far too slowly for steel structures.**
   - With 5,000 iterations per step it reproduces backward Euler to 4 digits (tested), so the implementation is right.
   - At affordable iteration counts it is badly wrong. Gauss-Seidel style node updates spread information only a few members per iteration, and the bending of a 48 m bridge involves the whole structure.
   - Its recommended warm start had to be the paper's restricted form (previous acceleration along gravity, clamped). The unrestricted form was measured to go unstable.
   - Augmented VBD only adds an augmented-Lagrangian term for *hard* constraints. With finite-stiffness members it behaves as VBD, so it does not change this result.
2. **Backward Euler gets the frequency right but damps the motion away.** ζ = 9% at one step per tick, against about 0.5–2% for real steel structures. BDF2 has the same cost and almost no artificial damping. Real damping will be added explicitly, per material (Rayleigh damping, which keeps the matrix constant).
3. **The stiffness matrix of the rest shape is only valid while the body has not rotated.** A spinning free block tears itself apart (member strain 1.1) when the solver uses the rest-frame matrix.
   - The fix keeps the one factorisation. Find the body's overall rotation R by shape matching (`rotation.rs`), rotate forces into the body frame, solve, and rotate back. This is exact because every node's mass is the same on all three axes: M + β²·R·K·Rᵀ = R·(M + β²·K)·Rᵀ.
   - The block then stays at a member strain of 2.4·10⁻⁵, which is the real centrifugal stretch (ω²r²ρ/E ≈ 10⁻⁵). This matches refactoring every step at 1/6 to 1/20 of the cost.
4. **A single linearised implicit step explodes under a violent impact**, even with backward Euler (the 30 m/s kick). Members rotate about 15° within one step, and the linearisation overshoots.
   - The fix: treat the step as minimising the incremental potential G(x) = Σ m/(2β²)·|x − x*|² + E(x) (Gast et al. 2015, "optimization integrators"). Take Newton steps that must lower G, halving a step until it does.
   - Energy then never rises above its starting value (tested every tick for 5 s).
   - Normal motion converges in 1–4 iterations of about 0.1 ms each.
5. **The factorisation is only a preconditioner now.** The residual uses exact member lengths and the line search guarantees progress. So a stale factorisation, after a member breaks or yields, makes convergence slower but never makes the result wrong or unstable. Refactoring can therefore run on a worker thread and be swapped in when ready. It costs 0.6–3 ms for 5k beams, and the symbolic analysis is reused when only values change.

## Decision

1. **Stiff structures** (anything built from members: buildings, bridges, vehicles, rockets, machines) are stepped with **implicit BDF2, 1 step per 60 Hz tick**, solved as an energy minimisation:
   - Newton steps with a backtracking line search, at most 4 per step, stopping at 1 µm;
   - a **sparse block Cholesky factorisation of M + β²·K** of the rest shape, computed once and reused;
   - each island's **overall rotation from shape matching** (R = I for anchored structures);
   - refactorisation when the structure changes, asynchronous where possible.

   Two steps per tick remain an option for high-fidelity cases (−0.3% frequency error).
2. **Our own sparse solver** (`creeer_physics::sparse`) instead of `faer`:
   - 3×3 blocks per node, a geometric nested-dissection ordering, left-looking factorisation;
   - plain scalar arithmetic in a fixed order, so results are **bit-identical on every platform** (golden-hash tests for the solve and for the full implicit step). A BLAS-style library picks its summation order from the CPU's vector width.
   - Nested dissection does 4.8× less work than natural ordering on an 80×80 mesh, and 6.8× less on 120×120.
3. **XPBD is no longer the solver for structures.** Its remaining role (contacts, joints, ropes, loose objects, and how they couple to the implicit structures) is decided in the contacts step of F2 (ADR-0005).
4. **VBD/AVBD is rejected** for structures, for the convergence reasons above.
5. Static structures keep the quasi-static FEM of ADR-0002, now on the sparse solver (`sparse_static_displacement`).

## Consequences

- Production work in F2:
  - move the solver out of `spike` into the real physics crate;
  - add beam elements (bending and torsion, 6 DOF per node as two 3×3 blocks);
  - Rayleigh damping;
  - plasticity and fracture, which trigger refactorisation;
  - contacts (ADR-0005).
- The validation tests in `compare_tests.rs` move along and stay green: frequency, damping, static accuracy, spinning body, impact energy and golden hash.
- `cargo xtask perf` now tracks `implicit_bdf2_5k_single_island_p50_ms` and the BDF2 frequency error.
- The plan's "unified XPBD" wording (plan.md §1.5, §4, F2) is replaced by this ADR.
- Risk: a structure with millions of fill blocks, such as a very large compact 3D lattice, would make factorisation expensive. Mitigations: split such structures into substructures (sleeping and static parts use the static FEM), and track factor size in the stress scenarios.

## Addendum 1 (F2): production solver for beam structures

The production solver lives in `creeer_physics::structure`. Validation is in `validation_tests.rs` and `dynamics_tests.rs`; costs come from `bench-spikes` ("F2 beam structures").

### Beam elements (6 degrees of freedom per node)

Members are 3D Timoshenko beams in co-rotational form (`element.rs`). Each element measures six deformations (stretch, twist, and bending at both ends about both section axes) relative to its current chord, so any rigid motion measures exactly zero. For small deformations it is exactly the textbook 12×12 frame element (tested to 12 digits). Its gradients are checked against finite differences.

| Validation | Result |
|---|---|
| Cantilever tip load (δ = PL³/3EI + PL/κGA), tip moment, torsion (TL/GJ), tension (NL/EA) | exact to 1e-9 |
| Large deflection, elastica PL²/EI = 1 (Bisshopp & Drucker) | within 0.3% (vertical), 1% (horizontal) |
| Euler buckling of a clamped column, π²EI/4L² | straight at 0.95 P_cr, buckles at 1.05 P_cr |
| First bending frequency of a cantilever, 1 step per tick | −0.83% (BDF2: −3.5%) |
| Solver error vs. the fully converged solution, tip motion over 10 s | at most 1/10 of the time discretisation's own error (test) |
| Material damping ζ = c·ω/2 | within 10% |
| Free frame tumbling at 2 rad/s for 10 s, no damping | energy within 1%, strain < 1e-4 |
| Frame corner hit at 30 m/s | energy never rises |

### Changes to the decision

1. **Newmark (average acceleration) instead of BDF2 as the default.** BDF2's period error is 3.5% at 2.7 Hz with 60 Hz steps and grows quickly with frequency. Newmark's is about (ωh)²/12, and it adds no artificial damping: high frequencies are damped by the materials' physical damping (stiffness-proportional, ζ = c·ω/2).
2. **Energy guard.** The trapezoidal rule can gain energy in strongly nonlinear events even when converged (Simo & Tarnow 1992; measured ×4.5 in one tick of the 30 m/s impact). Every Newmark step checks total energy, external work included. A step that gained energy is redone with BDF2, which only dissipates.
3. **Converge, don't truncate.** With a hard cap of 6 iterations, both schemes explode on that impact: unconverged steps inject energy. Every step now iterates until converged; the iteration limit is only a safety net (raised from 60 to 500 for piles of tumbling boxes, ADR-0005).
4. **L-BFGS on the factorised matrix** (Liu, Bouaziz & Kavan 2017): about 2× fewer iterations once a structure has deformed noticeably.
5. **Re-linearise when needed.** The matrix is built at the starting shape (often a deformed equilibrium), and rebuilt when a line search cuts a step below 1/4 (large local rotations). Statics rebuild it every iteration (Gauss-Newton). Without this the elastica crawled at 1/32 steps.
6. **Stopping rule from the estimated remaining error**, step·ρ/(1 − ρ) with the observed rate ρ, below **0.3 µm**. The tolerance is set by a test, not by speed: the solver's error in a released cantilever's motion over 10 s must be at most a tenth of the time discretisation's own error (reference: 8× smaller steps, fully converged). Looser settings were measured and rejected:
   - 10 µm with a 0.1 mm "good enough" exit after 6 iterations left the motion 3.9 cm off (of a 10 cm amplitude) after 10 s, through phase drift;
   - 1 µm still tripped the energy guard 15 times in 600 steps, and every BDF2 fallback damps.
7. **Parallel factorisation and solve** over the cuts of the nested dissection (`sparse/numeric.rs`): left-looking factorisation, forward substitution as "pull", backward as "push", plus a row-ordered copy of L. Results are bit-identical on any number of threads (tested on a 1,600-node mesh against the sequential path).
8. **Per-node co-rotation of the factorised matrix.** Before each solve, every node's forces are turned back by the rotation that node has made since the factorisation, and the answer is turned forward again (R·A₀·Rᵀ, a valid SPD preconditioner). The matrix is then exact up to the *relative* rotation across a member. With the matrix rebuilt every iteration Newton converges quadratically (2 iterations), which proved the slow convergence was matrix staleness. Co-rotation brings the fully converged cantilever from over 10 iterations per step down to 7.7 without any refactorisation. It replaces the single body rotation (shape matching), which it contains as the special case of a rigid motion.
9. **Fewer passes over the elements.** The line search's first trial also computes the element forces; when it is accepted (almost always), the next residual is only the per-node sums. The energy guard reuses the elastic energy of the last evaluation, and recomputes it only if the state was changed between steps (checked bit for bit).
10. **Newton starts at a Taylor step** x + h·v + h²/2·a. For a vehicle crossing a bridge this needs 8% fewer iterations than x + h·v.

### Cost per tick (7800X3D, one island, in equilibrium, 20 kN load driving across at 20 m/s)

| Structure | Beams | p50 / p99 (ms) | Iterations |
|---|---|---|---|
| Bridge 260 × 4 × 8 m | 4,832 | 1.7–1.9 / 2.4–2.7 (min-spec ≈ 3.5–3.8 ms, budget 4 ms ✓) | 2.46 |
| Tower 8 × 120 × 8 m | 3,940 | 1.4 / 1.8 | 2.0 |
| Vehicle chassis 24 × 4 × 8 m | 466 | 0.38 / 0.44 | 2.0 |
| Welded block 36 × 8 × 36 m | 4,864 | 2.3 / 2.6 (3D lattices fill in most) | 2.0 |

The bridge's 1.8 ms splits into: sparse solves 0.77, line search (incl. element forces) 0.46, residual sums 0.24, prediction 0.21, energy guard 0.06.

An earlier version of this benchmark moved the load one node per tick (120 m/s, a jump every tick). It was replaced by a realistic vehicle; the new one is harder for the solver, not easier, because the load is shared between two nodes and moves continuously.

### Measured and rejected

- **Supernodal storage (dense panels, CHOLMOD-style), with and without relaxed amalgamation:** slower without SIMD kernels. Solve 1.9–2.6 ms vs 1.3 ms; factorisation 147–174 ms vs 106 ms.
- **Single-precision copy of L for the preconditioner:** no gain, because the solve is not bandwidth-bound.
- **Smallest-separator and plane-aligned cuts:** fill 357k vs 354k blocks, no gain.
- **Keeping L-BFGS pairs across steps:** no gain.
- **AVX2 (x86-64-v3):** welded block 3.5 → 2.6 ms, bridge unchanged. Runtime dispatch for the solve kernel is a later option.

## Addendum 2 (F2): what contacts and plasticity changed

Details are in ADR-0005.

- **Scheme per group:** Newmark stays the default for free structures. Groups with contacts step with BDF2 (−3.5% frequency, within ±5%), impact steps with backward Euler. Impacts that deform a structure are stepped in substeps of at most 0.5 ms.
- **Material damping acts in series with plasticity** (on the elastic strain rate only). A dashpot in parallel carried crash loads far above the members' capacity.
- **The energy guard uses the minimisation's stored elastic energy.** The members' own energy counted the step's not-yet-permanent plastic flow as stored energy.
- **Contacts in the matrix:** a structure's factorised matrix includes its contacts' stiffness, and is refactorised only when that stiffness changes materially. A bridge resting on its piers needs 5 iterations per step instead of 48.
- **Benchmarks after these changes** (p50): bridge 1.91 ms, tower 1.64 ms, chassis 0.50 ms, welded block 2.48 ms. All are within the 4 ms min-spec budget.
