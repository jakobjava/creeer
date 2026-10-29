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
  - `character::refill` fills the tank: on creative servers only (re-planned 2026-10-22: no landing pod; in survival nothing refills it, `docs/g1-plan.md` G1.6b).
- **Attitude jets** damp the body's spin while it thrusts (at most 30 N·m from a pair at 0.3 m, paid in gas): EVA in free fall holds still.
- **The input.** `AvatarInput.thrust`: a fraction of the most push, in the world's frame, delta-coded as the walk is. In the client it is held Ctrl while walking: up, leaning the way WASD goes.
- **The tank's level.** The server tells each player their character's gas with every snapshot round (`ServerMessage::Own`), for the HUD (G1.9) and for the prediction, which sets its body's mass from it at each correction.
- **Held standing, free in the air.** A character is stepped as held (BDF2) while its legs stand on anything, and free (Newmark) in the air.
  - BDF2 takes part of a force switched on with it: a full tank burnt held fell 0.6 % short of Tsiolkovsky's Δv and never recovered, while Newmark's impulses are exact (to six digits). Recorded for contact groups in general in `docs/future-improvements.md`.
  - Standing, BDF2 is needed twice over: for momentum with a body the legs push, and for the client's prediction. A prediction set back to the server's state cannot know Newmark's carried acceleration: stepped free on the ground, 8 % of its corrections were over a centimetre (the test allows 5 %).

### The ragdoll and injury (G1.2d)

- **Knocked down** (`character::ragdoll::knocked`): leaning past 45°, or jolted by more than 2 m/s in a step beyond what gravity and its own loads explain.
  - People keep their feet by stepping after pushes that move their centre at 1–1.5 m/s, not after 2. Walking into a wall is not such a jolt; running into it, or landing on the hips from 3 m, is.
  - The jolt, not the contacts' forces: an impact's impulse is the impact law's, outside them, so on the terrain a 10 m fall read as no force at all.
- **The ragdoll** (`ragdoll::fall`): fifteen boxes on fourteen joints (ADR-0022) with a person's mass fractions (de Leva 1996, the suit spread alike, the backpack on the torso), a person's joint ranges, and each joint damped at a damping ratio of 0.5 of the segment beyond it swinging about it (2ζ·√(m·g₀·r·I): a foot about 0.08, a thigh about 5 N·m·s/rad; one damping of 2 for all left a hanging foot oozing for seconds, never asleep; ADR-0024).
  - The body becomes its pelvis, at the same index and entity, so whatever knows the character finds it.
  - The handover keeps momentum and angular momentum exactly: the segments are placed with their centre of mass on the body's, at its velocity, turning at the spin that gives their inertia the body's angular momentum.
- **Injury from mechanics** (`character::injury`): an impact's harm follows from the deceleration of the part struck, not from the height fallen.
  - A part's impact is its velocity change beyond gravity and its own loads, summed over the steps it lasts (`Strikes`: taken step by step, a 20 m fall was not fatal).
  - Its pulse lasts half a period of the part on a person's stiffness (τ = π·√(m/k): the head behind a helmet's liner 4·10⁵ N/m, the body through hips, back and shoulders 10⁶), and at least as long as it was seen to last.
  - The head: HIC of a half-sine, and Prasad and Mertz's curve (as NHTSA uses it) for the chance of a serious injury. The rest: the peak against the torso's tolerance, from 30 g to 90 g.
  - When the character falls, the impact it is in so far is the body's; its segments take the rest.
