# ADR-0035: P2 and P3: the structure solve and struck structures

- **Status:** In progress (2026-10-23). P2 built in part (SIMD kernels, benches); both of P2's targets and both of P3's are **not met**, and this ADR records why and what each would need.
- **Context:** plan.md, Phase B performance track, rows P2 and P3; `docs/g2-plan.md` G2.7.
- **Builds on:** ADR-0004 (the implicit structure step and its sparse block Cholesky), ADR-0005 (impacts, crush substeps of 0.5 ms), ADR-0017 (what was measured for fast ticks), ADR-0021 (the profiling pass: structures bound by memory)
- **Evidence:** `creeer_physics::sparse::kernels` (and `kernel_tests.rs`), `creeer_tools bench-spikes --only many` (`many_bench.rs`), `--only structures`, `--only reference`; `cargo test --release -p creeer_client struck_ -- --ignored --nocapture` (`struck_bench_tests.rs`); the profiles of `physics_profile.rs` (`--features creeer_physics/profile`)

## How it was measured

The development machine was shared with other agents' builds and tests throughout (CPU load 20–70 % from other processes for most of the session), so:
- timings of the same code varied by up to 2× between runs minutes apart; every comparison here is **interleaved** (old and new binaries alternating) and quoted as a range or as the best of several runs;
- the struck ticks are timed by `struck_bench_tests.rs`: the scene is run to just before its impact, kept, and its struck ticks are stepped again from the copy seven times, each tick's best time kept. The work (Newton iterations) and the end state's bits are printed and are the same in every run;
- a one-thread microbenchmark of a factor's solve first showed the SIMD kernels 2.5× faster. That was **wrong**: the benchmark scaled its vector by 10⁻³ each round, the values fell into the subnormal range, and the old scalar code slowed down there. With the vector reset each round the gain was within noise for the bridge (below). Recorded as a warning for later benches of this kind.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Gates, Kurzak, Charara, Haidar, Dongarra, "Batched BLAS" / interleaved batched kernels (2017–2019) | Many small dense matrices of one size stored interleaved, lane *i* of a register holding matrix *i*, so tiny factorisations fill the SIMD width | Not taken: it needs many independent matrices of one size and pattern. A structure's 3×3 blocks are dependent along each row of the solve, and different structures have different patterns. Lanes within a block (its three rows) are used instead |
| faer and pulp (Rust): supernodal sparse Cholesky with SIMD kernels and runtime dispatch | Fast, but its summation order follows the CPU's vector width | Not taken: the cross-platform golden hashes need the same bits on every CPU. pulp's dispatch idea is taken through `multiversion` (safe `target_feature` clones chosen at run time) |
| CHOLMOD supernodal (Chen, Davis, Hager, Rajamanickam 2008) | Dense column panels, BLAS-3 updates | Measured slower without SIMD (ADR-0004); with SIMD a triangular solve is still bound by the bytes it reads (below), and dense panels add zeros |
| Roofline model (Williams, Waterman, Patterson 2009); sparse triangular solve analyses | A sparse triangular solve does about 2 flops per 8-byte value read: bound by memory bandwidth on any modern CPU | Confirmed here: the bridge's solve reads its 12 MB factor twice at 41–48 GB/s, a single core's streaming rate from L3 |
| Mixed-precision preconditioning (Carson & Higham 2018; Abdelfattah et al. 2021, survey) | An f32 factor as preconditioner halves its bytes; refinement in f64 keeps the solution's accuracy | The only way found to cut the solves' bytes. Rejected again (ADR-0004, ADR-0021): it changes the iterates and so the trajectories, which this project does not trade |
| Belytschko & Mullen (1978); Belytschko, Yen & Mullen (1979) | Mixed explicit–implicit partitions of a mesh, stability of subcycled partitions | Background for P3's subcycling |
| Gravouil & Combescure (2001), *IJNME* 50 | Multi-time-step coupling of Newmark subdomains by Lagrange multipliers on interface velocities at every fine step | Stable, but dissipates energy at the interface |
| Prakash & Hjelmstad (2004), *IJNME* 61 (FETI-based multi-time-step coupling) | Continuity held at the coarse steps: energy exact at the interface | The method plan.md names for P3; not built (below) |
| Lew, Marsden, Ortiz, West (2003), asynchronous variational integrators | Each element its own time step, symplectic | Explicit only: Creëer's steel structures need the implicit step (ADR-0004) |
| BeamNG.drive (soft-body vehicles) | Every node of every vehicle at about 2 kHz, explicit | The opposite trade: small steps everywhere, on stiffness kept low enough for it |

