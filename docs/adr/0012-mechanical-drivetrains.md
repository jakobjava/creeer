# ADR-0012: Mechanical drivetrains

- **Status:** Accepted (2026-10-17)
- **Context:** F6.4 (`docs/f6-plan.md`). Rotating machinery from a flywheel on a bearing to a car's powertrain: compliant shafts, gear meshes, belts and chains, clutches and brakes, bearings, planetary sets and differentials. Electric machines (ADR-0011) drive them and are driven by them; their housings sit on the structures and bodies of the physics world (ADR-0004/0005), which must feel every reaction. Budget on the minimum-spec CPU: 1,000 drivetrains of ten elements each, 0.3 ms a tick.

## Decision

### A drivetrain: parts, elements, a housing (`creeer_mech`)

- **Parts** are rotating inertias, each about its own axis in the housing's frame. Node 0 is the housing itself: brakes, bearings and stators push against it.
- **Elements:** a compliant **shaft** (k·φ + c·φ̇; its twist is state); a **gear** (ω_b = ratio·ω_a, a negative ratio for an external mesh) with optional **backlash**; a **planetary set** (Willis: ω_s + Z·ω_r = (1 + Z)·ω_c; Z = 1 is an open differential); a **belt** or chain (a gear that grips up to its capstan capacity, (T₁ − T₂)·r with T₁/T₂ = e^(μθ), then slips); a **clutch** or brake (Coulomb friction of a set capacity); a **bearing** (viscous and Coulomb friction against the housing).
- **Torques on parts:** from the housing (a motor's: its stator takes the opposite) and from outside (a wheel's from the ground: its reaction is not the housing's). Their work is booked separately, so a motor's is exactly what its circuit exported.

### Integration: backward Euler with multipliers

- The parts' speeds by **backward Euler** (first order, L-stable: a stiff shaft between a flywheel and a pinion rings at kHz; a 60 Hz tick cannot resolve it and must not ring with it). Rigid elements (gears, gripping belts, stuck clutches and brakes, stuck Coulomb bearings, planetary sets) are **Lagrange multipliers**: exact constraints, no stiff springs.
- The system is solved through its constraints' **Schur complement**: the parts' block (inertia over the step, shafts, viscous friction; symmetric positive definite) by Cholesky, then S·λ = G·P⁻¹·b with S equilibrated (each constraint's own mobility ±1) and factorised with **full pivoting**. A redundant constraint (a stuck brake and a stuck bearing on one shaft, two meshes in parallel) is a dependent pivot below 10⁻⁹: dropped, its multiplier zero, the others carrying its share. Constraints hold exactly: a held brake never creeps.
  - *Rejected:* a small compliance on every constraint. It made the matrix regular, but a held brake crept by 10⁻⁹ of a step's motion forever, and a stopped part never quite stopped.
- **Friction by an active set**, round by round until consistent: a stuck element asked for more than its capacity slips; a slipping one whose slip reversed (by more than 10⁻⁹ of what its own friction changes in a step) sticks. Reaching zero slip is not reversing: an element held still by another slips at its capacity with zero slip, saturated (taken as sticking, the split between the two flipped forever). A released clutch keeps whichever way it slips (it carries nothing).
- **Efficiency in the direction power flows:** a transmission loses (1 − η) of the power through it, forwards η, backwards 1/η on its output's torque. Which way power flows is state, kept from step to step, and judged at the step's **midpoint speeds** (where its loss is booked, so a consistent direction never books a negative loss), only in rounds whose grips held. Power below 10⁻¹⁰ of the drivetrain's torques times its speeds (a differential going straight, an idle belt, a stalled gear) is rounding, not evidence. A transmission whose direction reverses twice within a step goes **neutral** (lossless) for the step: it carries about nothing either way.
- **Backlash:** a gear within its play carries nothing and its place in the play is state; at a flank it meshes, each flank pushing only. The engagement is backward Euler's plastic impact, its loss ½·J_a·J_b/(J_a + J_b)·Δω² booked.
- A step that does not settle in 30 rounds ends as its last round solved it, its books of the torques that solve had (none in the bench or the tests).

### Energy

- The parts' equations J·(ω − ω₀)/h = ΣT, multiplied by their **midpoint speeds** (ω + ω₀)/2 and summed, say ΔKE = h·Σ T·ω_mid **exactly**. Each element books the power its torques put in at the midpoint: a gear its loss, a slipping clutch its friction heat (the trapezoid: end speeds would miss C·h/(J·Δω) of it, a sixth of a small part's brake heat), a shaft its strain energy's change and the rest (its damping and backward Euler's, which within a cycle can go either way by O(h²)). The ledger closes to the solve's rounding (3·10⁻¹⁵ over the bench's 1,000 drivetrains; 2·10⁻¹² of the largest flow over 50,000 steps).
- Friction elements' heat is kept per element for the thermal network (`take_heat`).

