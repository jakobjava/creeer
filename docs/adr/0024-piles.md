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

## Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| **The multipliers refreshed by Newton's method** (Δλ = (J·H⁻¹·Jᵀ)⁻¹·c with the step's factorised matrix, H⁻¹·rowᵀ one solve per row; a hinge's misalignment taken along its two directions across the axis; only at a true minimum) | On its own quadratic: a joint's gap 4·10⁻⁶ → 10⁻¹⁰ → 10⁻¹⁴ m in two refreshes. On a ragdoll falling (240 recorded states): refreshes 841 → 760, iterations 6,361 → 6,315, time +18 % (the solves) | Friction's normal forces refresh with the multipliers: a joint's change moves the contacts' forces, their refresh moves the minimum, and the joints open again. Neutral alone; worth trying again only with friction on fresh normal forces. Patch kept: `docs/adr/0024/exact-multipliers.diff` (against b7f8dc7, with its A/B harness) and the new file `joint/exact.rs` beside it |
| Pitfalls found building it | A hinge's three world-axis misalignment rows are dependent (no turn moves it along the axis): the third's tiny pivot gave changes of 10⁵ N·m from micro-radians. A refresh after a line search failing short of the minimum: changes a thousand times too large, the ragdoll thrown apart | (Fixed in the patch, by the two directions across the axis and by refreshing so only at a converged minimum) |
| The plain refresh over-relaxed (λ += ω·k·c, ω = 2, 3, 5) | ω = 3: refreshes 333 → 28, but iterations after them unchanged and the time −17 % only by a changed trajectory; ω = 2 and 5 slower (2,337 and 1,766 iterations after refreshes against 1,076) | The inner contact solve after a refresh is what costs, not the refreshes' count |

## Measured and set aside (for now)

| What | Measured | Why not first |
|---|---|---|
| Turn substeps per island instead of for the whole world (a fast-turning box makes every group and the contact detection substep, up to 16 times) | In the crowd's worst tick, the groups that did not need them cost 4.7 ms of 63 | The flung hand's group cost 42 ms of it; struts remove the cause. Still worth doing: listed in G1.8 |
| Legs ignoring supports lighter than 10 or 50 kg (a measurement, not a fix) | *crowd* worst 58 → 51 ms, p99 11.9 → 13.7 | Not realistic (a boot on a hand presses it down), and the remaining slow ticks are the ragdolls' own steps |
