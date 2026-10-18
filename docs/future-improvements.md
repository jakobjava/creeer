# Future improvements

One list of every known limit and deferred improvement in Creëer. It is gathered from:

- the ADRs' limits, "Not modelled" and "Measured and rejected" parts;
- the plans' deferred items;
- code comments that name a later phase or something not modelled;
- the project memory.

Each item gives:

- **what** it is;
- **why** it was deferred;
- what it would **improve**: realism (R), speed (S) or gameplay (G);
- a rough **size**: small, medium or large;
- **where** it is recorded.

Limits that later work already lifted are not listed. Examples: members that could not buckle (F5), lighting seams between terrain levels (F5), dry ocean basins (F6.6).

**Kept up to date:** whenever something is deferred, it is added here as well as in its ADR.

**Not a place for slowness.** Performance and maximum realism together are paramount (plan.md): a scene or system that is too slow is fixed now, not listed here. Only speed-ups beyond the budgets (headroom for larger worlds) belong here, and what the user chose to defer after the measurements (the sandbox scenes' 2–3 ms target, 2026-10-18).

## Physics

### Structures and materials

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Necking**: past its ultimate strength a member flows on evenly instead of failing at its narrowest point | Softening makes the step's minimisation non-convex | R | Large | ADR-0005 (F5 hardening); `structure/plastic.rs` |
| **Kinematic hardening** (Bauschinger effect under reversed loading) | Isotropic hardening was enough for the crash and pull tests | R | Medium | ADR-0005 |
| **Local buckling of thin walls** (a tube's wall folding, plates between stiffeners) | Needs shell elements; members are beams | R | Large | ADR-0005 (F5 buckling) |
| **Residual stresses** of rolling and welding (columns carry 16–44 % more than EC3's curves at λ̄ = 1) | Not part of the beam model; would need an initial-stress state per member | R | Medium | ADR-0005; `buckling_tests.rs` |
| **Lateral-torsional buckling** (a beam bent about its strong axis twisting sideways) | Only compression splits members | R | Medium | ADR-0005 |
| **Members colliding with each other** (a folded column passes through itself) | Self-contact of a structure is not detected | R | Large | ADR-0005 |
| **Material damping model**: steel's damping over-damps a joint's kilohertz rattle (crash 5 % short at 5 m/s with it) | The crash test runs undamped; a frequency-dependent model is needed | R | Medium | ADR-0005 |
| **Impacts on structures cost substeps** (a 10 t block landing on a bridge: about 33 ms a tick while it strikes, 34 substeps of 0.5 ms at about 1 ms each, a third of that fixed costs); ten cars crashing in pairs at 15 m/s (F7.5): 91 ms the tick they strike (134 before F7.6 warm-started members' splits), 34 substeps of 50–75 Newton iterations in the struck groups | Error-controlled substeps were as accurate but no faster (ADR-0017, A1); the substeps are needed while the block strikes. **Subcycling the struck region** (its joints in short steps, the rest in the tick's, energy exact where they meet) is the method left, weeks of work: deferred by the user (2026-10-18) | S | Large | ADR-0005, ADR-0017 |

### Gravity and orbits

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Oblateness (J2) and mass concentrations**: point-mass gravity only (no sun-synchronous orbits, no lumpy asteroids) | The datum is level for exactly the point-mass field; J2 needs the datum redone with it | R, G | Medium | ADR-0006, ADR-0007; `gravity.rs` |
| **Restricted problem**: minor bodies and ships do not pull major bodies or each other; bodies in the bubble do not attract each other | Cost; the effect is negligible at game scales | R | Medium | ADR-0007 |
| **Spins never change** (pushes do not torque bodies; no tidal evolution) | Not needed before machines push moons | R, G | Medium | ADR-0007 |
| **Major-body collisions are detected, not resolved** (warp stops) | Rare; needs a merging or breaking model | G | Large | ADR-0007 |
| **Rails turn at constant angular velocity** (a tumbling vessel's precession is not followed) | Rails are kinematic | R | Small | ADR-0007 |
| **One recomputation per push** on a major body (a machine pushing continuously costs one per push) | Correct; only a cost | S | Medium | ADR-0007 |
| **Maneuver nodes** on the map | Gameplay (G8) | G | Medium | ADR-0007 |
| **Eclipses and other bodies' terrain**: only the nearest body is streamed, others are spheres and cast no shadows | Rendering cost; not needed yet | R | Medium | ADR-0007 |

### Heat, electricity, drivetrains

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Radiation between many surfaces**: pairwise only, no radiosity network | Pairwise was enough for the sandbox scenes | R | Medium | ADR-0010 |
| **Temperature gradients within a part**: a member is one node | Splitting parts into nodes is the builder's job for now | R | Medium | ADR-0010 |
| **Two-phase heat transfer** (boiling, condensing on surfaces) | Waits for the integration of F6.5's equilibria into heat transfer | R | Medium | ADR-0010 |
| **Mixed AC/DC** (rectifiers, inverters): a grid and a circuit are separate networks | The integration needed none (its generator feeds the stack through a diode); an average converter model is a design of its own | R, G | Medium | ADR-0011, ADR-0016 |
| **Unbalanced three-phase grids** | The per-phase equivalent assumes balance | R | Medium | ADR-0011 |
| **Transistors** | Plan: components come in order, transistors later | G | Medium | plan.md |
| **Gear-mesh and belt compliance**: rigid while they grip; **belt creep** below the slip limit | Rigid constraints keep the drivetrain solve small | R | Medium | ADR-0012 |
| **Stribeck friction** (static = kinetic) | Coulomb plus viscous was enough | R | Small | ADR-0012 |
| **Gearboxes' no-load churning losses** | Only the mesh's loss under load is booked | R | Small | ADR-0012 |
| **Two machines on one drivetrain coupled within a step** | Each machine–drivetrain pair is solved as one; pairs are not | R | Medium | ADR-0012 |
| **A drivetrain feeling its body's linear acceleration** (only its turning enters) | The mount passes the spin only | R | Small | ADR-0012 |
| **A shaft's twist damping as heat**: its loss and backward Euler's go either way within a cycle, so they stay the drivetrain's (0.1–0.2 J in the powerhouse's ten minutes) | Not heat any thermal node could take as it comes | R | Small | ADR-0016; `creeer_mech::book` |

## Contacts

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Sharp corners on flat faces are soft**: a face pressed near a corner follows its whole face's punch law, sinking 0.6–1.1 mm where Hertz gives about 30 µm | A local punch law on planes turned a sliding tall box unstable in yaw | R | Medium | ADR-0005 (F5 contact law) |
| **Settling is overdamped**: a box dropped on bumps rocks to rest over 2 s instead of a fraction of one | Contacts are critically damped at their resolvable stiffness | R | Medium | ADR-0005 |
| **Faces over curved ground cost points**: 50 crates landing on terrain take about twice as long a step as in F4 | Correctness first; each face measures 5–40 ground points | S | Medium | ADR-0005 |
| **Contacts resolve only what the step can**: beyond that they sink a reach per e-fold of their load (micrometres to a fraction of a millimetre) | The step's stiffness limit | R | Large | ADR-0005 |
| **Edge contacts and face patches disagree**: for the same overlap an edge pair's point and a steeply inclined face's patch give very different forces (800 N against 1 N), so a rock can rattle between them; nine rocks of the sandbox pile stay awake at about 1 ms a step | A relative edge bias broke the stacks' energy (+47 J), and keeping the last step's choice held rocks on edges where faces lay flush; a consistent contact for both needs the patch built from the true overlap | R, S | Large | ADR-0005 (F6, Newton) |
| **Drilling friction keeps no anchor**: under a torque below its limit a ball creeps at up to 10⁻³ rad/s, and only rigid bodies have it (a structure's nodes take no turning stiffness from contacts) | A rate-smoothed torque stops spins without saved state; an anchor would need saving like sliding friction's | R | Small | ADR-0005 (F6, Newton) |
| **Contacts found anew differ from step to step** (a face fitted again to curved ground): only the energy surplus is taken back, from the motion | Consistent geometry across steps is a larger change to detection | R | Medium | ADR-0005 (F6, Newton) |
| **Friction on stale normal forces**: the lag's cap of three refreshes leaves 19 % of the rock pile's steps (3 % of the stacks') ending with some contact's friction on a normal force more than 5 % off its own | Refreshing until consistent costs the pile 27 % more iterations; following λ every iterate (ADR-0017, C) fixes most of it at no cost in iterations but worse worst steps. To be fixed with the speed it costs | R | Medium | ADR-0017 (Measured) |

## Solvers and performance

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Supernodal (or multifrontal) Cholesky**: the up-looking one runs at about 1 GFlop/s (0.45 ms for a 2,500-junction grid) | The per-network rate meets the budget now; measured without SIMD kernels, supernodal storage was slower for the structures (ADR-0004) | S: large meshed pipe networks at 60 Hz instead of 20/10 Hz; thermal and electrical meshed islands faster | Large | ADR-0014, ADR-0009, ADR-0004; `docs/f6-plan.md` (Deferred) |
| **A cheaper structure solve** (SIMD kernels, runtime AVX2 dispatch, fewer passes over the elements): the 5k-beam bridge holds its 4 ms budget only on a cool machine; **the reference tick (plan.md §5) needs it**: 20,000 active beams take 22.6 ms of one thread's work a tick (1.1 µs a beam), three quarters of the tick, and the whole tick projects to 10.3–10.5 ms against 10 ms | The user accepted the bridge's budget as met; the F6 domains take only 1.75–1.88 ms of the tick projected | S | Large | ADR-0004 (Addendum 3), ADR-0016; memory |
| **Substeps for a body's turn per group**, not for the whole world: in the rock pile's 170 substepping ticks of 15 s the largest group turned slowly in 67, and per-group substeps would save 41 % of their body-steps | The worst ticks are those whose largest group turns fast itself (measured 2026-10-18): totals only, not the worst tick; the 0.05 rad limit itself is needed (0.1 rad let rocks sink 13 mm into each other, `overlap_tests.rs`) | S | Medium | this list |
| **Islands on their own thread groups**: four bridges stepped on four pools of four threads took 3.3 ms against 4.2 ms on the shared pool | A world's islands share one pool with every domain; a scheduler that keeps an island on a few threads is a design of its own | S | Medium | ADR-0016 |
| **Iterative solvers (PCG) for large meshed islands** (thermal, electrical) | No domain meshes yet; pipes now precondition with their own factor | S | Medium | ADR-0009 |
| **PGO**, **GPU compute** for soft bodies | Later optimisation options in the plan | S | Medium | plan.md |
| **A stiffer contact model** with all contacts in every structure's matrix before every step | Future work after the F5 contact law | R, S | Large | ADR-0005 |
| **Newton's method for groups with structures**: only groups of rigid bodies alone take their contacts' stiffness at each iteration's state; rigid bodies on structures keep the step's | The structures' matrices are factorised once a step; factorising them at every iteration was not tried | S | Medium | ADR-0005 (F6, Newton) |
| **Lagged friction's outer loop**: half the rock pile's iterations come after a friction refresh; the gap's curvature terms (B1) save only 7 % | Coupled Newton for friction (ADR-0017, C) measured on the same inputs: iterations −6 % on the pile, +4 % on the stacks, worse worst steps; not adopted. The closing contacts' cuts (a third of the pile's iterations) are the next lever | S | Medium | ADR-0017 |
| **The sandbox scenes' 2–3 ms target**: at the end of `lag-fix` (2026-10-18) the worst ticks are the bridge's 32.8 ms (p99 4.9), the rock pile's 18.2 (14.2), the crash's 19.0 (3.0), the stacks' 6.3 (1.8); gated in CI (work) and `cargo xtask perf` (time) | The levers within the design are measured and taken or rejected (ADR-0017): parallel contact evaluation within a group loses on this machine (rayon's wake-ups, cores shared by two threads). What is left is a different method: subcycling struck regions (above), and a large pile's contact solve split into pieces large enough for the cores, coupled by an outer iteration. Weeks of work: deferred by the user | S | Large | ADR-0017 |
| **The line search at planet scale**: 500 km from the origin the objective rounds at about 10⁻⁷ J, and steps stop short where a decrease that small cannot be seen | Within tolerance where it stops; an objective summed as differences from the step's start would see it | R | Medium | this list (F6) |
| **Structures' solves are bound by memory when many step at once**: eight 5,000-beam bridges, one per core, each step 8.2 ms against 5.2 alone (F7.6; 12.8 before it): the factor's 12 MB is read four times a step, and the four reference bridges with the domains' data fill the dev machine's 96 MB L3 (a min-spec CPU has a third of it) | F7.6 took what is exact (compact indices, flows built only when needed, members' hot fields first: ADR-0021); an f32 factor halves the traffic but changes trajectories (rejected again, ADR-0004); less fill needs another ordering (nested dissection already, ADR-0004) | S | Medium | ADR-0021 |
| **Large networks' first steps**: a 20,000-node thermal hall's first step takes 20.7 ms (its 11,000-node island's ordering and analysis 8 ms, a thousand small islands' first factorisations), 5,000 pipes' cost estimate 9 ms (a symbolic analysis on the tick thread, to pick their rate); once when built or loaded, and an edit of an island's topology pays its analysis again | Building a network's solvers beside the ticks (as the pipes' steps run) and swapping them in a fixed tick later; F7.6 built the islands in parallel and made the ordering linear at hubs | S | Medium | ADR-0021 |
| **One profile clock per process**: a hosting client's profiler shows the server's ticks (its loop holds the clock), the client's own threads' spans inside them | Spans are not tagged with the clock they belong to; a second clock per thread pool would be needed | S | Small | ADR-0021 |
| **Budgets close on a heat-soaked machine**: late in a long `cargo xtask perf` run the 10k-node DC circuit projects to 0.99 ms (0.86 cool), the 20k-node thermal step to 1.51 ms (budget 1.5; 1.28 cool), the 1,000 aero panels to 0.199 ms (0.2) | Each meets its budget measured on its own, as every budget is | S | Small | ADR-0011 (F6.8), ADR-0010, ADR-0014, ADR-0016 |
| **Settling stacks cost 3–6 % more** since rigid bodies' contacts are relinearised each iteration (piles 4–8 times less) | The pile's gain; the stacks' matrices barely change near convergence and are not made again there | S | Small | ADR-0005 (F6, Newton) |

## Fluids

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Waves**: sea state, and the radiation damping of rolling (only heave's waves are modelled) | A later phase; roll is lightly damped (drag at the corners only) | R, G | Large | ADR-0014; `sea/mod.rs` |
| **Water inside hulls sloshing** (a free surface): flood water sits low but does not move | A later phase | R | Medium | ADR-0014; `sea/flood.rs` |
| **Water over the deck**: flooding only through openings, not over a submerged gunwale | Simple flooding model first | R, G | Small | `sea/flood.rs` |
| **Currents and a wind field** (uniform wind only; the sea is still) | A later phase | R, G | Medium | ADR-0014; `aero.rs` |
| **Added mass as a tensor, and rotational added inertia** (taken isotropic, half the water displaced, translation only) | A sphere's value for every shape first | R | Medium | ADR-0014; `sea/mod.rs` |
| **Hull drag** on the wetted area the flow sees, with one coefficient | Crude by design until hull forms matter | R | Medium | ADR-0014 |
| **Aero interference**: no downwash on tailplanes, no panel's pitching moment about its own centre, no wake between bodies | Panels are independent | R | Medium | ADR-0014 |
| **The transonic gap** (Mach 0.7–1.3) bridged linearly | No simple theory spans it | R | Medium | ADR-0014; `aero/coefficients.rs` |
| **Water hammer and cavitation** (liquid networks are incompressible and quasi-steady) | Not needed by the reference scenes | R, G | Large | ADR-0014 |
| **Compressors in gas networks**; gas networks' hydraulics take one fixed gas (a stream's own composition is passive in them) | Isothermal pipelines first | R, G | Medium | ADR-0014; `carry_tests.rs` |
| **Heat exchange of pipes with their surroundings**, and friction heat into the fluid (content is carried adiabatically; a valve's and a turbine's losses stay the water's, unrouted to any thermal node) | Not needed by the integration's books, which count them as the water's friction | R | Medium | ADR-0014, ADR-0016 |
| **Swirl carried out of a turbine** into its tailrace: the casing takes the runner's reaction, as if the water left without swirl | The water's angular momentum is not a state of a pipe network | R | Small | ADR-0016 |
| **Two-phase pipe flow** (steam lines) | Planned for G4/G5 | R, G | Large | `docs/f6-plan.md` (Decisions) |
| **Avatars in water**: players' avatars are spared the air and the sea (they walk on the seabed) | Their players' machines predict them without fluids | G | Medium | ADR-0014; `creeer_sim::surroundings` |
| **The sea in the solar-system view** (only the planet view draws it) | The system view's body frames need its axis | G | Small | `system_view.rs` |

## Chemistry

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Activity models** (slags, brines, aqueous chemistry) and azeotropes: ideal solutions only | Raoult first | R, G | Large | ADR-0013; `vle.rs` |
| **Real-fluid gaps**: cross virial coefficients, the third virial term near critical points, polar virial terms of alcohols and ammonia | Data and model scope | R | Medium | ADR-0013 |
| **Cryogenic liquids** (O₂, H₂, CH₄): NASA has them only as reactants without coefficients | Data gap; the rockets of G8 need them | G | Medium | ADR-0013 |
| **Distillation-grade organic vapour pressures** (NASA's boil 0.8–5.2 K off) | Data gap; G5's distillation needs better | R, G | Medium | ADR-0013; `vle_tests.rs` |
| **Mechanism-level kinetics** (one global or elementary step at a time) | Scope | R | Large | ADR-0013 |
| **Condensed phases coexisting at a transition temperature** in adiabatic problems | Edge case of the minimiser | R | Medium | ADR-0013 |

## Terrain

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Small edits vanish at a distance**: brushes under half a cell leave coarse chunks (a 2 m hole is invisible beyond about 250 m) | Level-of-detail cost | R, G | Medium | ADR-0006 |
| **Ground contacts cost more than plane contacts** (a large crate samples a few hundred surface points near the ground) | Correctness first | S | Medium | ADR-0006 |
| **The sea is drawn as the datum's ellipsoid** (the true equipotential differs by metres) | Simple per-pixel intersection | R | Small | ADR-0014; `sea.wgsl` |

## Rendering

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **A sky-view table** for the atmosphere | Only if the per-pixel ray march becomes a bottleneck | S | Small | ADR-0003 |
| **Temporal upscaling** (beside the planned quality presets and render-scale slider) | Later | S | Large | plan.md |
| **MSAA memory** (4× on Rgba16Float, about 32 MB at 1080p): a quality setting could lower it | Not needed yet | S | Small | ADR-0003 |

## Networking

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Secure connections** (connect tokens) for public servers | Unsecured netcode first | G | Medium | ADR-0008 |
| **Structures' elastic vibration on clients** (only lasting shape changes over a centimetre are sent) | Bandwidth | R | Medium | ADR-0008 |
| **Predicting bumps into things**: the prediction world holds only the avatar and the terrain (bumps are corrected, not predicted) | Cost and complexity | G | Large | ADR-0008 |
| **Prediction drifts from the server's by over a centimetre on about 5 % of corrections**, even on a lossless link: the snapshot carries quantised state, and neither the solver's history nor the friction anchors | Measured in F6; the netcode test holds 5 % | G | Medium | this list (F6) |
| **A second copy of the terrain's caches** in singleplayer (server and client) | Same code for single- and multiplayer | S | Medium | ADR-0008 |

## Logic

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Tables keyed by tables or functions in `pairs`**: an error, since Lua visits them in the order of their addresses | Creation order would need a stamp on every table inside Lua | G | Medium | ADR-0015 |
| **Numbers as text** (`tostring`, `string.format`, reading numbers) go through the C library's `snprintf` and `strtod`, correctly rounded on glibc, MSVC since 2015 and macOS but not our own | They agree on every supported platform | R | Medium | ADR-0015 |
| **Saving a script's functions and coroutines**: only its `memory` data survives a save, and it starts again from its source | A Lua state's insides cannot be saved portably | G | Large | ADR-0015 |
| **Messages between controllers** other than wires (radio, buses) | Wires first | G | Medium | ADR-0015 |

## Data and mods

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **The physics sandbox's members use the solver's built-in S235** (`section::STEEL_S235`), not the packs' steel: a mod's steel changes the heat scenes, not the bridge | The part catalog that builds members from the packs' materials is G2 | G | Small | ADR-0018 |
| **A running server reloading its packs**: a server's data is fixed for its run (its world and its clients must agree) | Changing a running world's planet under its players' edits has no right answer; a restart applies changes | G | Medium | ADR-0018 |
| **Signed or checksummed packs** for public servers: the content hash identifies data, it does not authenticate it | Not needed while servers are authoritative and private | G | Medium | ADR-0018 |

## Simulation LOD

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Far awake bodies that never rest** stay simulated in full (parking needs rest) | None in the sandbox does; freezing or railing moving bodies on a planet would be unrealistic | S | Small | ADR-0019 |
| **Probing an abstract machine costs 15–27 % of running it in full while its slow states change** (the powerhouse's masses warming over hours; the plan aimed at 2 %; 0.3 % once steady) | Black-box rates follow coupled thermal states only with a probe every few minutes; a model-based abstract self (each domain's own slow linear dynamics, the thermal network's conductances as the rates' dependence) is a design of its own | S | Medium | ADR-0020 |
| **Machines sharing stocks** (a tank two machines draw from) in the world | The rate solver shares stocks; the world's machines are not connected until G2's building | G | Medium | ADR-0020 |
| **The first minutes of a machine's warm-up** are followed less closely abstract (6 % on heat stocks still small) | The predictor has no history yet | R | Small | ADR-0020 |
| **A machine's insides beyond its fast states** (its platform's motion, a circuit's and pipes' internal states) are rebuilt at rest when a save is loaded | They settle in seconds; saving every domain's state portably is larger | R | Medium | ADR-0020 |
| **Terrain edits far from players under parked islands**: edits need a player within 500 m, inside the active range; a future long-range edit (an explosion, a landslide) must wake what stands there first | No such edit exists yet | R, G | Small | ADR-0019 |

## Elsewhere

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **A Lua scripting API for mods** (mods are data packs now) | Later | G | Large | plan.md |
| **An emulated RISC-V CPU** for logic | An optional extra | G | Large | plan.md |
