# ADR-0008: Networking and persistence

- **Status:** Accepted (2026-10-17)
- **Context:** F5 (networking & persistence). The plan asks for a client/server split with an in-process server for singleplayer, interest management, snapshot interpolation, prediction for a test avatar, replication of soft bodies (plastic deltas) and terrain edits, a dedicated server binary, versioned saves with migration tests, and bandwidth budgets met (at most 256 kbit/s per client in typical play). The server is authoritative (plan.md §1): clients ask, the server decides.

## Decision

### One server, three ways to reach it (`creeer_net::link`, `udp`; `creeer_server`)

- **renet 2.0** carries the game: a reliable ordered channel (handshake, commands, spawns, edits, deformation) and an unreliable one (snapshots, inputs). renet slices large reliable messages and resends what is lost; it is transport-agnostic.
- **Dedicated server** (`creeer_server` binary): renet over **netcode** UDP (`renet_netcode`). Connections are *unsecured* for now: a client names its own id and nothing is encrypted. That is enough on a LAN or among friends, not for a public server, which needs a token service (connect tokens, `ServerAuthentication::Secure`). The protocol id is `"CREEER"` and the protocol version, so builds that differ never connect.
- **Singleplayer**: the client hosts the same `Server` on a thread of its own (`host::Host`, 60 ticks a second, catching up at most a quarter of a second) and joins it through an **in-process connection** (`link::memory`): the same renet connection, its packets handed over through channels. The game cannot tell the difference, so singleplayer tests multiplayer.
- **Tests** put a **simulated link** (`link::Relay`: latency, jitter that reorders, loss; every byte counted with its headers) between the two.

### Rates, and the network's own clock

- The server steps at 60 Hz and sends every second tick (30 snapshots a second); a client sends its inputs every second tick too, each packet repeating the last 8 (130 ms: a lost packet loses none). renet's acknowledgements ride with them, so a client sends 30 packets a second, not 60.
- Snapshots are numbered by the server's **network tick**, which only counts up. The world's own tick is part of a save and goes back when one is loaded; a connection's baselines and its playback clock must not (loading a save overflowed the delta encoding before this).

### Messages and the codec (`creeer_math::codec`, `creeer_net::message`, `quant`)

- A hand-written binary codec: little-endian integers, floats bit for bit, LEB128 varints (zigzag when signed), length-prefixed strings. Every length is checked against the bytes left, so a corrupt count never allocates gigabytes, and decoding refuses non-finite numbers where they cannot occur. A general serialiser would not give that control of every bit, nor the validation.
- **Quantisation** of what snapshots send: positions to 1/1024 m as integers (a 500 km planet's surface fits ±2³¹), orientations as the three smallest quaternion components in 16 bits each (about 4·10⁻⁵ rad, seven bytes), velocities and spins to 1/256. Quantising is idempotent, so the server compares what it would send with what the client has bit for bit, and a body at rest costs nothing.
- **Inputs**: the walk to 1/128 m/s, each input written against the one before (a flag for "the next number" and one for "the same walk"): a byte for an input that walks on as before, 20 bytes for eight inputs.

### Snapshots, interest and the budget (`snapshot`, `interest`, `channel`)

- A snapshot is written against the newest snapshot the client **acknowledged** (it says so with its inputs): a body whose quantised state did not change costs nothing, one that moved its differences, one new to the client its whole state, one that left a removal. The server keeps 32 snapshots per client; the client keeps 32 and drops one whose baseline it lacks (the next is written against what it did acknowledge).
- **Priority**: bodies are taken by accumulated importance ((1 + speed)/(1 + distance/10 m), added every round they wait) until the snapshot's budget is spent; those left out keep the state the client has and wait with a higher priority. No body waits for long (tested: 300 moving bodies, a 1 kB budget, none stale for 20 rounds).
- **Interest**: entities are binned into 32 m cells each round; a client's view is what lies within 600 m of its avatar, and what it has stays until 660 m (no churn at the edge). Spawns carry what a client needs to draw an entity (its shape, whose avatar it is, a structure's frame); despawns, edits and deformation go reliably.
- **The budget is a token bucket** (`channel::Pacer`) filling at 32,000 bytes a second on the wire, packet headers counted (IPv4, UDP, netcode, renet: 64 bytes a packet), holding a fifth of a second. Reliable messages wait in the server's per-client outbox until the bucket pays for them (a large one may put it in debt); the snapshot takes the rest, never less than 192 bytes, so the world keeps moving while a joining client is sent a large structure. renet itself is let send whatever it is given.
- **Measured** (test `in_typical_play_a_client_is_sent_less_than_its_budget`: 120 crates and balls and two steel frames around the player, 20 s of walking a wide circle through them, dropping a crate every 2 s and digging every 5 s, over a 40–50 ms link losing 1 %): **157 kbit/s to the client, 46 kbit/s from it**, every entity in view known to the client and every resting one drawn within a centimetre of where it rests.

### Drawing the world in the past (`interp`)

