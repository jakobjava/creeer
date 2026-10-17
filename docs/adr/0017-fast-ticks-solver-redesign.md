# ADR-0017: Fast ticks without loss: impacts, Newton and friction redesigned

- **Status:** Proposed (2026-10-17), for review before anything is built
- **Context:** branch `lag-fix`. The sandbox's bridge under a landing block froze for a moment and its rock pile lagged. Performance and maximum realism together are paramount (plan.md): the target is a worst tick of **2–3 ms per scene** on the development machine, with every validation test passing at its present tolerance.
- **Builds on:** ADR-0004 (the implicit structure solver), ADR-0005 (contacts, impacts, plasticity; its F6 Newton amendment)
- **Evidence:** `creeer_client --physics-bench` (timing gates), `physics_bench_tests.rs` (work gates), `physics_profile.rs` (`--features creeer_physics/profile`), the Hessian check (`CREEER_CHECK_HESSIAN=1`)

## Context

### Where the scenes stand

Measured on the development machine after the first three steps on `lag-fix`: the gates, the turn limit measured, and cheaper impact substeps.

| scene | worst tick | p99 | most work in a tick | what the worst tick is |
|---|---|---|---|---|
| bridge (10 t block lands on a 40 m bridge) | 33–36 ms | 4.8 ms | 316 iterations | 34 impact substeps, 4.4 iterations each |
| rocks (22 hulls down a 35° ramp) | 20–21 ms | 15.8 ms | 1,568 | 2 turn substeps, 10 group steps, 150–180 iterations |
| crash (frame struck by a falling block) | 19–20 ms | 2.9 ms | 207 | 34 impact substeps, 14.6 iterations each |
| stacks | 7 ms | 2.0 ms | 1,355 | crates settling in a heap |

### Where the worst ticks go (profiler, one tick each)

**The bridge's worst tick: 36 ms.**
- It takes 34 substeps at about 1.05 ms each.
- Every substep has fixed costs before Newton starts, about 0.33 ms in all:
  - setup 0.13 ms;
  - prediction 0.08 ms;
  - the energy guard 0.08 ms;
  - yield bookkeeping 0.05 ms.
- Its 4.4 iterations take about 0.15 ms each:
  - objective 0.07 ms;
  - solve 0.06 ms;
  - residual.

The tick's cost is roughly **substeps × (0.33 ms + iterations × 0.15 ms)**. Even at one iteration a substep, 34 substeps cost about 16 ms. **The number of substeps is the lever.**

**The crash's worst tick: 20 ms.** It runs 34 substeps and 495 iterations (14.6 a substep): its light joints ring against the wall. Of those iterations, 22 are friction-lag refreshes and 123 come after them. The flow law ran 0.5 million times.

**The rocks' worst tick: 21 ms.**
- The turn limit splits the tick into 2 world steps, with 10 group steps in all.
- They take 151–180 iterations and 200–235 line-search trials.
- Evaluating the objective is 10–11 ms of the tick. Of that, contact laws are 7–9 ms and contact geometry 2 ms.
- **35–50 % of the iterations come after a friction-lag refresh.**
- A group step takes 15–18 iterations, where Newton's method near its solution takes 3–5.

### Why tuning will not reach 2–3 ms

The present design has been tuned hard. Each of these changes kept the physics unchanged, to rounding or within tolerance:
- cached unit columns for the coupled solve;
- the flow law in w = u^(1/q), its rate terms kept and warm-started;
- the closing-step line search;
- the contact geometry memoised per state;
- contacts' levers taken at the iterate;
- FAST_FLOW, which hands slow sag back to 60 Hz.

These changes were measured and rejected:
- per-iteration λ;
- a remainder step after an impact;
- parallel contact loops;
- the turning normal's Jacobian in the matrix;
- an active set;
- L-BFGS without its pairs.

What remains is structural, in three places:
1. **Impacts on structures** are stepped on a fixed 0.5 ms grid for the whole tick, from the first touch until members stop flowing faster than 1/s.
2. **The matrix is Gauss–Newton.** The contact stiffness comes from the gap's gradient alone; the gap's curvature terms (λ·∇²g: the turning normal, the moving closest point) are left out because they are indefinite. Newton then converges linearly, not quadratically, and L-BFGS's pairs only partly make up for it.
3. **Friction's normal force is lagged** (IPC's scheme). Convergence is followed by up to three refreshes of λ, each a further minimisation.

## Proposal

Four parts, built and measured in this order. Each part goes in only if:
- every realism test passes at its present tolerance;
- its scene's work and timing ceilings come down (measured, best of three);
- it is reverted otherwise, as before.

Parts B1 and C change only how the solver reaches the step's minimum, not which minimum it is. Parts A1 and A2 change the time discretisation, and must prove their accuracy against the 0.5 ms grid.

### B1: a regularised true Newton for contacts (convergence only)

