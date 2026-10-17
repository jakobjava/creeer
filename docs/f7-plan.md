# F7 plan: scale and hardening

Status: **in progress** (started 2026-10-18), on branch `f7`.

F7 is the last milestone of Phase A. It makes the foundation hold at the scale of a real game: a planet with players far apart, factories that keep working when nobody is near them, content that comes from data packs anyone can write, and numbers that hold on Windows, Linux and macOS. It ends with the **foundation gate** (plan.md §7): every validation suite green and every budget met on all three operating systems before Phase B starts.

plan.md's exit criteria for F7: simulation LOD (active / abstract / on rails), the abstract-mode rate solver, stress scenarios on all three operating systems, the data/mod pipeline (data packs, validator, hot-reload), and a profiling pass.

## Principles

1. **LOD must not change the outcome.** What a factory produces while nobody watches is what it would have produced watched, within a stated tolerance that a test measures against the full simulation. What nature conserves (mass, energy, charge) is conserved exactly across every handover between levels, to rounding, as in F6's ledgers.
2. **The abstract model comes from the full one.** No hand-written recipe rates that could disagree with the physics: a machine's rates are measured from its own full simulation (its ledgers), and measured again by a short full run (a **probe**) whenever its state has drifted far enough for them to be stale.
3. **Cost follows the players.** A server's tick costs what is near its players; everything else costs (almost) nothing per tick. Gated by tests with many more bodies and machines far away than near.
4. **Handovers are exact or at rest.** A body leaves full physics only at rest (asleep) and comes back bit for bit as it left. A machine leaves only when its rates are steady and comes back at the state its rates imply.
5. **Data is the game's.** Every content file is read through one pack system: the base pack (`data/`), then mods in their order. Nothing reads a data file by its own path any more. Every pack is validated by the same checks the tests run.
6. **Deterministic everywhere.** Parking, waking, abstract steps and probes happen at defined ticks in a defined order; the same inputs give the same state hash on all three OSes.
7. **Worst ticks gated** (plan.md §1.11): a handover (a region waking, a probe) must not be a spike. Probes and wakes are spread or run beside the tick.

## Sub-milestones

### F7.1 Data packs, the validator and hot-reload (`creeer_data`, ADR-0018)

**Done** (2026-10-18, ADR-0018): packs with manifests, files from the last pack, tables merged by name (replacement in place, removals, origins kept for errors); every loader reads through `Data`; data found at run time (`--data`, `--mods`, environment, beside the executable); the content hash checked when a client joins (protocol 2); `creeer_tools validate` (30 ms for the base pack); the client's hot-reload (polled, validated first, the open sandbox started again); an example mod in `tests/packs/example`. Changed from the plan: a server's data is fixed for its run (restart to apply), recorded in `docs/future-improvements.md`.

- **Packs.** A pack is a directory with a `pack.ron` manifest: name, version, the packs it needs, a description. The base pack is `data/`; mods live in `mods/<name>/` (or wherever `--mods` points). Load order: the base, then the mods in dependency order (ties by name), so the result never depends on directory listing order.
- **Two ways to change content:**
  - **files** with one definition (a planet, a geology, a solar system): a pack's file replaces the one before it;
  - **tables** of named entries (materials, minerals, rocks, species): entries merge by name; a pack's entry replaces one of the same name, new names add. A manifest may also remove entries by name.
