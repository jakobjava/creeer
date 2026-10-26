# ADR-0024: Piles and ragdolls (P1)

- **Status:** Second pass done (2026-10-23, the user's "targeted pass"); its time gates not met, what remains measured and recorded (*Second pass*, at the end)
- **Context:** P1 of the performance track (plan.md, `docs/g1-plan.md` G1.8), brought forward after G1.2d: knocked-down players took the play scene *crowd* to 11.9 ms p99 and 58 ms worst (gates 2 and 5 ms). This ADR records what was measured on the way, kept or not, as the work goes.
- **Builds on:** ADR-0005 (contacts, the energy guard, friction's lagged normal forces), ADR-0017 (fast ticks without loss), ADR-0022 (joints), ADR-0023 (the character)
- **Evidence:** `creeer_tools play` with `PLAY_TRACE` (now with the physics' phases when built with `--features creeer_physics/profile`); a replay of single dumped steps (below)

## How it is measured

- **The scene's slow ticks, by phase:** `PLAY_TRACE=<ms> cargo run --release -p creeer_tools --features creeer_physics/profile -- play`.
- **One slow step alone:** the server world's state written before a step that turned out slow, then that step replayed and taken apart (who moves, what each group costs, each Newton iteration). This found the crowd's worst tick to be one step's, reproducible to the millisecond (61 ms replayed against 58 played).
- **Variants on identical states:** every tick of one reference run recorded, each state stepped once by each variant (ADR-0017's method); whole chaotic runs diverge and mislead.

## Found

### The worst ticks: a stiff load on a light body (a bug, fixed by struts)

The crowd's 58–63 ms ticks were a character standing on a ragdoll's hand.
- The legs' reaction went onto what they stand on as a load put on before the step. A load from a stiff spring is stable only on a body heavier than k·h², 8 kg for the legs (30 kN/m at 60 Hz).
- The hand (0.6 kg), lying still, took 3,081 N and 119 N·m. In one step it turned 0.9 rad and left at 64 rad/s. The turn check then redid the whole world's step in 16 substeps.
- **Kept:** the legs as a **strut**, a one-sided, capped spring-damper solved in the step with both bodies (`world/strut.rs`; ADR-0023 amended).

### A ragdoll's own step

On one ragdoll falling onto a plane:
- the step takes about 16 Newton iterations, against 3 to 5 for a well-conditioned one;
- the joints' multipliers ask for a refresh in nearly every step, all three refreshes allowed taken: 44 % of the iterations follow a refresh;
- the early iterations are cut short where the first open contact closes (0.5–7 % of the Newton step);
- an iteration costs about 80 µs, and a ragdoll at rest about 0.45 ms a step.

Against a character's body (105 kg) its line searches take 0.1–3 % steps for dozens of iterations. Still open.

### After the struts: the ragdoll under a crate

With the legs as struts, *crowd* runs differently from its first ticks. A 200 kg crate dropped by one player lands on another's head at tick 83 (its contact faces straight down onto the helmet, the legs at their most), and the ragdoll lies under the crate. The scene: 1.0 ms p50, 29.3 ms p99, 47.4 ms worst; its short run's most work 15,708 Newton iterations (summed over bodies) against CI's ceiling of 490.

The event alone, reproducible (`ragdoll_tests.rs`, `trace_a_crate_on_a_head`, ignored):
- knocked down at tick 27, the ragdoll under the crate costs 2–12 ms a tick for five seconds (787 ms in 300 ticks);
- freshly fallen, its tumbling segments put the whole world in 2–8 turn substeps, each 1–5 ms;
- settled, one step still takes about 21 iterations, 2.0–3.5 ms;
- of those 21: 11 to converge first (line searches cut to 1–35 % twice), then the three refreshes allowed, 3–4 iterations each. Each refresh moves the bodies 2.2, 1.2, then 0.75 mm: the step ends with its lagged quantities still moving;
- what each refresh changes: the joints' multipliers, and friction's normal force by more than 5 % on 39, 18, then 12 of the 112 contacts. The load spreads over many box corners in a statically indeterminate way, and keeps redistributing;
- an iteration costs about 105 µs: the objective 27 µs a line-search trial (the contacts' laws and the joints' terms most of it), the residual 12 µs, the matrix made again 9 µs; besides, 61 µs of prediction a group step and 120 µs of the pass's setup a tick.

A ragdoll lying alone under the crate (dropped on it at rest) is cheaper: 3.3 ms worst, 30 iterations.

### The ragdoll bench (`character/ragdoll_bench.rs`, run by hand)

Seven cases of ten seconds: a character knocked down by a 200 kg crate dropped from six places, and a ragdoll toppled alone.
- **Whole runs** (`ragdoll_bench`) are chaotic: any change moves how long things move. They judge the physics (do they settle?), not speed.
- **Speed is judged on the same states** (`ragdoll_states_written`, then `ragdoll_states_stepped` by each build): about 4,000 states after the falls, each stepped once. Repeatable to the millisecond: 7.18 s, worst 28 ms, 46,784 iterations (1.8 ms and 11.7 iterations a ragdoll step).
- Where it goes: the objective 2.9 s (26 µs a line-search trial, 1.5 trials an iteration), the contacts' laws 1.8 s and the joints' terms 1.3 s across objective, residual and matrix; 43 % of the iterations follow a refresh, and the joints ask for 93 % of the refreshes (with friction in 71 %, alone in 22 %; friction alone 6 %).
- After the fixes below (5.29 s): an iteration costs about 73 µs: the objective 32 µs (1.36 trials of 23 µs), the Gauss–Newton matrix about 25 µs (the coupling's blocks, which the `relinearise` slot does not time, and its factorisation 7.6 µs), the residual 10 µs, the rest about 6 µs. Nothing dominates: the contacts' laws 1.1 s, the joints' terms 1.0 s, the contacts' geometry 0.4 s, a group step's setup 68 µs. 13 iterations a step, 43 % after a refresh.

### Why the joints' share was not theirs (measured 2026-10-18)

What moved in each refresh of the ragdoll bench's states (5.29 s, 35,580 iterations; `Coupling::update_lag`):

| Nothing (converged) | The joints alone | Friction alone | Both |
|---|---|---|---|
| 2,967 | 1,505 | 389 | 5,000 |

The joints take part in 94 % of the refreshes that go on, which is the 93 % above; but friction's normal forces moved in 78 % of them too. Taking either away leaves most refreshes: the joint tree took the joints' and the refreshes' iterations fell by 8 %. Both must go for the refreshes to go: friction within Newton's iterations first, then the tree again.

### Ragdolls that never rest: three quarters of the bench (measured 2026-10-18)

The bench's whole runs, case by case: four cases come to rest within 2–3 s and cost 200–430 ms each; three never do in 10 s and cost 880–2,171 ms each, 4,164 of 5,529 ms in all. In the worst (the crate at (−0.35, 1.0, 0)) every body's velocity is exactly nought from 2.5 s on, yet each step moves bodies 1–2 mm and none falls asleep: a cycle of two steps.

- **Each step's contacts, found anew at the pose the last step ended in, hold 1.4–2.0 J more elastic energy than the last step's contacts held there** (the energy guard's surplus, `structure/group.rs`; about the whole elastic energy of the 2 kN under the 200 kg crate). The guard takes it from the kinetic energy, which leaves the velocities at nought, while the energy found anew pushes the bodies on: the minimum of each step's frozen contacts is not a rest of the contacts found at it.
- **Where:** along the crate's load path. Leaving one pair's contacts out of a step at a time, the crate on a forearm (1, 8), the forearm on a shank (8, 15) and that shank on the ground (15, ground) each break the cycle; the others (7, 13), (8, 14), (10, ground) do not. Over 300 steps the forearm and the other shank (8, 14) change their contacts every step (face patch found, then none), (8, 15) in 181 and (7, 13) in 110.
- **Not the feature choice alone:** Gregorius's bias for coherent contacts (Valve, 2015: prefer faces over edges and one face over the other; Box2D's tolerance likewise), 0.5 and 2 mm here against A's and B's faces and the edges, leaves the surplus as it was, from the same states. What the step freezes besides (each patch's clipped domain, its incident face, its corners) is measured at the step's start; at its end the contacts found anew describe the same pose differently.

This is the review's W2 and W8 (`docs/physics/review-2026-10-18.md`): contacts whose description follows the iterate within the step, and persists across steps, so that the step's minimum is a rest of the contacts as found there. On the bench's seven cases, had the three rested as the other four did, it would have taken about 2.3 s instead of 5.5.

**Corrected on a family of 24 (`ragdoll_rest_bench`, the same day).** The seven cases were too few: every change moved the restlessness from one case to another. Over 24 drops 17 rest within 10 s (23.6 s in all), and the restless ones have more than one cause:

- **The see-saw above** (one case of the seven, and not of the family's most).
- **A slump:** the torso of another case turned on steadily, 0.14 rad in 7 s, a ragdoll settling under the crate as fast as its joints' viscous damping lets it, with nothing to hold it once the loads balance (its joints have no friction).

And most of the time is the active phase (the crate's impact, the fall, the settling: 1–2 ms a tick for one ragdoll), which resting does not touch: making every case rest saves 14 % at most. The persistent manifold therefore does not fix the ragdolls' cost (*Measured and rejected*).

### Found and fixed on the way

- **The energy guard measured turning contacts unturned.** A contact whose normal turns with a body (a face's) or with two edges got its start (the bodies' orientations, the edges' sign) only when its step began, after the guard had measured the contacts: with the identity for a start, each normal was turned by its body's whole orientation. Found on a ragdoll's foot under a crate's edge: 6 mm apart, measured 4 mm deep, 21 J found every step, and the guard took it from the motion (it stopped the foot dead every other step, and the group never slept). A turning contact now starts where it is found, and again at each substep before the guard measures. Every physics test as before; it holds wherever bodies are turned, which most tests' are not.
- **A ragdoll's joints were damped alike** (2 N·m·s/rad): a hanging foot, a hundred times lighter in turning than a thigh, oozed towards rest for seconds and never slept. Each joint's damping is now a damping ratio of 0.5 of the segment beyond it swinging as a pendulum, 2ζ·√(m·g₀·r·I) (ADR-0023). The bench's whole runs: 5.5 → 4.9 s, three → five of seven asleep.

## Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| **The multipliers refreshed by Newton's method** (Δλ = (J·H⁻¹·Jᵀ)⁻¹·c with the step's factorised matrix, H⁻¹·rowᵀ one solve per row; a hinge's misalignment taken along its two directions across the axis; only at a true minimum) | On its own quadratic: a joint's gap 4·10⁻⁶ → 10⁻¹⁰ → 10⁻¹⁴ m in two refreshes. On a ragdoll falling (240 recorded states): refreshes 841 → 760, iterations 6,361 → 6,315, time +18 % (the solves) | Friction's normal forces refresh with the multipliers: a joint's change moves the contacts' forces, their refresh moves the minimum, and the joints open again. Neutral alone; worth trying again only with friction on fresh normal forces. Measured again on the ragdoll bench's states (where the joints ask for 93 % of the refreshes): 6.44 s against 5.29, 38,653 iterations against 35,580. Patch kept: `docs/adr/0024/exact-multipliers.diff` (against b7f8dc7, with its A/B harness) and the new file `joint/exact.rs` beside it |
| Pitfalls found building it | A hinge's three world-axis misalignment rows are dependent (no turn moves it along the axis): the third's tiny pivot gave changes of 10⁵ N·m from micro-radians. A refresh after a line search failing short of the minimum: changes a thousand times too large, the ragdoll thrown apart | (Fixed in the patch, by the two directions across the axis and by refreshing so only at a converged minimum) |
| **An impact only where a contact closes within the step** (closing faster than 5 cm/s and its gap under the step's travel), not wherever one closes fast: a far one (a foot 6 mm under a crate's edge) made every other step of its group an impact step | The frame of the save test no longer hit its wall and bent; a ball smashing into stacks gained 10 J | Impacts travel within a step (a ball strikes crate A, A strikes B): B's contact did not close at the step's start, yet the group must step as an impact (backward Euler) or its energy rises. The group's criterion stays any contact closing fast |
| More refreshes allowed (3 → 4, 5, 6, 8, 12, 30) | One case: 6 settled it by tick 228, 3 and 8 never; the bench (whole runs): 6 against 3 gave 4.95 s against 5.50, six of seven asleep against three, but a worst tick of 43 ms against 18 | Chaotic either way; a step's cost grows with them. Not the lever |
| Fewer substeps (as bounds: joints' substeps off; the world's turn substeps off) | The bench (whole runs): 5.47 s against 5.52; turn substeps off 7.33 s, worst 58 ms | Not where the cost is: without turn substeps the steps are harder |
| The plain refresh over-relaxed (λ += ω·k·c, ω = 2, 3, 5) | ω = 3: refreshes 333 → 28, but iterations after them unchanged and the time −17 % only by a changed trajectory; ω = 2 and 5 slower (2,337 and 1,766 iterations after refreshes against 1,076) | The inner contact solve after a refresh is what costs, not the refreshes' count |

## Measured and set aside (for now)

| What | Measured | Why not first |
|---|---|---|
| **A ragdoll of 11 segments** (hands merged into forearms, feet into shanks: masses and inertias kept; no wrists, no ankles) | The bench's whole runs: 4.05 s against 5.41, worst tick 19 ms against 30, as many asleep | A quarter to a third, not the factor the gates need; a change to the character the user decides on |
| Turn substeps per island instead of for the whole world (a fast-turning box makes every group and the contact detection substep, up to 16 times) | In the crowd's worst tick, the groups that did not need them cost 4.7 ms of 63 | The flung hand's group cost 42 ms of it; struts remove the cause. Still worth doing: listed in G1.8 |
| **Jointed trees stepped in their joint coordinates** (P1's item 1, built as planned below; its joints exact, its direction L-BFGS on Jᵀ·H·J in the tree's coordinates) | Joint tests all pass, several better: a tumbling pair's angular momentum 5·10⁻⁶ and 1.5·10⁻⁵ off (240 and 60 Hz) against 5·10⁻⁵, its joints holding to rounding. The ragdoll bench's states (written anew for the format): 5,335 ms against 5,010, iterations 52,201 against 56,681, refreshes' iterations 18,414 against 20,050; its dense reduced solve and assembly 968 ms against the joint solve's 446 | Slower, and the refreshes it was to end stayed: friction refreshes with the joints (below, *Why the joints' share was not theirs*). Set aside until friction is solved within Newton's iterations, then measured again on top of it. Patch: `docs/adr/0024/joint-tree.patch` (against 42d6733) |
| **Crossed edges kept from step to step** (a persistent manifold for edges, after Gregorius 2015 and Bullet: each step's crossed edges kept in their bodies' frames; where a pair's contacts are crossed edges again, the last step's that still cross within its margin added; at most four a pair) | The 24-case rest bench: 18 of 24 at rest against 17, 25.4 s against 23.6, worst tick 56 ms against 55. Also where the pair's contact is a face patch: 15 of 24, 24.3 s. The see-saw case itself then rocked between a face patch (the forearm's edge on the shank's face, two corners pressed) and crossed edges at the face's border | No gain, a cost. A fixed preference cannot settle face against edge (ADR-0005: rocks needed the edge, this forearm the face), nor hysteresis (rocks crept). Patch: `docs/adr/0024/crossed-edges.patch` (against 60a2218) |
| **The joints' blocks' outer products over each row's nonzeros** (found once, then an index loop) | Bit-identical; the joint terms 0.99 s → 1.39 s on the ragdoll states (with the `profile` feature) | The 36-entry loop the compiler makes of the plain form is faster than indexing with bounds checks. Kept instead (bit-identical, in `joint/blocks.rs`): the blocks' buffers reused (0.95 s) and the loop dense and branch-free (0.93 s): a joint's blocks are arithmetic, about 170 scalars a call with three 6×6 products each |
| Pitfalls found building it (each in the patch, fixed) | The trapezoid on exact constraints rings: the joint's force twice its mean and none on alternate steps, undamped, and the velocities with it (Cardona and Géradin 1989). Carrying the step's mean instead (one force a step, as Gonzalez 1999's energy–momentum method) ends it but is half a step late (a double pendulum 2.3·10⁻³ rad off against 1.75·10⁻³); the mean taken on half a step, (3·mₙ − mₙ₋₁)/2, is second order and quiet. Carrying none (RATTLE's split) or the bodies' own means: force and torque from different poses, angular momentum 1.5 % off in 2 s. BDF2 for trees instead: 10 % off. Gauss–Newton's reduced matrix lacks the forward kinematics' curvature under the step's forces (RedMax's geometric stiffness): 500 iterations of steps cut to a three-hundredth until L-BFGS ran in the tree's coordinates. A free tree's root not at its centre of mass: momentum off 3·10⁻⁹ by the kinematics' second order (a centroidal base fixes it) | Recorded for when it is tried again |
| **The joint system's buffers kept from one iteration to the next** (`structure/rigid_system.rs`: nine allocations an iteration, found by the profile's heap stacks) | Bit-identical; 5,143–5,258 ms against 4,856–4,965 (three interleaved pairs) | Slower. Patch: `docs/adr/0024/rigid-system-buffers.patch` (against a8a0a1e) |
| **Friction in the ragdoll's joints** (a measurement of the slump: one Coulomb torque for every joint, ADR-0022's joint friction) | The 24-case rest bench: 1 N·m 20 at rest, 27.8 s, worst 119 ms; 2 N·m 20, 27.0 s, 59 ms; 5 N·m 22, 20.4 s, 50 ms; none 17, 23.6 s, 55 ms | Rest comes, but the friction makes the moving steps dearer; the right torques are the body's passive joint resistance, to be taken from biomechanics, not chosen for speed. 14 % at best: not the ragdolls' cost |
| Legs ignoring supports lighter than 10 or 50 kg (a measurement, not a fix) | *crowd* worst 58 → 51 ms, p99 11.9 → 13.7 | Not realistic (a boot on a hand presses it down), and the remaining slow ticks are the ragdolls' own steps |

## What others found (research, 2026-10-18)

**This survey should have come first**, before P1's first measurement: it settled in an hour what weeks of building had circled (reduced coordinates for ragdolls; a split across cores only for large groups). Surveys of prior work are now required before every design (plan.md §1, principle 12; `docs/adr/README.md`).

Before building the split of a large group across cores, the literature on the same problem (stiff contacts, large mass ratios, articulated bodies):

| Work | What it does | What it means here |
|---|---|---|
| Distributed Affine Body Dynamics with Adaptive Consensus (arXiv 2605.15875, SIGGRAPH 2026) | IPC-type minimisation split into regions, each solved by Newton, joined by consensus ADMM on shared bodies with a per-body adaptive penalty (ρ from the body's mass, residual balancing) | Pays at hundreds to tens of thousands of bodies (about 6× on 8 workers for 29,760 spheres); 10–92 ADMM iterations a frame; finer partitions need more; mass and stiffness contrast slows it (1.85× the iterations). Right for large piles, not for a ragdoll's group of 16 bodies at a mass ratio of 300 |
| Schur complement-based substructuring of stiff multibody systems with contact (Peiret et al., ACM TOG 2019) | Subsystems solved by direct solvers, the interface's impulses by the Schur complement: exact, real time at large mass ratios | Makes the linear solve exact and parallel; here the linear solve is about 1 % of a ragdoll's step |
| Augmented Vertex Block Descent (Giles, Diaz, Yuksel, SIGGRAPH 2025) | Per-body block descent with an augmented Lagrangian for hard contacts and joints; GPU, few iterations | Plausible at low iteration counts, converged only with many: below this project's accuracy (ADR-0004 rejected VBD for the same reason) |
| PhysX articulations; MuJoCo (Todorov et al.); Featherstone's methods | Jointed trees in reduced coordinates: the root's pose and the joints' angles | Joints exact by construction (no multipliers to refresh: the joints ask for 93 % of a ragdoll's refreshes here), a sixth of the unknowns, large mass ratios tolerated. The usual objection, closed loops, is answered by the next |
| RedMax (Wang, Weidner, Baxter, Hwang, Kaufman, Sueda, ACM TOG 2019) | Reduced coordinates for the tree, maximal-coordinate constraints where loops close and for contact; implicit integration (BDF1/BDF2 in its later, differentiable version), near-linear solves | A hybrid that keeps this world's joints (ADR-0022) for loops and grabs while ragdolls step in reduced coordinates |
| Barrier-Augmented Lagrangian for GPU-based elastodynamic contact (arXiv 2407.00046, 2024) | Friction updated inside each inexact Newton iteration (fully implicit) instead of lagged | Fewer Newton iterations than IPC's lagged friction there: the same lever as friction on fresh normal forces (ADR-0017's C) |
| Primal–Dual Non-Smooth Friction for Rigid Body Animation (Chen, Ly, Wojtan, ISTA, SIGGRAPH 2024; visualcomputing.ist.ac.at/publications/2024/PDNSF) | Static and dynamic friction, non-smooth, made a smooth unconstrained problem by logarithmic barriers (interior point, primal–dual), converging to the exact solution; Newton with quadratic convergence; shown on stacks, grain piles, granular flows, interlocking assemblies | Normal forces and friction solved together: no lagged refreshes, true static friction (ours is smoothed), quadratic convergence where the pile now creeps linearly. To adapt to compliant laws (Hertz, patches) in a position-level incremental potential, and without joints shown; no timings on its page. Read first for G1.8's friction, with 2407.00046 (added 2026-10-18, not yet read in full) |

Sources: arxiv.org/html/2605.15875v1; doi.org/10.1145/3355621; graphics.cs.utah.edu/research/projects/avbd; docs.omniverse.nvidia.com (PhysX articulations); people.engr.tamu.edu/sueda/projects/redmax; arxiv.org/pdf/2407.00046.

**Measured here:** parallel regions within one Newton iteration (three regions of 20 µs, 10 µs of serial work between, 16 threads): 65 µs an iteration against 99 serially, at best 1.5×; with regions of 10 µs, no gain (`rayon_bound_tests.rs`).

## P1's plan from here (2026-10-18)

### 1. Jointed trees stepped in their joint coordinates

**Prior work** (above, and for this item):
- RedMax (Wang et al. 2019; S. Sueda's notes, `github.com/sueda/redmax`, *Analytically Differentiable Articulated Rigid Body Dynamics*, 2021): maximal and reduced coordinates joined by the Jacobian J (maximal twists from joint rates), the reduced mass Jᵀ·M·J, forces Jᵀ·f; implicit BDF1/BDF2 by Newton in reduced coordinates; constraints only where loops close.
- Embedded IPC (2024) for the same mapping in an incremental potential: ∇_q E = Jᵀ·∇_x E, ∇²_q E = Jᵀ·∇²_x E·J (exact there, as its map is linear).
- Rigid-IPC (Ferguson et al. 2021): an incremental potential written in rigid bodies' own poses.
- Drake's SAP (Castro, Permenter, Han, IEEE T-RO 2022) and MuJoCo solve articulated robots in generalized coordinates by Newton in a few iterations (SAP: 3–5 a step in steady state). Their velocity-level, linearised time-stepping is not taken: this engine's step is a position-level energy minimisation that carries its structures, plasticity and material contact laws.

**Adapted here:**
- The bodies stay what they are (rigid bodies, entities, saves, replication). In a group of rigid bodies alone, the spanning tree of its joints is found; each Newton step is taken in the tree's coordinates: its root's translation and turn, and each tree joint's turn (a hinge's one, a ball's three, in the child's frame).
- The step's direction solves (Jᵀ·H·J)·δq = Jᵀ·r, with H the group's present matrix (the rigid bodies' blocks and their couplings), J the tree's Jacobian (a body's twist from its ancestors' joints' turns and the root's), other rigid bodies of the group keeping their six coordinates.
- A line search's trial places the tree's bodies by forward kinematics from its root (a child's orientation its parent's times the joint's turned relative orientation, its centre hung from the joint's point): its joints hold exactly, so their constraints' terms vanish and are not refreshed.
- Joints that close loops stay ADR-0022's; limits, damping, friction and drives stay energies of the joints.
- Expected from the measurements: the joints' 93 % share of the refreshes gone, their penalty's stiffness out of the matrix. Measured against `ragdoll_states_stepped` before it is kept.
- **Built and measured: set aside** (*Measured and set aside*): 6.5 % slower, as friction refreshes with the joints (*Why the joints' share was not theirs*). Tried again once friction no longer lags.

### 2. Then, each with its own prior work recorded before it is built

- Contacts whose description follows the iterate and persists across steps (feature identities, as engines keep manifolds): its edge part measured and rejected; the rest kept for the see-saw's realism, not for speed (*Ragdolls that never rest*, corrected).
- **The ragdolls' cost is their active phase** (1–2 ms a tick each while the crate strikes and they fall and settle): where that goes is the next measurement.
- Friction within Newton's iterations (Alart–Curnier; the 2024 barrier-augmented Lagrangian), for its realism defect and its iterations; then the joint tree again on top (the refreshes wait on both).
- Large groups split across cores by consensus ADMM (Distributed ABD), for piles.

### Open items (kept until each is closed)

- *crowd*'s time gates (its CI work gate re-set to 5,200 by the user, 2026-10-18).
- The rock pile's targets (worst ≤ 5 ms, p99 ≤ 2 ms).
- `rayon_bound_tests.rs`: an ignored measurement, kept as this ADR's evidence.
- G1.2e (prediction of nearby bodies) after P1.

## Outcome of the first pass (2026-10-18)

P1 was brought forward from G1.8 to make the game fast enough to finish G1.2. Its gates (*crowd* p99 ≤ 2 ms, worst ≤ 5 ms) ask for five to ten times less work in a crate-on-ragdoll tick, and nothing measured here offers that: each lever is worth 15–35 % at best, and the exact optimisations are down to a few percent each. The first pass ends here; the contact core's redesigns go back to G1.8.

**Kept** (each bit-identical: the ragdoll states take 35,580 iterations before and after; times on the ragdoll states, interleaved runs):

| Change | Found by | Gain |
|---|---|---|
| Struts: the legs' load solved in the step | the crowd's 58 ms tick | the bug behind the worst ticks (above) |
| A face's layer measured for its energy alone in the face's line search (`contact/layer.rs`) | a sampled profile (WPR): `Layer::at` 11 % of the time | 3 % |
| A state's contact geometry looked up without building its key (`contact/geometry.rs`) | the profile's heap stacks (allocation and copies 14 % of the time) | 3.5 % |
| A small structure's node terms summed on its own thread (`structure/objective.rs`) | the same | 2 % |
| The joints' blocks in their own file, buffers reused, the outer product dense (`joint/blocks.rs`) | the phase profile | 1 % |

About 8 % in all (5,276 → about 4,900 ms). Realism is untouched: nothing was loosened. A crate-on-ragdoll tick costs about 1–2 ms a ragdoll while it moves and about 30 ms as the crate strikes; separate ragdolls run on separate cores.

**Rejected or set aside**, each with its numbers in the tables above and its patch beside this ADR: the joint tree (6.5 % slower alone: friction refreshes with the joints), crossed edges kept from step to step (no gain on 24 cases), friction in the ragdoll's joints (rest, but 14 % at most; its torques want biomechanics), the joints' outer product by index (slower), the joint system's buffers reused (`rigid-system-buffers.patch`: bit-identical, 6 % slower: 5,143–5,258 ms against 4,856–4,965).

**Back to G1.8** (`docs/g1-plan.md`), each from the facts measured here: friction within Newton's iterations together with the joint tree (the refreshes wait on both: 35–43 % of a ragdoll's iterations), contacts whose description follows the iterate and persists (the see-saw, the found-anew surplus), the ragdoll's passive joint resistance from biomechanics, the rock pile's targets, large groups across cores.

**The *crowd* work gate** (4,730 iterations against 490): re-set to the measured value a tenth above, 5,200, by the user's decision (2026-10-18; `play/tests.rs`); G1.8 brings it back down. Its time gates (p99 ≤ 2 ms, worst ≤ 5 ms in `cargo xtask perf`) stay, unmet, for G1.8.

## Second pass (G1.8, 2026-10-23)

The user chose a targeted pass after this pass's survey and first measurements. The gates need four to six times the speed, and every lever measured offered a fraction of that. The pass had three parts:
- turn substeps only for the boxes that need them, and only for their groups;
- friction's refreshes within Newton;
- face patches made cheaper per trial.

About twice as fast on the worst ticks was expected. What follows is what was found, kept and rejected.

### Prior work (surveyed first)

Two surveys came before any design: one on solving friction together with the normal force in one Newton iteration, one on how accurate simulators make piles fast.

| Work | What it says | What it means here |
|---|---|---|
| Drake's SAP (Castro, Permenter, Han 2022, arXiv 2110.10107); MuJoCo's Newton solver | 3–5 Newton iterations a step in steady state (21 in a clutter's transient). Geometry is frozen at the step's start, the line search is exact on cached quantities, and friction is convex | Convex friction glides: the gap is offset by μ·h·‖v_t‖, 8 mm a step at 60 Hz for μ = 0.5 at 1 m/s (Castro et al. 2023, arXiv 2312.03908; Le Lidec et al. 2023). Not for this engine |
| Drake's "Lagged" approximation (2312.03908) | Friction on the step's starting normal force, never refreshed: this engine's scheme with no refresh, first order in how the normal force changes | Drake's recommended default. Here it would leave friction on stale forces in every transient, against a realism gate of 5 % |
| Hüeber, Stadler, Wohlmuth 2008; Acary, Brémond, Huber 2018 (FCLIB); Larionov et al. 2022 (arXiv 2211.10618) | Coulomb friction coupled to the normal force has no potential. Coupled exactly, it needs a residual merit, not the energy. Semismooth Newton about halves a fixed point's iterations where compliance keeps the problem full rank | Why ADR-0017's coupled friction (C) lost its line search's footing. The next design must have a residual merit |
| Chen, Ly, Wojtan 2024 (Primal–Dual Non-Smooth Friction) | The normal force is lagged per iterate, with a filtered line search (accepted if either residual falls): 6 iterations on a cube stack, 7–15 on grain piles | The candidate for friction within Newton, with its rules against oscillation |
| Box2D v3, Jolt, PhysX TGS | Contacts are found once a step and kept through large turns (Box2D allows 0.25π a step). Manifolds and their features are cached while motion is small. Islands are split only above 128 constraints (Jolt) | A 22-rock island is below what engines split. The turn limit is what this engine pays where they keep features |
| Peiret et al. 2019; Distributed ABD 2026 (arXiv 2605.15875) | Large islands split across cores: 5× on a rock pile by substructuring; 10–92 ADMM rounds a frame | For hundreds of bodies, not this pile (as the first pass found) |

The surveys' sources are named here; their downloads stay out of the repository.

### Measured: where the slow ticks go

- **The play trace counted the clients' steps.** The physics phases listed for a slow tick included the eight clients' own prediction steps, because the counter is process-wide. `Play::tick` now returns the server's phases alone (`Tick::phases`).
- ***crowd*'s slow phase was a hand in the air.** A fallen ragdoll's hand (0.63 kg) whirled about the elbow at up to 27 rad/s for about 30 ticks. It touched nothing but its own forearm, which it passes. Because the turn limit counted every awake box, the whole 35-body world took 3–9 substeps a tick.
- **The rock pile's slow groups** were measured on the sandbox's 900 ticks, stepped from their saved states (`physics_states.rs`). A slow group step takes 60–75 iterations:
  - about 15 are cut short where a contact closes;
  - about 25 reject the full Newton step and take between a tenth and two fifths of it;
  - about 20 follow friction's refreshes, which move the solution by thousands of tolerances.

  Over the 900 states, 15,880 of 45,563 iterations (35 %) come after a refresh.
- **Why full steps are rejected.** Along a rejected step, each part of the objective was measured at four points.
  - The objective is close to a parabola, but 4–10 times steeper than the direction's model.
  - The excess is friction's. Inertia, the elastic terms and the damping match their model within a few percent.
  - Friction's own exact local quadratic overestimates it, at a sticking contact that starts to slide.
  - The direction used is too long because of L-BFGS pairs taken across stick–slip switches.
  - With pure Newton, the excess lies in sliding contacts turned back past their anchors, where the constant force the model holds reverses.
- **The bound.** With friction off, the same 900 states take 12,012 iterations against 20,662 (−42 %): 3.2 s against 4.5 s, worst step 9.1–9.5 ms against 18.4. Better friction can at most halve the pile's worst steps, and the cost per trial stays.
- **A face patch costs 0.3 µs an integration**, and a face search takes 1.7 integrations. A trial costs about 50 µs because a 17-rock group has about a hundred faces, and every one of them moves in every trial. Caching across trials would have nothing to reuse, so it was not built.
- **The ragdoll bench was broken** (found here). Since G1.3f a ragdoll's segments pass each other (`World::let_pass`), and that is not saved. Loaded from bytes, the bench's states had every limb meeting every other: 270 s instead of about 6. The bench now lets the pairs pass again after loading (`ragdoll_bench.rs`).

### Kept

| Change | Measured |
|---|---|
| **Turn substeps only for boxes the step can bring into contact.** A box (or hull) counts when one of its contacts found at the step's start is closer than 5 mm plus both sides' travel in the step at their present speeds. The detection's margin is twice that and more, so a box that touches nothing reaches nothing within the step. A body held in hands counts whatever it touches (`turn.rs`) | *crowd*'s slow phase: world substeps 3–9 → 1–5 a tick. Every physics test as before |
| **Turn substeps per group.** Each group takes the substeps its own boxes need, and the others step once. A group is bodies joined by contacts found at the start, by joints and by struts, as `pass.rs` groups them. A step restricted to some bodies (`World::only`, not saved) detects, groups and keeps books for those alone, and leaves the others' anchors, groups and left energies as their own step left them | A group stepped beside one that substeps moves exactly as it would alone, bit for bit, through substeps and redos (`turn_tests.rs`) |
| **The turn check's redo per group.** A group whose box turned more than twice what its substeps allow is restored and stepped again alone, in the substeps its turn needs | In *crowd*, one such redo had taken the whole world to 16 substeps: 147 group steps, 79 ms in a tick |
| Benches and tools: `physics_states.rs` (the sandbox's piles on the same states by any build, `DUMP` per state, `work_ensemble`); `creeer_tools play --headings <n>` (an ensemble's timings, with `MEMBER` and `PLAY_TRACE`); the server's phases in the play trace; the ragdoll bench fixed | — |

**A held body needs its substeps** (found by `trees_tests.rs`). The hands' loads are explicit springs on a light body (G1.4). Stepped once instead of in its turn's substeps, a branch carried into a trunk spun up to 26 rad/s. The substeps had been keeping it steady, as in the struts' lesson: a load from a stiff spring is stable only on a body heavier than k·h².

**The result** (*crowd*, full length, eight headings, on the development machine; before is 90258e4):

| | before | after |
|---|---|---|
| p99 tick, median / worst of the eight | 13.5 / 83.8 ms | 11.5 / 25.2 ms |
| worst tick, median / worst | 46.8 / 286 ms | 25.9 / 67 ms |

Single runs say little: with the first change alone, heading 0 went from 10.9 to 13.0 ms p99 because its trajectory changed. The other benches:
- **The rock pile's 900 states:** 4.42–4.60 s, p99 12.2–13.0 ms, worst 17.9–18.9 ms, as before. Rocks seldom spin free, and their worst ticks are one group's iterations.
- **The ragdoll states:** 6.29–6.33 s against 6.36–6.44 s.

*crowd*'s worst ticks now are a single group, a ragdoll with a crate, substepping 9–10 times at about 40 iterations a substep (4–5 ms each), while every other group steps once.

### Measured and rejected (second pass)

A patch with every experiment below, each behind a switch, lies beside this ADR: `docs/adr/0024/second-pass-friction.diff` (against 90258e4). The baseline, unless said otherwise, is the rock pile's 900 states: 20,662 iterations, 4.4–4.6 s, worst 18.0–18.5 ms.

| What | Measured | Why rejected |
|---|---|---|
| **Pure Newton for rigid groups** (no L-BFGS pairs where the matrix is made again every iteration) | Rocks: 19,816 iterations, 3.9 s, worst 15.4–16.8 ms. Stacks: 3,763 iterations against 3,157, worst 13.8 ms against 5.9 | The settling crates need L-BFGS: their worst step became 2.3 times longer |
| A share of the secant μλ/‖u‖ along every sliding contact's slip (0.1, 0.3, 1) | 22,035, 26,702 and 38,417 iterations; with pure Newton 26,293, 40,683 and 81,923 | Stiffer along the motion, sliding creeps (as F6 found) |
| **The secant along the slip only where the direction turns a sliding contact back past its anchor**, with the direction solved again (`contact/reversal.rs`). The friction's model then has its least where the true one has | With L-BFGS: 18,405–19,070 iterations, worst 19.6–20 ms. Pure Newton: 19,774–19,802 (no change). One state fell from 74 to 48 iterations, others rose by up to 46 | Neutral overall: each state's rejected steps have other causes too |
| Friction's λ extrapolated from the last two states (BDF's own extrapolation) | Refreshes 4,724 against 4,862, iterations +1 % | A tumbling pile's forces do not follow their history |
| **Refreshes only for contacts that slide or stick near their cone's edge.** A contact sticking well inside its cone carries the force equilibrium asks of it, whatever λ is | Refreshes 4,752 against 4,862 | In a tumbling pile nearly every loaded contact slides |
| **Refreshing before the lagged solve has converged** (at 30, 300 and 3,000 tolerances; the last pass still converged and was checked as before) | Iterations −13 to −15 % (39,677, 38,832 and 39,401 against 45,563); time −1 to +2 %; p99 worse at 300 | More refreshes for fewer iterations, no time saved |
| The cap's exponential segment removed (a diagnostic: is its stiffening what troubles Newton?) | 24,880 iterations, +20 % | It helps Newton; not the cause |
| **The turn limit as the sweep of a box's corners** (1.5 cm a substep, what the heap's rocks sweep at 0.05 rad; never more substeps than the angle gives) | Ragdoll states −7 % (5.90 s). Two heap rocks of 0.25 m, turning 0.15 rad a step in the two substeps it allowed, sank 27 mm into each other (`overlap_tests.rs`, limit 8 mm) | A many-faced hull changes its contact features with its turn, whatever its size. For boxes alone (a ragdoll's segments) it may hold, but it needs a test of small fast boxes first (next) |

### Found and recorded

- **Tests that a trajectory change tips, by chaos:**
  - The save test's frame meets its wall at tick 45 or 46, depending on how its slide goes. Its window is now 50 ticks.
  - The server's crash test waited 20 s for a block to come to rest on the bent frame. Of eight drops a micrometre apart, the block was still rocking after 20 s in three before this pass and in four after. It lost energy all the while, and one came to rest after 25 s. The wait is now 50 s. The see-saw is this ADR's (*Ragdolls that never rest*).
- **The rock pile's CI work ceiling** (`physics_bench_tests.rs`, 1,670) was set on one trajectory. Eight runs a nanometre apart spread 1,379–2,610 before this pass, four of them over it. With this pass, all eight peak at tick 40 with 1,803, on a state where 90258e4 spends 1,905. On the same states it works less; the trajectory went elsewhere. The ceiling is re-set to 1,985, a tenth above.
- **Goldens renewed:**
  - play *crowd* and *work* at full length, and stress *server* at both lengths: the scenes where a box spun without contact or a group now substeps apart;
  - the planet physics hash (`planet_tests.rs`): its crates and ball within 2.4 µm of the old trajectory after two seconds;
  - the style board's ground close-ups: the fallen engineer lies with his arms a little otherwise, his limbs no longer substepped while they turn in the air as he falls.

  Every other hash is unchanged.

### Where this leaves P1

The gates (*crowd* and the rock pile: worst ≤ 5 ms, p99 ≤ 2 ms) are not met. What remains, measured:

- ***crowd*'s worst ticks:** one ragdoll-and-crate group's turn substeps, 9–10 of them at about 40 iterations each. Next:
  - the sweep limit for boxes alone, proven on small fast boxes;
  - substeps that cost less (each substep starts its Newton afresh).
- **The rock pile:** its largest group's iterations, about half of them friction's (the bound). Next: friction within Newton with a residual merit (Chen 2024; Larionov 2022), then the joint tree again, since the refreshes wait on both.
- The rest is deferred (`docs/future-improvements.md`), by the user's choice of a targeted pass.

## After G1 (2026-10-23): what the new play scenes measured

G1.10 added *lumberjack*, *quarry* and *toolmaking* (`docs/g1-plan.md`). Measured at full length on the development machine (p50 / p99 / worst):

| Scene | Times (ms) | Where the time goes |
|---|---|---|
| quarry | 0.06 / 1.0 / 2.3 | Gates met; the blow that cuts a pit is 2.3 ms (ADR-0032) |
| work | 0.5 / 1.0 / 1.8 | Gates met |
| toolmaking | 0.2 / 5.6 / 8.5 | Ticks while stones are swung in the hands and put down: "group world" 3–4 ms with 40–60 Newton iterations; the hands' explicit springs need turn substeps (Second pass) |
| lumberjack | 1.9 / 11.0 / 29.4 | Felled trunks: detection 3–4 ms a tick (a 12-sided hull's faces sampled against the ground, `ground_hull.rs`), and they rarely rest: 3 of 4 trunks still awake 25 s after the last blow (one creeping 2–4 cm/s, the others held awake by touching it), about 7 ms a tick with nobody near |

Tried for the trunks: rings every 1 m instead of 3 m (to make their faces shorter on curved ground): no better at rest, and slower. The trunks' unrest is the never-resting contacts of the first pass (contacts found anew hold more energy than the step before; the guard zeroes velocities; bodies jitter), not something new; the persistent contacts it needs are this ADR's open work. Each is in `docs/future-improvements.md`.

## G2.0: rest (2026-10-23)

G2.0's task: felled trunks lying on the terrain never come to rest (`docs/g2-plan.md`). The prior work was surveyed before any change (the plan's G2.0 entry): persistent manifolds with feature identity (Box2D, Bullet, Jolt, PhysX), contacts born with no energy, guards that do not zero velocities, clean terrain manifolds, friction regularisation, island sleep.

### How it was measured

- **The scene's state, kept.** *lumberjack* played (1,800 ticks) and left alone for 10 s by the code before this change; its felled parts (players taken out) written as a world, `play/fixtures/lumberjack-felled.bin` (version 12). Stepped from there, trunk by trunk or together, each variant is tried on the very state `rest_probe.rs` measured.
- **Each contact's elastic energy, matched from step to step.** At each step's start every contact found was keyed by its kind (point, bump, patch), its bodies and its point in A's frame, and its energy compared with what the same key held at the last step's end, with the guard's books beside it (its energy before and after, the surplus found anew). This names the contacts whose energy appears from nothing.

### Found

**1. Bumps of the ground found, lost and found again (the trunk creeping alone).** Trunk 5 (82 kg, 19 m) alone on the terrain, tick by tick: one point contact, a *bump* (`ground.rs`: a point of a face or an edge where the ground rises more than 1 mm beyond what the face's patch sees), held 20 µm in at a step's end and was not found at the next step's start, because the face's patch, measured again, now saw the ground there within 1 mm. Without it the trunk sank 26–46 µm into the ground there within the step, unopposed, and the next detection found the bump again holding 9–37 mJ (the guard's surplus 9–100 mJ). The step then threw the trunk 1–2 mm (the probe's *jumps*), the guard took the kinetic energy (velocities to nought), and the cycle began again every 5–6 ticks: the stick-slip creep downhill. With μ = 5 the same cycle, stiffer. Friction, the stick spring and the guard only carried it; the energy came from the bump losing its identity. With no bumps at all (a diagnostic) the trunk slept at 5 s.

**2. Contacts between hulls that hop (the trunks on trunks).** With the bumps kept, three restless cases remained, all between two hulls (`hull_sat.rs`), each one point or patch chosen afresh every step by the separating-axis test:
- *The rims of two bucked halves* (the fixture's trunks 4 and 5, cut apart and touching end to end): the contact is a crossing of two edges of their twelve-sided rims. As the trunk rocks, the deepest crossing moves to the next edge pair, found 66 µm in and holding 60 mJ where the last held 9 mJ at 27 µm. Every few ticks a surplus of 7–51 mJ; the trunk rocks about its axis at 50–180 mrad/s and never sleeps. Edges are the chosen axis throughout.
- *A log across a log* (the scene after this change, bodies 6 and 8): the contact flips between a face patch (7 mJ) and an edge pair's point (38 mJ at 71 µm, the same overlap). The face's separation swings by a millimetre for a turn of 0.3 mrad: a long hull's face axis is set by the other hull's far vertices, metres away (−126, −492, −1,237 µm in three ticks), so the 0.1 mm bias for faces decides one way and then the other. An exact 11-tick cycle, 0.3–0.9 mJ of motion, for good.
- *A trunk against a player's body* (bodies 2 and 4): a face patch the trunk sinks into (3 → 62 mJ in five ticks) until the edge axis is better by 0.1 mm, then a point holding 548 mJ at 161 µm; a 7-tick cycle.

This is ADR-0005's open limit (*Edge contacts and face patches disagree*) and this ADR's first pass (*Ragdolls that never rest*): one physical contact, representations whose laws hold different energies for the same overlap, chosen anew every step.

**3. Rolling with the crown's inertia.** A felled part's inertia includes its crown, whose branches do not collide (ADR-0031's limits): a 125 kg trunk alone rolled to and fro in a hollow at up to 1 rad/s for 6 s before it slept. Each face it rolls onto takes only about 6 % of its energy (Housner's rocking polygon with the crown's radius of gyration, 0.19 m, about a 0.1 m stem) where a real crown would dig in at once. Motion of the model as it is, not of the contacts; recorded with the trees (`docs/future-improvements.md`).

### Kept

| Change | Measured |
|---|---|
| **Bumps kept from step to step** (`ground_rise::bump`, boxes and hulls alike). A bump found again (its point in the body's frame within 0.1 mm of the last step's, kept with its friction anchor) stays while it presses or is within 1 mm of pressing, with the inset the last step gave it while it presses; not pressing, its inset follows the patch again but no deeper than touching. A bump found for the first time is as before (an impact may find it pressed). Saved with the anchors (version 13) | The fixture's trunk alone: asleep at 3.7 s and staying asleep, where before it crept 10 mm in 1.5 s and never slept in 10 s (`play/rest_tests.rs`, in CI, 0.7 s; the rule itself in `ground_tests.rs`). The fixture's trunk 4 alone 6.1 s, its 125 kg trunk alone 7.6 s (rolling, Found 3) |

### Measured and rejected

The experiments below, each behind a switch, are in `docs/adr/0024/g2.0-experiments.diff` (against 2add7e6; switches `REST`, `LOCAL`, `MANIFOLD`, `EDGES`, `ALLEDGES`, `NOKEEP`, and the energy trace above as `TRACE`).

| What | Measured | Why rejected |
|---|---|---|
| **Point contacts born with no energy** (`REST`): a point contact found with no anchor, in a group the last step left, offset to just touching if found pressed in; the offset kept in its anchor and let go only where that adds no energy | The log across a log: its edge point born touching, but the face patch it flips back to was found 11.8 mJ in, and the cycle went on at 0.3 mJ (against 0.9) | A contact born touching lets its body sink unopposed until it carries the load: gravity's work becomes motion as the surplus did. Patches would need it too, and the flips remain |
| **The face's depth where its patch would press** (`LOCAL`: the incident face clipped to the reference face, its deepest point, instead of the slab's separation) | Where a face and an edge cross, the clipped depth is the edge's (−71.3 µm both); where the clip is empty only the slab's separation is left, and the flip came back unchanged. A hull-of-a-box test fails without the same in `sat.rs` | Not the lever: the choice still flips |
| **Edge axes as patches** (`MANIFOLD`, after Bullet's clipping: the face beside the crossing edges most turned to the axis, clipped against the other's) | On the bucked rims the faces beside the edges are the end caps, which do not overlap: no patch, the point as before | A rim needs a manifold of points, not a face |
| **Edges always** (`EDGES`), and **every crossing within 1 mm of the deepest** (`ALLEDGES`, up to four) | The bucked rims unchanged: edges were the axis already, and the pair the trunk rocks onto does not cross within both edges until the last one stops crossing. Every crossing within the margin threw the lighter half off at 9.7 m/s (pairs on the rims' far sides) | The hop is from a crossing that ends to one that begins deeper: a persistent manifold must keep the last step's points beside the new ones, as `crossed-edges.patch` does |
| A synthetic test (a 12 m twelve-sided log, its crown offset, on rough Catmull–Rom terrain at 4° and 7°) | Logs rolled or rested alike with bumps kept or not (asleep 1.4–6.8 s) | It did not reproduce the lost bump; the kept test uses the scene's own state |

### Where this leaves G2.0

- **Trunks asleep within 5 s of the last blow: not met.** The whole scene (`lumberjack_s_trunks_sleep_within_5_s_of_the_last_blow`, run by hand): the last blow at tick 1,747, and three parts still awake 15 s on: a 125 kg trunk against a player (Found 2, third case) and the log across a log (second case). The ground's share is fixed; what remains is between hulls.
- **lumberjack's times** (development machine, full length, p50 / p99 / worst; the machine busy with other builds, so runs spread): this change 0.9–1.7 / 21–28 / 37–59 ms over three runs; 2add7e6 run beside them 1.7–4.1 / 52–84 / 103–164 ms. Gates (2 and 5 ms) not met: the scene's slow ticks are the felling and bucking themselves and the hull–hull groups above, and G1.10's ground sampling (3–4 ms of detection a tick) remains. To be measured again on a quiet machine.
- **Goldens renewed:** play at full and short length. With the save format's version written into the recordings, every scene's hash changes by the version alone (13 against 12); with the version held at 12, *lumberjack* and *toolmaking* hashed as before at short length and *crowd*, *work* and *quarry* differed: crates and boulders on the terrain whose bumps are now kept. Stress at both lengths: *cars* and *server* differ by motion even with the version held at 12 (cars and crates on the terrain), *beams*, *factory* and *flyover* not; *cars* still does 568.5 kJ of plastic work with 80 members buckled. Every test, the physics accuracy tests among them, passes at its tolerance (`cargo xtask ci`).
- **Next** (`docs/future-improvements.md`): a persistent manifold between hulls, its points kept in the bodies' frames while they still touch, the new ones added beside them (Bullet, Jolt; this ADR's `crossed-edges.patch`, measured on the ragdolls only, to be measured again on the fixture's rims); or one law for an edge crossing whichever way it is found.

## G2.0, continued: contacts between hulls (2026-10-24)

The remainder of G2.0: hull–hull contacts that hop (Found 2 above). Measured first on the fixture's pair (bodies 0 and 1 of `lumberjack-felled.bin`, the only pair still awake there), and on the scene's state after its last blow kept as a world (players taken out), so each variant steps the same state. The probes are not kept (a trace of each pair's separations, chosen axis and faces, per tick).

### Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Bullet `btPersistentManifold` (Coumans) | Up to four points a pair, kept in both bodies' frames while they still touch, new points merged by proximity | All its points obey one law; Creëer's points and patches do not, and a point kept beside a patch counts one overlap twice. The persistence that matters here is the friction anchor's |
| Box2D (Catto, GDC 2006–2014): feature ids; v3's speculative contacts | Contact points identified by the features that make them, to warm start their impulses | No impulses to warm start; `anchor.rs` already keys anchors by the point on A |
| Jolt body-pair cache (Rouwé) | A pair's manifold reused while the relative pose changes by less than about 1 mm and 2° | Not taken: a millimetre unseen is many times the micrometre overlaps that hold the energy here |
| Gregorius, "The Separating Axis Test between Convex Polyhedra" (GDC 2013), "Robust Contact Creation" (GDC 2015) | A bias for face axes over edge axes; the incident face as the most antiparallel; clipping | The bias kept; the most antiparallel face is wrong for long hulls (Found 2) |
| Mindlin (1949); Johnson, *Contact Mechanics* §7.4; Cundall & Strack (1979); Tsuji et al. (1992) | A sticking contact dissipates by microslip; discrete element models damp their tangential springs | Measured as a sticking dashpot and rejected (below) |

### Found

1. **The log across a log is an edge lying across a face, and the edge pair names one end.** Body 0's long edge lies across body 1's face (33° apart, the face 3 cm wide). Edge pairs (51, 70) and (53, 70) are the line's two ends; for 600 ticks the test took one or the other (255 and 345 ticks), 27 µm in at one end and 57–94 µm found at the other, the guard zeroing the motion every 5–6 ticks. The face's own separation was −0.2 to −9 mm, set by vertices metres away.
2. **The incident face was the most antiparallel of all.** A long hull has nearly parallel faces at every ring; the most antiparallel lay in another ring, its clip against the reference face was empty, and the pair had no contact for a tick (tick 220; 1 mm in at the next).
3. **A patch with its corners on B forgot its friction every step.** A's point is where B's corner falls on A's face, found afresh; its anchor (B's corner) was under it again each step, so static friction held only within a step. A 40 kg crate on a 400 kg crate on a 20° ramp (μ = 0.6) crept 4.5 mm in 2 s (`stick_tests.rs`); the fixture's logs crept and rocked at 3–5 mrad/s with no guard fallback.
4. **`World::touching` reported a patch at its rectangle's corners**, up to 0.8 m from where two poles cross: with crossing edges now patches, fastening there failed ("not on the body as it was").
5. **The log bridge's stringer never touches its abutments** (`play/log_bridge.rs`, for its author): `across * s.orientation[0]` turns the item's upright frame, whose x is not the world's. The three rods lie within 21° of each other, the stringer 0.8 m beside the abutments, all on the ground from the first tick. A scene set-up fault, not detection's.
6. **The builders scene panicked** once its motion changed: `Hull::new` of a member's half-prism (two nearly coplanar twelve-sided rings) gave edges with one face. The incremental hull took every face a new point saw by more than ε; a face within ε lay between them, so the horizon was no loop.

### Kept

| Change | Measured |
|---|---|
| **An edge lying across a face is that face's patch** (`hull_sat::edge_on_face`; the same in `sat.rs` for boxes, so a box's hull still touches as the box): where the edges are within the bias of the faces or better, their axis within about a degree (`sat::ACROSS`) of a face beside one of them, pressed as deep as the edges (within the bias). That face is the reference, the face beside the other edge most opposed to it the incident; of several, the face the edge lies flattest across | The whole crossing presses (`hull_across_tests.rs`: one patch, about 50 cm², its deepest point within 2 µm of the line's deeper end, tilted ±2 mrad); the fixture pair a patch every tick, the same faces from tick to tick; a box hull and a box alike in 2,000 random poses |
| **The incident face**: of those reaching over the reference face within the margin, the most opposed within the bias of the deepest (`hull_sat::pressing`) | No tick without a contact; boxes as before |
| **A patch's corners on B keep their stick offset** (`Contact::projected`; `pass.rs`, `anchor.rs`), as a rolling contact keeps its (`Kept::Slip`) | The crate on a crate: 4.5 mm → under 10 µm in 2 s, either body as A |
| **`touching` reports a patch at its pressing polygon's middle**, at its least gap (`query.rs`) | The building tests' crossed poles are fastened where they cross |
| **The hull's visible region grown from the face seen best** (`hull_build::visible_region`); a corner dropped only where no face bends | The builders' member hull, now a test (14 faces, 24 corners) |

### Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| Edge-across-face patches alone, friction as before | The fixture pair rocked at 4–8 mrad/s for 10 s, energy falling 12 µJ a tick, no guard fallbacks | Friction forgot each step's slip (Found 3) |
| Swapping A and B in detection so a patch's corners are always on A | The fixture pair slept (tick 311); a character walked past a standing trunk (`a_character_walks_into_a_trunk_and_stops`): static geometry cannot be A | Replaced by the kept stick offset, which covers fixed geometry too |
| The incident face as the first in order of opposition whose clip is not empty | A face of the next ring, short of the crossing; the face then taken was 0.36 mm off the edges' depth, or 4.5 mm out: 17 J out of nothing in the scene's pair | The deepest within the bias, then the most opposed |
| The face beside the edges nearest in depth (not in direction) | A trunk against a player's box flipped between its two faces beside the edge every few ticks | The face the edge lies flattest across |
| Edge-across-face only where the edges are clearly better | The trunk against the player's box alternated between the face test's flank (its separation set far away) and the edge test's box face | Within the bias as well |
| **A sticking dashpot** (`docs/adr/0024/sticking-dashpot.diff`): the stick spring damped critically, c = 2·√(2μλ/e·m), capped (Huber) at a share of μλ, only while sticking at the step's start | Uncapped: a wedge on a 40° ramp held at 1 cm/s (6 m expected), a tumbling cube's spin turned back, the pile gained 100 kJ. Capped at 0.1·μλ and gated to contacts at rest (moved under 5 µm the last step): every accuracy test passed, but the single trunk no longer slept and the scene's pair still rocked. Capped at 0.1 without the rest gate: the scene's pair slept at 9.6 s, the fixture's at 3.75 s, but accuracy tests failed | It decided sleep one way or the other by its gates, and broke accuracy tests ungated. A damping inside Coulomb's cone, not added to it, is the lesson |

### Measured (development machine, shared with other builds)

- **Gate, trunks asleep within 5 s of the last blow: not met.** After these changes three parts awake 15 s on (4: 0.0, 5: 8, 6: 55 mrad/s; before: 4, 6, 8). Found on the scene's state with the players taken out: an 82 kg log balanced across another (its ground contacts carrying 0–2 N) rocks about its own axis at 3 Hz, 1–5 mrad/s, its energy constant to 10⁻⁷ J for 15 s, an undamped mode on the friction anchors' springs (with friction between the logs off it sleeps in 6 s). Trunk 6 rests against a held player's body, one patch now, moving with the player.
- **lumberjack**, full length, p50 / p99 / worst: 1.08–1.34 / 14.6–18.3 / 45.6–48.2 ms over three runs (before: 0.9–1.7 / 21–28 / 37–59). Short: 0.21 / 0.35 / 0.40–0.57. Gates (2 and 5 ms) not met.
- **builders**, full length: 3.7–3.8 / 53.7–64.6 / 80–90 ms, its most work 1,508 at tick 127 (gate 4,792); 26 ms p50 measured before by the parent. It ran only after the hull fix (it panicked before).
- **lumberjack's work** at short length 41 → 52 at tick 145: two players' torsos pressed together, their faces' friction now kept. Its ceiling raised 50 → 57 (`play/tests.rs`): a gate raised, for the parent to confirm.

### Goldens renewed

Play and stress at both lengths. Motion changes: crates and boxes on one another (friction kept, edges across faces), trunks (edges across faces); with ADR-0034's self-contacts *cars* (569.3 kJ of plastic work against 568.5, 80 members buckled) and the G2.8 scenes. `planet_physics_is_bit_identical_across_platforms` renewed with its note.

### Where this leaves G2.0

The hops are gone: one physical contact keeps one representation from step to step, and friction keeps what a step built. What keeps felled trunks awake now is an undamped elastic mode of sticking contacts (the anchors' springs, e = 0.1 mm) and a trunk resting on a held player. Next (`docs/future-improvements.md`): damping the stick springs within Coulomb's cone, or a stiffer stick so that mode is fast and the face dashpots reach it. (Corrected below: it was not that mode.)

## G2.0: at rest (2026-10-26)

What kept felled trunks awake after the hull–hull hops were fixed (*G2.0, continued*): the 82 kg log balanced across another, and the 125 kg trunk lying on a standing player.

### How it was measured

- **The scene's own state.** *lumberjack* played to its end by the build under test, its felled parts written as a world with the players taken out (`play/rest_stick_probe.rs`, `rest_stick_write`), then stepped 20 s by any build or switch (`rest_stick_step`: `ONLY` keeps some bodies; `TRACE` prints one second tick by tick: each body's spin, Newton iterations, the guard's fallbacks, its height and kinetic energy, and each contact's normal force, stick offset and torque).
- **With the players.** `rest_stick_scene`: the scene left alone with its players; one body's group and contacts tick by tick, and what `World::touching` reports against another body (`World::group_contacts`, added for it).
- **The gate.** `lumberjack_s_trunks_sleep_within_5_s_of_the_last_blow` (run by hand) now also prints each part's last tick awake and the ticks' mean cost from 10 to 15 s after the last blow.
- **The guard's books**, step by step (a temporary print, not kept): energy before and after, the limit, the surplus found anew; and, temporarily, the hull test's candidate faces.

### Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Cundall & Strack (1979); Tsuji, Tanaka & Ishida (1992); LAMMPS' and LIGGGHTS' granular history models | A sticking contact's tangential spring damped like the normal one, the sum of spring and dashpot capped at μ·F_n (the spring rescaled where it is exceeded) | Built (friction's argument the damped slip u + γ·u̇: its force inside the cone by construction) and measured: not needed once the guard was right (*Measured and rejected*) |
| Box2D v3's soft step and soft contacts (Catto, *Solver2D*, 2024; GDC 2011, "Soft Constraints"); Bullet's ERP/CFM; MuJoCo's solref/solimp (a time constant and a damping ratio, 1 by default) | Contacts are damped springs of a chosen frequency and damping ratio; friction is a velocity-level constraint inside the cone (impulses clamped by μ·λ in Box2D, Bullet and PhysX; MuJoCo's friction rows have no position term, only the reference velocity's damping): the tangential motion within the cone is taken each step, Moreau's maximal dissipation | Creëer's static friction is a position-level anchor spring (it never creeps, ADR-0005) and stores energy where impulse friction does not. The undamped stick mode the last section named was looked for: no sustained motion here was that mode |
| Box2D, PhysX and Jolt sleeping (islands asleep once every body has been slow for a while; PhysX's threshold on mass-normalised kinetic energy) | Sleep is a test of rest | Creëer's thresholds (1 mm/s and 1 mrad/s for 1 s) kept: what failed was rest, not the test |
| Simo & Tarnow (1992); Laursen & Love (2002); Hauret & Le Tallec (2006) | Energy-conserving and energy-controlling schemes with contact: the scheme's energy may not rise, and where contacts act it is made to fall | ADR-0004/0005's guard is of this kind. Its tolerance is what this section is about: a rise allowed in proportion to energy that does not move lets energy in |
| Jolt's manifold reduction (`mUseManifoldReduction`: a pair's manifolds of nearly one normal merged into one); Gregorius, "Implementing Quickhull" (GDC 2014: nearly coplanar faces merged as the hull is built) | One contact region where a shape's faces lie nearly in one plane | Taken at the contact (*Kept*): a trunk's faces are bent at each ring, and merged they would not be a plane face |

### Found

1. **The log balanced across another was not an undamped stick mode.** On the scene's state (82 kg on a 69 kg log, 700 of its 800 N on one patch, its far end on the ground) it rocked at 2–3 Hz, 5–16 mrad/s, for 20 s. Its stick offsets moved by under 1 µm while it rocked (the stick distance is 100 µm): friction's springs barely took part. The face dashpots five times critical, or a stick dashpot five times critical, left it rocking (6 and 7 mrad/s at 19 s). Converging each step to 3·10⁻⁹ m instead of 3·10⁻⁷ m made it decay (27 → 0.8 mrad/s in 19 s): its motion was being fed. The guard's books, step by step: gains of up to 92 µJ a step accepted, 1.4 mJ net in 20 s. The guard allows a rise of a thousandth of the kinetic and elastic energy, and the elastic energy was the 62 mJ its contacts held under its weight: 62 µJ a step allowed, against a rocking of 0.2 mJ. With the allowance a hundred times smaller the log slept at 3.9 s.
2. **The trunk on a standing player hopped between two of its own faces.** The player's body box lies with an edge across the trunk where two of its rings meet, whose faces (16 and 17) lie within 0.1° of one plane. The edge test named one end of the line or the other as the trunk rocked, and with it one face, whose patch held its part of the line alone. Each time the choice changed (every ten ticks, exactly) the other part was found pressed in, 388 µm against 362 µm, 0.34 J beyond what the last step left; the guard took it from the motion (the trunk stopped dead), and the cycle began again (55–138 mrad/s).
3. **No longer hopping, the trunk on the player still rocks at 1–3 mrad/s**, fed as the log was through the allowance. Its group holds the player's legs, struts storing 82 J under the player and the trunk: 82 mJ a step allowed. Unsolved (*Where this leaves G2.0*).

### Kept

| Change | Measured |
|---|---|
| **The guard's allowance counts of the contacts' energy only its change over the step** (`structure/group.rs`; both energies in one evaluation, `Coupling::elastic_and_found`): the kinetic energy and the members', joints' and struts' elastic energy whole as before, and what the contacts' elastic energy changed by. A resting body's contacts hold its weight; a bounce or a landing changes their energy by about as much as they hold, so contacts in motion keep their allowance | The log across a log asleep at 3.5 s in the scene, where it rocked for good. Every accuracy test as before |
| **A hull's faces in one plane with the pressed face are pressed too** (`hull_sat::clip_flat`): the reference hull's other faces within `ACROSS` (about 1°) of it that the same incident face presses give their own patches, the incident face clipped to each one's sides (disjoint parts of the overlap); in the face test and the edge-across-face test | The trunk on the player: no hop in 900 ticks (no surplus, no velocities zeroed). Boxes have no such faces: a box's hull touches as the box (`hull_across_tests`) |

### Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| **Static friction damped within Coulomb's cone** (`docs/adr/0024/stick-in-cone.diff`, against bb006a4): friction's argument the damped slip s = u + γ·u̇, γ = 2·√(m/k) critical for the stick spring k = 2μλ/e at the contact's mass, u̇ the scheme's slip rate from its reference at x̂ (as the normal dashpot's); its energy μλ·f₀(\|s\|)/(1 + γ/β), whose force μλ·f₀'(\|s\|)·ŝ is spring and dashpot while it sticks and never beyond μλ; the matrix 1 + γ/β times stiffer | Alone, on the first scene state, the log pair slept in 2 s (it rocked before); in the scene the log still rocked (8.5 mrad/s at 15 s). With γ fixed at the step's start the tumbling cube's spin turned back on landing (4.96 → −0.18 rad/s: λ ≈ 0 at the start made γ huge, then λ was refreshed); γ refreshed with λ, every physics test passed. With the kept changes: the scene's logs asleep at 4.4 s with it, 3.5 s without; the 100 kg trunk at 6.7 s with it, 5.1 s without | Not what kept the log awake (*Found* 1), slower to rest in the scene, and it changes every friction contact. Kept as a patch, with its pitfall |
| Every step converged to 3·10⁻⁸ or 3·10⁻⁹ m (with the stick dashpot) | The log decayed (asleep at 13 s; 0.8 mrad/s at 19 s); the scene's log still 9 mrad/s at 15 s | Starves the feed instead of closing it; dearer |
| **The allowance without any coupling energy held whole** (contacts, joints and struts all by their change; with the stick dashpot) | The gate met: every part asleep 3.9 s after the last blow. But a standing character's own step gains up to 30 mJ while its legs hold 20 J: 1,080 more steps of *builders* redone by backward Euler, its most work 2,801 Newton iterations at tick 273 (short length, ceiling 1,659; 1,259 without the change), p99 70–78 ms against 55 | The characters' legs need their own books first (*Where this leaves G2.0*) |
| The same, the loads' booked work added to the allowance | *builders* as above (2,801) | The characters' gains are not their loads' work |

### Validation

- **The gate** (the whole scene; the last blow at tick 1,778; each part's last tick awake after it):

  | | 69 kg log (4) | 82 kg log across it (5) | 125 kg trunk on a player (6) | 100 kg trunk (7) | ms a tick 10–15 s after |
  |---|---|---|---|---|---|
  | bb006a4 | awake at 15 s | awake (8.0 mrad/s) | awake (55 mrad/s) | 5.4 s | 6.87 |
  | this change | 3.5 s | 3.5 s | awake (1.3 mrad/s) | 5.1 s | 1.73 |

  Not met: the trunk on the standing player, and the 100 kg trunk 0.1 s late.
- `a_felled_trunk_alone_on_the_terrain_comes_to_rest` (CI) passes; every test at its tolerance (`cargo xtask ci`).
- **lumberjack**, full length: 1.17 / 16.2 / 59.6 ms p50 / p99 / worst (before 1.08–1.34 / 14.6–18.3 / 45.6–48.2 over three runs); its slow ticks are the felling and bucking. *builders* full length 3.6 / 62.7 / 89.4 ms, its most work 1,518 at tick 127 at both lengths (ceiling 1,659).
- *log_bridge*'s full run spends 412,756 Newton iterations at tick 941 (about 2.5 s), exactly as bb006a4 does: not this change's.
- **One test's window lengthened.** `overloaded_the_nail_lets_go_and_the_pole_falls` looked at the pole 2 s after the nail let go, but the 2.5 kN until then flings it 5–6 m up at about 20 m/s with either build: bb006a4 caught it falling past 0.7 m, this build at 2.1 m. It now looks after 5 s, when it is down.

### Goldens renewed

- **The allowance** (with it alone, every scene hashes as with both at short length): play *crowd*, *work*, *quarry*, *toolmaking* and *builders* at both lengths and *log_bridge* at full length: crates, boulders, stones and poles at rest on one another or on the terrain, their steps' small gains now redone or taken from the motion where they were let in before. Stress *cars* and *server* at both lengths: cars and crates resting on one another and the terrain.
- **Both changes:** play *lumberjack* at full length (its felled trunks; at short length nothing is felled yet, and it hashes as before).
- Every other hash is unchanged, and the save fixtures in `creeer_sim` pass unchanged.

### Where this leaves G2.0

- Logs lying on logs rest within 3.5 s, and a trunk on a standing player no longer hops; a tick with the parts lying costs 1.73 ms instead of 6.87.
- **Not met:** a trunk resting on a standing player stays awake at 1–3 mrad/s. The guard allows each step a thousandth of the player's legs' 82 J, and a character standing alone gains up to 30 mJ a step that it lets in. Until the legs' work is booked (as the joints' drives' is, `Coupling::driven_work`), their energy cannot leave the allowance. Next: find and book what the legs gain, then count the struts by their change as the contacts are.
- The 100 kg trunk sleeps at 5.1 s, at the gate's edge.
