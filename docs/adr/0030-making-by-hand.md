# ADR-0030: Making by hand, and the one generic tool

**Status:** accepted (2026-10-22, G1.6d done).

**Context:** G1's re-plan (`docs/g1-plan.md`, principles 9â€“13). A survival player starts with nothing and must make tools. The user asked for flexible, attribute-driven systems: a tool's properties come from what it is made of, not from a list of tools, and the same system later takes forged and cast heads.

**Builds on:** ADR-0027 (strikes, carrying), ADR-0028 (items), ADR-0029 (loose stones and branches).

**Evidence:** `creeer_sim::making` (`making/tests.rs`), `creeer_sim::items::assembly`, `Piece.edge`, `Item::Made`, `creeer_sim::strike::swing_of`, rocks' `grain` and `fracture_energy` (`geology/rocks.ron`), `spruce_bast` (`materials/engineering.ron`), `creeer_server::world::making` (`making_tests.rs`), `Command::Strip`, `Command::Bind` (protocol 16).

## Prior work

| Source | What it does | For CreÃ«er |
|---|---|---|
| Fracture mechanics of rock (Griffith; K_Ic compilations: Atkinson & Meredith 1987, Whittaker et al. 1992) | A crack grows when the energy released per new surface reaches G_c = K_IcÂ²/E | Taken: each rock's G_c; a blow breaks off the surface its energy pays for |
| Lithic technology (Cotterell & Kamminga, *Mechanics of Pre-industrial Technology*; experimental knapping) | Fine-grained, isotropic, brittle stone (flint, chert, obsidian) flakes to sharp edges; coarse stone crumbles; flake margins 20â€“45Â°; obsidian edges a few nanometres | Taken: grain size decides both an edge's sharpness (half a grain) and whether a flake crumbles |
| Comminution efficiency (Bond; drop-weight tests) | Only about a percent or less of the energy of breakage goes into new surface | Taken: a share of 0.5 %, calibrated so a 20 J hard-hammer blow takes a chert flake about 4 cm long |
| Hafting (Neolithic axes; bark and bast cordage) | Heads bound to hafts with bast or sinew; bindings loosen under blows | Taken: a head across the haft's end, bound with a bark strip whose hold is its strength Ã— wraps Ã— friction |
| Cross and Bower's swing law (ADR-0027) | Swing speed from the moment of inertia about the handle's end | Taken for any held thing, from its parts |

## Decision

1. **Edges are an attribute of any piece**: an included angle, a length and a tip radius. No edge is sharper than half its material's grain.
2. **A blow breaks a brittle piece by physics** (`making::knap`): the blow's collision energy (the reduced mass at the swing's speed) times 0.5 % buys new surface at G_c. The flake has two faces of L Ã— 0.8L and a fifth of L's thickness, and takes at most a third of what is left. If the grains are coarser than a tenth of the flake's thickness, it crumbles to grit (a bulk of the rock); otherwise it comes off whole with a 30Â° edge, and the core keeps a 70Â° edge. The same rule knaps chert, crumbles granite and lets a steel pick break stone. Mass is kept exactly.
3. **One generic tool** (`items::Assembly`): parts placed in the tool's frame (the grip at the origin, the haft along +x, the head across it) and bindings. Its mass, its moment of inertia about the grip and its balance and reach follow from the parts' forms, so it swings by the same law as the catalogue's steel tools (`strike::swing_of`). A single piece in the hand is swung the same way, about its end.
4. **Bark** peels by hand from a fresh branch at least 6 mm thick, as a strip of bast 1.5 mm thick and half the branch's girth wide; the branch loses the strip's mass. A strip is flexible: in a pocket it takes the room of its coil.
5. **Binding**: a head is set across the haft's end and a strip wound about the joint. It holds 2 Ã— wraps Ã— the strip's strength Ã— its section Ã— 0.5 friction. A blow whose peak force (impulse over 2 ms of contact) passes that makes the tool come apart into its pieces.
6. **No recipe and no menu**: knapping is a blow with what the hands hold on a stone lying before you; R strips the branch in the hands; B binds the head in reach to the haft in the hands, with the first strip in a pocket. The left click strikes with what the hands hold; T throws it (it was the left click).
7. **The catalogue's tools are the model's test**: a steel axe made of a 1.6 kg steel head and a 0.5 kg hardwood haft gives GrÃ¤nsfors Bruk's axe as `tools.ron` has it.

## Measured and rejected

| What | Measured | Why not |
|---|---|---|
| The hands' turn spring from the least diagonal entry of the inertia tensor (as G1.4f had it) | A made axe (its head off the haft's axis, so its axes are not principal) spun up to 190 rad/s in 25 ticks and was dropped | The least diagonal entry is above the least principal moment. Gershgorin's bound (each row's diagonal less its off-diagonals), never above it, holds the axe steady |