- Entities are drawn 4 ticks (67 ms: two snapshots) behind the newest snapshot, on the cubic **Hermite** curve through the two snapshots around that moment with their velocities (a thrown crate keeps its parabola between snapshots, exact to the quantisation), orientations along the shortest arc. Past the newest snapshot an entity runs on at its velocity for at most 6 ticks, then waits.
- The playback clock advances with the client's time and is pulled a tenth of the way towards the newest snapshot's tick each frame: smooth however packets arrive; a stall of half a second restarts it.

### The player's own avatar, predicted (`predict`, `creeer_server::predicted`, `creeer_sim::avatar`)

- **The avatar** is a ball (0.4 m, 80 kg) the player rolls: a force at its centre closes the difference to the walk asked for in 0.15 s, at most 9 m/s² standing (1.5 in the air); a walk is along the ground's plane, and a slope steeper than 50° is not stood on. A jump is the push of the legs, over in less than a step: an **impulse** to 4.5 m/s, with the opposite impulse on what it stood on (`World::apply_impulse`). As a one-step load through the implicit integrator it delivered only two thirds of the impulse, and landing with jump held doubled it. An avatar asked for nothing is braked until it is still enough to sleep; one whose player left is steered with an empty input (the last push would carry it on for ever).
- **Whether it stands** is asked of the state as it is (`World::touching`: the ground, fixed geometry and other bodies within 1 cm, by the narrow phase), not of the last step's contacts: a client that sets its avatar back to the server's state has another state's contacts, and jumps were applied on one side and not the other (corrections of 0.9 m before this).
- **Prediction**: each input is quantised as it is made, applied to the client's own avatar at once (alone on the client's terrain, in a physics world of its own), kept, and sent. The server applies one input a tick from a **jitter buffer** that waits for 2 inputs before it starts, grows by one (to 6) each time it runs dry and shrinks after 10 s with two to spare (dropping one, which the client then corrects), holding at most 10. Each snapshot says the newest input applied; the client sets its avatar to the server's state, drops the inputs applied and applies the rest again. A correction slides out over 0.1 s (one past 2 m is a jump, drawn at once).
- **Measured** (test `over_a_slow_lossy_link_the_avatar_is_rarely_corrected`: 100–130 ms each way, 5 % loss, 10 s of walking and jumping): of 310 reconciliations 14 moved the avatar by more than a centimetre, none by more than 3 cm.
- The prediction world holds only the avatar and the terrain: touching a crate or another player shows as corrections, slid out. Putting nearby bodies into it as obstacles would remove most of those (later).

### Authority: what a player may ask (`creeer_server::world`, `server`)

- A command acts only within 500 m of the asking player's avatar and at finite points; an edit is one edit with a brush of at most 10 m (tunnels at most 60 m long); a client gives at most 5 commands a second (10 at once); a name has 1–32 characters and is not playing already; a client of another protocol version is told why and disconnected.
- **Placement**: anything put into the world is lifted along the vertical until it is clear of the ground and of other bodies (by the narrow phase: a frame put on a slope had a joint 13 cm in the ground and was blown apart; crates were spawned inside frames).
- Players own avatars by **name**: one who comes again gets the avatar they left, where they left it.

### Structures: shapes that change for good (`creeer_server::replicate`)

- A structure is drawn from its node 0's state (snapshots) and its nodes' places relative to it in its frame (sent at spawn). Nodes that moved by more than **1 cm** since they were last told are sent again, at most every 6 ticks, to everyone who has it; a member list that changed (a member buckled and split) is sent whole, and members newly broken by index. Every client is told the same shape (the one the server keeps as told), so the changes fit what each has: a frame bent in a crash is drawn bent (within 1.2 cm, tested), a broken one broken, while a frame swaying by a millimetre is drawn rigid.

### Terrain edits

- Edits go to every client (the terrain is everyone's), 16 to a message, in order: `Edits { first, log }` means "keep your first `first` edits, these replace the rest", so a client joining gets the whole log and a client whose world was reloaded gets it again from 0.

### Saves (`creeer_physics::world::save`, `creeer_sim::save`)

- **Version 2** (F5): `CREEERSV`, the version, then tagged sections: `META` (planet data file, tick, next entity id), `EDIT` (the terrain's edit log), `WRLD` (the physics world), `ENTS` (each body's entity id and owning player). A section a version does not know is skipped; a later version's file is refused ("saved by a later game"); a damaged one is refused with where, never a crash (tested with cuts and random byte changes).
- **The physics world** is saved with everything that carries from step to step: every body's structure (nodes, members with their plastic state, broken and crooked flags, splits), its solver's history (previous positions, velocities, orientations, spins, Newmark accelerations, the members waiting to split), shapes (hulls whole), surfaces, sleep state, and the friction anchors. What a step finds again (contacts, groups) is left out. A solver's factorised matrix is a cache, but a preconditioner reused across steps, so a rebuilt one changes results at the solver's tolerance, and a crushing frame amplified that to a millimetre in two seconds. So the solver records **what its matrix was built from** (the shape then and which members were whole) and the loaded solver builds the very same matrix: **a loaded world goes on bit for bit as the saved one** (tested), and saving it again gives the same bytes.
- Decoding checks what the physics needs: counts that agree, indices in range, masses positive, every number within 10¹⁸ and every rotation a unit quaternion, and a matrix that can be factorised (else "damaged"). A save is written beside its file and moved over it, so a crash while saving leaves the last save as it was.
- **Version 1** is the F3 sandbox's edit log (`CREEDITS`): read as the reference planet with those edits and nothing standing on it. The tests keep a fixture of each version (version 1 written byte by byte from its format's description, version 2 as this version wrote it); a change to how a version is written that would not read its fixture fails, so it must be a new version with a migration.