- **Each contact's full Hessian.** This is the gap's gradient term plus λ·∇²g: the normal turning with its body, the closest point moving on a ball against a hull, and crossed edges.
- **Projected to positive semi-definite per contact.** An eigen-clamp of each small block (at most 12 × 12, two nodes' six degrees of freedom), as IPC does per element (Li et al. 2020). The matrix keeps the positive curvature Gauss–Newton drops, and is never indefinite. Adding the turning Jacobian unprojected was slower (rejected on this branch): its negative curvature sent the line search back.
- **Measured before it is built.** The Hessian check measures convergence as the spectral radius ρ = ρ(A_true⁻¹(A_used − A_true)) at recorded states. The spike measures ρ with and without the projected terms on the rocks' and the stacks' worst steps. It goes ahead only if ρ falls clearly, say below half its present value.
- **Gate:** the pile's first four seconds in at least 30 % fewer iterations; the scenes' trajectories unchanged within the solver's tolerance.

### C: friction coupled without the outer loop (convergence only)

Measured in order, cheapest first:
1. **λ predicted, not taken from the start.** λ starts each step at the forces of the step's start state. It could start at the predicted state, or at the force each contact carried at the end of the last step (kept with its anchor). If the first refresh then rarely moves λ by more than 5 %, the refreshes vanish.
2. **λ refreshed inside Newton, under a safeguard.** Per-iteration λ updates broke the energy tests (rejected). The candidate is to refresh λ only when the iteration has contracted, ‖r‖ below a fixed fraction of the last refresh's, so the lagged problem is nearly solved each time (an inexact outer loop, as in augmented-Lagrangian solvers).
- **Gate:** the lag's share of iterations from 35–50 % to under 10 %; Coulomb's law (`friction_tests.rs`) and the energy tests unchanged.

### A1: impact substeps of controlled size (discretisation; accuracy proven)

- **Now:** one fixed step of 0.5 ms for the whole tick, chosen once from the crush test. 1 ms was 7 % short at 5 m/s; 0.5 ms is within 1 % from 5 to 20 m/s.
- **Proposed:** the step size is chosen by a local error estimate:
  - the difference between each substep's solution and its predictor (Milne's device, as variable-step BDF codes do), on the velocities of the contact nodes and the plastic strain increments;
  - the tolerance is set so the crush test stays within 1 % of its analytic crush.
  - The first substep after a touch is 0.5 ms (it must resolve a joint ringing against the wall, a period of 2–3 ms). The step then grows while the estimate allows, and shrinks again on a new touch.
  - BDF2 takes its variable-step coefficients; the history is reset where impulses change velocities, as it is now.
- **What it buys depends on how long the deceleration lasts.** That is unknown until measured. In the bridge's ten crushing ticks the block has already stopped, and the members sag at 1–10 /s against an almost constant yield force, which BDF2 integrates exactly. That is where large steps should be allowed.
- **Gate:**
  - the crush tests within 1 % from 5 to 20 m/s;
  - the bridge's sag and plastic strain within 1 % of the 0.5 ms grid;
  - the energy books;
  - the crash frame bent within the spread its rounding gives (2.7–3.6 cm);
  - the bridge's worst tick under 10 ms in the first measurement.

### A2: the substep's fixed costs (no physics change)

Setup, prediction, guard and yield bookkeeping cost about 0.33 ms a substep, a third of it. That is about 11 ms of the bridge's 36:
- the contacts' per-step data kept across substeps where only h changed;
- the guard's energy sums made incrementally;
- buffers kept rather than remade.

After A1, fewer substeps remain, and this matters less. It is measured again then, and done if still worth it.

### Not proposed

- **An energy-conserving contact integrator at 60 Hz** (Laursen & Love 2002). It would carry the impact's energy into plastic work rather than into numerical damping. But a 16.7 ms step cannot follow a deceleration capped by yield over a 30 ms crash: it would fail the 1 % crush test.
- **Subcycling only the struck region** (Gravouil–Combescure's multi-time-step coupling). The bridge's substeps would cost the struck members only. It is a large design, and is kept in reserve if A1 falls short.
- **Turn substeps per group.** The rock pile's worst ticks are those whose largest group turns fast itself (measured, `docs/future-improvements.md`), so totals would drop, not the worst tick.

## Expected outcome, and what is not known

These are estimates, each to be replaced by measurement:
- **B1 and C** cut iterations, not evaluation cost. Halving the rocks' iterations would roughly halve their worst tick, to about 10 ms. Reaching 2–3 ms then needs cheaper contact evaluations as well (7–9 ms of laws over 200 evaluations), or fewer group steps.
- **A1** decides the bridge and the crash. If the impact needs about 6 substeps rather than 34, both reach about 5–7 ms. Below that, A2 and B1 (fewer iterations per substep) must close the rest.
- **2–3 ms may not be reached in one pass.** The ceilings come down with every part, and what remains is measured and reported, not deferred.

## Validation that must hold throughout

- **Structures:** `crush_tests.rs` (analytic crush within 1 %), `buckling_tests.rs`, `hardening_tests.rs`, `plastic_tests.rs` (flow balanced to rounding), and the bridge's sag and frequency (ADR-0004).
- **Contacts:** `friction_tests.rs` (Coulomb, unevenly pressed faces, tumbling cubes), `hull_ground_tests.rs` (rest without gaining energy), `overlap_tests.rs` (smash under 5 mm, heap under 8 mm).
- **Energy:** the energy books of impacts and contacts found anew.
- **Golden hashes:** they change only after the trajectories are compared with those of the previous commit, and the change is explained.
- **Server:** the crash frame drawn bent; the avatar's prediction agreeing with the server's.

## Decision requested

1. The order: B1 → C → A1 → A2, each measured before the next. The alternative is A1 first, since the bridge is the worst scene.
2. Whether the impact substeps may vary in size (A1). This changes the time discretisation, within the accuracy proven above.
3. Whether subcycling (Not proposed) should be started now rather than held in reserve.
