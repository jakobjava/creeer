# F6 plan: simulation domains

Status: **done** (2026-10-17), with Lua 5.4 instead of Luau: F6.1 to F6.8 (ADR-0009 to ADR-0016; validation report `docs/physics/f6-validation.md`).

F6 builds the engines behind everything a player will make: what things are made of and how they react (materials & chemistry), heat, electricity, shafts and gears, fluids and air, and logic. Each one must be **realistic** (validated against analytic or reference results, conserving what nature conserves) and **fast** (each has a budget inside the 10 ms tick of plan.md §5). Each gets a sandbox scene, like F2–F5.

## Principles for every domain

1. **One source of truth per physical quantity.** Thermodynamic data comes from one place: each species' Gibbs energy G(T). Equilibrium constants, vapour pressures, reaction heats, melting points and electrolysis voltages are all derived from it, never typed in separately. Then they cannot contradict each other: a boiler, a still and a furnace all agree on when water boils.
2. **Conservation is tested, not hoped for.** Every domain keeps a ledger of energy (and of mass and charge where relevant). Every coupling between domains records what it moves. Tests check that over a run, in = out + stored + dissipated, to rounding. This is the cross-domain version of the energy guard that caught so many physics bugs.
3. **Implicit where it is stiff.** Small time constants (a capacitor, a stiff shaft, a thin wall) must not force small steps. Everything stiff is integrated implicitly (backward Euler/BDF2/trapezoidal, as in SPICE), with Newton iterations for the nonlinear parts.
4. **Networks share one solver kit.** Heat, current, torque and flow on a network are the same mathematics: an effort (temperature, voltage, speed, pressure) at nodes and a flow (heat, current, torque, mass flow) along edges. One deterministic sparse toolkit serves all of them (§F6.1).
5. **Islands, sleep, multi-rate.**
   - **Islands:** each network splits into connected components, solved in parallel.
   - **Sleep:** components at steady state sleep, like resting bodies.
   - **Multi-rate:** electrical, mechanical and fluid networks step at 60 Hz (plan.md §1.5), thermal at 20 Hz, chemistry at 10 Hz. Exchanges between rates are energy-conserving: accumulated, not sampled.
6. **Deterministic and cross-platform.**
   - Fixed operation order and `libm` functions, so golden hashes hold on all three OSes.
   - No SIMD dispatch in the numerics.
   - Saves hold every domain's state.
7. **Data, with sources.** Species, materials, reactions and atmospheres live in `data/` RON files, each value with its source. The data integrity tests of plan.md §6 run over all of it: elements and charge balance in every reaction, ΔG signs of known reactions, and every unit string parses.

## Sub-milestones

The order follows dependencies: materials first (everything needs their properties), then the network domains from simplest to hardest, chemistry before fluids (gases need species), logic last (it drives the others), then an integration milestone.

### F6.1 Foundations: materials data and the network kit

**Done** (2026-10-17, ADR-0009): `creeer_math::sparse` (deterministic assembly, LU with pivot reuse, parallel Cholesky, complex scalars), the engineering materials database, `creeer_network` (islands, ledgers, exchange, schedule, sleep). PCG for meshed islands deferred: no domain meshes yet.

- **Engineering materials** (`creeer_materials`, `data/materials/`), each property temperature-dependent where it matters:
  - mechanical: density, E(T), G, yield(T), UTS, elongation;
  - thermal: thermal conductivity k(T), specific heat cp(T), expansion α;
  - electrical: resistivity ρ(T);
  - surface: emissivity and solar absorptivity;
  - melting range.
  
  Covers steels, aluminium alloys, copper, titanium, wood, concrete, glass, rock (from the existing geology) and water ice. `creeer_physics::Material` is then built from this data instead of constants.
