# ADR-0029: What lies loose on the ground

**Status:** accepted (2026-10-22, G1.6c done).

**Context:** G1's survival start (`docs/g1-plan.md`, principle 9). A player with nothing but clothes needs stones and wood lying about to make their first tools (G1.6d). A planet holds billions of them. They must cost nothing until touched (principle 5) and be the same for every player.

**Builds on:** ADR-0006 (terrain, the cube-sphere, geology), ADR-0019 (cost follows the players), ADR-0028 (items).

**Evidence:** `creeer_sim::scatter` (`scatter/tests.rs`), `data/geology/loose.ron`, chert, obsidian and rhyolitic glass in `data/geology/`, `creeer_server::world::loose` (`loose_tests.rs`), `Command::Pick` and `ServerMessage::Taken` (protocol 15), `SavedItems.taken` (save version 10), `creeer_client::loose_view`.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Deterministic procedural placement (Minecraft's decorators, No Man's Sky's and Astroneer's scattering) | Scenery is a function of the world seed and a chunk's coordinates, made where needed and never stored; only changes are saved | Taken: a cell's contents from the seed and its key; only what was picked up is saved |
| Fragmentation size distributions (Turcotte 1986, *J. Geophys. Res.* 91: fragmented rock follows a power law, N(> d) ∝ d^−D with D ≈ 2–3) | Natural rock fragments' sizes | Taken: D = 2 between the smallest kept (2 cm) and each ground's largest |
| Pebble shape (Zingg 1935; pebbles' intermediate and short axes about 0.7 and 0.5 of the long) | The shape classes of clasts | Taken: 1 : 0.55–0.85 : 0.35–0.55, lying flat (short axis up) |
| Chert and obsidian as tool stone (lithic archaeology: fine-grained, isotropic, brittle; chert grows as nodules in limestone, obsidian is rhyolitic glass) | What early toolmakers used | Taken: both added as rocks found loose (nodule shares), not as ground |

Densities of loose stones on open ground are estimates (no survey was found that gives counts per square metre by ground rock); `loose.ron` says so, to be refined.

## Decision

1. **Cells on the cube-sphere.** A face's warped coordinates are cut into squares of about 8 m (`cell / radius`), so every surface point is in exactly one cell and cells are within a few percent of one area. A cell's things are made from the planet's seed and the cell's key (SplitMix64), their number a Poisson draw for the cell's area.
2. **Ids carry their cell.** A thing's id packs its face, its cell and its index in it (49 bits), so the server finds one again from the id alone (`Scatter::find`) without storing anything.
3. **Of the ground they lie on.** A cell's stones are of its ground rock at 0.3 m depth; on soil, of the bedrock beneath (`from_below`). A share are nodules of another rock (chert in limestone, obsidian with andesite). Branches lie on soil.
4. **On the ground as it is.** Each is placed from the heights, then onto the terrain's density, so dug ground is followed; stones lie flat, turned at random about the vertical.
5. **Picked up, an item.** `Command::Pick` (G into the hands, E into a pocket) is checked by the server: the thing exists, is not taken, and is within reach of the hands. It becomes an item: a stone a lump (an ellipsoid) of its rock at its density, a branch a rod of its wood. The world remembers it taken, saves the list, and tells every client (`Taken`), who no longer draw it.
6. **What the player sees** (principle 9): a stone's size, mass and colour, never its rock's name. Telling chert from granite is the player's to learn. Items now carry their material's colour to clients (`Matter.colour`, `ItemView.colour`).
7. **Drawn plainly** (principle 12): stones and branches as boxes in their colours, within 40 m of the camera; aimed at as the ellipsoids in those boxes.
8. **The hands move the arms too** (found here: ADR-0027 amended). A 14 g stone was lifted to elbow height in two ticks and torn from the hands. The lift's acceleration is now half of what the hands' strength leaves over the load's weight, shared by the load and both arms (4.94 % of a man's mass each, de Leva 1996).

## Validation

- The same cells give the same things every time; each thing is found again by its id, in its own cell.
- Stones on the spawn's soil: their count against the rule's density over about 10,000 m², within four standard deviations of Poisson's spread; the share above twice the smallest against the power law's, likewise; every stone's underside within a centimetre of the ground.
- Nodules come in their share (a third, set for the test, within its spread).
- A picked stone's item mass is its rock's density times the ellipsoid's volume, to rounding.
- In the world: a stone in reach goes into a pocket once (its mass on the body to 10⁻¹², picked again refused); one picked into the hands is held for a second and drawn in its rock's colour; one 10 m off is refused; a save keeps what was taken.

## Cost

A cell is made in a few density samples per thing, on the client as the camera moves (40 m about it) and on the server only when something is picked. Nothing is made or sent per tick.

## Limits

- A cell's stones are all of the rock at its middle: a rock boundary through a cell is blurred by up to 8 m.
- Densities and largest sizes per ground are estimates.
- Stones do not gather where they would (scree under cliffs, stream beds) and lie on steep slopes as on flat ground.
- Branches lie on all soil, not only under trees (G1.7 ties them to trees).
- Breaking stones is G1.6d's (knapping), with the rocks' strength data.

Each is in `docs/future-improvements.md`.
