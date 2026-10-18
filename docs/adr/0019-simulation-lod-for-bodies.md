# ADR-0019: Simulation LOD for bodies: active, parked, on rails

- **Status:** Accepted (2026-10-18)
- **Context:** F7.2 (`docs/f7-plan.md`). Until F7 the server's planet world simulated every body it ever held, wherever it was: a server whose players left crates all over a planet paid for all of them every tick (asleep, 0.17 µs each in the broad phase, and every awake one in full). plan.md §1.6 asks for full physics within about 2.5 km of players and nothing far away. The universe's bubble (ADR-0007) already packs vessels on rails and lands them beyond 3 km of its one focus; the server's world has many players.

## Decision

### Three levels (`creeer_sim::lod`)

- **Active:** every body within `lod::ACTIVE` (2.5 km) of a player is simulated in full.
- **Parked:** an *island* of bodies that is entirely asleep and farther than `lod::PARK` (3 km) from every player is taken out of the physics world and kept as it was. It costs nothing per tick. When a player comes within 2.5 km it is put back, bit for bit, still asleep.
- **Awake far bodies stay simulated** until they rest (then they are parked). A body is never frozen or put on rails mid-motion on a planet: a landslide of crates finishes falling. An awake lone body is a small island; a far one that never rests would stay a cost (none of the sandbox's bodies does: see the avatar fix below).
- **On rails** (the universe's vessels) uses the same two ranges (`universe::RANGE_IN = lod::ACTIVE`, `RANGE_OUT = lod::PARK`). The universe's rails are ADR-0007's; F7 does not change them.
- The two radii are a hysteresis: a player walking at the edge does not park and wake the same bodies over and over.

### Islands

Bodies whose bounding balls come within `lod::MARGIN` (5 cm) of each other are one island (union-find over a 4 m hash grid; a rigid body's ball is its shapes' bounding radius, a structure's the box of its nodes and shapes). A stack goes as one: a crate is never parked from under another. Islands are found only when some resting body is beyond `PARK` of every player (usually none is, and the pass is a distance check per body).

### Why parking loses nothing

A sleeping body is not stepped, and the world keeps no friction anchors or contact energies for it (they are the last step's, of the bodies it stepped: `World::anchors`, `world::left`). So taking a sleeping island out and putting it back loses nothing of its state. Two physics changes make the move exact:

- `World::take_bodies` takes bodies out (returning them and the index map) and `World::put_back` puts a body back unchanged (unlike `World::add`, which fits a new body to the world: planet gravity, its rest place moved to it, its solver's history reset).
- **Removing bodies no longer forgets the energy the other contacts hold.** `World::renumber` used to reset `left` (the contacts' elastic energy the energy guard compares with, ADR-0005 "Amended in F6") for every body whenever any body was removed; it now renumbers it, as it already did the friction anchors. Without this every park would have cleared the record of every resting stack in the world.

### When (deterministic)

- Every `lod::PERIOD` (10) ticks the pass finds the parked islands within `ACTIVE` of a player (a hash grid of 2.5 km cells: 27 cells a player) and parks the resting islands beyond `PARK`.
- **Bodies come back over several ticks**, at most `lod::WAKE_PER_TICK` (200) a tick, nearest island first. An island may come back in parts: its bodies are asleep, and nothing awake is within reach of them (a player is 2.5 km away, four times the view of ADR-0008) before all of it is back. Measured: a 1,000-crate yard woken at once added 1.5 ms to a tick; spread, 0.43 ms.
- The pass runs at fixed ticks in a fixed order (islands by first body, parked islands by key, nearest-first ties by key): the same inputs give the same state.

### The server's world

- `ServerWorld::regions(centres)` before each step, the centres being the joined players' avatars (`Server::tick`). With no player at all every resting island is parked.
- A parked body keeps its entity id, whose avatar it is, and the water moving with its floating shapes (`Surroundings::water_of`, `expect`).
- **An offline player's avatar** is parked like any body; `avatar_of` puts it back when the player joins again.
- `Command::Clear` puts every parked body back first (it clears all but avatars).
- **Clients** see nothing of it: a parked body is beyond every client's view (600 m, ADR-0008); a region's bodies reach a client through the snapshots as usual.
- **Saves** (version 6): a `PARK` section holds the parked islands (a physics world of their bodies, then each island's bodies' ids, players and water). Bodies on their way back are saved as one more island. Version 5 saves read as before.

### Found on the way: idle avatars crept downhill for ever

An avatar asked for nothing was braked by a push proportional to its velocity (ADR-0008's test avatar): on a slope that settles where the push equals gravity's pull along the ground, at v = g·sin θ·RESPONSE/ROLLING (0.13 m/s on 7°), so an idle player's avatar never rested (it rolled 0.64 m in 5 s on 7°; parking found it). Its legs now hold it against gravity's pull along the ground while it stands (`avatar::steer`); its load is then the same every step and it sleeps. Tested on 7°, 15° and 30° (crept < 1 cm in 5 s, asleep); the old code fails the test.

## Measured (`bench-spikes --only lod`, this machine; min-spec = ×2)

| What | Measured | Budget (min-spec) |
|---|---|---|
| The LOD pass, 8 players, 400 bodies near them, 100,000 parked | 0.010 ms p50, 0.025 ms worst (0.05 projected) | 0.1 ms |
| The tick with 100,000 parked against none | +0.3 % (0.618 against 0.616 ms p50) | +5 % |
| Waking a parked yard of 1,000 touching crates | worst tick +0.43 ms (0.86 projected) | +2 ms |
| A woken sleeping body's cost per tick | about 0.2 µs | — |

## Tests

- `creeer_sim::lod_tests`: balls, islands (a stack, a lone crate, a row of ten; apart by more than the margin), which islands park (only resting ones, only beyond every player; with none, all), parked islands found nearest first, taken, counted.
- `creeer_server::world::lod_tests`: a resting stack 4 km away is parked in one island; the world without the pass, stepped as long, is the same for everything left in it; a player coming near brings the stack back bit for bit as the stack that never left; a tower toppling 4 km away is simulated through its fall (its top crate falls over 1.5 m) and parked only at rest; parked islands are saved and loaded (bytes identical, woken bodies identical); an offline player's avatar is parked and back where he left it when he returns.
- `creeer_sim::save::tests`: the version 6 fixture (two parked islands, water on a floater) reads back as written; a parked id repeating a live one is refused; version 5 reads as before.
- `creeer_sim::avatar::tests::it_stands_still_on_a_slope_and_sleeps`.

## Not done (in `docs/future-improvements.md`)

- **Far awake bodies that never rest** stay simulated (none in the sandbox; a perpetual machine far away would be a machine of F7.4).
- **A parked island under a terrain edit**: edits need a player within 500 m (ADR-0008), inside `ACTIVE`, so the ground under a parked island cannot change; a future long-range edit (an explosion, a landslide) would need to wake what stands there first.