- **People on the server** (`creeer_server::world::people`): each player's character has a health (a share of a whole one), healed 1/600 a second.
  - A knocked-down character falls as its ragdoll, whose segments become entities of their own (the avatar's id is the pelvis).
  - Lying still (under 0.1 m/s and 0.3 rad/s) for 2 s, it stands up again where it lies. Dead, it lies for 5 s, and its player stands whole at the spawn point, its tank full.
  - A ragdoll is never parked (simulation LOD): it stands up first, or is taken away.
  - The world's air and sea renew a body replaced in place (`Surroundings::renew`).
- **What the player is told.** `ServerMessage::Own` carries health and whether the character is down beside its gas.
  - Clients now take `Own` in from the unreliable channel: before, only snapshots were read there, and the gas never reached the prediction.
  - Down, the client draws its avatar as the server has it and stops predicting it. Up again, it is predicted afresh from the next snapshot, with no correction slid through from where it fell.
  - An entity that became something else (a character fallen, or up again) is despawned for every client that has it and spawned anew as it is now.
- **Saves.** The `PEOP` section holds the people: health, a fallen one's segments (they must be bodies of the world), how long it has lain still and been dead.

### Amended in P1 (2026-10-18): the legs as a strut, and three corrections to falling

- **The legs are a strut** (`world::strut`, ADR-0024), solved in the step with the body and what it stands on, no longer a load put on what they stand on before the step.
  - A load from the legs' spring is stable only on a body heavier than k·h², 8 kg at 60 Hz. A character standing on a ragdoll's hand (0.6 kg) put 3 kN on it; the hand was flung at 64 rad/s, and the world's step was redone in 16 substeps (the play scene's 58 ms tick).
  - The strut runs from the body's centre to the foot's point on what the ray found (or the ground), along the vertical, its rest length shifted by the hip's offset: the force is the old one (k·compression + c·its rate, at least nought, at most 3.6 kN), and no new torque.
  - What stays a load: the walk, and on slopes the friction cone's correction to the legs' part (from the legs' force as the step begins).
  - Set even while the character sleeps: a support that wakes wakes it.
  - Validation (`world/strut_tests.rs`): a body on a strut rests at m·g/k; a 105 kg body standing on a 0.6 kg plate on the floor presses it down (it rocks once at 2.8 rad/s as the floor takes up the load, then lies still) and the floor carries both within 2 %; between free bodies a strut keeps momentum, pushes only, at most its most; its damper against its one-sided law integrated finely (RK4), within 3 %. Every character test as before.
- **Injury sees the struts.** A part's velocity change less gravity, its loads and the struts' push on it (`World::strut_pushes`: its legs', or the legs of one standing on it) is what struck it. Without, the legs braking a 3 m landing read as a blow (harm 2.0).
- **Struck down, it falls once its strike is over.** Fallen in the strike's first step, the body's leftover speed (3.4 m/s at 3 m) went to every segment at once, and they struck the ground together; the body now takes the whole strike as one pulse, then falls. Toppled, it falls at once.
- **The ragdoll is laid out with the legs it has.** Legs pressed shorter than they stand (the body on its knees or on its box) are folded to their length, and a segment still in the ground is put on it. Laid out standing, the legs were half a metre in the ground and its contacts threw them out: a fall from 3 m that had ended at rest took two healths. (This was there before the struts, and inflated the falls' numbers.)
- **Injury by region**, in its first form (`injury::Region`): the head by HIC; pelvis and torso by their deceleration against the torso's tolerance; a limb by the force of the blow (½·Δv·√(m·k)) against the load that breaks its bones (upper arm 3, forearm 2.5, hand 2, thigh 8, shank and foot 5 kN). A break takes a fifth of a person's health, a judgement until G9. Judged as torsos, a hand catching a fall at 3 m/s struck at 130 g.
- **A strike's duration** is the steps it was seen in less one (it starts and ends within its first and last): taken as all of them, a 20 m fall's 32 ms strike seen over three steps read as 50 ms, and two thirds as hard.
- **Falls**, each from five leans (people land differently; one landing alone is chance): 1 m unhurt, standing; 3 m 0–0.12, down; 5 m 0–0.14; 10 m 0.53–0.61; 20 m 1.0–1.06, killed every time.

### Amended in G1.3f (2026-10-21): the ragdoll in a real person's proportions

The ragdoll had been laid out for a 1.86 m person carrying a jetpack, its arms 9 cm too wide and 15 cm too high, its hands half as long as a person's. It now has the engineer's proportions (ADR-0026), so the drawn body, skinned on these segments (G1.3e), is the simulated one.

- **Measured, not guessed** (`art/engineer/segments.py`, run on the fitted engineer in Blender): each vertex of his skin is given to a segment by its weights, split at the joints (the pelvis down to the gluteal fold, as de Leva segments it). A limb's box runs from joint to joint; across, it is the limb's measured section, an ellipse, as a box of the same area. The trunk's, the head's and the feet's likewise from their extents. Joints at the person's joint centres: shoulders at 1.459 m and ±0.186 m, elbows 1.148, wrists 0.899, hips 0.959, knees 0.524, ankles 0.111 (on boots' 3 cm soles); the back at the waist (1.113), the neck at the base of the neck (1.56). The masses keep de Leva's shares.
- **The rule within its segments** (`art-style.md` §2): every vertex of the trunk, head, hands and feet lies within its box grown by 3 cm. The forearms and shanks miss on 0.3–1.5 % of theirs (by up to 3.8 cm), the upper arms on 5 % (4.7 cm, the shoulder's muscles) and the thighs on 7 % (4.5 cm, the hip's): boxes large enough to take them would meet the trunk's.
- **No two segments not joined overlap as they stand** (a test): the closest, a forearm and a thigh as the arms hang by the hips, are a centimetre apart. The arms hang 19° down from the base mesh's A-pose rather than 21°: at 21° a forearm's box overlapped a thigh's by 6 mm.
- **Legs are folded from the ragdoll's own hip joints** (0.959 m), not the standing body's hip (0.92): folding below the joints left them pulled apart from the thighs, and the crowd scene's worst tick took 385 ms.
- **Within a ragdoll, only the arms meet the trunk and head** (`ragdoll::pass_within`, `World::let_pass`, the user's choice): the other pairs of segments pass through each other. With every pair colliding, the limbs, now as close together as a person's, kept fallen ragdolls restless (the contacts found anew each step, G1.8). The pairs are not saved; the server lets them pass again for the fallen when it loads (`people::restore_people`).

| Within a ragdoll (24 crates dropped on a character, 10 s each) | At rest | Time | Worst tick |
|---|---|---|---|
| The old ragdoll, every pair colliding | 17 | 19.9 s | 48 ms |
| Corrected, every pair colliding | 10 | 34.4 s | 116 ms |
| Corrected, all but arm against leg | 9 | 31.6 s | 185 ms |
| Corrected, all but leg against leg | 13 | 26.6 s | 84 ms |
| **Corrected, only arms against the trunk and head (taken)** | **17** | **23.7 s** | **52 ms** |
| Corrected, none | 19 | 21.3 s | 72 ms |

- **The play scene *crowd*** at full length: p50 1.08 → 0.94 ms, p99 15.1 → 10.6 ms, worst 35 → 28 ms. Its most work in a tick at CI's length rose from 4,730 to 6,651 Newton iterations (tick 82, a crate landing on a fallen player; the old ragdoll reached 6,347 at full length): the user raised its ceiling to 7,300 (`play/tests.rs`). The stress scene *server* at full length: p99 24.5 → 15.2 ms, worst 67 → 31 ms.
- **Goldens renewed** (*crowd* both lengths, *server* at 1,200 ticks), after the old ragdoll with the new code reproduced every old hash: the change is the ragdoll's alone.

### Amended in G1.3i (2026-10-22): standing along the plumb line

The character stood along the radius from the planet's centre. The world turns with the planet (6-hour day), so gravity includes its centrifugal pull: at latitude 42° that leans the plumb line off the radius, and the legs, pushing along the radius, left 0.021 m/s² of it sideways. The walk, a damper on velocity, balanced it at a creep: a character standing still slid downhill at 5.3 mm/s and never slept (so never parked: `world/lod_tests.rs` found it once the spawn moved off the pole, where the pull is 0.4 N and the creep under the sleep threshold).

- **Up is the plumb line** (`character::plumb`): against gravity with the centrifugal pull, from the world's own field; level ground is level by it, as the terrain's datum is that field's equipotential. The server, the client's prediction, the knocked-down test and standing up all take it.
- **The predicted avatar sleeps when the server's does** (`predicted.rs`): standing still now lets it sleep, and the client had woken its copy at every snapshot (the bodies near it already slept as the server's do, ADR-0025).
- **The spawn** moved to the first land of the spread between 300 and 1,500 m at a temperate latitude (30–45°) whose ground rises at most 1.5° every way out to 5, 15 and 40 m: latitude 45°, the noon sun 45° high. By the pole it never rose above 4°.
- **Goldens renewed** (*crowd* both lengths, *server* both lengths): the spawn moved. The stress scenario *server* dropped its crates and dug along the world's X and Z axes, horizontal only by the pole: at 45° the crates fell on the players and their ragdolls took the ticks (p50 0.9 → 2.1 ms). Along each player's east and north instead: p50 0.87 ms, p99 2.2 ms (16.0 by the pole), worst 15.8 ms (32.4). *crowd* at CI's length on level ground: its worst tick 1.5 ms (36 ms by the pole), its most work 248 Newton iterations (9,051 by the pole with the plumb line, over its ceiling). Its gate is now an ensemble (`play/tests.rs`, ADR-0025 amended).

### Not yet (G1.2's next slice)

Prediction of bodies near the player. Climbing needs something to climb: ladders and handholds come with G2's building.

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
| The ragdoll's handover | momentum and angular momentum within 10⁻⁹; its segments weigh what the body did |
| A ragdoll fallen | comes to rest on the ground within 10 s, its joints shut within 1 mm; stands up with its mass |
| Falls onto rock (five leans each) | 1 m: unhurt, standing; 3 m: 0–0.12, down; 10 m: 0.53–0.61; 20 m: killed every time |
| A fall on the terrain (server) | down, hurt; stands up after 2 s still, where it fell, its segments gone, healing |
| Killed | stands whole at the spawn point after 5 s, its tank full |
| Saved down | loads down, with its health and segments, and stands up as it would have |
| The player's view | told it is down and hurt; its ragdoll drawn as the server has it; up again, a character predicted within 5 cm |

## Cost

The play scene *crowd* (8 characters walking, jumping and dropping crates among each other): 0.74 ms p50, 2.2 ms p99, 4.1 ms worst on the server before G1.2d.

With G1.2d, crates and characters running into each other knock them down. The scene is then 1.6 ms p50, 11.9 ms p99 and 58 ms worst. Its slow ticks are the ragdolls' groups on the ground, measured on one ragdoll:
- the step takes about 16 Newton iterations, against 3 to 5 for a well-conditioned one;
- each joint refresh gains about a third (all three refreshes allowed are taken);
- the early iterations are cut short where the first open contact closes;
- an iteration costs about 80 µs.

That is P1's work (the large group's contact solve), brought forward from G1.8 to come next.

## Amended after the user's play (2026-10-23): stopping and jumping held

- **Stopping brakes at the legs' most.** Letting go of the keys, the walk eased to a stop with its own response (0.25 s), creeping on for 1.3 s and 1.4 m from a 3 m/s walk while the legs stepped on (the user: "walks way too long after stopping"). People stop in about a step (Ridge et al. 2013). The client marks the inputs of the second after the last walk as stopping (`AvatarInput::stopping`, protocol 25), and the legs then close the gap in 0.05 s, within their 420 N: from 3 m/s it stands within 0.9 s and 1.2 m (`it_stops_from_a_walk_or_a_run_within_a_second`). A body pushed while standing is not braked so: braking every slowing (and standing still) stiffly toppled a player struck by a rolling trunk and worsened prediction against crates.
- **A held jump waits for the landing.** Held, the key jumped again at the legs' first touch, still falling, so the push only cancelled part of the fall and the character bounced in 0.2 m hops on its legs' tips. A jump now needs the legs to bear half the weight and the fall stopped (`running_with_jump_held_it_lands_between_full_jumps`).
- **The drawn stop** (`creeer_client::animate::stop`) finishes the step in hand and the other foot's closing step, then stands; a foot steps into place only past 15 cm once the body is at rest. The hips give a landing leg a stance's bent knee through late swing (it snapped 13° in the landing frame).

## Amended in G2.10 (2026-10-28): standing on a light body

A loose green spruce pole (1.6 m, 50 mm, 2.5 kg, a 16-sided prism) lying near the players cost 7–15 ms a server tick for 105 ticks, and *builders*' first 100 ticks 20–30 ms. It had been put down to the terrain's 12.5 cm cells near players. It was a player standing on it: the players spawn 0, 2, 4, … m east of the spawn, and the third stands where *builders*' first pair (and the probe's pole) lies, 4 m east. The same pole 4 m north, among the same players, costs what it does on coarse terrain: 244 world steps in 240 ticks, no turn redone, about 560 Newton iterations of its own (580 on coarse terrain).

### How it was measured

- `play/poles_probe.rs` (by hand): the pole under the third player (`poles_settling`, `COUNT=1 POLES=lower`; `FIRST=2` moves it 4 m north), the same pole alone on coarse terrain (`a_pole_settling`), *builders* tick by tick at CI's length with its clients (`builders_first_ticks`) and its heavy ticks' groups and its poles' fastest spin (`builders_heavy_ticks`). Physics counters with `--features creeer_physics/profile`.
- Temporary prints, not kept: the turn check's turns (`world/turn.rs`), each step of the pole's group (velocities before and after the impulses, the impact's budget), the hull's ground detection (`ground_hull.rs`), and the loads the feet put on the body under them (`character::steer`).

### Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Jolt's `CharacterVirtual` (`CharacterVirtual.h`: `mMass` 70 kg "used to push down objects with gravity when the character is standing on top", `mMaxStrength` 100 N "maximum force with which the character can push other bodies") | A kinematic character presses what it stands on with its weight and pushes others with a capped force | What a character puts on a body under it is a push where it stands, within a strength; nothing turns the body but where the push lands |
| PhysX's character controller (`PxUserControllerHitReport::onShapeHit`) | Kinematic; pushing dynamic bodies is left to the game, an impulse at the body's centre scaled by the masses (the samples skip vertical pushes as "useless stress on the solver") | A load at the centre ignores where and how the feet press |
| Winter, "Human balance and posture control during standing and walking" (*Gait & Posture* 3, 1995); Horak & Nashner, *J. Neurophysiol.* 55 (1986) | The ankle's torque on the ground is the normal force times the centre of pressure's offset, which stays within the base of support | The balance's torque reaches a body only as far as the soles' patch on it lets the pressure's centre move |
| Winter, *Biomechanics and Motor Control of Human Movement* (2009), table 4.1 | Foot length 0.152 of the stature | The feet's reach about the point under the hips: 0.133 m for 1.75 m |
| Catto, "Solver2D" (box2d.org, 2024): soft steps; Baraff & Witkin, "Large steps in cloth simulation" (SIGGRAPH 1998) | A constraint or damper solved with both bodies' masses is stable whatever their ratio; taken explicitly a damper c is stable only on a mass above c·h/2 | The walk's damper on a pole's top (0.94 kg) at 420 N·s/m and 60 Hz is not; solved in the step, it is |
| Coulomb's friction cone; Lubkin (1951) for the torsion over a contact (as `contact/twist.rs`) | Within the cone a sole grips; beyond it slides with μ·N against the slip; a patch's torsion is μ·N times its mean distance | The walk's grip and the balance's torsion are bounded so |

### Found

1. **The balance's torque was put whole on the body stood on, at its centre.** A person leaning a few milliradians puts 10.8 N·m on it, 7.1 about the pole's own axis of 7.85·10⁻⁴ kg·m²: 9,000 rad/s². One step turned the pole 1.1–1.9 rad and left it at over 100 rad/s; the turn check (`MAX_TURN`, G1.8, unchanged) redid every tick in 16 substeps, 17 world steps a tick even at rest.
2. **The legs' strut pressed a point of the body turning with it.** Under a person's 2 kN a pole was an inverted pendulum about its axis, −N·r = −50 N·m/rad, and rolled out from under the foot at 30–40 rad/s.
3. **The walk was a damper put on as a load from the speeds as the step began.** That is stable only on what is heavier than c·h/2 where it pushes, 3.5 kg at 420 N·s/m and 60 Hz; a rolling pole's top is 0.94 kg. It rolled the pole at up to 76 rad/s, back and forth tick by tick.
4. **A foot on a body steeper than the friction holds** put the part of the legs' push that friction does not hold on as a load for the step: 800–980 N where a foot met a pole's side 43–51° steep near its end, 400–560 N·m about its middle. A pole spun at 790 rad/s.
5. **The legs' vertical push on a tilted face holds friction of N·sin φ itself** (a sole gripping). It acts whether or not the sole grips. With the walk's grip set at μ·N·cos φ − N·sin φ both ways, a body slipping under the soles was held by μ·cos φ − 2·sin φ of N only, and a pole spun under a standing player at 30–40 rad/s for 20 ticks.

6. **The impact's budget forgot the legs before the step** (`world/stepping.rs`, ADR-0005). It counted the bodies' energy before the step and, after it, the bodies', the contacts' and the joints' (struts and grips). A person's legs hold 17–41 J under them. The budget fell below nought, and every impact in the group stopped every body of it dead: the pole and the person on it, every other 1 ms substep, the pole hanging 7–10 mm above the ground, its contacts found and lost in turn. It had hidden Found 1–4: with it mended alone, *builders*' poles spun at up to 290 rad/s.

### Kept

| Change | Where | Measured |
|---|---|---|
| **The impact's budget counts the joints' and struts' energy before the step as after** | `world/stepping.rs` (ADR-0005) | The pole under a person: 1,072 → 344 world steps, 96 → 2 turns redone, 10,901 → 2,530 Newton iterations (this change alone); with the rest it sleeps (Found 6) |
| **A strut's points move with their sides' turns to first order**, p = x + a₀ + θ × a₀ from the step's start: a sole presses a round body through its axis however it turns, and a plank pressed at its end is turned by the lever it was pressed at. The rows are the length's slope by the solver's turn (J⁻¹(θ), checked by finite differences) | `world/joint/strut.rs` | No inverted pendulum (Found 2) |
| **A foot sliding on a body presses along its friction cone's edge**: the strut turned that way, its rest giving that force as the step begins; none of it a load | `character/feet.rs` `slides` | No 400–560 N·m kicks (Found 4) |
| **The walk on a body is the strut's** (`world::strut::Walk`): a damper along the ground on the person's speed over the body's point, towards the speed asked for within the walk's strength, solved in the step; its drive booked as a grip's (`walk_work`); its grip μ·N·cos φ − N·sin φ uphill, μ·N·cos φ + N·sin φ downhill, μ·N·cos φ across, so a body slipping under the soles meets μ·N against the slip; asking for a walk wakes the person and the body | `world/strut.rs`, `world/joint/strut.rs`, `character/feet.rs` `walks` | What rolls under the soles carries the person (Found 3, 5) |
| **The balance's torque reaches a body only through the soles' patch on it**: the face of its shape the legs' ray met, clipped to the feet's reach, 0.133 m. About the axes in its plane the pressure's centre stays within the patch, a moment N·r × n; about its normal μ·N times the patch's mean distance from that centre (exact, by fans). The person's balance is the same torque: what the feet do not carry it does not get. A ball carries none; a face that holds the whole of the feet holds them as the ground does. | `character/feet.rs` `carried` | A pole's 9.75 mm faces carry 4.9 N·m about its axis under 1 kN (Found 1) |

Fixed ground under the feet keeps all of it as it was: the balance whole (its 600 N·m), the walk a load (steady on what does not yield), the legs' strut on a fixed point.

### Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| **The impact's budget alone** | *builders* at CI's length: poles up to 290 rad/s, 57 ticks with one over 20 rad/s (49 and 13 before), most work 2,555 at tick 493 (ceiling 1,659) | The zeroing had hidden the loads' spin-ups (Found 1–4); kept with the rest |
| The budget counting the joints as the lesser of before and after (a strut is set anew each step: on a landing it starts pressed by what it never stored) | The raising test and *log_bridge*'s ceilings pass, but the pole under the third player 213 Newton iterations at its landing and 9.5 rad/s; *work*'s client 150, the crate walk's prediction as below; *crowd* at full length still two players toppled | It takes back a strut's real release, and keeps what moves the other gates |
| **The walk through the body's mobility at the step's start**, (1 + c·h·W)·F = c·(wanted − going) as the step's load (`docs/adr/0023/walk-through-mobility.rs.txt`) | Stable, but it holds nothing within the step: with the strut's turning point, the pole under the person 1,507 world steps and 164 turns redone; with the patch and the first-order point, *builders*' poles up to 790 rad/s (Found 4 not yet kept), then 169 rad/s with it | The soles grip within the step or not at all: superseded by the walk in the strut |
| The walk in the strut, its grip μ·cos φ − sin φ both ways | *builders*: 48 ticks with a pole over 20 rad/s, one spinning under a standing player at 30–40 rad/s for 20 ticks | Found 5 |
| The patch clipped to the feet's reach on every face | On a crate a quarter of the ground's 600 N·m: *crowd*'s players landing on crates fell over where on the ground they stand | A face that holds the whole of the feet holds them as the ground does |
| The walk in the strut, its work unbooked | A person asked to walk on a crate on ice went at 0.4 mm/s: the energy guard took the walk's gain for the step's and stopped it | Its work is the person's, as a grip's is the hands' |
| The legs pushing along the face's normal, all the friction the walk's (considered, not built) | A damper alone holds a person on a tilted face only by creeping: 0.47 m/s on a pole's 11° face at 420 N·s/m | A sole's grip needs an anchor, as the contacts' friction has (`docs/future-improvements.md`) |

### Validation

- `world/strut_tests.rs`: a 2.5 kg plank landing under a 105 kg body on its legs stops only itself, the body going on down on its legs and no energy gained (fails before: both stopped dead); a pole stood on over its axis, set rolling, comes to rest under the foot; a walk gets its speed over the body within its most, momentum kept to 10⁻⁹.
- `world/joint/strut.rs`: a body turned under a strut by up to 0.6 rad is pressed where it lies (a turning point falls by r·(1 − cos θ)), the rows the length's slope at a turned state; the walk's spring each way its own most.
- `character/feet_tests.rs`: a pole's top face carries 1 kN times half its 9.75 mm about its axis and half a foot along it, μ·N times the strip's mean distance about the vertical; a crate holds the feet whole and, near its edge, keeps the pressure's centre on it; a ball nothing; fixed ground the torque whole; a square's mean distance from its centre and a corner, a strip clipped to the feet.
- `character/tests.rs` unchanged: walking on a crate on ice keeps momentum within 10⁻³, slopes hold at 25° and slide at 36°.
- **The gate** (`play/stood_on_tests.rs`): *builders*' players by a pole where the third spawns: at most 121 Newton iterations a tick (110 measured, at the landing in tick 0), the pole's spin at most 7.1 rad/s (6.5 at the landing, under 0.001 rad/s after the first second), asleep by tick 117 (106). Before: 405 iterations, 125 rad/s within each tick's first step. *builders*' work ceilings a tenth above what it measures now (1,134 and its client's 1,294).

### Measured (development machine; g2 at d6a86c7 → the feet alone (the first commit) → with the impact's budget)

| | before | the feet | with the budget |
|---|---|---|---|
| The pole under the third player, 240 ticks: world steps, turns redone, Newton iterations (players' 1,208 in) | 1,072, 96, 10,901 | 248, 0, 3,200 | 248, 0, 1,927 |
| Its ticks (the players' own 0.3 ms in) and sleep | 7–18 ms for 105 ticks, then asleep | 1.2–1.4 ms, awake at tick 240 (every impact under the person stops it dead) | 6.8 ms at the landing, 1.2–2.0 ms to tick 30, 0.6–1.0 ms to tick 106, then asleep |
| The same pole 4 m north among the players; alone on coarse terrain | 244 steps, no redo, 1,769 iterations; asleep at tick 110 | the same | the same |
| *builders*' first 100 server ticks, mean and worst | 33.2 ms, 55.0 ms | 13.2 ms, 25.8 ms | 6.6 ms, 29.4 ms |
| *builders* at CI's length: most work in a tick; a client's most | 1,537 at tick 5; 1,917 at tick 521 | 1,039; 1,667 | 1,031 at tick 5; 1,176 at tick 211 |
| *builders* at CI's length: worst tick; a client's worst | 69.5 ms; 106.6 ms | 28.5 ms; 60.9 ms | 34.6 ms; 47.2 ms |
| *builders*' poles: the fastest; ticks with one over 20 rad/s | 49 rad/s; 13 | 27 rad/s; 2 | 45 rad/s; 7 |

### Where this leaves G2.10

- **Five gates move with the impact's budget alone, not raised here (for the user).** Impacts under a person no longer stop everything in its group dead, and what rested on that moves:
  - `world::raising_tests::walked_up_its_foot_in_a_hole_a_pole_comes_up_to_stand`: the hands' pull draws the pole's foot 0.77 m out of the hole's mouth while it is raised (the zeroing held it there); it comes up to 20° and, let go, falls to 89°. The steering it scripts, or what it asks, is a gameplay matter.
  - `predicted::tests::walking_into_crates_is_predicted`, lossless: one placement of eight has 5.5 % of its corrections over 1 cm (gate 2.7 %; the others 0.6–2.5 %, the median 0.9 %): a player walking into a crate is no longer stopped dead on both sides alike.
  - *log_bridge*'s work 110 at tick 1 (ceiling 84; 94 with the budget alone, 78–82 without) and its client's 176 (159): the players standing on the logs as they settle. *work*'s client 136 (104; 94 without).
  - At full length *crowd* has players toppled off its piles of crates (three in 1,800 ticks, none before; with the budget alone, one), each a ragdoll of 1,500–3,700 Newton iterations a tick, the worst 11,606: p99 2.0 → 20.6 ms, the worst tick 3.1 → 68.9 ms. *log_bridge*'s breaking tick 941 takes 445,513 Newton iterations (368,146 before, 428,020 with the feet alone).
- *builders*' first 100 ticks are not under 5 ms: past the players' arrival its eight crossed pairs of poles rock at 2–40 mrad/s and never sleep, about 0.6–0.8 ms a pair a tick, the upper pole on the lower (ADR-0024, G2.0's hull contacts; `docs/future-improvements.md`).
- A pole on its own needs no rolling resistance: its faces already hold it with a lever of R·sin(π/16) = 4.9 mm (above dry sand's 4.2 mm by Bekker's law and wood on wood's 0.5–1 mm, Blau 1905) and it sleeps within 2 s. A smooth cylinder with a measured rolling resistance would be the better body (`docs/future-improvements.md`).