## P2: the structure solve

### Built: SIMD block kernels, bit for bit (`sparse/kernels.rs`)

- The 3×3 block products of the factorisation (L_ij −= L_ik·L_jkᵀ) and of both substitutions (b −= L·x forward, x_k −= L_jkᵀ·x_j backward) are written lane by lane: a block's three rows in one register, each lane doing exactly glam's operations for that row in its order ((c₀·x + c₁·y) + c₂·z, then the subtraction). Rust never fuses a multiply and an add nor reorders a sum, so a lane in an AVX2, NEON or scalar register gives the same bits. Tested against the glam expressions bit for bit with zeros and signed zeros (`kernel_tests.rs`); every golden hash unchanged (`stress --check`, `play --check`, the solver's own golden tests).
- Compiled twice on x86-64 (baseline and AVX2 without FMA), chosen at run time by `multiversion` (no `unsafe` in Creëer's code); aarch64 has NEON throughout.
- A block is read as three overlapping four-lane columns of its nine contiguous values: no padding, no extra bytes.

**Measured** (one thread, interleaved, the vector reset each solve):

| | before | after |
|---|---|---|
| 5,000-beam bridge: a solve | 0.66–0.74 ms | 0.59–0.66 ms (noise) |
| welded block 36 × 8 × 36 m: a solve | 1.08–1.10 ms | 0.99 ms (−9 %) |
| welded block: a factorisation | 140–144 ms | 120 ms (−15 %) |
| the reference tick's 20,000 beams (four bridges in parallel) | 4.2–4.7 ms | 4.3–5.2 ms (noise) |
| the whole reference tick, p50 | 5.4–6.0 ms | 5.4–6.1 ms (noise) |

The bridge's solve does not get faster: it is bound by memory, not arithmetic (Roofline, above). Its factor is 170,933 blocks of 72 bytes (12.3 MB; 3 % of its values are exact zeros), read twice a solve, in 0.6 ms: 41 GB/s, what one core streams from L3 (48 GB/s measured, `many_bench.rs`).

### Target 1: the reference tick ≤ 8 ms projected: not met

On a quiet machine the reference tick was 4.54–4.61 ms (9.1–9.2 projected; ADR-0021); during this session it measured 5.4–6.1 ms both before and after (the shared machine). The kernels did not move it, since the four bridges' solves are bound by memory. Reaching 4.0 ms (8 projected) needs about 0.6 ms off the structures' 3.4–3.9 ms. What is left of a step (5.2 ms on one thread: prediction 0.83, element forces 0.96, line search 1.41, solves 1.29, guard 0.1) is the members' co-rotational evaluation (about 160 ns per member and evaluation, three evaluations a step) and the nodes' rotations; both are long scalar chains of rotations, square roots and the plastic split, which lane-wise SIMD across members would need restructured into structure-of-arrays form, bit for bit. That is the next step for P2 (`docs/future-improvements.md`).

### Target 2: eight bridges at once ≤ 1.3× one alone: not met, and not reachable without fewer bytes

| (ms a step, each bridge on one thread) | one alone | eight at once |
|---|---|---|
| whole step | 5.2–6.1 | 10.8–14.2 (ratio 2.1–2.5 on the shared machine; 1.6 quiet, ADR-0021) |
| of it, the solves | 1.29–1.53 | 5.4–7.4 |
| of it, the rest | 3.9–4.6 | 5.4–6.9 |

Streaming a 12 MB array (a bridge's factor): one thread 47–48 GB/s; eight threads at once, each its own, 140–161 GB/s together, 17–20 GB/s each: **2.4–2.8× slower per thread** than alone, for reading alone. Eight factors (98 MB) do not fit the 96 MB L3, so they come from DRAM. The solves are a quarter of a step alone; at 2.5× and the rest at 1.3×, the step is 1.6× alone, as measured on a quiet machine. To reach 1.3× the solves' bytes must fall by about three times. The ways found:
- an f32 factor (half the bytes): changes trajectories (rejected, ADR-0004, ADR-0021);
- fewer solves a step (two a step now: the second iteration is the convergence test): changes the converged state (never loosened);
- less fill: the factor is already nested dissection, with 43 blocks a block row; a band ordering along the bridge would keep about two cross-sections of 15 nodes (60 blocks) in each row's reach, an estimate that leaves no ordering halving it (smallest-separator and plane-aligned cuts were measured no better, ADR-0004);
- a machine whose L3 holds the eight factors (the min-spec CPU has a third of this one's).

None is within the rules, so the target is recorded as needing a decision (`docs/future-improvements.md`).

### Measured and rejected

| Tried | Measured | Why rejected |
|---|---|---|
| A transposed second copy of L for the backward substitution (lane-natural products both ways). Patch: `docs/adr/0035/two-copies-of-l.diff` | Alone the same (0.57 against 0.61 ms a solve). Eight bridges at once: their solves 7.0 ms a step against 5.4 with one copy | The backward pass reads another 12 MB instead of starting on the blocks the forward one left in the cache |
| (Measurement error, kept as a warning) a solve microbenchmark scaling its vector by 10⁻³ a round | 2.5× faster solves, 2.1× faster factorisations | Subnormal values slowed the old code; redone with the vector reset: the table above |

## P3: struck structures

### Measured first

`struck_bench_tests.rs`, best of seven, the same work and end state in every run:

| scene | worst tick | the struck ticks | work |
|---|---|---|---|
| bridge (10 t block on a 126-node deck), ticks 18–40 | 31.7–33.2 ms | 16, 30, 32, 10, 24, 33, 6, 27, 5, 23 ms | 2,146 iterations |
| crash (a 12-node frame into a wall at 15 m/s), ticks 0–60 | 16.8–17.5 ms | one tick of 17, the others ≤ 4 | 2,675 iterations |

The same as ADR-0017's 32.8 and 19.0 ms, and the same before and after P2's kernels. Where the struck ticks go (`physics_profile.rs`; the profile's own timers inflate the totals by about 2×, the shares are what counts):

- **Bridge**, each worst tick: 34 impact substeps of 0.5 ms, 88–158 Newton iterations (2.6–4.6 a substep), 4–5 factorisations. The line search's evaluations 20–38 % of the tick, the solves (the structure's and the joint solve of the block on its nine deck joints) 15–25 %, the forces 8–15 %, the minimisation's setup 5–12 %, the energy guard 3–10 %, yield 3–9 %, the prediction 4–7 %.
- **Crash**, its worst tick (6): 34 substeps, 495 iterations (14.6 a substep), 564 line-search trials, 26 factorisations. The trials' evaluations 56 % (the members' plastic splits: 18,202 splits, 92,822 sweeps over end-moment pairs, 179,525 rate-lifted increments, 341,677 of their Newton iterations), the forces 15 %, the guard 14 %.

### What 5 ms means

At 34 substeps, 5 ms is 0.15 ms a substep, everything included. ADR-0017 measured the bridge's fixed costs (setup, guard, prediction, yield) as a third of each substep: even with none of them the bridge would take about 22 ms. The crash's frame is struck as a whole: twelve nodes, all of them in the impact. Subcycling the struck region cannot help it; its tick is its Newton iterations (14.6 a substep at 34 µs each, most of it its members' plastic splits).

### Struck region: how local the substeps' work is

Measured to decide whether Newton's work could be restricted to the struck subgraph (the simpler first step): in every impact substep of the bridge's struck ticks (ticks 18–40, 817 iterations), how many of the deck's 126 nodes each Newton direction moves (the tolerance is 0.3 µm). Temporary instrumentation in `minimise.rs`, not kept.

| | iterations | nodes moved > 1 µm | > 0.1 µm | > 10 nm |
|---|---|---|---|---|
| a substep's first iteration | 239 | 69.5 on average (most 114) | 119.0 (most 126) | 125.8 |
| its later iterations | 578 (71 %) | 3.0 (most 54) | 16.5 (most 108) | 61.8 |

- The first iteration moves the whole deck: the prediction is the whole structure's, and the block's load spreads through it within a substep. Only after it is the work local: on average 16 nodes still move more than 0.1 µm, but half the deck still moves by more than 10 nm.
- Restricting the later iterations to a struck subgraph would therefore leave the far nodes off their minimum by up to some tens of nanometres a substep: within the tolerance, but a different converged state (new bits, and a drift through 34 substeps a tick to be proven against the 0.5 ms grid's own accuracy). Its best case, if the later iterations cost nothing beyond the subgraph, is about 71 % of the iterations' share of the tick (two thirds) cut by five sixths: the bridge from about 32 to about 16 ms. Short of 5 ms by three times, so not built; recorded as the cheapest partial step (`docs/future-improvements.md`).

### Not built, and why

- **Prakash–Hjelmstad subcycling** (the struck region's joints in 0.5 ms steps, the rest of the structure in the tick's, continuity held at the coarse step with Lagrange multipliers on the interface, energy exact there). It is the method for large structures struck at a few joints (G2's buildings): what it needs from this solver is a split of a structure into two subdomains with their own factorisations (the region's small), Newmark/BDF2 with its own step in each, the interface's dual problem (a Schur complement on the interface's degrees of freedom, solved every fine step), the energy guard's books per subdomain plus the interface's, and plasticity, contacts and the joint solve within the region. It does not reach the crash's target at all (above), and for the sandbox's bridge it would cut what scales with the deck's size, not the per-substep costs that remain. Weeks of work and its own validation (`crush_tests.rs`, the bridge's sag and plastic strain against the 0.5 ms grid, the energy books), not started in this session.
- **Newton's work restricted to the struck subgraph** (the simpler first step): measured above, at best about 32 → 16 ms for the bridge and nothing for the crash.
- **Cheaper substeps for the crash:** fewer iterations (its Newton converges linearly: the matrix lacks the plastic tangent, and the friction lag refreshes add a quarter of the iterations) or cheaper plastic splits. Both are convergence or evaluation work, each measured before (ADR-0017 B1, C; ADR-0021 warm splits).

## Validation

- `sparse/kernel_tests.rs`: the three kernels against glam's expressions, bit for bit, over values of every magnitude with zeros and signed zeros.
- Every golden hash unchanged: `creeer_tools stress --check` and `play --check` (both lengths), the sparse solver's and the implicit step's own golden tests; `cargo xtask ci`.
- The struck scenes' work (2,146 and 2,675 iterations over the measured ticks) and end states unchanged by P2.

## Cost

The kernels cost nothing at run time beyond one indirect call per row range (`multiversion`'s cached dispatch). The `many` spike section takes about 15 s.

## Limits (each in `docs/future-improvements.md`)

- P2's targets: the reference tick (needs the members' evaluation in SIMD, bit for bit) and eight structures at once (needs fewer bytes a solve: a decision on mixed precision).
- P3's targets: subcycling for large struck structures; the crash's iterations and plastic splits.

## G2.8: refactorisations that changed nothing (2026-10-24)

The *tower* play scene (17 × 20 × 17 bays, about 19,600 beams) took 2.2–3 s a tick every few ticks: each tick was a factorisation of its matrix. The structure refactorises when its contact blocks change by more than half their own size. The air's dashpots on its 6,000 nodes (a few N·s/m each, doubling and halving with the breeze) did that all the time, and so did the contacts of its clamped base nodes.

**Kept:**
- A held-fast node's blocks are left out: they are masked out of the matrix anyway.
- Air's and water's blocks (dashpots, added mass) change materially only by more than half their own **and** more than a hundredth of the node's inertia in the matrix (m/β², the blocks entering it × β²).
- Contacts are judged as ever. Without air or water on a structure, the rule is exactly the old one.
- The tower's worst tick: 3.0 s → 0.12–0.2 s. Every stress and play hash is unchanged; only the tower's own changed.

**Measured and rejected:** the same materiality for contacts too. It moved the save fixtures' steel frame (it changed when that frame refactorised), and seven fixture tests failed: they hold that motion bit for bit.

**Still open (the user decided on 2026-10-24 to record it and move on):** the tower runs p50 28 ms at full length and about 120 ms a tick while awake. The cost is its solve and factorisation, against the 5 ms gate.
