# ADR-0016: Integration and multi-rate coupling

- **Status:** Accepted (2026-10-17)
- **Context:** F6.8 (`docs/f6-plan.md`). The F6 domains were each built and validated on their own (ADR-0009 to ADR-0015). F6.8 joins them in one machine, through every domain, and measures the reference load of plan.md §5. Two questions it had to answer:
  - how a fast coupling (a shaft, every tick) meets a network that steps more slowly (a large pipe network at 20 or 10 Hz, ADR-0014) without breaking either's energy books;
  - how a slow network's step stays off one tick's critical path.

## Decision

### Turbines in pipe networks (`creeer_fluids::pipes::turbine`)

- **A turbine is a link**, `LinkKind::Turbine { swirl, radius, loss, speed }`, by Euler's turbomachine equation:
  - the water enters the runner with its guide vanes' swirl and leaves with the blades', both in proportion to the flow: torque T = ρ·Q·(a·Q − r²·ω), with a = r₁·cot α₁/A₁ + r₂·cot β₂/A₂;
  - the head it takes for that work is ω·(a·Q − r²·ω)/g, and friction in its passages takes k·Q|Q| more;
  - held still it is a nozzle with the stall torque ρ·a·Q²; let run free it turns at a·Q/r² (runaway), its torque gone.
- **Its books.** A step dissipates the water's whole drop across the turbine, as it does a pipe's. The work its shaft takes each tick (`Network::turbine_work`) moves from dissipated to exported; driven faster than runaway, the shaft's work on the water is imported. The network's books close at every tick.

### Fast couplings every tick (`creeer_sim::hydro`)

- **The torque is linear in the shaft's speed** at the flow the network last found: T = T₀ + s·ω (`Network::turbine_curve`). The drivetrain's step is linear in the torque on a part: ω₁ = free + mobility·T (`Drivetrain::response`, ADR-0012).
- **Solved at the step's midpoint speed,** T = (T₀ + s·(ω₀ + free)/2) / (1 − s·mobility/2): the speed the drivetrain books its work at (ΔKE = h·Σ T·ω_mid, ADR-0012). So the work the water gives is, joule for joule, the work the drivetrain imports. The torque follows the shaft every tick, however seldom the network steps; the network's next step starts from the shaft's speed then.
- **The couplings are split in two** (`prepare` before the shared step, `finish` after), so several act on one circuit or drivetrain: `drive` (machines, ADR-0012), `electrochem` (cells, ADR-0013), `hydro` (turbines). Two couplings on one drivetrain see each other's torque as it was set: the later sees the earlier's of this tick, the earlier the later's of the last.

### Slow networks beside the ticks (`pipes/schedule.rs`)

- **Every tick:** a network that steps every tick steps at once.
- **Slower networks:** a network at 20 or 10 Hz begins its step on a worker (rayon) on the tick its rate falls on, from a snapshot of itself, and ends it on its next such tick. The step's heads, flows and open links, its solver and its books' increments are then taken in. Its results are those of its interval's end, taken at its end. The step runs beside the ticks between, not as one tick's spike.
- **Deterministic.** What a step is begun from and ended on are fixed ticks; no thread's timing can change what it finds. A step run beside the ticks is the step itself, bit for bit (`a_step_beside_the_ticks_is_the_step_itself`). Settings changed in between (a valve, a turbine's speed) count from the next step.
- **Clean edits.** Editing the network (a node or link added) ends a step under way first. A network with a step under way cannot be copied: the copy would lack its outcome.
- **Physically:** a quasi-steady incompressible network's states (its tanks' levels, its content) move over seconds. Its answer one interval (0.1 s) late is a delay a real network's controls see too.

### Heat from every domain

- **The generator's armature** and the rectifier: `Circuit::take_heat`.
- **The drivetrain's bearings and brakes:** `Drivetrain::take_heat`.
- **The reactor's heat:** its ledger's outflow less its inflow beyond the stack's electrical energy, reaching its coolant through its UA.
- All of it is received by the thermal network, which gives it to the air.
- **A shaft's twist damping is not heat.** Its loss and backward Euler's go either way within a cycle (`creeer_mech::book`), so they stay the drivetrain's loss. In the powerhouse that is 0.1–0.2 J in ten minutes, all in the first jolt.

