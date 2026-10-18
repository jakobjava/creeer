# F7 validation report: scale and hardening

What F7 was checked against: that the levels of detail lose nothing, that abstract machines make what full ones make, that content from data packs is checked before it is used, and that the whole holds at scale and on three operating systems. The decisions are in ADR-0018 to ADR-0021; this report gathers the evidence. Every result is an automated test (`cargo xtask ci`, on Windows, Linux and macOS in CI) unless it is marked as a benchmark (`cargo xtask perf`, the development machine; min-spec projections ×2).

## How determinism is tested at scale

- **Stress hashes on three operating systems.** CI's `stress` job runs the five stress scenarios at its lengths on Windows, Linux and macOS and checks each state hash against `tests/scenarios/stress.txt`: 50,000 beams, ten cars crashing (plastic hinges, buckling), every domain's reference load with a hundred powerhouses (half abstract), a server with 100,000 parked bodies and abstract machines, a flyover's terrain. Identical on all three since F7.5.
- **Handovers are bit for bit or at rest.** A parked island comes back as the island that never left (`lod_tests`); an abstract machine's books close across leaving, probing and coming back; saves of parked islands and abstract machines read back byte for byte.
- **Speed changes are exact or proven rounding only** (F7.6, ADR-0021): every change of the profiling pass kept every hash but one, whose change was shown to be rounding (every node within 1.4·10⁻¹⁵ m) before it was regenerated.

## F7.1 Data packs (ADR-0018)

| Check | Result |
|---|---|
| The base pack loaded and validated (1,798 species, 9 materials, 38 minerals, 25 rocks, 4 planets) | 30 ms (benchmark; budget 1 s) |
| The compiled-in reference tables | equal to the base pack's |
| The example mod | replaces, adds, keeps the base's rock ids, validates |
| The validator's refusals | a unit of the wrong dimension, an unknown mineral, a missing geology file, a species making water unstable, a negative heat capacity, a missing template: each caught once, with its pack, file and entry |
| A client with other data | refused, told the server's packs |
| A hot-reload | an edited pack reloads; a broken one is refused, the data before it kept |

## F7.2 Simulation LOD for bodies (ADR-0019)

| Check | Result |
|---|---|
| A resting stack 4 km away, parked and brought back | bit for bit the stack that never left |
| The world with parking against the world without | the same for everything left in it |
| A tower toppling 4 km away | simulated through its fall, parked only at rest |
| An offline player's avatar | parked, back where they left it |
| Idle avatars on a 7° slope | stand still and sleep (they crept 0.13 m/s before) |
| The LOD pass, 8 players among 400 bodies, 100,000 parked | 0.025 ms worst (benchmark; budget 0.1 ms) |
| The tick with 100,000 parked against none | +0.3 % (budget +5 %) |
| Waking a 1,000-crate yard | worst tick +0.43 ms (budget +2 ms) |

## F7.3 The rate solver (ADR-0020)

| Case | Result | Reference |
|---|---|---|
| A tank draining through a constant process | empty at the exact time | closed form |
| Exponential decay | exact in one segment | e^(−t/τ) |
| A level-dependent tank filling to its cap; a held outlet handing over to free decay | at the analytic times | closed forms |
| Consumers sharing a limited inflow; a full bin holding back its feed; a starved consumer | throttled in proportion | water filling |
| A stock peaking between a segment's ends | caught at its crossing | sampling and regula falsi |
| A factory's books over a year | closed to 10⁻¹⁶ of what passed through | — |
| 10,000 machines (50,000 stocks) through a day | 2.6 segments a machine, 9.6–9.7 ms projected (benchmark; budget 10 ms) | — |

## F7.4 Machines abstract (ADR-0020)

| Check | Result | Tolerance |
|---|---|---|
| The powerhouse abstract against full, small tank (22 min to dry) | every stock within 0.5 % at the end, 0.7 % along the way; dry within 0.1 % of the time | 1 % |
| The same, full-size tank (4.5 h) | every stock within 0.8 %, dry within 0.01 % | 1 % |
| Coming back | the shaft within 0.5 % of its speed at once | 0.5 % |
| In the server's world | abstract far away, back within 1 % at the world's time exactly; saved and loaded byte for byte | 1 % |
| 1,000 abstract machines, nothing due | +0.003 ms a tick (benchmark; budget 0.05 ms) | — |
| A tank running dry (found on the way) | at its floor when Torricelli says, stays dry, fills again | EPANET |

**Not met:** probing costs 15–27 % of running in full while the machine's slow thermal states change (the plan aimed at 2 %; 0.3 % once steady). Recorded.

## F7.5 Stress scenarios (benchmark, `creeer_tools stress`)

