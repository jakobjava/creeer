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

**Research before lifting.** Before an item here is designed, how others have solved it is surveyed (papers, engines, games: plan.md §1, principle 12), and the survey goes into its ADR's Prior work section (`docs/adr/README.md`).

**Not a place for slowness.** Performance and maximum realism together are paramount (plan.md): a scene or system that is too slow is fixed now, not listed here. Only speed-ups beyond the budgets (headroom for larger worlds) belong here, and what the user chose to defer after the measurements (the sandbox scenes' 2–3 ms target, 2026-10-18).

**Scheduled.** plan.md's Phase B places these items in the milestones whose gameplay first exposes them: the speed items in its **performance track** (P1 piles in G1; P2 the structure solve and P3 struck structures in G2; P4 networks beside the tick in G3–G4; P5 meshed pipes in G5; P6 many vehicles in G6; P7 servers on VMs in G4; P8 PGO and real min-spec hardware in G11), and the realism items under each milestone's "Lifts". An item lifted there is removed from this list.

## Physics

### Systemic (the review of 2026-10-18, `docs/physics/review-2026-10-18.md`)

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **One integrator** instead of Newmark, BDF2 and backward Euler switched per group and step around an energy guard: a smooth part that cannot gain energy, impacts within the same solve (nonsmooth generalized-α; Moreau–Jean with an energy-consistent impact law) | The largest change; needs its own survey and ADR | R, S | Very large | Review W1 |
| **Contacts persistent across steps, their set following the iterate** (no frozen manifold, no world-wide turn substeps, no found-anew surplus) | G1.8; measured in P1's first pass (ADR-0024): a see-saw of one ragdoll case (1.4–2 J found anew each step at the same pose, a jitter of millimetres) is one of its defects, but kept crossed edges alone did not help, and resting is not the ragdolls' cost | R, S | Large | Review W2, W8; ADR-0024 |
| **Friction by a semismooth Newton** (Alart–Curnier) within the step instead of lagged refreshes | G1.8 (P1's first pass measured it: neutral alone, as the joints refresh in the same steps; with the joint tree the refreshes, 35–43 % of a ragdoll's iterations, could go) | R, S | Large | Review W3; ADR-0017; ADR-0024 |
| **Property tests with random orientations** for every contact and joint law; an energy audit per step naming the term that gained | Next, before more solver work | R | Small | Review W9 |
| **Every explicit coupling checked against its stability criterion** (springs k·h² ≪ m, dashpots c·h ≪ m, added mass) | Next | R, S | Small | Review W7 |

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

### Joints

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **A ball's twist measured about A's axis**: it loses meaning as the swing nears 90° (B's reference along A's axis), and its stops are let go there | A swing–twist split with exact derivatives is a measure of its own; ragdolls' twist stops sit within it | R | Small | ADR-0022 |
| **Jointed trees stepped in their joint coordinates** (exact joints, momentum to 10⁻⁵; built in P1) | 6.5 % slower on the ragdoll bench: friction refreshes with the joints, so their refreshes stayed (ADR-0024, measured). Tried again once friction is solved within Newton's iterations; its dense reduced solve then wants Featherstone's composite-body assembly | R, S | Large (patch kept) | ADR-0024; `docs/adr/0024/joint-tree.patch` |
| **Joints pressed between stiff contacts hold within 1 % of their force**, not to a micrometre (a ragdoll's feet on the ground: up to tenths of a millimetre) | Each refresh of the multipliers gains little against stiffness a hundred times the penalty; stiffer penalties were slower and rang (ADR-0022, measured) | R | Medium | ADR-0022 |
| **A ragdoll on the ground costs 1.4 ms p50** (15 worst): its fifteen bodies are one group, whose contact solve takes twice the iterations. With G1.2d's falls, the *crowd* scene's knocked-down players take it to 11.9 ms p99 and 58 ms worst | P1's first pass (after G1.2d) fixed the 58 ms tick (struts) and took about 8 % off the step; a crate-on-ragdoll tick still costs about 1–2 ms a ragdoll while it moves, about 30 ms as the crate strikes. The rest is G1.8's: friction within Newton with the joint tree, persistent contacts (ADR-0024, measured) | S | Large | ADR-0022, ADR-0023, ADR-0024; `docs/g1-plan.md` |

### Characters

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **A ragdoll's passive joint resistance** (its joints have viscous damping only: under a crate a ragdoll slumps on as slowly as the damping lets it, 0.14 rad in 7 s, and never rests) | The torques must come from biomechanics (passive joint moments), not be chosen for speed; measured with one torque for all, they cost more in the moving steps than they save at rest (ADR-0024) | R | Small | ADR-0024 |
| **Injury by region, fully**: regions are judged (head by HIC, torso by deceleration, limbs by their bones' breaking loads), but their harms are summed into one health, a break is a fixed fifth of it, and nothing is scored as AIS per region or an injury severity score | Injuries need consequences (a broken leg slows you), which are G9's needs | R | Medium | ADR-0023; `character/injury.rs` |
| **Soft ground's yielding** in injuries: every ground counts as rock (stiffer than a person) | Soil's give is not modelled in contacts | R | Medium | ADR-0023; `character/injury.rs` |
| **Keeping one's feet by stepping**: a character is knocked down by a jolt over 2 m/s or a lean past 45°, not by a model of balance recovery | Its legs are one spring under the hip, with no steps to recover with | R | Medium | ADR-0023; `character/ragdoll.rs` |
| **Getting up**: a ragdoll at rest becomes the standing character at once | Needs a driven ragdoll (the joints' drives, G1.1) pushing itself up | G | Medium | ADR-0023; `world/people.rs` |
| **A person's joint ranges**: the ragdoll's ball joints are symmetric cones (the shoulder 1.6 rad and the hip 1.4 rad in every direction), past a person's range in several (the American Academy of Orthopaedic Surgeons' normal ranges: the shoulder extends about 60°, the hip about 30°, and it opens about 45° and closes about 30°; the ankle bends up about 20°, against 34°). Fallen ragdolls take poses no one takes, and the engineer's clothes fit badly there (the shorts 11.9 mm short of the body with the hip opened 80°). Limits per direction (swing–twist, an ellipse for each cone, as PhysX's D6 joint has) would keep it to a person's poses | The cones were enough for the first ragdoll (G1.1); the poses' cost to the clothes showed when they were fitted (G1.3f) | R | Medium | ADR-0026, Limits; ADR-0023 |
| **The segment boxes against the outfit**: the boxes keep each limb's cross-section and mass, so they are narrower than the limb and centred on its middle. The knee pads lie up to 9.5 cm beyond the shank's box at the front (11 % of their points past the 5 cm margin), the elbow patches beyond the forearm's (38–40 %), the coverall's legs past the shanks' (10–11 %), the belt's pouch past the right thigh's (25 %). A ragdoll fallen on its knees rests on the boxes and sinks the pads up to about 4.5 cm into the ground. A box grown to the outfit (or a contact over the kneecap) is a physics change, tested | Found by the detailed model's check (G1.3f); a change to the ragdoll's shapes and masses is the user's | R | Small | ADR-0026, Limits; `docs/g1-plan.md` G1.3f |

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
| **Fine-grained parallel loops when waking threads is expensive**: in a Linux container on the development machine (Docker Desktop's WSL2 VM, 16 vCPUs) a 5,000-beam bridge steps in 6.0 ms on one thread, 4.5 on four and 14.8 on sixteen (Windows on the same machine: 5.6, 1.9, 1.8); the reference tick projects to 28.8 ms there. Single-threaded work is as fast as on Windows (the rate solver faster), and every stress hash is the same | A virtual machine's thread wake-ups (CI's Linux VMs were no worse than its Windows ones); a server on such a VM would need coarser parallel grains or fewer threads, chosen by measuring the wake-up cost at start | S | Medium | `docs/physics/f7-validation.md` |
| **One profile clock per process**: a hosting client's profiler shows the server's ticks (its loop holds the clock), the client's own threads' spans inside them | Spans are not tagged with the clock they belong to; a second clock per thread pool would be needed | S | Small | ADR-0021 |
| **Budgets close on a heat-soaked machine**: late in a long `cargo xtask perf` run the 10k-node DC circuit projects to 0.99 ms (0.86 cool), the 20k-node thermal step to 1.51 ms (budget 1.5; 1.28 cool), the 1,000 aero panels to 0.199 ms (0.2) | Each meets its budget measured on its own, as every budget is | S | Small | ADR-0011 (F6.8), ADR-0010, ADR-0014, ADR-0016 |
| **BDF2 takes part of an external load switched on**: a group stepped with BDF2 (in contact, or held) receives an O(h) wrong impulse when a load starts or stops (a jetpack's full burn held fell 0.6 % short of Tsiolkovsky's Δv, never recovered); Newmark's impulses are exact. Characters step free (Newmark) in the air (ADR-0023), but a jetpack lifting off, or a thruster or winch on a body in contact, meets it | A two-step scheme's start from a step in the force; a correction carrying the load's change into the history (as Newmark's previous load) would make BDF2's impulse exact, a change to every contact group's stepping | R | Medium | ADR-0023 |
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
| **The streamer's update has rare slow frames**: in the stress flyover (1,200 frames) its p99 is 0.7 ms, but about one run in three has one frame of 4–7 ms, at no fixed frame, while its workers keep every core but two busy | Not reproducible at a frame (scheduling or the queue's lock with fourteen workers); gated at half a 60 Hz frame (8 ms). Fewer workers while the camera moves fast, or a lock-free queue, would tell | S | Small | ADR-0021 |
| **Small edits vanish at a distance**: brushes under half a cell leave coarse chunks (a 2 m hole is invisible beyond about 250 m) | Level-of-detail cost | R, G | Medium | ADR-0006 |
| **Dug rims smooth up close**: on the finest level (1 m cells) a rim's 2 m round spans about two vertices, so a few metres away its lighting shows the triangles as spikes. A finer finest level near the camera (0.5 m cells), or normals per pixel from the density field, would make it smooth | G1.3c fixed the zigzag and flaps (folds 150° → about 50°); finer terrain costs streaming and memory everywhere near the player | R | Large | ADR-0006, amended in G1.3c |
| **Ground contacts cost more than plane contacts** (a large crate samples a few hundred surface points near the ground) | Correctness first | S | Medium | ADR-0006 |
| **The sea is drawn as the datum's ellipsoid** (the true equipotential differs by metres) | Simple per-pixel intersection | R | Small | ADR-0014; `sea.wgsl` |

## Rendering

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **A sky-view table** for the atmosphere | Only if the per-pixel ray march becomes a bottleneck | S | Small | ADR-0003 |
| **Temporal upscaling** (beside the planned quality presets and render-scale slider) | Later | S | Large | plan.md |
| **Edge crawl under slow motion**: 4× multisampling steps as edges cross its samples. Measured by the shimmer check (G1.3c): with every surface one colour, 1.64 % of the image flickers, along edges only. Temporal antialiasing (with the upscaling above) or 8× samples on a quality preset would smooth it | The surfaces were the G1.3c goal (they flicker on 0.079 %); edges need a temporal pass | R | Medium | ADR-0026, Validation |
| **MSAA memory** (4× on Rgba16Float, about 32 MB at 1080p): a quality setting could lower it | Not needed yet | S | Small | ADR-0003 |
| **Weather in ageing**: rain, humidity, shelter under a roof and pollution (ISO 9223's inputs: time of wetness, SO₂, chlorides) setting each object's weathering rate, and dirt and moss gathering on what faces up. With it, exposure by mechanism: wood greying by UV alone in vacuum, where rust and patina stop (today vacuum stops all ageing) | G1.3c's ageing knows only air, sea and vacuum, as one rate: there is no weather or shelter yet | R, G | Medium | ADR-0026; `art-style.md` §6b |
| **A colour-blind mode**: a client setting that swaps the player colours (and the red/green signal and interface state colours) for sets told apart under protanopia and deuteranopia. Measured 2026-10-21 (`creeer_art::vision`, Machado et al. 2009): for deuteranopes today's blue and violet, and violet and indigo, are ΔE₀₀ 1.3 apart, orange and ochre 1.7; for protanopes violet and indigo 2.9. A set of sixteen at least ΔE₀₀ 10 apart under all three visions, keeping orange, exists (ADR-0026, "Measured and set aside") | The user, 2026-10-21: "right now lets not consider colour blind people, we could do that later if i ever want to publish it" | G | Small | ADR-0026; `art-style.md` §3.6 |
| **A character creator** beyond skin tone and hair colour: faces, hair styles, body shapes (each body shape changing the ragdoll's segments and masses, de Leva's data per build), and women's bodies (the base mesh bundle has a realistic woman; ANSUR II measured 1,986 women). MakeHuman's sliders could set builds directly where the engineer's is fitted | G1.3 makes one base body; wearables come first (the user, 2026-10-21) | G, R | Medium | ADR-0026; `docs/g1-plan.md` G1.3f |
| **Every pair of a ragdoll's segments colliding**: today only the arms meet the trunk and head; the legs pass through each other and the arms (`ragdoll::pass_within`). With every pair colliding, the limbs (as close as a person's) kept 14 of 24 fallen ragdolls restless | The contacts found anew each step hold more energy than the last step's (ADR-0024): G1.8's persistent contacts lift it, and the crowd scene's work ceiling (7,300) comes down with them | R, S | Medium | ADR-0023, amended in G1.3f |
| **The engineer's fit, two misses** (`art/engineer/physique.py`): the thighs are 4.5 % under ANSUR's normal-weight girth (fitted in 1 cm bands about each band's middle; the blend into the hips takes some back), and the tape over the neck's base reads 0.55 m against 0.422 (the shoulders' slope is lowered by its 5 cm cap and the plane still crosses their tops). Fitting the thigh about its measured loop, and the slope to the acromion and neck-base landmarks, would close both | The blockout was approved as it is; the detailed body refits from the same scripts | R | Small | ADR-0026, Limits |
| **Rounded convex hulls**: the physics sandbox's rocks, wedge and prism drawn with rounded edges, from each hull's inner hull (its faces moved in by the bevel radius) | They are drawn at their own size, so the primitives' scaling trick does not apply; the rocks get their real look with natural content | R | Small | ADR-0026, Limits |
| **A bevel radius per instance**, so a part can have sharper or softer edges than the one rule (8 % of its smallest side, at most 2.5 cm) | The instance has no room left (64 bytes, ADR-0026); no part needs another radius yet | R | Small | ADR-0026, Limits |
| **A finer box level** for the camera closer than about 0.2 m, where 8 segments may show by more than half a pixel | Rare: a part must almost touch the camera, and adding a level costs nothing elsewhere | R | Small | ADR-0026, Limits |
| **The engineer drawn two-sided**: his garments are single shells, thick only at their edges, and the mesh pass culls back faces (`mesh_pass.rs`): a look into a collar, a sleeve or a trouser leg would see through its far side. His pieces are drawn without culling, normals turned to the eye on back faces | Due with his drawing (G1.3e); the fit check already counts every drawn face as stopping a line of sight | R | Small | ADR-0026, decision 8 |
| **The player's colour as the preview lays it**: half strength over `#5c5a57` (FFF-218), in the shader | Due with the engineer's drawing (G1.3e); the preview and the approved colours already do it | R | Small | ADR-0026; `art-style.md` §3.3 |
| **Worn leather**: `leather` wears to nothing, as no scuffed brown keeps ΔE₀₀ 12 from both soil and gravel (`art-style.md` §3.3) | Its grounds' contrast rule; the boots and gloves get their wear from grime and chips meanwhile | R | Small | `art-style.md` §3.3 |
| **Hair with strands at its edge**: the engineer's hair is a shell with the Hair pattern on it; its outline is smooth, with no strands against the sky | Strands (cards or shells in layers) cost triangles the close-up budget has not got | R | Medium | ADR-0026, decision 8 |
| **Correctives for the joints closed fully**: skinning cannot part what a closing joint presses together. At the elbow bent fully the coverall comes 10.9 mm through the jacket in its crook, at the knee the cloth under the strap 9.5 mm through it; the hand bent fully meets the cuff (11.5 mm, contact); with the hip flexed 80° the jacket's hips turn 10 mm through the belt. Shapes blended in by the joint's angle (pose space deformation: Lewis, Cordner and Fong, SIGGRAPH 2000), or edge loops across each joint in every layer (about 900 more triangles drawn close up) | Within a person's range but at its end, mostly in a crook out of sight; the close-up budget is spent | R | Medium | ADR-0026, Limits |
| **The far level's soles joined to the boots**: decimated apart, the boots' uppers and soles open a seam up to the level's error (13 mm), through which the hidden feet are seen from 7–9 points at rest. Drawn only beyond about 16 m and in shadows, where that is under half a pixel; merging the sole into the upper before decimating would close it | Under half a pixel wherever it is drawn | R | Small | ADR-0026, Limits |

## Networking

| What | Why deferred | Improves | Size | Recorded in |
|---|---|---|---|---|
| **Secure connections** (connect tokens) for public servers | Unsecured netcode first | G | Medium | ADR-0008 |
| **Structures' elastic vibration on clients** (only lasting shape changes over a centimetre are sent) | Bandwidth | R | Medium | ADR-0008 |
| **Predicting other players and ragdolls**: the bodies near the avatar are predicted (G1.2e), but not other characters (their inputs are their own) nor jointed bodies (the client has no joints): walking into another player is corrected a round trip late | Their inputs would have to be guessed (Rocket League repeats the last; error then grows with the ping), and ragdolls need the joints sent | G | Medium | ADR-0025 |
| **A bubble body pushed from outside the bubble** is predicted as if untouched until the server says otherwise; the replay costs about 2 ms a snapshot on a slow link (a few on a misprediction) | The bubble is the cost's bound (12 bodies within 6 m) | G, S | Small | ADR-0025 |
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
