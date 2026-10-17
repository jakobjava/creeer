# ADR-0014: Fluids, air and water

- **Status:** Accepted, in progress (2026-10-17; completed with F6.6)
- **Context:** F6.6 (`docs/f6-plan.md`). Pipe networks of liquids and gases that carry what flows in them; planets' atmospheres; air on panels, wings and bluff bodies; oceans, buoyancy and flooding. Budgets on the minimum-spec CPU, per 60 Hz tick: 5,000 pipe segments 1 ms, 1,000 aero panels 0.2 ms, 200 floating bodies 0.2 ms.

## Decision

### Atmospheres

- Per planet in its data file (`AtmosphereData`): composition, the surface's pressure and temperature at the datum, layers of constant lapse rate by geopotential altitude (as the US Standard Atmosphere 1976 defines Earth's).
- `creeer_fluids::atmosphere`: hydrostatic closed forms layer by layer under the planet's real g(r) (geopotential), density by the ideal gas of the mixture's molar mass, viscosity by Sutherland, sound by γ·R·T/M. Validated against the standard's base pressures, density, sound and viscosity.

### Liquid pipe networks: the global gradient method

- Junctions, reservoirs, tanks; pipes (Darcy–Weisbach with Colebrook, Hagen–Poiseuille below Re 2000, Dunlop's cubic between), pumps (affinity laws, a parabolic efficiency), valves, check valves. Todini and Pilati's global gradient method, as EPANET: Newton on heads and flows, the flows eliminated, one symmetric positive definite system in the heads. Tanks by backward Euler. Every joule booked (pump work in, friction and pump losses to heat, water's ρgHQ at reservoirs and consumers, the tanks' potential energy held).
- **Exact slopes.** Each law's slope is analytic: Colebrook's by its implicit derivative, the laminar law linear (finite at no flow, where a difference quotient of the turbulent law gave Newton a zero slope).
- **Speed** (`pipes/solve.rs`, `friction.rs`):
  - a small network's step keeps its buffers, refactorises a factor that costs no more than four solves rather than preconditioning, keeps which junctions reach a reservoir while the same links are open, and books its energy from the laws as the last solve linearised them (exactly the heads' differences: the books close to rounding, and no law is evaluated again);
  - Colebrook started from the pipe's last solve carried along its slope to the new Re (one logarithm instead of four Newton steps from Swamee and Jain);
  - friction factors warm-started from the pipe's last solve (one or two Newton steps instead of four), the laws evaluated in parallel;
  - each step factorises once; its later Newton iterations are solved by conjugate gradients preconditioned with that factor (judged done when M⁻¹·r is rounding's: a stale preconditioner with a near-singular direction never passes as converged);
  - Newton stops when the flows stop moving or converge quadratically below the tolerance (next move ≈ C·move², C measured).
- **A leak found:** the system's 10⁻¹² regularisation sat on every junction and leaked 10⁻¹² m³/s per metre of head; now only on junctions cut off from every reservoir and tank.

### Gas pipe networks

- Isothermal compressible flow (a long pipe's gas takes its ground's temperature): p_a² − p_b² = (RT/M)·f·(L/D)·(ṁ/A)², Weymouth's or Colebrook's factor; stores (V·M/RT·dp/dt) by backward Euler; Newton on the stores' pressures with the sparse LU. A fixed (Weymouth) factor is floored by the laminar 64/Re (its law had no slope at no flow); convergence is judged on the mass balances as well as the step. Validated against Weymouth for a whole and a sectioned line.

### What pipes carry

- A tracer on either network (`pipes/content`): any per-kilogram quantities (moles of each species, enthalpy), conserved exactly with a ledger each. Liquid pipes are plug flow (Lagrangian parcels, no smearing); junctions mix by mass; stores are well mixed; links holding nothing (pumps, valves, a gas pipe whose volume its stores hold) pass content on, nodes settled upstream first. A step is split so no pipe is flushed of more than half its fluid in a part. A mixture's temperature comes back from its enthalpy (`of_mixture`, `temperature`): a pipe delivers what the reactor made, at its temperature.

### Rates: each network picks its own

- A network small enough to step within its share of the budget runs at 60 Hz, every tick. Only large meshed networks drop to a slower rate, 20 Hz, or 10 Hz at most. The rate is chosen automatically from the network's estimated cost.
- The estimate comes from its structure, not from measured time (`pipes/schedule.rs`): a step's fixed cost, its links' laws (two Newton iterations, in parallel from 256 links) and content, one factorisation's multiply-adds and about five solves with the factor. The constants are measured on the development machine and doubled for the minimum-spec CPU, as the benches project. The chosen rate is therefore deterministic and the same on every machine and run. It is reconsidered only when the network's structure changes.
- **Shares.** A network's share of the 1 ms budget for 5,000 segments is its links' part of them, but at least 50 segments' (10 µs). A step's fixed cost (about 2 µs on the minimum-spec CPU) is not paid per link, and small networks, which a world has many of, run every tick.
- **Measured** (fluids bench, projected to the minimum-spec CPU):
  - a 4,900-pipe meshed town grid estimates 3.1 ms a step (measured 2.9) and picks 10 Hz: 0.48 ms a tick;
  - 250 houses of 21 links each pick 60 Hz, stepped in parallel as separate machines: 0.18 ms a tick.
- **The cost per tick is the step's over its interval.** A slow network's step falls in one tick. F6.8's scheduler should run it spread over its interval (or beside the tick), so that it is not one tick's spike (ADR-0016).
- Physically, a quasi-steady incompressible network's only states are its tanks' levels and its content, which move over seconds; a slower rate converges each of its steps to the same tolerance.
- Fast couplings do not wait for a slow network: a turbine's or pump's torque on a drivetrain is taken every tick from the machine's curve at the shaft's speed and the heads the network last found, with the energy books still closing (F6.8, ADR-0016).

### Aerodynamics

- **Surfaces** (`creeer_fluids::aero`): panels (plates, fins, wings), spheres, bluff bodies.
  - **Attached flow:** Helmbold's lifting-line slope and induced drag.
  - **Stalled:** a plate's normal force C_N90·sin α (Viterna's C_N90), blended in over 2°.
  - **Compressibility:** Prandtl–Glauert below Mach 0.7; Ackeret above 1.3, capped by the pitot shock's stagnation pressure; modified Newtonian where that is more; the transonic gap bridged linearly.
  - **Skin friction:** Blasius / Prandtl–Schlichting, compressible.
  - **Spheres:** Morrison's drag curve and the measured transonic rise.
  - **Heating:** the recovery temperature with Reynolds–Colburn; Whitaker's correlation on spheres, Sutton–Graves at hypersonic speed.