- **One way in.** `Data` (the merged packs) is what every loader reads: materials, species, minerals and rocks, planets and geologies, solar systems. The compiled-in reference tables stay for tests, and a test checks they equal the base pack.
- **Finding the data at run time.** Today the binaries find `data/` by the path they were compiled at, which breaks on any other machine. Order: `--data <dir>`, then `CREEER_DATA`, then a `data/` beside the executable, then the source tree (development).
- **Content hash.** The merged packs hash to one 64-bit number (files in canonical order). The server sends it in its welcome; a client with other data is refused with a message naming the differing packs.
- **The validator** (`creeer_tools validate [--mods <dir>]`): parses every file of every pack, checks every unit string, every reference (a planet's geology file, a geology's rocks, a rock's minerals, a system's planet templates), duplicate names within a pack, and runs the data integrity checks of plan.md §6 over the merged data (species element balance, positive heat capacities, the materials' curves monotone where physics needs it). Its error messages name the pack, the file and the entry.
- **Hot-reload.** The client watches its packs (polling file times twice a second: no new dependency, works the same on every OS). A change is validated first; a pack that fails keeps the old data and shows the errors. A valid change is applied: the planet sandbox rebuilds its terrain, the physics sandbox its scene with the new materials, the lab its species. The server (dedicated or hosted) reloads on request and sends its new content hash.
- **Validation:** a test mod pack (`tests/packs/`) that overrides a material, adds a rock and replaces a planet; merge order and removal tests; the validator catches each kind of error in a set of broken packs; a hot-reload test edits a file and sees the change applied.
- **Budget:** loading and validating the base pack (1,798 species) ≤ 1 s; a hot-reload applied within 1 s of the file's change (the terrain's rebuild excluded).

### F7.2 Simulation LOD for bodies: active, parked, on rails (ADR-0019)

**Done** (2026-10-18, ADR-0019): resting islands farther than 3 km from every player are parked and put back bit for bit within 2.5 km (over several ticks, nearest first); awake bodies are simulated until they rest; an offline player's avatar is parked and back when they return; save version 6 (`PARK`); removing bodies keeps the contacts' energies (ADR-0005 amended); idle avatars no longer creep downhill (found on the way, ADR-0008 amended). The pass 0.025 ms worst with 100,000 parked; the tick +0.3 %; a 1,000-crate yard woken +0.43 ms worst.

- **Active regions.** Each player's avatar is the centre of an active region: bodies within 2.5 km of any player are simulated in full, as today (plan.md §1.6). The universe's bubble already does this for one focus (ADR-0007: `RANGE_IN` 2.5 km, `RANGE_OUT` 3 km); the server's planet world gets the same rule for many players.
- **Parked.** A body (with everything it touches: its contact island) that is asleep and farther than 3 km from every player is **parked**: taken out of the physics world, stored in the planet's frame exactly as it was. It costs nothing per tick. When a player comes within 2.5 km it is put back, bit for bit, still asleep. A body that is awake stays in full physics wherever it is, until it comes to rest (a far landslide of crates finishes falling, then parks): realism first, and an awake lone body is a small island.
- **Region checks are cheap.** A spatial grid of parked islands (like the interest grid of ADR-0008), so checking which to wake is proportional to what is near the players, not to everything parked.
- **On rails.** In the universe (ADR-0007) vessels already go on rails and land; F7 makes the bubble's rules and the server's the same code (one `lod` module), and saves hold parked bodies like any other.
- **Spreading wakes.** A player arriving near a large parked yard wakes its islands over several ticks (nearest first), so arriving is not a spike; the worst tick is gated.
- **Networking.** Parked bodies are outside every client's interest anyway; a client entering a region sees its bodies appear through the normal snapshot path.
- **Validation:** park and wake a stack of crates bit-exactly (state hash before and after); a crate tower parked mid-topple stays active until it rests; a save with parked bodies loads them parked; players moving between regions; determinism of a run with parking.
- **Budget:** a planet with 100,000 parked bodies and 8 players among 400 active ones: the LOD bookkeeping ≤ 0.1 ms a tick, the tick as with the 400 alone (+5 %); waking a 1,000-body yard spread so no tick costs more than 2 ms extra.

### F7.3 Abstract mode: the rate solver (`creeer_rates`, ADR-0020)

