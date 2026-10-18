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

### The jetpack (G1.2c)

- **Cold nitrogen.** Isp 70 s (NASA's MMU and SAFER packs), at most 1.5 kN, from a 3 kg tank (two 6 l bottles at 300 bar).
  - On a planet that is honest and short: hovering a 105 kg person takes 1.5 kg/s, so the tank lifts it for 2 s (Δv 19.9 m/s in all). Better propellants are G8's.
- **The gas is the body's mass** above its dry 102 kg: nothing else to keep or save.
  - A step's burn is its push over the exhaust's speed (Isp·g₀).
  - The body steps with half of it spent (the mean mass: Tsiolkovsky to second order) and ends the step without it.
  - Its momentum's share leaves with the gas.
  - `character::refill` fills the tank (the pod, G1.6).
- **Attitude jets** damp the body's spin while it thrusts (at most 30 N·m from a pair at 0.3 m, paid in gas): EVA in free fall holds still.
- **The input.** `AvatarInput.thrust`: a fraction of the most push, in the world's frame, delta-coded as the walk is. In the client it is held Ctrl while walking: up, leaning the way WASD goes.
- **The tank's level.** The server tells each player their character's gas with every snapshot round (`ServerMessage::Own`), for the HUD (G1.9) and for the prediction, which sets its body's mass from it at each correction.
- **Held standing, free in the air.** A character is stepped as held (BDF2) while its legs stand on anything, and free (Newmark) in the air.
  - BDF2 takes part of a force switched on with it: a full tank burnt held fell 0.6 % short of Tsiolkovsky's Δv and never recovered, while Newmark's impulses are exact (to six digits). Recorded for contact groups in general in `docs/future-improvements.md`.
  - Standing, BDF2 is needed twice over: for momentum with a body the legs push, and for the client's prediction. A prediction set back to the server's state cannot know Newmark's carried acceleration: stepped free on the ground, 8 % of its corrections were over a centimetre (the test allows 5 %).

### Not yet (G1.2's next slices)

The ragdoll and injury, prediction of bodies near the player. Climbing needs something to climb: ladders and handholds come with G2's building.

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
| A full tank burnt in free fall | Δv = Isp·g₀·ln(105/102) = 19.899 m/s within 10⁻⁵; empty, no more push |
| A hover at its weight's thrust | lasts gas·Isp/m (2.0 s) within 5 %; holds itself up (lighter, it rises a little) |
| Attitude jets | a spin of 1.1 rad/s damped below a fifth in 2 s |
| At sea (the home planet's) | floats with its centre at the surface (−0.01 m), at rest; swims at 0.68 m/s; rises 0.1 m when asked |

## Cost

The play scene *crowd* (8 characters walking, jumping and dropping crates among each other): 0.74 ms p50, 2.2 ms p99, 4.1 ms worst on the server; its slow ticks are contact groups (P1, G1.8).