### The powerhouse (`creeer_sim::powerhouse`)

- **What it is:**
  - an upper tank (30 m², 5 m deep, its floor 25 m up) drains down an 80 m, 12 cm penstock, through a 5 cm regulating valve, through a 10 cm reaction turbine, into a lower basin;
  - the runner turns a generator (0.16 V·s/rad, 20 mΩ) on a compliant shaft between bearings;
  - the generator feeds twelve 300 cm² alkaline cells through a rectifier (so the stack cannot motor it as a fuel cell), splitting water into hydrogen and oxygen in a reactor at 1 bar;
  - the generator's iron (60 kg), the frame (300 kg) and the stack's coolant (200 kg of water) take the heat and give it to the air;
  - a Lua PI script holds the shaft at 150 rad/s with the valve;
  - everything stands on a 2 t platform free to turn (gravity off: on a frictionless turntable), which takes every reaction torque through the drivetrain's mount (ADR-0012).
- **Each tick, in order:**
  1. the controller reads the shaft and sets the valve;
  2. the couplings' `prepare`: turbine, generator, stack;
  3. the circuit's step, the drivetrain's, the turbine's work out of the water, the reactor's step;
  4. the platform's step with the drivetrain's reaction;
  5. the pipes' tick, the heats into the thermal network, its step.
- **Client:** a "Powerhouse (F6.8)" window: the setpoint, 1×/10×/60× time, plots of the shaft, the powers and the temperatures, and where the water's energy went.

### Validation (`powerhouse/tests.rs`, `hydro_tests.rs`, `turbine.rs`, `schedule_tests.rs`)

- **Turbine:**
  - it stalls as a nozzle with ρ·a·Q² and runs away at a·Q/r² with no torque;
  - its shaft's power is ρ·g·Q·(the Euler head) to 10⁻¹², and the books close with it exported.
- **On a flywheel:**
  - the runner settles where Euler's torque meets its bearing's, within 10⁻⁶ of the bisected fixed point, with the pipes stepping every tick and every sixth tick;
  - every tick, the water's exported joules are the shaft's imported ones (10⁻¹²), and both books close.
  - Braked, it holds the stall torque within 10⁻⁹ and takes no work.
