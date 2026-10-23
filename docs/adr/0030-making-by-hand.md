# ADR-0030: Making by hand, and the one generic tool

**Status:** accepted (2026-10-22, G1.6d done).

**Context:** G1's re-plan (`docs/g1-plan.md`, principles 9–13). A survival player starts with nothing and must make tools. The user asked for flexible, attribute-driven systems: a tool's properties come from what it is made of, not from a list of tools, and the same system later takes forged and cast heads.

**Builds on:** ADR-0027 (strikes, carrying), ADR-0028 (items), ADR-0029 (loose stones and branches).

**Evidence:** `creeer_sim::making` (`making/tests.rs`), `creeer_sim::items::assembly`, `Piece.edge`, `Item::Made`, `creeer_sim::strike::swing_of`, rocks' `grain` and `fracture_energy` (`geology/rocks.ron`), `spruce_bast` (`materials/engineering.ron`), `creeer_server::world::making` (`making_tests.rs`), `Command::Strip`, `Command::Bind` (protocol 16).

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Fracture mechanics of rock (Griffith; K_Ic compilations: Atkinson & Meredith 1987, Whittaker et al. 1992) | A crack grows when the energy released per new surface reaches G_c = K_Ic²/E | Taken: each rock's G_c; a blow breaks off the surface its energy pays for |
| Lithic technology (Cotterell & Kamminga, *Mechanics of Pre-industrial Technology*; experimental knapping) | Fine-grained, isotropic, brittle stone (flint, chert, obsidian) flakes to sharp edges; coarse stone crumbles; flake margins 20–45°; obsidian edges a few nanometres | Taken: grain size decides both an edge's sharpness (half a grain) and whether a flake crumbles |
| Comminution efficiency (Bond; drop-weight tests) | Only about a percent or less of the energy of breakage goes into new surface | Taken: a share of 0.5 %, calibrated so a 20 J hard-hammer blow takes a chert flake about 4 cm long |
| Hafting (Neolithic axes; bark and bast cordage) | Heads bound to hafts with bast or sinew; bindings loosen under blows | Taken: a head across the haft's end, bound with a bark strip whose hold is its strength × wraps × friction |
| Cross and Bower's swing law (ADR-0027) | Swing speed from the moment of inertia about the handle's end | Taken for any held thing, from its parts |

## Decision

1. **Edges are an attribute of any piece**: an included angle, a length and a tip radius. No edge is sharper than half its material's grain.
2. **A blow breaks a brittle piece by physics** (`making::knap`): the blow's collision energy (the reduced mass at the swing's speed) times 0.5 % buys new surface at G_c. The flake has two faces of L × 0.8L and a fifth of L's thickness, and takes at most a third of what is left. If the grains are coarser than a tenth of the flake's thickness, it crumbles to grit (a bulk of the rock); otherwise it comes off whole with a 30° edge, and the core keeps a 70° edge. The same rule knaps chert, crumbles granite and lets a steel pick break stone. Mass is kept exactly.
3. **One generic tool** (`items::Assembly`): parts placed in the tool's frame (the grip at the origin, the haft along +x, the head across it) and bindings. Its mass, its moment of inertia about the grip and its balance and reach follow from the parts' forms, so it swings by the same law as the catalogue's steel tools (`strike::swing_of`). A single piece in the hand is swung the same way, about its end.
4. **Bark** peels by hand from a fresh branch at least 6 mm thick, as a strip of bast 1.5 mm thick and half the branch's girth wide; the branch loses the strip's mass. A strip is flexible: in a pocket it takes the room of its coil.
5. **Binding**: a head is set across the haft's end and a strip wound about the joint. It holds 2 × wraps × the strip's strength × its section × 0.5 friction. A blow whose peak force (impulse over 2 ms of contact) passes that makes the tool come apart into its pieces.
6. **No recipe and no menu**: knapping is a blow with what the hands hold on a stone lying before you; R strips the branch in the hands; B binds the head in reach to the haft in the hands, with the first strip in a pocket. The left click strikes with what the hands hold; T throws it (it was the left click).
7. **The catalogue's tools are the model's test**: a steel axe made of a 1.6 kg steel head and a 0.5 kg hardwood haft gives Gränsfors Bruk's axe as `tools.ron` has it.

## Measured and rejected

| What | Measured | Why not |
|---|---|---|
| The hands' turn spring from the least diagonal entry of the inertia tensor (as G1.4f had it) | A made axe (its head off the haft's axis, so its axes are not principal) spun up to 190 rad/s in 25 ticks and was dropped | The least diagonal entry is above the least principal moment. Gershgorin's bound (each row's diagonal less its off-diagonals), never above it, holds the axe steady |

## Validation

- A 20 J blow on chert: a flake 3–6 cm long with a 30° edge of half a grain (5 µm), the core keeping an edge, mass kept to 10⁻¹². The same blow crumbles granite (grit, no edge), mass kept. A weak blow (0.05 J) changes nothing.
- Bark: the branch's loss is the strip's mass; a branch under 6 mm does not peel.
- A steel axe made of its parts against the measured one: mass within 3 %, balance, gyration and reach within 2 cm, swing energy within 5 %.
- A stone axe's hold is a whole number of wraps of its strip; a strip too short for two wraps binds nothing; the axe comes apart into pieces of exactly its mass.
- A stone in the fist swings harder than a pebble and softer than an axe (5–60 J).
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

The user's play: knapping made the game "incredibly laggy" and it froze after another strike. Measured: flakes of 1–3 cm as bodies settling on the terrain took 50–190 Newton iterations a tick each (the small-body contacts P1, G1.8, is to rework), 24–52 ms ticks with three flakes about.

**Decided:** an item that lies still is no body but laid where it lies, as the scatter's stones are (`world/laid.rs`): no cost per tick, told to clients (`ServerMessage::Laid`, protocol 19), drawn and aimed at with the scatter's, picked up and struck alike (a struck one is a body for its blow's second). What a blow breaks off, and its core, are laid at once; a small item body (its longest edge at most 0.3 m) that falls asleep, held by nobody and aimed at by no swing, is laid. Saved with the items (`SavedItems.laid`). Larger things stay bodies, to be walked into.

Measured after: three stones knapped in turn, the worst tick 2.1–4.7 ms and at most 43 Newton iterations (gated at 50 in `loose_tests.rs`); only the player and the hammerstone are bodies once each blow has fallen.

| What | Measured | Why not |
|---|---|---|
| Flakes and cores as bodies | 50–190 Newton iterations a tick each settling; 52 ms ticks | Nothing at rest needs simulating; P1's contact work will make small bodies cheaper, but laying them costs nothing at all |
| Laying a body asleep while a swing was aimed at it | The blow fell on nothing: the core lay unbroken | A body a swing is on its way to stays one until the blow |

Found in the same play: carried sticks spun up (the hands pulled at the grip, off a stick's axis; now at the centre of mass, the turn held through the full inertia: ADR-0027 amended), and stones lying on the ground could not be struck (scenery is no body: `Command::StrikeLoose` makes one where it lies).
