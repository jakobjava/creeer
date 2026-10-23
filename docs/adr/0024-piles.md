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