## Validation

- A 20 J blow on chert: a flake 3â€“6 cm long with a 30Â° edge of half a grain (5 Âµm), the core keeping an edge, mass kept to 10â»Â¹Â². The same blow crumbles granite (grit, no edge), mass kept. A weak blow (0.05 J) changes nothing.
- Bark: the branch's loss is the strip's mass; a branch under 6 mm does not peel.
- A steel axe made of its parts against the measured one: mass within 3 %, balance, gyration and reach within 2 cm, swing energy within 5 %.
- A stone axe's hold is a whole number of wraps of its strip; a strip too short for two wraps binds nothing; the axe comes apart into pieces of exactly its mass.
- A stone in the fist swings harder than a pebble and softer than an axe (5â€“60 J).
- In the world: a hammerstone's blow on a chert core lying before the player breaks a flake off and loses no mass; stripping puts the strip into a pocket; binding puts a made axe in the hands with nothing lost; a made axe with no binding to speak of comes apart on a blow.

## Cost

A blow's breaking is a few arithmetic operations and two bodies replaced. Nothing runs per tick.

## Limits

- The core's shape stays an ellipsoid scaled down; flakes do not come from where the blow fell.
- The striking stone does not chip itself.
- Grinding (ground stone axes of tough rocks) is not built.
- Bark strips any fresh branch the same way, whatever its species or season.
- A made thing is one hull for the physics and is drawn as its bounding box in its head's colour.
- Bindings do not loosen gradually: they hold or come apart.

Each is in `docs/future-improvements.md`.

## Amended after the user's play (2026-10-23): small things at rest are scenery

The user's play: knapping made the game "incredibly laggy" and it froze after another strike. Measured: flakes of 1â€“3 cm as bodies settling on the terrain took 50â€“190 Newton iterations a tick each (the small-body contacts P1, G1.8, is to rework), 24â€“52 ms ticks with three flakes about.

