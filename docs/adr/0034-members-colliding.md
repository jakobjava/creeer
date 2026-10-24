# ADR-0034: Members of one structure colliding with each other

- **Status:** Accepted (2026-10-24), G2.5's first half (lateral–torsional buckling is the other).
- **Context:** `docs/g2-plan.md` G2.5: "members colliding with each other: a collapse must not fold through itself". A built structure is one body (ADR-0004, ADR-0033); its members carry half-prism hulls on their nodes (`creeer_building::assembly`). Shapes of one body never met each other, so a folding frame passed through itself.
- **Builds on:** ADR-0005 (contact laws: Hertz points and face patches), ADR-0024 (G2.0's hull contacts), ADR-0033 (fastened joints).
- **Evidence:** `creeer_building/src/self_contact_tests.rs`; `world/detect.rs` (`World::meet_within`).

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Wriggers & Zavarise (1997), *Comput. Mech.* 19 | Beam-to-beam contact at the closest points of two beams' axes | The capsule-against-capsule idea; Creëer's members already collide as prisms, so their prisms' contacts are used instead |
| Meier, Popp & Wall (2016–2017), all-angle beam contact | Points at large angles, Gauss points along the line at small ones, from one potential | Creëer's face patches integrate over the pressed area whatever the angle (ADR-0024's edge across a face) |
| Durville (2010), proximity zones | Contacts between fibres found by zones near each other, skipping neighbours along a fibre | The rule skipping shapes that touch in the rest shape |
| Bullet, PhysX, Jolt: collision filters within an articulation | Links next to each other along a chain do not collide; others do (PhysX articulations: parent–child pairs disabled) | The same idea, decided by geometry, not by the graph |
| BeamNG.drive | No beam–beam contact (nodes against triangles only) | — |

## Decision

Shapes of one body on different nodes meet each other through the same detection and contact laws as shapes of two bodies, unless they may touch in the body's rest shape: each shape's ball about its own middle (its hull's bounding box's centre, the radius to its farthest corner), placed at its node's rest position (orientations are turns from the rest shape), apart by more than 1 cm (`REST_APART`).

- This skips the two halves of one member's prisms at a node and at the next node along it, members fastened together (their nodes centimetres apart), and nothing else: a frame's arms 40° apart meet beyond their corner's first element.
- The ball about the shape's middle, not about its node: about the node, a 1 m member's half-prism reached 0.5 m and the frame's arms passed through each other (measured, below).
- The balls are measured only for bodies on more than one node, once a detection, and the rest test is made only for pairs whose balls overlap where they are now (the sweep's own test first).
- The contacts are the ordinary ones: face patches between prisms (and ADR-0024's edge across a face), Hertz points for balls. Friction, dashpots and the energy guard as between bodies.
- Joints broken (the parent splits a structure into bodies when it falls apart): pieces still connected fold on themselves and are stopped. Two members joined by a joint that breaks stay skipped near that joint (they touched at rest); away from it they meet.

## Measured

| Test | With self-contact | Without (the rule switched off) |
|---|---|---|
| A V of two 1 m poles hinged at the corner, weightless, folded shut at 1 rad/s each: the least distance between the outer halves' nodes (`a_frame_folding_shut_stops_on_itself`) | 84 mm (stopped on each other) | 7 mm (through each other) |
| The same falling 0.3 m onto the ground as it folds at 1.5 rad/s (`a_collapsing_frame_does_not_fold_through_itself`) | over 45 mm | 25.6 mm |
| A ladder of twenty poles nailed (two rails, eighteen rungs) at rest on the ground, 120 steps (`a_ladder_of_twenty_poles_rests_on_the_ground`, three runs each, interleaved) | 370–372 ms | 360–363 ms (the code before G2.5) |

The ladder: about 2.5 % (0.08 ms a step), the balls' measures and the rest tests of neighbouring shapes; no self-contact is found in it. The tower (about 20,000 beams) hashes the same with or without and takes the same time (122–124 ms p50, the structure solve's, ADR-0035).

### Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| Nodes' rest distance against the shapes' balls about their nodes | The V's arms passed through each other (7 mm): a 1 m member's half-prism reaches 0.5 m from its node | The ball about the shape's middle |
| The same, its test first in the pair's conditions | Measured for every pair the sweep met, asleep or apart: +5 % on the ladder | Last, after the sweep's distance test |
| Each ball computed per pair | +5 % on the ladder | Once per shape and detection |

## Goldens

Stress: *cars* (car frames folding in their crashes now meet themselves: 569.3 kJ of plastic work against 568.5, 80 members buckled, both lengths). Play: the scenes with built structures (G2.8's). Every accuracy test passes.

## Limits

- **Near a broken joint** the two members it joined stay skipped (they touched at rest); `docs/future-improvements.md`.
- **Members as prisms only**: a structure's members without shapes (the sandbox's bare frames) still pass through themselves.
- The rest shape decides: a structure built folded (members touching that later part) never meets itself there.