### Fixed on the way (found by these tests)

- **Removing bodies** from a world (the sandbox's "remove bodies", a vessel leaving the physics bubble) left the friction anchors of the bodies after them pointing at the wrong bodies (a crate on a ramp was dragged 3.8 mm), and the energy of the last step's groups indexing past the end. `World::retain_bodies` and `World::remove_body` renumber the anchors and drop the rest.
- **A structure built away from where it is** (a frame built about the origin, the planet's centre, and put on the surface) had its gravitational potential measured from its rest shape, infinitely deep at the centre: its energy was infinite, which **blinded the energy guard** for it and made the impact budget NaN (a crushed frame's velocities became NaN). Bodies added to a planet's world have their rest shape moved, whole, to where they are.
- **Settling a buckled member's new joint** (Gauss–Newton) ran away in a frame already crushed out of shape (a joint flung 10⁶ m): steps are now limited to a tenth of the longest member and 0.1 rad, and the joints end where the residual was least.
- **Factorising a matrix that cannot be made positive definite** looped for ever on a non-finite or empty diagonal; it now fails (and a structure read from a damaged save is refused).

## Consequences

- Singleplayer runs the same code as multiplayer; its cost is a second copy of the terrain's caches (the server's and the client's) and a server thread.
- A dedicated server needs the planet's data files (and so does every client: they stream the terrain themselves from the same generator and the edits).
- Unsecured connections: a public server needs connect tokens first.
- Structures' elastic vibration is not drawn on clients (only lasting changes of shape over a centimetre); a structure's deformation shows within 0.1 s.
- The prediction world holds the avatar and the terrain only: bumping into things is corrected, not predicted.

## Amended in F7 (2026-10-18): the avatar stands on slopes; saves of version 6

- **The test avatar holds itself on slopes.** Braked by a push proportional to its velocity, an idle avatar settled where that push equalled gravity's pull along the ground: it crept downhill for ever (0.13 m/s on 7°) and never slept. Standing, its legs now cancel gravity's pull along the ground (`avatar::steer`); it rests and sleeps on 7°, 15° and 30° (ADR-0019, where parking idle avatars found it).
- **Clients send the hash of their data packs** in their hello; a server with other data refuses them, naming its packs (protocol version 2, ADR-0018).
- **Save version 6** adds the parked islands (`PARK`, ADR-0019).

## Amended in G1 (2026-10-18): the view found from occupied cells

- **A client's view cost 3 ms a send.** G1.0's play harness, the first to run eight clients through real sessions, found the server's tick at 25.8 ms p50 with eight players: each session's view (`interest::Grid::around`, 600 m in 32 m cells) looked up about 55,000 cells, twice. The grid now visits its occupied cells instead whenever there are fewer of them than the view's box spans, and a session searches once (the wider radius) and takes the nearer set from it. The same entities in the same order: the play scene's state hash is unchanged, and the tick fell to 0.65 ms p50.

## Amended in G1.3c (2026-10-21): ages; protocol 7, save version 9

- **Ages** (`creeer_sim::ageing`, ADR-0026). The server keeps each entity's weathering (its exposure as of a tick, the rate it grows at since) and its wear from impacts, by entity, so parking and reordering bodies leave it alone.
- **Protocol 7** adds `ServerMessage::Age`: an entity's exposure as of a network tick, its rate and its wear. It is queued right after the entity's spawn (and dropped with the spawn if it leaves view first), and again when the rate or the wear changes. A client works out the exposure at any moment from the rate: nothing is sent per tick.
- **Save version 9** adds the optional section `AGES`. Earlier saves load with everything new.

## Amended after the user's play (2026-10-28): changes drawn at their round; protocol 30

- **Changes to the drawn world wait for their round.** A client draws the world 4 ticks in the past (`interp`), but applied what came reliably at once: a felled part limbed, despawned and replaced by a new body in one tick, was drawn by none for 3 ticks, since the new body showed only from its first snapshot (ADR-0031, amended, has the measurements). As Unity's Netcode for Entities does for its interpolated ghosts, the client now holds despawns, things laid and taken, and trees cut until the snapshot whose bodies it draws is the one they went with (`creeer_server::client::held`). Spawns, edits, inventories and notes still take effect at once (an edit's ground is the prediction's, at the present).
- **Protocol 30** adds `ServerMessage::Round(tick)`, sent before the first reliable message of a round (5 bytes, paced with them): the snapshot of `tick` went with what follows. The reliable channel keeps their order through losses and resends, so a held message's round is exact.
- **Cost:** play scenes' bandwidth unchanged at the kbit/s shown (crowd 95 / 38.6 down / up); every play and stress hash unchanged by it.