**Decided:** an item that lies still is no body but laid where it lies, as the scatter's stones are (`world/laid.rs`): no cost per tick, told to clients (`ServerMessage::Laid`, protocol 19), drawn and aimed at with the scatter's, picked up and struck alike (a struck one is a body for its blow's second). What a blow breaks off, and its core, are laid at once; a small item body (its longest edge at most 0.3 m) that falls asleep, held by nobody and aimed at by no swing, is laid. Saved with the items (`SavedItems.laid`). Larger things stay bodies, to be walked into.

Measured after: three stones knapped in turn, the worst tick 2.1â€“4.7 ms and at most 43 Newton iterations (gated at 50 in `loose_tests.rs`); only the player and the hammerstone are bodies once each blow has fallen.

| What | Measured | Why not |
|---|---|---|
| Flakes and cores as bodies | 50â€“190 Newton iterations a tick each settling; 52 ms ticks | Nothing at rest needs simulating; P1's contact work will make small bodies cheaper, but laying them costs nothing at all |
| Laying a body asleep while a swing was aimed at it | The blow fell on nothing: the core lay unbroken | A body a swing is on its way to stays one until the blow |

Found in the same play: carried sticks spun up (the hands pulled at the grip, off a stick's axis; now at the centre of mass, the turn held through the full inertia: ADR-0027 amended), and stones lying on the ground could not be struck (scenery is no body: `Command::StrikeLoose` makes one where it lies).

## Amended after the user's second play (2026-10-23): made things drawn as their pieces; nothing found by trying

The user: a made tool was "a square" (drawn as the box about it, in its head's colour), and "it feels like you have to randomly click at things until it works". Found: binding took only a body as the head, but flakes now lie as scenery, so a flake could never be bound and nothing said why.

- **Drawn as pieces:** a made thing's body is told to clients with its pieces (`ServerMessage::Parts`, after its spawn) and a laid one with them (`Laid.parts`): the haft a stick and the head a stone, each in its material's colour.
- **Told, not found by trying:** under the crosshair, what the hands hold (its size, mass and edge: "an edge 4 cm long, 30Â°, sharp"), the keys that would act on it and on what the crosshair is on ("click: strike it Â· R: strip bark Â· B: bind it on as a head"), and what came of the last act (`ServerMessage::Note`: "a flake came off: â€¦", "it crumbled: 12 g of grit", "it only bruised", "bound: â€¦; it holds 2,400 N") or why it was refused. The keys, not recipes: which stone makes a good head stays the player's to know.
- **Binding checks its parts:** the haft a stick of 25 cm or more, the head a stone (a rock, not a strip), the strip from a pocket or lying in reach; a lying head is bound by its id (`Command::BindLying`, protocol 20).

## Amended in G2.3 (2026-10-23): split hafts, and every cutting blow's load alike

The user's play: "when i strike a felled tree the tool breaks".

**Found:** the three ways a made tool cuts loaded its binding three ways.
- A dig loaded it with the edge stopping over the depth it went in (`dig::peak_force`, G1.5).
- Felling a standing tree loaded it with nothing.
- A blow on a felled part loaded it with its whole impulse stopped in 2 ms. On a trunk of 100 kg that is several kN, so every blow broke the tool.

**Now** all three load it with `dig::peak_force`: the energy over the depth it cut, never more than the impulse in 2 ms. Measured: a 69.5 J blow cutting 3.7 cmÂ³ of green spruce, about 1.5 cm deep, gives 3.0 kN. A flake only lashed across a stick's end holds 2.4 kN.

**The user chose split hafts.** `making::bind_split`: a head thinner than the haft's radius goes into the haft's split end, as Neolithic axes were hafted (the Ã–tzi axe's forked yew; split-hafted adzes). A blow drives it against the split's two walls, which bear across the grain over about the haft's diameter squared each: f_c,90Â·2Â·dÂ². That comes on top of the lashing's friction, which keeps the split closed. A thicker head is only lashed on, as before.
- A 12 mm chert flake in a 36 mm green spruce haft (1.8 MPa across) holds 2.4 + 4.7 kN.
- It cuts a felled crown's stem through in 221 blows, its binding holding all the while (`made_tools_tests.rs`, 20 blows in CI).

Toolmaking's play hashes changed: its tools' saved holds include the wedge.

## Amended in G2.8 (2026-10-24): a head as thick as the haft

The user's play: "tools brake again after one strike", "hitting a tree also makes the tool brake instanly".

**Found:** knapping gives a flake a fifth as thick as it is long (decision 1), so a 10 cm flake is 20 mm thick: thicker than a 36 mm haft's radius. Such heads were only lashed on (about 1.9 kN), and every blow on wood, standing or felled, passes that. The G2.3 test used a 12 mm flake, which no 6 cm-or-longer knapped flake is.

**Now** a head no thicker than the haft goes into its split end: the two halves spring apart about it and the lashing shuts them. A thicker head is still only lashed on. `made_tools_tests.rs` strikes a standing tree and a felled stem with a 10 Ã— 8 Ã— 2 cm flake; under the old rule both came apart on their first blows on wood.

A made tool keeps the hold it was bound with: tools bound before this keep their lashing-only hold until bound anew.

**Still broke** (the user's play, a tool bound to hold 6+ kN): the split's walls bore over the haft's diameter squared, so a bigger head held *less* (the lashing's wraps shorten about it) while it is swung harder. A 15 cm flake on a 1 m haft took 6–8 kN a blow into a tree on a 7 kN hold. **Now** the walls bear over the face of the head they hold, the haft's diameter by the head's width along the haft: f_c,90 · 2 · d · w. A 10 cm flake in a 36 mm haft holds 10.4 kN wedged; a blow into a tree loads it 3–3.5 kN. `making/tests.rs` checks flakes of 6–15 cm on hafts of 0.4–1.2 m against their own blows into 5 MPa wood. The note when a binding gives now says the blow's force and the hold.

**And still** (the user's play, a tool bound to hold 6 kN broken by its first blow): a knapped flake is small (3 × 2 × 1 cm off a fist-sized core), and in the split end it reached a little less far than the haft's end. What strikes was the part farthest out, so the haft's blunt end struck: "it only bruised the bark", and a blow that cuts nothing is stopped in a millimetre, its whole impulse on the binding. The tests above had bound a 10 cm flake, which no hand-knapped core gives. **Now** the farthest-out part with an edge strikes, if any has one. `made_tools_tests.rs` now also makes the tool as a player does (a core knapped three times, a branch stripped, the sharpest flake bound) and strikes a standing tree: it cuts a notch every blow and holds.

**And the share the binding bears** (the user's play: a 14 × 10 × 5 cm core, 0.8 kg, lashed across a 1.19 m, 44 mm C24 stick, held 6.6 kN and broke at its first blow into a tree). What a blow strikes stops the head; the binding only stops the rest of the tool behind it. The head taken as its mass at the striking point and the rest as its own effective mass there: J = F · m_rest / (m_rest + m_head) (`making::binding_share`). Every cut had loaded it with all of F. That tool's blow: 268 J, 11.1 kN at its edge, 1.7 kN on its binding of 6.6 kN. A light flake on a heavy haft still puts nearly all of F on its binding.

## Amended after the user's play (2026-10-28): a stone struck as it lies is a body from its blow on

The user: "hitting a stone with another caused 30 ms frames".

**Measured first, on the real path.** `creeer_client --play-bench knap` plays a singleplayer game as the window does: the server hosted on its own thread in real time (`host.rs`), the client's frames paced at 60 a second, each frame's phases timed. In *toolmaking*'s world the player takes up the lighter stone and strikes the core every 2.2 s. While each swing was on its way, the client's session (taking in snapshots, its prediction set back and replayed) took 7–11 ms a frame, at worst 16–52 ms. Frames ran 20–26 ms, at worst 30–73 ms: 180 of 885 frames over 16.7 ms. Rendering, the trees and loose things, the snapshot and the aim took what they take with no stone struck. The hosted server's ticks were 0.1–2 ms, on their own thread: not the frames' cost.

**Found** (`play/client_probe.rs`: one player knapping in lockstep, and the play harness now timing the clients' own work, `Tick::client_ms`):

1. `Command::StrikeLoose` made the stone a body where it lay when the swing began, a second before its blow. A laid thing rests box-high on its plumb line (`lay`), not where its hull would rest. On *toolmaking*'s slope the first core, as a body, tipped (its spin growing 1.22 times a tick, from 0.0015 to 1.07 rad/s) and rocked for the whole second. Cores laid again after a blow dropped 1–2 cm and settled for a quarter of one.
2. The client predicts the bodies within 6 m of its avatar (ADR-0025). A moving body's snapshot never agrees with the prediction within the quanta, so every snapshot set the prediction back and replayed the inputs the server had not yet applied (2 in lockstep, 3 in real time), the stone with them.
3. A step of the avatar with a 62-cornered lump on the ground costs about 1 ms: the hull's ground sampling (its faces' and edges' points against the terrain, `ground_hull.rs`) took 1.3 ms of detection a step in the clients' profile. Replayed, that is 3–4 such steps a tick.

**Now** the swing is aimed at the thing as it lies (`Aim::Lying`). It is made a body when the blow falls, if it still lies there and its top is within reach (`rouse_struck`). Nothing moves a stone lying still, so a body for the swing's second simulated nothing real; from its blow on it moves as struck. A blow that breaks it lays the core and what broke off in the same tick, so it is never a body at all: no step and nothing to predict. A blow that only bruises it leaves a body moving with the blow's impulse, laid once it rests.

| Knapping | Before | Now |
|---|---|---|
| Real path, the client's session a frame (`--play-bench knap`, two runs) | 7–11 ms while a swing was on its way; worst 21–105 ms | worst 0.5–0.95 ms; p99 0.07–0.10 ms |
| Real path, frames | p99 60.7 ms; 180 of 885 over 16.7 ms | p99 11.1–13.8 ms; 1–4 of 965 over (the render's spikes, the machine shared) |
| *toolmaking*'s knapping (ticks 0–800, lockstep): a client's worst tick | 7.99 ms, 120 Newton iterations | 0.44 ms, 6 |
| Three stones knapped in turn (`loose_tests.rs`): the world's most Newton iterations in a tick | 43 | 6 (gated at 7, was 50) |

| What | Measured | Why not |
|---|---|---|
| The struck stone made a body as the swing begins, placed at its hull's rest rather than box-high | Not built: a hull's rest on sloped, bumpy ground is found only by settling it (the cost found above), and a body put to sleep where it does not rest would hover or sink until struck | A stone lying still needs no body until something moves it |
| Predicting only the bodies the avatar could touch within the replay (Unity's prediction switching, ADR-0025's prior work) | Not built: the core lies 0.7 m from the knapper's hands, within a lunge's reach in the replay's 0.17 s | It would not have spared the knapper; the bubble is ADR-0025's to change |

**Tests:** `a_stone_lying_struck_with_a_stone_in_hand_breaks_where_it_lies` (no body before the blow; taken from the ground when it falls) and `knapping_stones_lying_about_costs_the_world_next_to_nothing` (6 Newton iterations at most, gated at 7); `every_player_makes_a_tool_and_digs_with_it` gates a client's prediction while *toolmaking*'s players knap (6 Newton iterations a tick at most, gated at 7; 120 before). Each play scene's clients' prediction work is gated (`play/tests.rs`, `CLIENT_WORK`), and `--play-bench knap --gate` fails a frame whose own work passes 6 ms on the development machine.

**Goldens:** *toolmaking*'s play hashes renewed at both lengths: its cores are no longer bodies during the swings, so they no longer rock and settle on the server. Its most work (137 at tick 877, a hammerstone put down) is unchanged, and every other scene hashes as before.

**Still costly** (found here; in `docs/future-improvements.md`): a stone put down from the hands falls and settles for about a second near the player, and the client predicts it as above. That is 2–8 ms of client work a tick for one player in lockstep (*toolmaking*'s four players' put-down: 226 Newton iterations and 17 ms in a client's worst tick), and 0.8 ms a tick while it lies at rest awake, until it sleeps.