Full lengths on the development machine at the end of F7 (ms; the first tick, which takes a scenario's setup, apart), with F7.5's before the profiling pass; gated in `cargo xtask perf` by ceilings a quarter above:

| Scenario | Ticks | First | p50 | p99 | Worst | At F7.5 (p50 / worst / first) |
|---|---|---|---|---|---|---|
| 50,000 beams under moving loads (ten 5,000-beam bridges) | 600 | 15.1 | 13.6 | 15.9 | 16.4 | 18.4 / 20.5 / 20.2 |
| Ten cars crashing in pairs at 15 m/s | 300 | 1.0 | 0.39 | 21.2 | 92.0 | 0.42 / 134 / 0.6 |
| Every domain's reference load and 100 powerhouses (half abstract) | 600 | 37.1 | 1.68 | 2.82 | 4.99 | 1.66 / 4.5 / 40.4 |
| A server: 8 players, 100,000 parked, 10 abstract machines | 1,200 | 6.0 | 0.79 | 2.32 | 6.47 | 0.83 / 6.3 / 26.5 |
| A flyover from 1,100 km to the ground, then along it at 300 m/s | 1,200 | 0.02 | 0.32 | 0.70 | 1.81 | 0.33 / 0.9 / 0.01 |

- **The crash:** 568 kJ of plastic work of the 880 kJ the cars carry into it; 80 members buckle, none breaks. The impact tick is the worst tick (impact substeps, ADR-0017; recorded).
- **The flyover:** 238 meshes a second; along the ground at 300 m/s the detail under the camera is never behind; descending, each finer level comes a frame or two after it is wanted; the longest wait is the start's, 51 ms, with nothing built yet.
- **The factory's first tick** is setup: the 20,000-node thermal hall's first factorisations and its 11,000-node island's analysis, and the pipes' first cost estimate (recorded).

## F7.6 The profiling pass (ADR-0021)

| Change | Evidence |
|---|---|
| The structure step's memory (compact indices, flows built when needed, members' hot fields first) | every hash unchanged; ten bridges 19.5 → 14.0 ms a tick |
| Height tiles generated on the pool | every hash unchanged; a loaded server's first tick 26 → 5.9 ms |
| Members' splits warm-started | warm and cold splits within 4·10⁻¹⁵ (rate-lifted) and 1.6·10⁻¹⁴ (perfectly plastic) of a yield measure; the structure golden within 1.4·10⁻¹⁵ m; no scene's Newton iterations changed; the cars' impact tick 134 → 91 ms |
| The solver's books kept across a buckling split | the crash scene's worst tick: 495 iterations (207 counted before) |
| Minimum degree linear at hubs | the same order as the plain algorithm (random patterns, a hub); 73 → 3.1 ms at a hub of 10,000 |
| The crash's energy | the cars do 568 kJ of plastic work out of the 880 kJ they carry into it; a car into a wall, crashing at any point of its tick, 122–128 kJ of 169 |

### The reference tick (benchmark, `bench-spikes --only reference`)

20,000 active beams, 8 players and 1,000 machines across every domain, side by side: **4.54–4.61 ms p50 on the development machine, 9.09–9.23 ms projected to min-spec: plan.md's 10 ms is met** (5.1–5.8 ms and 10.3–11.7 projected before the pass). The beams alone 3.4–3.9 ms (4.9); the F6 domains 0.86–0.94 ms (budget 5 ms projected: met).

## The foundation gate

| Criterion | Result |
|---|---|
| Every validation suite green on Windows, Linux and macOS | CI's `check` job (`cargo xtask ci`: format, clippy, every test) green on all three for every F7 commit |
| Every stress hash identical on the three OSes | CI's `stress` job on all three, and a Linux container on the development machine: every hash equal to the goldens |
| Every budget on the development machine (Windows, `cargo xtask perf`) | met; the reference tick 9.09–9.67 ms projected (the higher late in a long run); every stress scenario within its ceilings; the baseline saved at the end of F7 |
| Linux on the development machine (a container: Docker Desktop's WSL2 VM, 16 vCPUs) | single-threaded work as fast as on Windows or faster (XPBD 1.08 ms, meshing 952 chunks/s, the rate solver 5.8 ms against 9.7); parallel work much slower: the bridge 6.0 ms on one thread, 4.5 on four, 14.8 on sixteen (Windows: 5.6, 1.9, 1.8), the reference tick 28.8 ms projected. The VM's thread wake-ups: CI's Linux VMs were no worse than its Windows ones. Recorded (`docs/future-improvements.md`) |
| macOS (CI's report) | as Windows and Linux CI: shared runners of two to three cores miss the parallel budgets two to three times over (the 5k bridge 11 ms projected, against 8.6 on Linux CI and 11.4 on Windows CI); informational, as since F0 |

## Not validated here

- **The min-spec CPU itself:** every budget is a projection from the development machine (per-core speed × 0.5). Memory-bound work (the structures' solves when many step at once) may project worse on a CPU with a smaller cache, and fine-grained parallel work worse where waking threads is expensive (a VM; both recorded).
- **Machines sharing stocks**, mods with scripts, signed packs: not in F7 (`docs/future-improvements.md`).