- **Sparse solver kit** (`creeer_math::sparse` or a small new crate):
  - **scalar sparse LU** with fill-reducing ordering (AMD), pivoting that stays deterministic, and refactorisation that reuses the symbolic analysis (KLU-style, for circuits and fluids);
  - a **scalar sparse Cholesky** for symmetric networks (heat conduction);
  - a **complex** variant for AC.
  
  All bit-identical across platforms, like the structure solver.
- **Network framework:**
  - nodes, edges and element "stamps" (each element writes its contribution to the system);
  - island detection, sleep and wake;
  - an energy ledger per network;
  - a multi-rate scheduler in `creeer_sim` (which domain steps when, how exchanges accumulate).
- **Parts with ports** (sandbox level; the real building system is G2): a part sits on a body node and has ports (thermal node, electrical terminals, shaft, pipe ports, signal pins). Its reaction forces and torques go back to that node.
- **Saves and networking:** a save section per domain; domain state replicated at low rate, values for UI and overlays sent on demand.
- **Tests:** solver accuracy against dense solves and bit-identity across thread counts, data integrity tests.
- **Budget:** LU of a 10,000-node circuit-like matrix refactorised in ≤ 1 ms on min-spec (projected).

### F6.2 Thermal network (`creeer_thermal`)

**Done** (2026-10-17, ADR-0010): enthalpy as the state (the ledger closes to rounding), backward Euler (not BDF2: first order is within the correlations' accuracy at 20 Hz, and monotone) with Newton per island, lagged convection and conduction coefficients, exact radiation and melting; air and water; sunlight, sky, view factors, ISO 834 fires, joints; members of structures heated, lengthening and weakening (save version 3); scenes "cooling blocks", "radiator in orbit", "frame in a fire"; 21,000 nodes in 0.64 ms (1.28 ms projected ≤ 1.5). Found on the way and fixed: the joint solve of bodies on structures (ADR-0005, amended in F6).

- **Nodes:** heat capacities m·cp(T), with an enthalpy method across melting, so latent heat is honoured.
- **Edges:**
  - **Conduction** G = kA/L, with contact conductance at joints.
  - **Convection** h·A to the surrounding air or water. h comes from standard Nusselt correlations: Churchill–Chu for natural convection, flat plate and cylinder correlations for forced flow, using the relative wind from the physics.
  - **Radiation:**
    - εσ(T⁴ − T⁴_sky) to the environment;
    - sunlight absorbed with the real sun direction and atmospheric attenuation;
    - between facing parts, with simple view factors.
  
  Radiators work in space.
- Implicit (backward Euler/BDF2), Newton on the T⁴ terms; linear islands factorise once.
- **Heat sources from the other domains:**
  - plastic work (the structure solver already computes it);
  - friction (contacts, clutches, bearings);
  - Joule heating;
  - reaction enthalpy;
  - aerodynamic heating (re-entry, Sutton–Graves stagnation heating).
- **Thermal expansion** feeds back to structures as a temperature strain in members (a heated bridge lengthens, a constrained one builds stress). Strength falls with temperature, so a fire can weaken a frame.
- **Validation:**
  - lumped cooling (exponential, time constant mc/hA);
  - steady 1D conduction (linear profile);
  - a semi-infinite slab against the erfc solution;
  - radiative equilibrium of a plate in sunlight;
  - melting against the Neumann/Stefan solution;
  - energy ledger closes.
- **Sandbox:** hot blocks cooling in air and in vacuum, a radiator panel in orbit turning in and out of sunlight, a frame heated until it sags.
- **Budget:** 20,000 nodes at 20 Hz ≤ 0.5 ms per 60 Hz tick (min-spec).

### F6.3 Electrical network (`creeer_electric`)

**Done** (2026-10-17, ADR-0011): MNA on the LU (DC/transient by BDF2 with backward-Euler restarts, not the trapezoidal rule: it rings undamped at a 60 Hz tick), AC grids as per-phase phasors on the complex LU; every device listed below with its validation; Tellegen-exact energy booking; Joule heat coupled to the thermal network (tested); circuit bench window with an oscilloscope (capacitor, motor on a battery, solar through a day, transformer and induction motor). 10k-node DC 0.47 ms (0.93–0.97 projected ≤ 1), 2.4k-bus AC 0.12 ms.

- **Modified Nodal Analysis** on the sparse LU, two modes:
  - **Transient:** companion models (trapezoidal, backward Euler right after switching to avoid ringing), Newton for nonlinear devices, events (switches, fuses) refactor.
  - **AC:** complex phasors at the grid frequency, quasi-static per tick, for power grids.
- **Components:**
  - wires (R from ρ(T)·L/A, so they heat and their resistance rises);
  - resistors, capacitors, inductors, sources, switches;
  - fuses and breakers (I²t);
  - diodes and LEDs (Shockley with series resistance);
  - batteries: open-circuit voltage against state of charge, internal resistance and an RC pair, capacity, Peukert behaviour, heat;
  - solar cells (single-diode model with real irradiance and temperature);
  - transformers;
  - DC motors and generators (back-EMF k·ω, torque k·I);
  - induction motors (equivalent circuit, slip-dependent torque).
- **Couplings:**
  - Joule heat into the thermal network.
  - Motors and generators couple to shafts (F6.4).
  - Electrolysis cells come in F6.5: Faraday's law, decomposition voltage from ΔG.
- **What a 60 Hz tick cannot resolve** (PWM switching, AC waveforms) is modelled by averages and phasors. That choice is written down, not hidden.
- **Validation:**
  - voltage divider, RC time constant;
  - RLC resonance frequency and Q;
  - diode I–V curve;
  - battery discharge against a datasheet curve;
  - solar cell against a datasheet I–V;
  - DC motor speed–torque line;
  - transformer ratio and losses;
  - power balance (sources = loads + losses).
- **Sandbox:** a circuit bench with an oscilloscope (egui plots): charge a capacitor, run a motor from a battery, a solar panel through the day, a small AC grid with a transformer.
- **Budget:** 10,000-node DC/transient network ≤ 1 ms per tick, 2,000-bus AC grid ≤ 1 ms (min-spec).

### F6.4 Mechanical drivetrains (`creeer_mech`)

**Done** (2026-10-17, ADR-0012): parts, shafts, gears with backlash, planetary sets and differentials, belts (capstan limit), clutches, brakes and bearings; backward Euler with exact constraints (Schur complement, redundant ones dropped), friction by an active set that settles (stick, slip, saturated), losses in the direction power flows; energy booked at midpoint speeds (the ledger closes to rounding); housing reactions with the gyroscopic term, mounts on bodies keeping angular momentum exact; DC machines solved jointly with their drivetrains (one system). Validations as listed below and more; 1,000 drivetrains in 0.17–0.21 ms projected (≤ 0.3). Sandbox: drivetrain bench window, flywheel station scene. Found and fixed in the structure solver (ADR-0004 amended): torques' work and changing loads in the energy guard, momentum under torque first order in h. Not modelled: mesh and belt compliance, belt creep, Stribeck friction, gearboxes' no-load losses.

- A 1D rotational network:
  - **Inertias and shafts:** flywheels and rotors, compliant shafts (torsional stiffness from G·J/L of the material).
  - **Gears:** rigid meshes reduce to one degree of freedom per gear group, through their ratio.
  - **Belts and chains:** slip limited by the capstan law e^(μθ).
  - **Clutches and brakes:** stick–slip Coulomb friction, resolved implicitly, not chattering.
  - **Bearings and backlash:** viscous plus Coulomb friction, optional backlash.
- Solved together with the electric motors and generators on it (one Newton system), so a motor stalling under load is consistent on both sides.
- **Newton's third law:** every torque on a shaft reacts on the structure node holding its bearing or motor housing. A vehicle's chassis twists against its engine; a free-floating station spins up the other way when a flywheel spins up (angular momentum conserved, tested).
- **Validation:**
  - two-inertia torsional natural frequency;
  - energy through a gear train (out = in − losses);
  - clutch engagement loss ½·J_eq·Δω²;
  - flywheel spin-down with bearing friction;
  - belt slip limit;
  - angular momentum of a free body with an internal motor.
- **Sandbox:** a motor driving a gearbox, flywheel and brake; a belt drive slipping under overload; a spinning wheel on a free-floating frame.
- **Budget:** 1,000 drivetrains of 10 elements each ≤ 0.3 ms per tick.

### F6.5 Materials and chemistry (`creeer_materials`, `creeer_chem`)

**Done** (2026-10-17, ADR-0013): NASA Glenn's 1,798 species (the CEA program's thermo.inp, NASA-9) imported with two corrections written down; equilibrium by the CEA method with the phase rule, phase swaps and feed starts added; vapour pressures with real-gas vapour (water to IAPWS within 0.4 % to 450 K), Raoult flashes, liquid densities, viscosities and conductivities (Wilke); reversible Arrhenius kinetics, batch and CSTR reactors (BDF2, energy ledger closed), the shrinking core; electrolysis cells in circuits coupled to reactors (Faraday to the mole). Validations as listed below; 500 equilibria at 10 Hz 0.04–0.05 ms a tick projected (≤ 0.5). Sandbox: the Lab window. Not modelled: activity models (slags, brines), cryogenic liquids (G8), distillation-grade organic vapour pressures (G5).

