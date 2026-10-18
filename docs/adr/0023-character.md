# ADR-0023: The character

- **Status:** Accepted (2026-10-18), growing with G1.2's slices
- **Context:** G1.2 (`docs/g1-plan.md`). Players were F5's test ball, pushed at its centre with a grip limit and no reaction on what it stood on. G1 needs a person: a body that walks, lands, slips, swims, flies a jetpack, falls over and gets hurt, from the physics alone (plan.md, Phase B, principle 1).
- **Builds on:** ADR-0005 (contacts), ADR-0008 (prediction), ADR-0022 (joints)
- **Evidence:** `creeer_sim::character` and its tests, `creeer_physics::world::ray`, the play scene *crowd* (`creeer_tools play`)

## Decision

### The body, its legs, its walk and balance (G1.2a)

- **One rigid body** (105 kg: a person and a work suit with its backpack): a box from the knees (0.5 m above the feet) to the top of the helmet (1.85 m), 0.48 m wide and 0.32 deep. It is a box, not a rounded hull: two 40-cornered hulls cost SAT 6,400 edge pairs, and a crowd of eight among crates took 1.9 ms a tick to find its contacts (0.43 ms as boxes). A body that does not turn about the vertical loses little by being boxy.
- **The legs** are a spring and damper from the hip (0.92 m) to the ground a ray finds below it (`World::ray`, new: the ground by sphere tracing and bisection, fixed geometry, bodies' shapes):
  - stiffness 30 kN/m, a person's leg stiffness in hopping and running (about 20 kN/m for 70 kg, Farley & Morgenroth 1999), scaled to the mass;
  - 0.6 of critical damping;
  - at most 3.6 kN (3.5 body weights).
  
  They hold the body up, absorb a landing, step down and up, and push what they stand on back. Compressed beyond their reach, the body meets the ground: an impact with the contacts' deceleration (a 0.5 m step lands on the legs under 4 g; a 4 m fall strikes).
- **Walking** is a force at the foot: towards the velocity asked for (at most 4 m/s), closing the difference in 0.25 s, at most 420 N. The foot's force (legs and stride) is kept within the friction cone of the ground under it (μ = min(boots 0.9, the ground's)). A slope beyond the friction angle cannot be held: the character stands on 25° and slides on 36° at μ = 0.6 (31°).
- **Balance** is a torque towards upright (5 kN·m/rad, damped), at most 600 N·m (a strong person's ankles and hips). The feet damp spin about the vertical. In the air there is neither: no air control (the jetpack's thrusters, G1.2c).
- **Reactions.** The legs' force and the balance torque push what the feet stand on back. They are loads for one step, taken off after it (`character::release`). A character and what its legs push in a step are stepped as bodies in contact are (BDF2, `Body::held`, saved). Equal and opposite loads keep momentum only under one scheme: Newmark averages a load over the step's two ends and BDF2 takes the end's, and a character walking on a crate on ice gave the pair 2.8 kg·m/s.
- **Sleep.** Asleep and asked for nothing, a character keeps its loads as they were. Its legs' force taken again from where it rests differs from the one it fell asleep under in its last bits, and a changed load wakes a body: it never slept, and was never parked.
- **Protocol 5.** A character is `Kind::Character { player }`: its shape is the game's. Clients draw its box and two legs until G1.3's suit.

### Swimming (G1.2b)

- **The sea and the air act on characters** as on every body (`creeer_sim::surroundings`: buoyancy, drag, added mass). They were spared before, because their players' machines predicted them without; the client's prediction now steps in the same air and sea, built from its own packs, which the server checked are its own (`Connection::attach_terrain`).
  - Lifts *avatars in water* (`docs/future-improvements.md`).
- **Swimming.** With no ground under its legs and its body in the water (its centre less than 0.4 m above the surface), a character swims: towards the velocity asked for, and up while asked to rise, within a swimmer's thrust (80 N; a person sprints at 60–100 N), keeping upright with at most 100 N·m.
  - The water's own forces do the rest: a person in a sealed suit (105 kg in 0.21 m³) floats with half the body under, and swims at 0.68 m/s, where 80 N meets the drag.
  - In shallow water its legs reach the bottom, and it wades.
- **Drowning** comes with G9's needs; until then the suit is sealed.

### Not yet (G1.2's next slices)

The jetpack and EVA thrusters, the ragdoll and injury, prediction of bodies near the player. Climbing needs something to climb: ladders and handholds come with G2's building.

## Validation (G1.2a)

| Test | Result |
|---|---|
| Stands on its legs | feet within 1 cm of the ground, at rest within 10⁻³ m/s, upright, no body contact |
| Walks | 1.4 m/s asked: 1.4 within 0.02 m/s after 1.5 s; stops when asked for nothing |
| Slopes at μ = 0.6 | holds on 25° (slides under 1 cm in 2 s), slides on 36° |
| Walking on a crate on ice | the crate moves back; horizontal momentum kept within 10⁻³ of the character's |
| Jump | rises v²/2g within 8 cm (2.8 m/s: 0.40 m) |
| Landing | 0.5 m on the legs (under 4 g, no body contact); 4 m strikes (over twice that) |
| Sleep | an idle character falls asleep, stays asleep, and wakes when asked to walk |
| Rays | planes, turned boxes, balls, hulls, a body skipped, the ground (a sphere planet) against analytic distances; a leg's ray within 4 density evaluations (safeguarded Newton: bisection took 25) |
| At sea (the home planet's) | floats with its centre at the surface (−0.01 m), at rest; swims at 0.68 m/s; rises 0.1 m when asked |

## Cost

The play scene *crowd* (8 characters walking, jumping and dropping crates among each other): 0.74 ms p50, 2.2 ms p99, 4.1 ms worst on the server; its slow ticks are contact groups (P1, G1.8).
