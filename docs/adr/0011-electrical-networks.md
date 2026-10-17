# ADR-0011: Electrical networks

- **Status:** Accepted (2026-10-17)
- **Context:** F6.3 (`docs/f6-plan.md`). Circuits from a lamp on a battery to a factory's wiring and a town's grid: resistors and wires that heat, capacitors and inductors, sources, switches, fuses and breakers, diodes and LEDs, batteries, solar modules, DC machines; transformers and induction motors on AC grids. Joule heat goes to the thermal network (ADR-0010), torque to shafts (F6.4). Budgets on the minimum-spec CPU: a 10,000-node DC/transient network and a 2,000-bus AC grid, each 1 ms a tick.

## Decision

### Two solvers on the sparse kit (ADR-0009)

- **`Circuit`: DC and transient**, modified nodal analysis on the LU. Its unknowns are node voltages, plus a current for each voltage source, battery and machine armature, and an internal node behind each diode's and solar module's series resistance. Islands (the parts only ground joins) are solved in parallel. **GMIN**, 1 TΩ from every node to ground (SPICE's), keeps a node an open switch cuts off defined; its leakage is booked.
- **`Grid`: AC**, balanced three-phase networks as their per-phase equivalent in **phasors** at the grid's frequency, quasi-static each tick, on the complex LU. A 60 Hz tick cannot follow a 50 Hz waveform; its phasor is what a grid's behaviour over a tick is. Inputs and outputs are three-phase (line-to-line volts, total watts).
- What the tick cannot resolve is written down, not hidden: PWM is an average, a waveform a phasor, a transient faster than the tick settles where it settles.

### Integration: BDF2, not the trapezoidal rule

- The trapezoidal rule's amplification tends to −1 for modes far faster than the step. A microhenry of wire or a capacitor behind an ohm at a 60 Hz tick would ring at the tick's rate forever. **BDF2** (second order, L-stable) is used instead, with **backward Euler for the two steps after a restart** (the first step, a switch, a fuse). BDF2 extrapolating from a history that spans a jump overshoots it by about h/τ of the jump; started two steps later, by about (h/τ)², 10⁻⁸ V on a 5 V step.
- **Charge**: BDF2's companion current does not telescope to C·Δv. The charge delivered differs from what a capacitor holds by bounded boundary terms (½·C·(v₂ − v₁) from the restart: 0.15 nC in the 1 µs-RC test). It never accumulates; the energy it implies is booked where it arises.
- A machine's rotor is eliminated each step (ω = (r + k·i)/D, its inertia reflected into the armature as k²/D), its speed computed after the solve: one unknown per machine.

### Newton's method

- Diodes and solar modules are linearised once an iteration and stamped from that. The step stops once **every device's exact current at the new iterate is within 1 pA + 10⁻⁹ of its linearisation**: the circuit's own equations then hold, and no second stamp-and-solve is needed. A circuit near steady state takes one iteration a step.
- Junction voltages are limited as SPICE's pnjlim does (a step above the critical voltage taken on the logarithm), so the exponential cannot overflow and Newton converges from any start.
- Linear components keep their matrix entries between restarts; each iteration restamps only their right-hand sides, and the nonlinear devices' entries in place (each component's entries have a fixed offset in the pattern). The LU refactorises only when an entry changed, reusing its pivots.

### Energy

- At a solution Kirchhoff's laws hold, so by **Tellegen's theorem** the components' v·i sum to zero. Each component books its own share:
  - a resistor's, wire's or contact's as heat;
  - a store's (capacitor, inductor, battery, machine) as the change in what it holds, the rest as heat (its losses, and the integration's damping of what the step cannot resolve);
  - a source's as supply;
  - an LED's light and a machine's shaft work as energy passed on.
- **During BDF2 a step's power is averaged over the step** (Tellegen holds at both ends), so a resistor's heat is second order: taken at the step's end alone it was 1 % short at h/τ = 0.01. Right after a restart the power is taken at the step's end, where a jump's energy is (a source charging a capacitor at once gives C·V²).
- The ledger closes to Newton's tolerance, and to the LU's backward error for a linear circuit (2·10⁻¹⁶ J in the stiff-RC test). AC grids book phasor powers the same way (Tellegen for complex powers).