### Newton's third law: the housing

- The **reaction** on the housing is minus every torque on the parts from the housing (elements to it, motors' stators, the meshes' couples where axes differ) minus spin × L, the torque that carries the parts' angular momentum round as the housing turns (gyroscopic). Tested: the housing's reaction and the parts' change of angular momentum seen from outside cancel every step, with bevel gears and a turning housing.
- **Mounting** (`creeer_sim::mount`): a housing fixed to a body's node steps in the node's turning frame; its reaction is a torque on the node. The reaction uses the spin the step starts with, but the housing turns by what the body's step makes of it: O(h) a step. So the mount compares the impulse it gave with what the parts' momentum, seen from outside, actually changed by, and owes the difference to the next step. The body's and the parts' momentum together then stay what they were, but for that one step's correction (and, for a body stepped by Newmark, half its last torque's impulse, which its step carries into the next).

### Machines on drivetrains: one system

- A drivetrain's step is linear in the torque on a part, for the friction states it starts with: the part ends at ω = free + mobility·T (`Drivetrain::response`: one factorisation, two solves). A DC machine whose rotor is that part takes this as its rotor's equation (`Circuit::set_shaft`), so its armature current, back-EMF and torque are solved with the drivetrain's response. The drivetrain then steps with that torque and ends where the circuit said (`creeer_sim::drive`). Its work is booked on both sides as the drivetrain books it (torque × midpoint speed), so the circuit's export is the drivetrain's import, joule for joule.
- *Not exact:* a friction element that switches within the drivetrain's step (a brake locking) changes the response the circuit was given; for that step the two differ by what the switch changed (each ledger still closes; the next step starts from the drivetrain's speed). Two machines on one drivetrain each see their own part's response, with the other's torque as it was the step before.
- A released brake slips from the moment it is released (`set_capacity`), so a response asked before the next step sees it free.

### Physics fixes found on the way (ADR-0004 amended)

The free-floating station (a flywheel spun up inside a tumbling frame) found three faults in the structure solver: the energy guard took a torque's work for numerical gain, Newmark's mean of changing loads was not credited, and rotations under torque gained momentum first order in h. See ADR-0004, "Amended in F6.4".

## Consequences

- **Validated against closed forms** (`creeer_mech` tests, 13; `creeer_sim` drive and station tests):
  - two inertias on a shaft ring at √(k·(J₁ + J₂)/(J₁·J₂)) (10⁻⁴; backward Euler's own period);
  - a gear train in steady state passes η of the power;
  - a clutch engaging loses ½·J_eq·Δω² whatever its capacity, all of it the clutch's heat (10⁻⁹);
  - a flywheel on viscous and Coulomb friction spins down as (ω₀ + T_c/c)·e^(−ct/J) − T_c/c, stops within two steps of J/c·ln(1 + c·ω₀/T_c), and stays stopped;
  - a brake holds below its capacity and slips above it;
  - a belt slips at its capstan torque, crossed or open;
  - a planetary reduction gears by 1 + Z and passes 1 − (1 − η)·Z/(1 + Z);
  - an open differential splits its torque equally whatever its wheels' speeds;
  - backlash: free across its play, the plastic impact's loss, letting go and clunking back;
  - redundant constraints, and two meshes of different ratios jamming;
  - a scooter motor through a gearbox runs at V·k/(k² + R·B) and stalls against a brake at V/R, both sides at rest together, one step apart only where the brake locked;
  - a turbine driving a generator into a load;
  - a flywheel spun up in a tumbling free frame: the frame turns the other way, total angular momentum to 2·10⁻⁸ of the flywheel's over ten seconds, no step redone by the energy guard.
- **Budget met:** 1,000 cars' powertrains (engine and clutch, gearbox, propeller shaft, open differential, two braked wheels on the road, a belt to the alternator) in 0.085–0.105 ms on 16 threads, 0.17–0.21 ms projected (≤ 0.3). It took:
  - an active set that settles (17.8 → 1.4 rounds a step);
  - buffers kept per drivetrain (no allocation in a step);
  - Cholesky for the parts' block;
  - **factors reused while the matrix is unchanged** (the same step, grips, flows and build give the very same matrix: bit-identical).
  - Measured: 3.4 → 0.085 ms.
- **Sandbox:** the drivetrain bench window (a battery-fed motor through a gearbox onto a flywheel with a brake; a belt drive under a load that can slip it) and the "flywheel station" physics scene (headless: `--physics flywheel --view station`).
- **Not modelled:** gear-mesh and belt compliance (rigid while they grip), belt creep below the slip limit, Stribeck friction (static = kinetic), the no-load losses of gearboxes (churning: only the mesh's loss under load), cross-coupling of two machines on one drivetrain within a step, and torques on a drivetrain from a body's linear acceleration (only its turning enters).
