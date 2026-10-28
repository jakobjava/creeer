# ADR-0021: The profiling pass: a profiler of our own, and what it found

- **Status:** Accepted (2026-10-18)
- **Context:** F7.6 (`docs/f7-plan.md`). plan.md's F7 asks for a profiling pass; plan.md §5's reference tick (20,000 active beams, 8 players and 1,000 machines of every domain) projected to 10.3–10.5 ms on the minimum-spec CPU at the end of F6, against 10 ms; F7.5's stress scenarios showed worst and first ticks to explain. The rule (plan.md §1.11; the user, 2026-10-18): performance and realism together, never one for the other, and worst ticks gated.

## Decision

### The profiler (`creeer_core::profile`)

- **Named scopes** (`profile::scope("physics: detect")`): a guard timing until dropped, recorded into its thread's buffer with its depth in other scopes. **While nobody wants a profile, a scope is one relaxed atomic load**; wanting one is a guard too (`profile::want()`), held by an open profiler window or a server with watchers.
- **One clock a process** cuts the ticks into frames: every thread's spans that ended since the last tick (the pool's workers included), with the frame's names and threads. A higher priority takes the clock from a lower: a server's loop (2) from a client's simulation thread (1), which runs on beside a hosted server. The last ten seconds of frames are kept, each numbered in the process (`frames_since`: what a watcher has not had).
- **Frames carry their own names**, so a server's frame is shown by a client as its own: `ClientMessage::Profile(on)` asks for a report a second, `ServerMessage::Profile` carries the ticks' lengths since the last and the longest tick's 300 longest spans (about 3 kB a second of a client's 32 kB budget; protocol 4).
- **Why our own:** `puffin_egui` pins egui 0.33 and the client is on 0.36; what is needed is small (three hundred lines).
- **Where the scopes are:** the server's tick (network in and out, the LOD pass and its parts, commands, machines, surroundings), the physics step (detection, groups, each group, after), each structure step, the client's simulation tick; `creeer_tools stress --profile` prints the first and worst ticks' totals per scenario.
- **The window** (F9, `profiler_ui.rs`): every tick's length against 60 Hz, a click choosing one (else the worst of the last five seconds); its spans as a flame graph, a band per thread and a row per depth, hovered for name and length; where its time went by name (a name inside itself counted once). Of this process, or of the server it is connected to.

### What the pass found, and what was done (exact, or proven rounding only)

1. **The structure step is bound by memory when structures step at once.** A 5,000-beam bridge alone on one thread steps in 5.6 ms; eight at once, one per core, took 12.8 ms each, and the factor's solves 3.7 times longer (the factor's 12 MB read four times a step; the reference's four bridges and the domains' data fill the 96 MB L3). Without changing a bit:
   - the triangular solves read compact `u32` column indices instead of the factorisation's 16-byte pairs;
   - a member's plastic flows (104 bytes each, two sets of six a member: 1.2 kB, written every step and read by every evaluation) are built only when a measure passes its limit (`StepFlows`), from the same inputs by the same function; the step's start writes six limits a member;
   - `Element` in C order, the fields every evaluation reads first.
2. **Height tiles were generated on one thread** within the tick that first touched them: a loaded server's first tick took 26 ms, 23 of them eight avatars' first ground queries. Rows are now generated on the pool (each sample by itself: the same heights); a terrain streamer's workers, which have cores of their own, keep generating by themselves.
3. **A flowing member's end-moment pairs were split from nought** at every evaluation: 14 coordinate-descent sweeps a split in a crash, two rate-lifted increments a sweep. They start where the member's last split in the step left them. The same minimum of the same convex potential: warm and cold splits agree to 4·10⁻¹⁵ of a yield measure rate-lifted, 1.6·10⁻¹⁴ perfectly plastic, which is the cold start's own truncation at its 24 sweeps (3 yield measures × 4⁻²⁴; `split_tests.rs`). The structure golden moved within 1.4·10⁻¹⁵ m of every node (regenerated); every sandbox scene's Newton iterations are unchanged.
4. **A member buckling lost the solver's books:** its split builds a new solver, which began `iterations_total` (the sandbox's work gates) and the plastic work at nought. Kept now. The crash scene's gate had measured 207 iterations where its worst tick takes 495 (ADR-0017's profile had seen 495); its ceiling is now 545. The plastic work is booked (`ImplicitSolver::plastic_work`): the stress cars do 568 kJ of it out of the 880 kJ they carry into the crash.
5. **A thermal network's first step** built its islands one by one; in parallel now. **The minimum-degree ordering** rebuilt a hub's whole neighbour list for each node eliminated beside it (quadratic: 73 ms for a hub of 10,000 leaves): lists keep eliminated nodes until their next merge, the live degrees counted apart, and the queue is a heap whose stale entries are skipped. 3.1 ms, the same order exactly (`order_tests.rs` against the plain algorithm).

