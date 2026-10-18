# ADR-0024: Piles and ragdolls (P1)

- **Status:** In progress (2026-10-18)
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

- *crowd*'s CI work gate (4,730 iterations against 490) and its time gates.
- The rock pile's targets (worst ≤ 5 ms, p99 ≤ 2 ms).
- `rayon_bound_tests.rs`: an ignored measurement, kept as this ADR's evidence.
- G1.2e (prediction of nearby bodies) after P1.
