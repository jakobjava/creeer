# ADR-0017: Fast ticks without loss: impacts, Newton and friction redesigned

- **Status:** Accepted (2026-10-17); closed 2026-10-18. Every part was measured; what paid is in (bit for bit), the rest is recorded below, and the 2–3 ms target is deferred by the user (`docs/future-improvements.md`).
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

## Decision

The user's answer (2026-10-17): optimise for the best times with maximum realism, and so that similar simulations later in the game gain too. All parts are to be built, each kept only where its measurements pass; subcycling is held in reserve for impacts that A1 cannot make cheap enough.

## Measured: B1 and C (2026-10-17)

### How it was measured

- **Where the iterations go.** A trace of every iteration in the rock pile's worst ticks (groups of four or more rocks): 51 % come after a friction refresh, 33 % are cut short where a contact closes (`Coupling::closing`), 12 % backtrack, and 44 % are spent within a thousand tolerances of the end. Newton converges linearly there, at rates of 0.1–0.7.
- **Upper bounds first (oracles).** Newton's matrix for a group of rigid bodies built from central differences of the exact gradient and solved densely, with friction's λ lagged (B1's bound) or following the state (C's bound). Iterations a step, groups of four or more, the pile's first four seconds, friction consistent in every variant:

| matrix | iterations a step |
|---|---|
| the solver's own (Gauss–Newton, L-BFGS), lag refreshed until consistent | 16.1 |
| exact Hessian, λ lagged (B1's bound) | 15.0 |
| exact Jacobian, λ following (C's bound) | 9.4 |
| the solver's own plus the analytic coupling term, λ following | 12.2 |

- **Same inputs for both.** Every tick of a reference run is stepped once more from its own saved state by each variant: chaotic piles go their own way after any change, and whole runs compared differently grouped piles.

### B1: not built

The exact Hessian of the lagged problem saves 7 % of the iterations. The turning terms it adds are worth that only with C, and C is not adopted.

### C: built, measured, not adopted

- **Built:** each contact's φᵢ·ψᵢᵀ (friction's gradient per unit of λ times ∂λ/∂x, a face's corners through ∂F/∂δ itself, not the matrix's tangent seen through the face's rigid motions), checked against differences of the gradient (700 states, worst 3 %); GMRES on the factorised matrix as its preconditioner (four iterations a solve on average); λ refreshed at every accepted iterate.
- **From the first iterate:** 31 % fewer iterations in the pile's large groups, but its global phase, where contacts open and close, has no energy to guarantee progress. λ flipped with a contact between two iterates for 500 iterations, and settled piles crept at a rate of 0.98 until the matrix was made again at every iteration.
- **After the lagged solve first converges** (or after its three refreshes), against the solver as it is, same inputs:

| | rocks, lagged | rocks, following | stacks, lagged | stacks, following |
|---|---|---|---|---|
| iterations | 42,940 | 40,312 | 8,425 | 8,799 |
| time (900 steps) | 3.05 s | 3.03 s | 178 ms | 211 ms |
| worst step | 20.5 ms | 26.6 ms | 7.4 ms | 12.7 ms |
| steps with friction > 5 % off | 169 | 16 | 25 | 2 |

  Its direction aims at the consistent fixed point, not at the minimum of the step's objective with λ held, and the line search takes more trials (292 against 176 in one step); making the matrix every iteration cost 4 ms against 1.2. It fails the gate. The code is kept outside the tree for the realism fix below.

### A realism defect found: friction on stale normal forces

The lag's cap of three refreshes leaves 19 % of the rock pile's steps (169 of 900) and 3 % of the stacks' ending with some contact's friction (one carrying at least 1 % of the group's load) on a normal force more than 5 % off its own. Refreshing until consistent fixes it (1 step in 900) at 27 % more of the pile's iterations; following λ after the lagged solve fixes most of it (16) at no cost in iterations but with worse worst steps. Recorded in `docs/future-improvements.md`, to be fixed together with the speed it costs.

## Measured: A1 (2026-10-17)

### Built

Impact substeps chosen by BDF2's local error, estimated as 4/7 of how far a step ends from its second-order prediction x + h·v + ½h²·a (Milne's device): doubled under an eighth of the tolerance, halved past it, on the ladder 0.5 ms·2ʲ (so that the matrix is refactorised only when the step changes). The shortest step was kept wherever a contact would close within the step or had touched within the shortest step before, and wherever a flowing member would tear within it (a tear is found at a step's end).

### Accuracy: as good as the fixed grid, or better

| test | fixed 0.5 ms | controlled, 10⁻⁶ m |
|---|---|---|
| tube, perfectly plastic, 5 / 10 / 20 m/s | −1.42 / +0.15 / +0.61 % | +0.17 / +0.43 / +0.06 % |
| tube, real steel, 5 / 10 / 20 m/s | −1.21 / −1.05 / −0.69 % | −0.96 / −0.95 / −0.27 % |

A step ending where its contact closes starts each impact at its touch: the 5 m/s crush moved from −1.4 % to +0.2 %. Without the rules for touching contacts the first step of a crush was 1 ms (5 % short); without the tear rule a 30 m/s tube tore 7 % late in a 4 ms step.

### Speed: no gain, not adopted

The estimate tells the phases apart (the bridge's: under 10⁻⁷ m falling, up to 5·10⁻⁴ m as the block strikes, 10⁻⁶ m sagging), and the bridge took 6–32 substeps a tick instead of 34. Its ticks did not get cheaper: a longer step takes more Newton iterations, every change of step refactorises the bridge (7–16 factorisations a tick against 4–5), and two more ticks substepped. Worst tick 42 ms against 33, p99 13.7 against 4.8; the crash's p99 halved (2.85 → 1.3 ms), its worst stayed (19 ms). It fails the gate (the bridge's worst under 10 ms). The code is kept outside the tree.

### What the three measurements say

Within this solver, a deforming impact costs its substeps × about 1 ms (a third of it fixed costs, A2), and a pile its iterations × about 0.1 ms in its largest group, which no other core shares. The two converge as Newton can (B1, C) and the impact's short steps are needed while it strikes (A1). 2–3 ms for these scenes needs either the fixed costs and the contacts' evaluation spread over the idle cores, or a different solver architecture: decided with the user.

## Measured: cores within a group, and fixed costs (2026-10-18)

The user's decision after the measurements above: the work on the idle cores and the fixed costs, performance to the most, the 2–3 ms target kept, no corner cut.

### Contacts of a large group in parallel: no gain on this machine

- **Each step stepped twice from its saved state, serially and in parallel** (rayon, a face's law per task, sums in the same order): the same result bit for bit, and slower, 3.88 s against 3.02 s for the pile's 900 ticks.
- **Rayon's fixed cost.** A parallel loop of 45 × 0.8 µs (a pile's faces' laws, 36 µs serially) costs 37 µs even run from within the pool, against 1–8 µs for loops back to back: its idle workers sleep within microseconds, and waking one on Windows takes about as long as the work.
- **Workers kept awake** (tasks yielding to the pool's pending work while a large group solves, `rayon::yield_now`): the loop took 16 µs, and the faces' laws in the pile's worst tick 8.7 ms against 5.5. But five spinning workers slow the serial rest by 14 % (20.3 → 23.2 ms with nothing in parallel at all): the 7800X3D's eight cores carry sixteen threads, and a spinning worker shares its core with the solving one. With both, 21.2 against 21.2 ms.
- Not adopted; the code is kept outside the tree. The structures' own parallel loops do pay (serially the bridge's worst tick took 35.5 ms against 32.8).

### What did pay: the faces' laws without allocation (bit for bit)

Their partly pressed triangles (97 % of the 100,000 the pile's busiest ticks integrate) allocated their polygons twice each. On the stack and found once for both integrals: the pile's total 2.94 → 2.73 s, p99 15.8 → 14.2 ms, worst 20.6 → 18.2 ms, every golden hash unchanged.

### Fixed costs of the impact substeps

In the bridge's worst tick (36 ms, 34 substeps): setup 4.3 ms, of which the joint solve of the block on the bridge 3.9 (each substep turns the nine joints' 27 columns A⁻¹·E, full-length, with the nodes' rotations); the energy guard 2.7; prediction 2.8; yield 1.6. Its iterations: 4.4 a substep, the structure's solve 48 µs and an evaluation 70 µs each; 70 of its 148 iterations after friction refreshes, which do not come from rising forces (λ extrapolated from the substep before changed none of them: contacts under the ringing block switch on and off), but halved the crash's p99 (2.99 → 1.52 ms); not adopted, as it changes the trajectories of the saved fixtures for that alone.

### Where this leaves 2–3 ms

A deforming impact in this design costs its substeps × about 1 ms, all of them needed while it strikes, and its fixed costs are a third of that: even without any, the bridge's worst tick would be about 22 ms. A pile's worst tick is its largest group's iterations, about 40 a step at 0.1 ms each, on one core. Reaching 2–3 ms needs a different method for these cases: subcycling the struck region (its few joints in short steps, the rest of the structure in the tick's), or the contact solve of a pile restructured so that its work splits into pieces large enough for the cores.

## Closed (2026-10-18)

The user's decision: keep the performance as it stands and move on, rather than spend weeks on a different method. Where `lag-fix` ends (development machine, best of three):

| scene | worst tick | p99 | worst when the gates came in (f0c4383) |
|---|---|---|---|
| bridge | 32.8 ms | 4.9 ms | 38 ms |
| rocks | 18.2 ms | 14.2 ms | 20.7 ms |
| crash | 19.0 ms | 3.0 ms | 38 ms |
| stacks | 6.3 ms | 1.8 ms | 7.0 ms |

At the branch's start the bridge froze for half a second in ticks of about 120 ms, and the rock pile ran slower than real time in ticks of 17–33 ms.

The worst ticks are rare and hard: a 10 t block striking a bridge, a pile tumbling. Gated: the Newton iterations of every scene's worst tick in CI (`physics_bench_tests.rs`), the time of the four scenes' worst and p99 ticks in `cargo xtask perf` (ceilings about a quarter above the measurements). Left for later (`docs/future-improvements.md`): subcycling struck regions; a large pile's contact solve split into pieces large enough for the cores; friction on stale normal forces (a realism limit found here).