### Measured and not taken

- **An f32 copy of the factor** would halve the solves' traffic, but changes trajectories (a preconditioner, not rounding); rejected twice before (ADR-0004).
- **The prediction's node pass** (0.48 ms of a bridge's 5.2 ms single-threaded): no single hot spot; the rest inertia's inverse in place of the turned inertia's would change rounding for a few per cent.
- **The cars' impact tick** (91 ms): 34 impact substeps of 50–75 Newton iterations in the struck groups; the method that would cut it, subcycling the struck region, stays deferred (ADR-0017).
- **Large networks' first steps:** the hall's 11,000-node island's ordering and analysis (8 ms), the pipes' cost estimate on the tick thread (9 ms): once when built or loaded. Building beside the ticks is recorded.

## Measured

On the development machine (min-spec projections ×2), before the pass and after it:

| What | Before | After |
|---|---|---|
| **The reference tick** (plan.md §5), p50 | 5.1–5.8 ms (10.3–11.7 projected) | 4.54–4.61 ms (**9.09–9.23 projected: within 10 ms**) |
| Its 20,000 beams alone | 4.93 ms | 3.36–3.87 ms |
| A 5,000-beam bridge, eight at once one per core | 12.8 ms a step each | 8.2 ms |
| Ten bridges (stress `beams`), p50 / worst | 18.4 / 20.5 ms | 13.6 / 16.4 ms |
| Ten cars crashing, the impact tick / p50 | 134 / 0.42 ms | 92 / 0.39 ms |
| A loaded server's first tick (stress `server`) | 26.5 ms | 6.0 ms |
| A factory's first tick (stress `factory`) | 40.4 ms | 37.1 ms |
| Minimum degree, a hub of 10,000 leaves | 73 ms | 3.1 ms |
| A scope while nobody wants a profile | — | one relaxed atomic load |

## Tests

- `creeer_core::profile_tests`: nothing recorded while nobody wants a profile; scopes of two threads nested in one frame; one clock holder at a time, a higher priority taking it and the lower cutting no more frames; totals count a name inside itself once; a frame's longest spans keep their order, names and threads.
- `creeer_net::message::tests`: the profile messages read back; a report of a tick of 5,000 spans stays under 4 kB.
- `creeer_server::server::tests`: a client that asks is sent the server's profile once a second, and none after it stops asking.
- `creeer_client::profiler_ui_tests`: the window draws this process's ticks.
- `creeer_physics::structure::split_tests`, `creeer_math::sparse::order_tests` (above); every golden and stress hash otherwise unchanged.

## Not done (in `docs/future-improvements.md`)

- Structures' solves bound by memory when many step at once; large networks' first steps; one profile clock per process; impacts on structures (the cars' 91 ms).

## Amended after the user's play (2026-10-28): a client's frame measured

The user reported 30 ms frames knapping and slow frames felling. A singleplayer game's frame is not its server's tick: the hosted server ticks on a thread of its own (`host.rs`), while the frame holds the client's session (snapshots taken in, its prediction set back and replayed), the terrain, trees and loose things about the camera, the snapshot drawn, the scene built, the crosshair's aim and the render. Nothing measured those headlessly.

- **The play harness times the clients' own work** beside the server's tick: `Tick::client_ms` (the most of any client), `client_work` (its prediction's Newton iterations, replays included, gated per scene in CI: `play/tests.rs`, `CLIENT_WORK`) and `client_phases` (the physics' phases of the clients' steps, with the `profile` feature). `creeer_tools play` prints a client's p99, worst and most work; `PLAY_TRACE` lists ticks over its threshold for either.
- **`creeer_client --play-bench <knap|fell>`** plays a singleplayer game as the window does, headless (the world from the *toolmaking* or *lumberjack* scene, a scripted player, the frame rendered at 1600 × 900 off-screen, paced at 60 a second), and prints each frame phase's p50, p99 and worst, the hosted server's ticks, and every frame a felled part is drawn by none. `--each-second` prints each second's worst phases; `--gate` fails on a flash, or on a frame whose own work (all but the render) passes 6 ms, on the development machine otherwise idle.
- **What they found:** knapping's frames were the prediction replaying a struck stone (ADR-0030, amended); felling's were the crosshair's aim building every felled part's hull at every ray (ADR-0031, amended); the flash was the client applying a despawn before it drew the tick it came at (ADR-0008 and ADR-0031, amended). Times on the development machine vary with what else runs on it: the work counts are what CI holds.