**Done** (2026-10-18): stocks, processes with linear stock dependence and trusted boxes, proportional water-filling at stocks held at a bound; segments are exact (the matrix exponential of the linear system with the stocks' integrals, a closed form for one state, held processes' φ linear in the stocks), events found in closed form for steady stocks, by convexity for one state, by sampling and regula falsi otherwise; trapezoidal segments only where a varying rate is held. Books close to 10⁻¹⁶ of throughput. 10,000 machines through a day: 2.6 segments a machine, 4.8–4.9 ms (9.6–9.7 projected ≤ 10: a thin margin, recorded; off the tick's path).

- **Stocks:** amounts of something conserved (a species' mass, water's volume, charge in a battery, heat in a large mass), each with its bounds (empty, full).
- **Processes:** what a machine does per second at full rate, as a vector of flows into and out of stocks (its stoichiometry, measured), with how the rates change with the stocks it depends on (a linear model around the point it was measured at, and the range it may be trusted in).
- **The solver.** Between events, every process runs at a fraction u ∈ [0, 1] of its rate. A stock at a bound limits the processes that drain (or fill) it: its consumers share what flows in, in proportion to what they ask (a deterministic fixed point, the same rule as Factorio's power networks, but over every stock). Stocks then change linearly until the next **event**: a stock reaching a bound, leaving one, or a process leaving its trusted range. Events are found exactly (the first root in time), so a factory that runs out of ore at 03:12:47 stops then, not at the next sample.
- **Exact books.** Each process's amount over a segment is computed once and applied to all its stocks through its stoichiometry, with compensated sums: the ledgers close to rounding over days.
- **Lazy time.** Abstract machines are advanced only when something needs them (a player arrives, a save, a shared stock's other user, an event they scheduled); a day of game time is a few dozen segments, not 5 million ticks.
- **Validation:** closed forms (a tank draining through a constant process; two consumers sharing a limited inflow; a chain with a buffer filling then blocking upstream; cycles); exact event times; ledgers over a simulated year; determinism under reordering of machines' ids.
- **Budget:** 10,000 machines with 50,000 stocks advanced through a day of game time ≤ 10 ms; per tick, when nothing is due, ≈ 0.

### F7.4 Machines going abstract and coming back (`creeer_sim::abstraction`, ADR-0020)

- **A machine** is a set of the F6 domains' networks that form one thing (the powerhouse is the reference machine). It names its stocks (the powerhouse: its two tanks' water, the reactor's hydrogen, oxygen and water, the heat in its large masses) and can be measured (its ledgers) and written (its stocks).
- **Leaving.** Out of range, a machine is watched over a window of its full simulation (by default 10 s): when its rates over two consecutive windows agree (0.5 %), they are its process, with their dependence on the stocks from how they changed over the windows. A machine that never settles stays active (as an awake body does). Its full state is kept.
- **While abstract,** the rate solver advances it. When its stocks have moved far enough that its rates are stale (outside its trusted range, or an event changed its regime: a tank ran empty), a **probe** runs: its full state, with the abstract stocks written in, is simulated for a short window beside the ticks (on a worker), and its rates are measured again. The probe's seconds are real time for the machine (its books continue through it), so nothing is lost or made in the switch.
- **Coming back.** When a player is near, the stocks are written into the full state, and the machine runs on in full from the state its last probe settled.
- **Validation:** the powerhouse run until its upper tank is empty, in full (every tick) and abstract (with probes): the water drained, the hydrogen made, the heat given to the air within 1 %, the time the tank runs dry within 1 %, every ledger closed to rounding in both, and the shaft back at its speed within 0.5 % right after it is loaded again. The cost of the abstract run is a small fraction of the full one (target: under 2 %).
- **Integration:** the server's world holds machines and switches them by its players' regions (F7.2); saves hold abstract machines with their stocks and processes; the planet sandbox can place a powerhouse and shows its state (active, settling, abstract, probing) in a panel.
- **Budget:** a probe never adds to a tick (it runs beside); 1,000 abstract machines cost ≤ 0.05 ms a tick when nothing is due.

### F7.5 Stress scenarios on three operating systems (`creeer_tools stress`)

plan.md §5's stress scenarios, headless, each logging its tick percentiles (p50, p99, worst) and a **state hash** at the end:

- **a large factory:** the reference load's machines across all domains, with a hundred powerhouses (half abstract);
- **50,000 beams:** ten 5,000-beam structures under moving loads;
- **10 cars crashing:** ten space-frame cars into each other and a wall (plastic hinges, fractures);
- **a planet flyover:** the camera from orbit to the ground and along it at 300 m/s, the terrain streamed (chunks per second, the longest wait for a needed chunk: no hitch);
- **a busy server:** 8 scripted players walking, digging and dropping crates across a planet with 100,000 parked bodies and abstract machines.

Each scenario's hash is checked against its golden value on every OS in CI (determinism across platforms at scale, not only in unit tests). Times are gated on the development machine against the baseline (`cargo xtask perf`) and reported from CI's machines (shared runners: informational, as the spikes are). The CI workflow also runs on pushes to the milestone branch, so every sub-phase is checked on the three OSes.

### F7.6 Profiling pass (ADR-0021)

- **In-game profiler:** a small profiler of our own in `creeer_core` (named scopes timed per thread, collected per tick, nothing when off): `puffin_egui` pins egui 0.33 and we are on 0.36. An egui window shows the last ticks' phases as a flame graph and the worst tick of the last seconds, on the client and (over the network, on request) the server's.
- **The pass:** every stress scenario profiled; every phase that costs more than it should is fixed, exact (the rule of plan.md: never by loosening physics).
- **The reference tick** (plan.md §5): 10.3–10.5 ms projected against 10 ms at the end of F6, three quarters of it the 20,000 beams' solver (1.1 µs a beam a tick). F7 brings it within budget: a cheaper structure solve (SIMD kernels over independent nodes, bit-identical across platforms because nothing is reordered; fewer passes over the elements). Measured against the solver's accuracy tests and goldens.

### F7.7 The foundation gate

- Every validation suite green on Windows, Linux and macOS (CI).
- Every budget met: on the development machine (`cargo xtask perf`, Windows), and on Linux on the same machine where it can be measured (a container), macOS from CI's report.
- Every stress scenario's hash identical on the three OSes.
- A validation report `docs/physics/f7-validation.md`; plan.md's F7 row; `docs/future-improvements.md` current.
- Demo and review with the user, then squash-merge `f7` into main as "F7".

## Budgets at a glance

| What | Load | Budget |
|---|---|---|
| Data | base pack loaded and validated; a hot-reload applied | 1 s; 1 s |
| LOD bookkeeping | 100,000 parked bodies, 8 players | 0.1 ms a tick |
| Waking a region | 1,000 parked bodies | no tick + 2 ms |
| Rate solver | 10,000 machines, 50,000 stocks, a day of game time | 10 ms in all |
| Abstract machines | 1,000, nothing due | 0.05 ms a tick |
| Abstract accuracy | the powerhouse until its tank is empty | 1 % on what it made; books to rounding |
| Reference tick | plan.md §5 | 10 ms projected on min-spec |

## Decisions (defaults taken; open to the user's review)

1. **Abstract rates are measured from the full simulation** (with probes), not written as recipes: the physics stays the one source of truth (F6's principle 1).
2. **Parking only at rest.** Awake far bodies stay in full physics until they rest, instead of being frozen or put on rails mid-motion on a planet.
3. **Our own profiler**, since `puffin_egui` does not support our egui.
4. **Mods are data only** (plan.md: a Lua API for mods comes later).
5. **Linux budgets** measured in a container on the development machine when Docker runs; otherwise from CI's numbers, said so in the report.

## Risks

- **Abstract machines that depend on each other** through shared stocks change each other's regimes; probes per machine are then frequent. The tests include two machines sharing a tank.
- **Machines that never settle** (oscillators, a Lua script that blinks) stay active; windows averaged over a period would let periodic ones go abstract. Measured before it is decided.
- **The reference tick** needs a faster structure solve; ADR-0004's "Measured and rejected" list is read first so nothing rejected is tried again blindly.
- **macOS** has no machine here: CI is the only check, as before.