- **Species database:**
  - formula (existing parser), phase, molar mass;
  - thermodynamic polynomials: NASA-7 for gases and condensed phases (NASA Glenn data), and mineral data from Robie & Hemingway (USGS) for the geology's minerals;
  - transport properties: viscosity, conductivity, with mixing rules (Wilke);
  - liquid densities.
- **Mixtures:**
  - composition, T, P, phases;
  - enthalpy, entropy and Gibbs energy;
  - ideal-gas and ideal-solution models (activity models later where needed: slags, brines);
  - vapour–liquid equilibrium (Raoult, from the same G data) with flash calculations.
- **Equilibrium:** Gibbs energy minimisation under element balance (the NASA CEA method), with condensed phases appearing and disappearing.
- **Kinetics:**
  - Arrhenius rates, made reversible through the equilibrium constant (detailed balance, so kinetics can never overshoot equilibrium);
  - shrinking-core models for solids (ore reduction, roasting);
  - a stiff (implicit) integrator per reactor.
- **Reactors:**
  - lumped batch and CSTR, with full mass and energy balances;
  - connected to pipes (F6.6), to heat (F6.2) and to electricity (electrolysis: Faraday's law, decomposition voltage from ΔG, overpotential).
- **Validation against reference results:**
  - adiabatic flame temperature of methane–air (≈ 2226 K) and hydrogen–oxygen;
  - water's vapour pressure curve and boiling point against altitude;
  - CaCO₃ calcination temperature (≈ 1170 K at 1 atm CO₂);
  - the Boudouard equilibrium (CO/CO₂ over carbon);
  - Ellingham crossovers (when carbon reduces iron oxide);
  - Haber ammonia yield against T and P;
  - first- and second-order kinetics against their closed forms;
  - electrolysis of water (mass per coulomb, 1.23 V reversible).
- **Sandbox:** a "lab" window:
  - mix species, set T and P, watch equilibrium and kinetics run, plot composition;
  - an Ellingham diagram drawn from the data;
  - a small furnace reducing hematite with carbon monoxide.
- **Budget:** 500 reactors with equilibrium at 10 Hz ≤ 0.5 ms per tick.

### F6.6 Fluids, air and water (`creeer_fluids`)

**Done** (2026-10-17, ADR-0014): atmospheres from planet data (US Standard 1976 to 5·10⁻⁴); liquid networks by the global gradient method with exact slopes (Hagen–Poiseuille, the Moody chart, Torricelli, a pump on its system curve, energy books to rounding); isothermal gas networks (Weymouth); content and enthalpy carried (plug flow, mixing, a reactor's gas at its temperature); each network at its own deterministic rate; aero surfaces (lifting line, stall, Prandtl–Glauert, Ackeret, Newtonian, skin friction, sphere drag curve, heating) as loads with implicit dashpots in the structure step; oceans to sea level with exact buoyancy, added mass, hull drag, heave wave damping, flooding; the sea drawn per pixel. Budgets met (pipes 0.48 ms at the town grid's 10 Hz and 0.18 ms for 250 houses at 60 Hz; panels 0.19 ms; floating 0.14 ms). Sandbox: the pipe bench window, the air drop and harbour scenes, the planet's coast. Not modelled: waves and roll radiation, sloshing, currents and wind fields, two-phase pipe flow (G4/G5).

- **Pipe networks:**
  - nodes (junctions, tanks) and edges (pipes, pumps, valves, check valves);
  - Darcy–Weisbach with the Colebrook friction factor (laminar to turbulent), plus minor losses;
  - pump curves;
  - liquids solved by the global gradient method (Todini–Pilati, Newton);
  - gases compressible: density from the equation of state, tank and pipe volume as storage;
  - mixtures carry composition and enthalpy, so a pipe delivers what the reactor made, at its temperature.
  
  Single-phase flow in pipes. Boiling and condensing happen in vessels by equilibrium (F6.5); two-phase pipe flow waits for G4/G5.
- **Atmospheres** per planet (data):
  - composition and surface pressure and temperature;
  - a temperature profile (lapse rate, then isothermal layers);
  - hydrostatic pressure integrated with the real g(r).
  
  This gives density, pressure, temperature, viscosity and speed of sound at any altitude. Drag uses them instead of today's fixed coefficients.
- **Aerodynamics:**
  - **Panels** on bodies (plates, fins, wings), as forces in the implicit structure step.
  - **Plates:** normal force for any angle of attack.
  - **Wings:** lift slope ≈ 2π corrected for aspect ratio, with stall.
  - **Speed effects:** skin friction, compressibility (Prandtl–Glauert below Mach 1, Newtonian at hypersonic speed).
  - **Heating:** aerodynamic heating to the thermal network.
- **Oceans and buoyancy:**
  - planets with water get seas filling the basins to sea level (the equipotential they already have);
  - buoyancy from the exact submerged volume of each shape;
  - hydrostatic pressure, water drag;
  - **added mass**, which is needed for realism and for the stability of light bodies;
  - a simple water surface in the renderer.
- **Validation:**
  - Hagen–Poiseuille, the Moody chart;
  - Torricelli tank draining;
  - a pump against its system curve;
  - isothermal gas pipeline (Weymouth);
  - the standard atmosphere (ISA) on an Earth-like planet;
  - terminal velocity of a sphere;
  - flat-plate lift at small angles;
  - floating draft = ρ_body/ρ_water · h;
  - metacentric stability (a wide box rights itself, a tall one capsizes);
  - mass and energy ledgers.
- **Sandbox:**
  - tanks, pumps and valves in a pipe network;
  - a crate with a parachute panel, a glider;
  - boats floating, rolling and sinking when flooded.
- **Budget:**
  - 5,000 pipe segments ≤ 1 ms per tick. Each network picks its own rate, deterministically, from its estimated cost: small networks every tick (60 Hz), only large meshed ones at 20 Hz, or 10 Hz at most (ADR-0014);
  - 1,000 aero panels ≤ 0.2 ms;
  - buoyancy of 200 bodies ≤ 0.2 ms.

### F6.7 Logic (`creeer_logic`)

**Done** (2026-10-17, ADR-0015): signals, gates, latches, timers, comparators, PID blocks, sensors and actuators on any domain (`creeer_sim::controls`), evaluated in topological order with loops through a one-tick register and the network's unjoined parts side by side; standard Lua 5.4 through `mlua`, built from our copy of `lua-src` (fixed string hash seed, our own pow, no fused multiply-adds), sandboxed, with instruction and memory budgets no `pcall` can catch and `memory` kept by saves. Validations as listed below (a Lua PI script holds a tank's level within a centimetre and a motor's speed within 0.5 %; a run hashes the same in debug and release). 10,000 gates and 100 scripts in 0.07 ms a tick (0.14 ms projected, ≤ 0.5). Sandbox: the control panel window. Not done: tables keyed by tables in `pairs` (an error), saving scripts' functions.

- **Signals:** booleans and numbers on wires.
- **Gates and blocks:** gates, latches, timers, comparators, PID blocks.
- **Sensors:** they read any domain (temperature, voltage, pressure, speed, position).
- **Actuators:** they set switches, valves, motor setpoints.
- **Order:** evaluated once per tick in topological order; feedback loops pass through a one-tick register, deterministically.
- **Lua 5.4 microcontrollers** (standard PUC-Rio Lua, the language Factorio scripts in), through `mlua`, built from the Lua sources by our own build script so we control its compile flags:
  - a sandbox with no I/O, `os` or `load` of bytecode;
  - an instruction budget per tick (count hook) and a memory limit (allocator limit);
  - deterministic across OSes and runs:
    - the string hash seed is pinned (stock Lua randomises it per run, which would change `pairs` order);
    - `pairs` iterates in a defined order;
    - our `libm` maths replaces the platform's.
  - on loading a save, a script restarts with its saved persistent variables (a Lua VM's internal state cannot be saved portably; written down).
- **Validation:**
  - truth tables, latch and timer timing;
  - a PID script holding a tank level and a motor's speed;
  - a runaway script stopped by its budget;
  - a memory hog stopped by its limit;
  - determinism hash across platforms.
- **Sandbox:** a control panel driving the F6.3–F6.6 scenes.
- **Budget:** 10,000 gates and 100 scripts at 1,000 instructions each ≤ 0.5 ms per tick.

### F6.8 Integration: one machine through every domain

**Done** (2026-10-17, ADR-0016): Euler turbines in pipe networks, their torque on a drivetrain every tick at the step's midpoint speed (the water's work the shaft's, joule for joule, however seldom the network steps); slow networks' steps run beside the ticks, from one due tick to the next (deterministic, bit for bit the step itself); the couplings split in two so several share a circuit or drivetrain. The powerhouse (tank, turbine, generator, rectifier, twelve electrolysis cells, reactor, heat to the air, a Lua PI on the valve, a free platform): over ten minutes the shaft holds 150 rad/s within 0.1 %, the water's 1.96 MJ are accounted for to 10⁻⁹, the water and the reactor's mass to 10⁻¹², hydrogen by Faraday to 10⁻³, angular momentum to 10⁻⁷, with the pipes every tick and at 10 Hz. Sandbox: the Powerhouse window. The reference load is measured (`bench-spikes --only reference`): the F6 domains side by side 0.88–0.94 ms (1.75–1.88 ms projected, their share ≈ 5 ms); everything with 20,000 active beams and 8 players 5.1–5.3 ms, 10.3–10.5 ms projected against plan.md's 10 ms: **not met**, three quarters of the tick's work being the beams' solver (a cheaper structure solve is the first item of `docs/future-improvements.md`, Solvers). Found on the way: a world's step moved every body out and back (200 floating bodies 0.53 → 0.26 ms); the DC circuit's right-hand sides side by side (0.50 → 0.43 ms, 0.86 ms projected ≤ 1).

- **A scene coupling everything:**
  1. a water tank drains through a turbine (fluids → shaft);
  2. the turbine turns a generator (shaft → electricity);
  3. the generator feeds an electrolysis cell (electricity → chemistry: hydrogen and oxygen into tanks);
  4. everything heats up and cools (thermal);
  5. a Lua controller regulates it (logic);
  6. all of it sits on a structure that feels every reaction torque.
- **Fast couplings every tick.** A turbine's or pump's torque on a drivetrain is taken every tick from the machine's curve at the shaft's speed, even where its pipe network solves more slowly; the energy books still close (ADR-0016).
- **Slow networks spread out.** A network stepping at 20 or 10 Hz has its step run across its interval (or beside the tick), so that it is not one tick's spike (ADR-0014, ADR-0016).
- **The test:** the total energy ledger over 10 minutes closes (potential energy of the water in = hydrogen's chemical energy + heat + losses, to rounding). The same for mass and angular momentum.
- **The reference load of plan.md §5:**
  - 1,000 machines across all domains, 20,000 beams, 8 players;
  - measured against the 10 ms tick on min-spec, with every F6 budget in `cargo xtask perf`.
- **Budgets with margin.** The 10k-node DC circuit measured 0.93–1.00 ms projected against its 1 ms budget (F6.3: 0.93–0.97): give it margin before the perf baseline is saved again.
- **ADRs:**
  - ADR-0009: network kit;
  - ADR-0010: thermal networks;
  - ADR-0011: electrical networks;
  - ADR-0012: mechanical drivetrains;
  - ADR-0013: materials and chemistry;
  - ADR-0014: fluids, atmospheres, aero and buoyancy;
  - ADR-0015: logic and the Lua sandbox;
  - ADR-0016: integration and multi-rate coupling.
  
  (One ADR a sub-phase, as they were written; the plan first grouped them differently.)
  
  Validation reports go in `docs/physics/`.

## Budgets at a glance (min-spec, per 60 Hz tick)

| Domain | Reference load | Budget |
|---|---|---|
| Thermal | 20,000 nodes at 20 Hz | 0.5 ms |
| Electrical | 10,000 nodes transient; 2,000-bus AC | 1.0 ms each |
| Mechanical | 1,000 drivetrains | 0.3 ms |
| Chemistry | 500 reactors at 10 Hz | 0.5 ms |
| Fluids | 5,000 pipe segments (each network at its own rate: 60 Hz when it fits its share, large meshed ones 20 or 10 Hz); 1,000 aero panels; 200 floating bodies | 1.0 + 0.2 + 0.2 ms |
| Logic | 10,000 gates; 100 scripts | 0.5 ms |
| **All domains** | | **≈ 5 ms**, leaving room for structures and contacts in the 10 ms tick |

## Decisions (confirmed 2026-10-17)

1. **Lua 5.4** (standard Lua, not Roblox's Luau) for microcontrollers, built from source for determinism.
2. **AC as phasors** (quasi-static per tick), not full waveforms. Waveforms at 60 Hz ticks would need 100× smaller steps. Phasors now; an electromagnetic-transient mode only if a gameplay need appears.
3. **Oceans in F6.6**, with a simple water surface: buoyancy needs water, and the planets' basins are dry today.
4. **Two-phase pipe flow** (steam lines) in G4/G5, not F6. Boiling and condensing in vessels is in F6.
5. **Order of the sub-milestones** as above, with a demo and review after each one (like the F2–F5 sandboxes).

## Deferred

- **Supernodal Cholesky** in `creeer_math::sparse`. The up-looking factorisation runs at about 1 GFlop/s through its indices. A supernodal one would do the same work in dense blocks several times faster. It would let large meshed pipe networks run at 60 Hz instead of 20 or 10 Hz, and it would speed up the thermal and electrical networks' meshed islands (ADR-0014).

## Risks

- **Scope.** F6 is the largest milestone yet. Each sub-milestone stands alone (crate, validation, sandbox, budget), so it can be reviewed and paused at any boundary.
- **Stiff coupling between domains.** Motor and shaft are solved together; other couplings exchange energy-conserving accumulations. The ledger tests catch any coupling that makes energy.
- **Data volume and licences.** NASA Glenn coefficients and USGS data are public. Values are physical facts; every file cites its source.
- **Determinism of Lua.** Its hash seed is pinned, `pairs` is ordered and its maths replaced; tests compare script results across the three OSes.