### Devices (`devices.rs`, `induction.rs`), each with its sources

- **Diodes**: Shockley's law with series resistance (1N4148 and 1N4007 SPICE models); **LEDs**: the same with their emitted fraction (a red LED: 1.92 V at 20 mA).
- **Batteries**: a kinetic battery model (two charge wells; Manwell & McGowan) under a Thevenin circuit (R₀ and an RC pair). Charge is conserved; the rate effect Peukert's law describes emerges from the available well emptying first: a flooded lead-acid battery gives 60 % of its C/20 capacity at C/1. A rested battery recovers. The energy released as the wells level is heat. The open-circuit voltage collapses as the available well empties, which is what ends a discharge.
- **Solar modules**: the single-diode model with irradiance and temperature (Villalva et al. 2009; the KC200GT within 1–2 % of its datasheet's I_sc, V_oc, I_mp and P_max).
- **DC machines**: back-EMF k·ω and torque k·I, armature R and L, rotor inertia and friction, the shaft's load from outside.
- **Fuses**: pre-arcing I²t. **Breakers**: thermal-magnetic, an inverse-time bimetal plus an instantaneous magnetic trip.
- **Transformers**: series impedance and magnetizing branch on the primary, ideal ratio. **Induction motors**: the IEEE per-phase equivalent circuit, slip from the rotor's speed; the rotor stepped by backward Euler on its electromagnetic torque at the step's bus voltage.

### Validation (`tests.rs`, `validation.rs`, `grid_tests.rs`; `creeer_sim::joule_tests`)

- Voltage divider; RC charging against the exponential, the resistor's heat ½CV²; a transient a million times faster than the tick settling monotonically; RLC ringing at f₀ (0.2 %) with its Q (2 %).
- A wire's resistance with temperature (CRC copper); a diode against Shockley solved independently (10⁻⁹); an LED's operating point and light.
- Battery capacities against rate; recovery at rest; the KC200GT's I–V curve; a DC motor's speed–torque line (10⁻⁶); fuse and breaker trip times.
- A mixed circuit's power balance; AC resonance and Q swept in frequency (10⁻⁶); a transformer's ratio, losses and regulation against its equivalent circuit (10⁻⁹); an induction motor's torque against the textbook's Thevenin form (10⁻¹²) and its loaded speed; a constant-power load against the quadratic; a mixed grid's balance.
- Determinism (parallel against one thread). A self-heating wire settling where I²R(T) = G·(T − T_air), every joule reaching the thermal network.

### Measured (`cargo xtask perf`, section `electric`)

| network | size | step p50 | projected | budget |
|---|---|---|---|---|
| factory DC wiring: 3,330 stations, each a breaker, motor, capacitor and LED, on 48 V batteries | 10,000 nodes, 16,700 unknowns | 0.47–0.48 ms | 0.93–0.97 ms | 1 ms |
| town AC grid: 20 feeders, 400 transformers, loads and motors | 2,400 buses | 0.12 ms | 0.24 ms | 1 ms |

The DC margin is slim and was measured on a heat-soaked machine (structures run about 10 % slower than cool, ADR-0005). Its step is one Newton iteration (stamping 0.13 ms, LU solve 0.09 ms, device checks 0.05 ms) and the components' finish in parallel (0.2 ms).

### Rejected

- **The trapezoidal rule**: undamped ringing of unresolved modes.
- **Parallel linearisation and checking of devices**: slower. A few thousand exponentials cost less than waking the threads.
- **Peukert's law as a capacity derating**: it destroys charge (the ledger would leak). The two-well model gives the effect and conserves charge.

## Consequences

- Couplings are explicit and booked: `take_heat` per component for the thermal network, `set_temperature` back. A machine's `set_load` and `level` (its speed) are what F6.4's shafts will drive.
- Mixed AC/DC (rectifiers, inverters) is not modelled yet: a grid and a circuit are separate networks. A converter is an average model between them, deferred to the integration (F6.8) or later.
- Unbalanced three-phase grids (a single-phase load on one phase) are not modelled: the per-phase equivalent assumes balance.