- **In the implicit step: dashpots** (`creeer_physics::structure::Dashpot`). A surface's force at the step's start goes on its node as a load. Its slope's damping part (symmetric, its positive part by Smith's closed-form eigenvalues) goes on as a dashpot, implicit in the step's objective. Under Newmark it is taken h/β times: the trapezoidal rule rang, a 10 g body in 100 N·s/m flipping its velocity each step. The accelerations carried are then the forces' own. The energy guard credits the dashpots' work, and Newmark's mean of the last step's.
- The panel's slope is analytic (checked against differences of its force); the surfaces on one node are done together in parallel and share one dashpot. The air's work is booked by the force the step's scheme took (a crate under a canopy: lost energy = air's work to 0.1 %).

### Oceans and buoyancy

- Planets with an ocean in their data fill every basin to sea level, the datum's equipotential. Near a body, the sea is the plane tangent to it.
- **Buoyancy** comes from the exact submerged volume of spheres (the cap), boxes and hulls (faces clipped, tetrahedra from the waterline).
- **Added mass** is half the water displaced. It enters the step as inertia without weight (`AddedMass`): the momentum exchanged with the water, M·v − P. Going in, a body shares its momentum with the still water; going out, it leaves the water's behind.
- **Hull drag** acts on the wetted area the flow sees.
- **The waves a heaving body makes** damp it: Haskind–Newman's long-wave limit B = ρω³A²/(2g), rolled off for shorter waves.
- **Flooding** runs through an opening by Torricelli, and the water inside weighs low in the hull.
- **Validated** against Archimedes' draft, the wall-sided righting arm, a wide box righting and a tall one capsizing, hydrostatics, a dropped ball's books, and a holed hull sinking.
- **The renderer** draws the sea per pixel where the view ray meets the datum's ellipsoid, writing its depth so land above sea level hides it. It is lit through the air and reflects the sky (Fresnel) and the sun's glint.

## Future improvements

- **Supernodal Cholesky.** The sparse Cholesky (`creeer_math::sparse`) is up-looking: row by row, through indices. Its throughput is about 1 GFlop/s; one factorisation of a 2,500-junction meshed grid takes about 0.45 ms. A supernodal (or multifrontal) factorisation would do the same work in dense blocks at several times the speed.
  - It would let large meshed pipe networks run at 60 Hz instead of dropping to 20 or 10 Hz.
  - It would also speed up the thermal and electrical networks, whose meshed islands use the same factorisation.
  - Deferred: the per-network rate meets the budget now.
- **Waves:** sea state, and the radiation damping of rolling as well as heaving.
- **Water inside hulls:** a free surface that sloshes.
- **Moving water and air:** currents, and a wind field.
- **Two-phase pipe flow:** G4/G5, as the plan has it.

## Consequences

- The structure solver takes velocity-dependent loads and added mass from any domain through two general hooks. Neither is saved: each is set before every step by whatever exerts it.
- A pipe network's rate is a property of its structure, and couplings that need every tick take it from the machine's curve.