- **Ten minutes of the powerhouse,** with the pipes every tick and every sixth tick (10 Hz):
  - the shaft holds 150 rad/s within 0.15 rad/s (0.1 %), the valve 38 % open;
  - **energy:** the water gives 1.96 MJ. Hydrogen and the reactor's warmth get 59 %, the water's friction (mostly the valve's throttling) 28 %, heat held 10.5 %, heat to the air 1.4 %, the shaft and circuit hold 0.4 %, unrouted shaft damping 10⁻⁷. The sum closes to 10⁻⁹ of what the water gave;
  - every coupling has the same joules on both sides (10⁻⁹), and every domain's books close (10⁻⁹);
  - **mass:** the water between the tanks to 10⁻¹², the reactor's mass to 10⁻¹², hydrogen by Faraday's law to 10⁻³ (the cell rule's averaging), oxygen half the hydrogen to 10⁻¹²;
  - **angular momentum:** the shaft's 110 kg·m²/s is the platform's, opposite, to 10⁻⁷. The platform turns the other way;
  - everything warms; the stack from 293 K to 297.7 K.
- **The client's panel** holds the setpoint a minute in, its books closing.

### The reference load (`creeer_tools bench-spikes --only reference`)

- **What one tick holds:**
  - **20,000 active beams:** four 5,000-beam bridges, a 20 kN load crossing each at 20 m/s, their islands in parallel;
  - **8 players** walking circles by a yard of 100 crates;
  - **1,000 machines across all domains,** each domain at its own reference load:
    - a factory hall's 20,000 thermal nodes at 20 Hz;
    - the 10,000-node DC circuit and the 2,000-bus AC grid;
    - 1,000 cars' drivetrains;
    - 500 furnaces' equilibria at 10 Hz, a sixth a tick;
    - the 5,000-pipe mains at their rate, beside the ticks;
    - 1,000 aero panels on 100 tumbling bodies;
    - 200 floating bodies;
    - 10,000 gates and 100 Lua scripts.
- **Side by side:** every part is a task on the work-stealing pool, each still parallel within: a server's tick, its domains independent within it and coupled between ticks.

| Measured (development machine, p50) | Tick | Projected to min-spec |
|---|---|---|
| The F6 domains alone, side by side | 0.88–0.94 ms | 1.75–1.88 ms (their share: ≈ 5 ms) ✓ |
| Everything side by side | 5.1–5.3 ms | 10.3–10.5 ms (plan.md §5: 10 ms) ✗ |
| Everything one after another | 6.7–8.1 ms | |

- **The whole tick misses plan.md's 10 ms by 3–5 %**, and the F6 domains are not why:
  - one thread's work per tick is 30 ms, three quarters of it the beams (22.6 ms: 1.1 µs a beam a tick, two Newton iterations; the F2 solver, ADR-0004);
  - the bridges alone take 4.2–4.3 ms side by side, and the domains add about 1 ms to that;
  - the projection divides by the min-spec CPU's per-core speed (0.5). Its 6 cores against the development machine's 8 would add more for work this parallel.
  - Meeting the target needs a cheaper structure solve: recorded in `docs/future-improvements.md`, with ADR-0004's.
- Recorded in `cargo xtask perf`:
  - `reference_domains_p50_ms` (its budget checked);
  - `reference_tick_p50_ms` (gated against regression);
  - `reference_alone_p50_ms` and `reference_tick_p99_ms` (reported).
- **The perf baseline** (`perf/baseline.txt`) gains every F6 metric, from a full run late in a long session. The F0–F5 metrics keep the values they were saved with. Their code is unchanged or faster, and in that run untouched code (orbit ticks, terrain meshing, XPBD) measured 4–20 % slower: the machine's heat.

### Speed found on the way

- **A world's step no longer moves its bodies** out of the world and back (2.2 kB each) to step each group: each group borrows its own (`world/pass.rs`).
  - 200 floating bodies' world step: 0.53 → 0.26 ms;
  - 100 tumbling bodies: 0.27 → 0.15 ms;
  - 100 sleeping crates: 0.016 → 0.008 ms.
  - Bit-identical: the golden hashes are unchanged.
- **The DC circuit's right-hand sides** are worked out side by side and added in order, the same bits (`IslandSolver::stamp_rhs`). The 10k-node circuit's step went from 0.50 to 0.43 ms: 0.86–0.87 ms projected against its 1 ms budget, now with margin (ADR-0011, F6.8 amendment). Reusing the devices' laws from the last convergence check saved nothing measurable and was not kept.
- **Measured and rejected:**
  - coarser parallel grains in the structure solver (512 instead of 128: the bridge 2.03 → 2.44 ms);
  - capping a structure's parallel parts while several islands step side by side (no gain beyond noise).
  - Each bridge on its own thread pool ran 4 bridges in 3.3 ms against 4.2 ms on the shared pool. Not pursued: a world's islands share one pool with every domain.

## Future improvements

- **A cheaper structure solve** (the reference tick's 10 ms needs the beams' 1.1 µs a beam a tick about halved): see `docs/future-improvements.md`, Solvers and performance.
- **Water's angular momentum through the turbine:** the casing takes the runner's reaction, so the water enters and leaves the machine without swirl. Swirl carried out into the tailrace is not modelled.
- **Friction heat in the water** (the valve's and turbine's losses) is not routed to a thermal node: the pipes' content is carried adiabatically (ADR-0014).

## Consequences

- Any coupling between a fast and a slow domain takes the slow one's last answer as a curve in the fast one's state, solved at the fast step's midpoint, and books the work on both sides as one number.
- A slow pipe network's results arrive one interval after its step begins. Controls reading it see the delay, as real ones do.
- Every F6 budget is in `cargo xtask perf`, and the reference load is measured there. The F6 domains take under a fifth of the tick projected; the beams' solver is what the 10 ms target waits on.
