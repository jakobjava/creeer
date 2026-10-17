# F6 validation report: simulation domains

What each F6 domain was checked against, how conservation is tested, and what the integration showed. The decisions and the full lists are in the ADRs (ADR-0009 to ADR-0016); this report gathers the evidence. Every result below is an automated test (`cargo xtask ci` runs them all) unless it is marked as a benchmark.

## How conservation is tested

- **Every domain keeps a ledger** (`creeer_network::Ledger`): what was supplied, dissipated, exported to another domain and imported from one. Its imbalance against the energy it holds (`relative_imbalance(stored)`) is the test: the books close to rounding (10⁻⁹ to 10⁻¹²) in every domain's tests.
- **Every coupling books the same number on both sides:**
  - a circuit's machine work and its drivetrain's;
  - a cell's electrical energy and its reactor's;
  - a turbine's water and its shaft;
  - Joule, friction and reaction heat and the thermal network.
- **Mass** is tested where it moves: pipe content (each tracer's own ledger), reactors (atoms), tanks (volume), Faraday's law (charge to moles).
- **Angular momentum** is tested where torques cross bodies: drivetrain housings on free bodies (mounts).
- **Determinism:** parallel results are compared with one thread's, bit for bit (sparse factorisations, thermal and electrical islands, structures, logic parts), and golden hashes fix whole runs (planet physics, structures, a Lua run in debug and release).

## Domain by domain

### Network kit (ADR-0009)

- **Sparse LU and Cholesky:**
  - parallel factorisations equal the sequential ones bit for bit;
  - a refactorisation equals a fresh factorisation;
  - complex scalars are checked against dense solves.
- **Islands, sleep and the ledger's closure** are tested on random graphs.

### Thermal (ADR-0010)

| Case | Result | Reference |
|---|---|---|
| Lumped cooling | backward Euler's exact recurrence to 10⁻⁹ K | the exponential |
| Steady rod | linear profile to 10⁻⁶ K | Fourier |
| Semi-infinite slab | 0.5 % of ΔT | erfc |
| Plate in sunlight at 1 AU | 331 K to 10⁻⁴ K | (S/2σ)^¼ |
| One-phase Stefan problem | 3 % | Neumann |
| Steel tube in the ISO 834 fire | within 2 K over an hour | EN 1993-1-2 §4.2.5.1 |
| Correlations | within their scatter | Ostrach, McAdams, Blasius, Hilpert, Ranz–Marshall |

### Electrical (ADR-0011)

- **Transients:**
  - RC charging (the resistor's heat ½CV²);
  - RLC ringing at f₀ to 0.2 % and Q to 2 %;
  - a transient a million times faster than the tick settles without ringing.
- **Devices:**
  - Shockley's diode to 10⁻⁹ and an LED's operating point;
  - batteries' rate capacity and recovery;
  - the KC200GT solar module's I–V curve;
  - a DC motor's speed–torque line to 10⁻⁶;
  - fuse and breaker trip times.
- **AC grids:**
  - resonance and Q swept in frequency to 10⁻⁶;
  - a transformer against its equivalent circuit to 10⁻⁹;
  - an induction motor's torque against Thevenin's form to 10⁻¹²;
  - constant-power loads.
- **Coupled:** a wire heating itself to I²R(T) = G·(T − T_air), every joule reaching the thermal network.

### Drivetrains (ADR-0012)

- **Against closed forms:**
  - two inertias on a shaft ring at √(k·(J₁ + J₂)/(J₁·J₂));
  - a clutch's engagement loses ½·J_eq·Δω² (10⁻⁹);
  - viscous and Coulomb spin-down and its stopping time;
  - a belt slips at its capstan torque;
  - planetary ratios and efficiencies;
  - an open differential's equal split;
  - backlash's plastic impact.
- **With machines and bodies:**
  - a motor through a gearbox at V·k/(k² + R·B), stalling at V/R on both sides at once;
  - a flywheel spun up in a tumbling free frame keeps the total angular momentum to 2·10⁻⁸.

### Materials and chemistry (ADR-0013)

| Case | Result | Reference |
|---|---|---|
| CH₄/air adiabatic flame | 2224 K | CEA 2226 K |
| H₂/O₂ adiabatic flame | 3074.5 K | 3075–3079 K |
| Boudouard CO at 1000 K | 71.0 % | 70.9 % (JANAF) |
| CO over iron and wüstite | 61.3 % (1000 K), 71.1 % (1273 K) | 62.0, 69.6 % (Darken and Gurry) |
| NH₃ at 700 K, 300 bar | 36.7 % | 36.6 % |
| Water's vapour pressure | 0.01–1.9 % to 550 K | IAPWS-95 |
| Gas viscosities | within 2.5 % | REFPROP |
| Electrolysis | 1.229 V reversible, 1.481 V thermoneutral, Faraday to the mole | textbook |

### Fluids, air and water (ADR-0014)

- **Atmospheres:** the US Standard Atmosphere 1976 to 5·10⁻⁴.
- **Pipes:**
  - Hagen–Poiseuille, the Moody chart, Torricelli's tank, a pump on its system curve;
  - Weymouth's gas line, whole and sectioned;
  - energy books to rounding;
  - content carried as plug flow, its mass exact.
- **Aero:**
  - lifting-line slopes, stall, Prandtl–Glauert and Ackeret;
  - the sphere's drag curve;
  - a crate under a canopy loses the air's work to 0.1 %.
- **Sea:**
  - Archimedes' draft;
  - the wall-sided righting arm;
  - a tall box capsizing;
  - a holed hull sinking.

### Logic (ADR-0015)

- Truth tables, latches, timers, the PID block (to its setpoint within 10⁻³).
- **Lua PI scripts:**
  - hold a tank's level within a centimetre;
  - hold a motor's speed within 0.5 % under a load.
- **The sandbox:**
  - runaway and recursing scripts stop, however they catch it;
  - a run's hash is the same in debug and release.

## Integration (ADR-0016)

### Turbines

| Case | Result |
|---|---|
| Held still | a nozzle: Q = √(H/k), stall torque ρ·a·Q² (10⁻³) |
| Let run free | runaway at a·Q/r², torque under 10⁻⁶ of stall |
| Running | shaft power = ρ·g·Q·(Euler head) to 10⁻¹²; books close with it exported |
| On a flywheel and bearing (pipes every tick, and at 10 Hz beside the ticks) | settles at the bisected fixed point of Euler's torque and the bearing's within 10⁻⁶; the water's exported joules are the shaft's imported ones every tick (10⁻¹²) |
| Braked | holds the stall torque within 10⁻⁹; no work taken |

### Slow networks beside the ticks

- A meshed grid held at a step every 3 or 6 ticks begins a step on each due tick and returns it on the next.
- A step run beside the ticks equals the step run at once, heads and flows bit for bit, books equal.

### The powerhouse: ten minutes, every domain

The scene: an upper tank, a penstock, a regulating valve, a reaction turbine and a lower basin; a generator on a compliant shaft; a rectifier; twelve alkaline cells and their reactor; heat to the air; a Lua PI controller; a free platform. It runs with the pipes every tick and at 10 Hz.

**Where the water's 1.96 MJ went** (pipes every tick):

| To | Share |
|---|---|
| Hydrogen and the reactor's warmth | 59.3 % |
| The water's friction (mostly the valve's throttling) | 28.4 % |
| Heat held (generator iron, frame, coolant) | 10.5 % |
| Heat given to the air | 1.4 % |
| Held by the shaft and the circuit | 0.4 % |
| The shaft's twist damping (not heat) | 10⁻⁷ |
| **Sum against what the water gave** | **equal to 10⁻⁹** |

**Checks:**

| Check | Result |
|---|---|
| Shaft speed (setpoint 150 rad/s), last eight minutes | within 0.15 rad/s |
| Every coupling, both sides | equal to 10⁻⁹ |
| Every domain's books | close to 10⁻⁹ |
| Water between the tanks | 10⁻¹² |
| Reactor mass | 10⁻¹² |
| Hydrogen against Faraday's law | 10⁻³ (the cell rule's averaging of the current) |
| Oxygen against half the hydrogen | 10⁻¹² |
| Angular momentum of the platform and the shaft (110 kg·m²/s exchanged) | 10⁻⁷; the platform turns the other way |
| Temperatures | all rise; the stack 293 → 297.7 K |

### The reference load (benchmark, `creeer_tools bench-spikes --only reference`)

- **The F6 domains alone,** side by side: 0.88–0.94 ms on the development machine, 1.75–1.88 ms projected to min-spec. Their share is ≈ 5 ms: met.
- **The whole reference tick:** 20,000 active beams, 8 players and the 1,000 machines across all domains take 5.1–5.3 ms, 10.3–10.5 ms projected. Plan.md's 10 ms is not met. Three quarters of the tick's work is the beams' solver (F2, ADR-0004); a cheaper structure solve is recorded in `docs/future-improvements.md`.

## Not validated here

- **Two-phase pipe flow**, waves and sloshing, mixed AC/DC converters, transistors: not modelled in F6. See `docs/future-improvements.md`.
- **The min-spec CPU itself:** every budget is a projection from the development machine (per-core speed × 0.5).
