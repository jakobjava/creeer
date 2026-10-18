# G1 plan: player & interaction

Status: **planned** (2026-10-18). Branch `g1`, squash-merged into main as "G1" at the end.

G1 is the first gameplay milestone. The loop it makes playable: **land on the planet, walk to a forest and an outcrop, fell a tree, dig ore, carry both home, with friends.** It turns F5's test ball into a person with a body, gives that person hands and tools, makes the planet's rock and trees something to take apart, and gives the game its look.

plan.md's G1: the character (G1.1), interaction (G1.2), mining with real composition (G1.3), trees (G1.4), inventory (G1.5), HUD and first vitals (G1.6), performance package P1 (piles). This plan orders and details them, and adds what they need first: joints in the physics core, a harness for play scenes, and the art direction with an asset pipeline.

## Principles

1. **A body, not a controller.** The character is a physics body like any other, with real mass. It walks by pushing on the ground through its feet, so whatever it stands on is pushed back. Friction limits the slopes it can climb, and its strength limits what it lifts. No kinematic shortcuts: the server steers every character, and its own client predicts it, with the same code and the same inputs (ADR-0008).
2. **Harm from mechanics.** Injury comes from what the body undergoes (peak acceleration, impulse, the head-injury criterion), not from a height or a speed threshold. A 3 m fall onto concrete hurts more than one onto soil, because the contacts carry the materials' stiffness (ADR-0005).
3. **What you dig is what was there.** A dug volume yields the mass and mineral composition that the geology has at that place: the deposit models' grades, with mass conserved exactly between the terrain and the items.
4. **Trees are structures.** A tree is beams of real wood. It falls the way its cut and its weight make it fall, and it breaks where its stresses exceed the wood's strength.
5. **Cheap until touched.** A forest is scenery: instanced, static and costless per tick. A tree becomes a physics structure only when something cuts it, strikes it or leans on it. The same goes for loose boulders. Cost follows the players (F7).
6. **Factorio's vibe in 3D** (below, "Art direction"). Readable first: every material has its own colour, silhouettes are clear, objects are grounded by soft shadows and ambient occlusion, and shapes are chunky and believable. Every new thing in G1 is built in this style from its first version: no programmer art left at the end.
7. **Gated play scenes** (plan.md, Phase B): every scene's worst tick ≤ 5 ms on the development machine (aim 2–3 ms), its p99 ≤ 2 ms, and its bandwidth ≤ 256 kbit/s per client.

## Art direction: Factorio's vibe in 3D

What makes Factorio's look, and how Creëer carries it into 3D: a summary. The rules themselves (palette, limits, checks) are in **`docs/art-style.md`**, the authority, which every asset is checked against.

- **Readability over realism of surface.** You can tell what a thing is, and what it is made of, at a glance from the play camera. Colour belongs to the material: rocks already carry their colours in `rocks.ron`, and parts take the colour of their material. Silhouettes are distinct: no two tools share an outline.
- **A grounded, earthy, industrial palette.** The terrain is muted and earthy (as it is now). Objects sit slightly brighter and more saturated than the ground they stand on, so they read against it. Saturated accents are saved for what matters: hazard yellow and black, glowing hot metal, a player's own colour.
- **Soft detail, no photos.** Surfaces get procedural detail (triplanar noise, as the terrain already has), wear and grime on edges and in creases (from curvature and ambient occlusion), and never photographic textures.
- **Weight and contact.** Strong sun shadows, ambient occlusion (SSAO, new in G1.3) and contact shadows ground everything; Factorio's pre-rendered look owes much of its density to them. The tonemapper keeps authored colours (ADR-0003, Khronos PBR Neutral).
- **Chunky, mechanical, believable shapes.** Things look like what they are and how they are made: bevelled edges that catch the light, bolts, rivets, welds, handles. Proportions are slightly exaggerated (thicker handles, a chunkier suit) so they read at a distance.
- **Dense, lived-in ground.** Pebbles, grass tufts, bushes and fallen branches, instanced in their thousands, so the planet never looks empty between the trees.
- **The engineer.** The character wears a chunky work suit and helmet, readable as a silhouette, with an accent in the player's own colour (as Factorio colours its players).
- **Icons from the models.** Every item's icon is rendered by our own renderer from its 3D model, in one fixed three-quarter view and lighting: consistent, like Factorio's icons, and none drawn by hand.
- **The UI.** In Factorio's spirit, not a copy of it: dark, warm grey panels, bevelled buttons, one warm highlight colour, and dense tooltips full of real numbers (mass, composition, temperature).

## Sub-milestones

### G1.0 The play-scene harness

**Done** (2026-10-18): `creeer_server::play` (a server and scripted players over counting in-process links; each tick's server time and Newton work, both ways' bytes, an end-state hash), recordings (`play::record`, written by the clients themselves while `Connection::record` is on), the scenes (`play::scenes`, *crowd* first with F5's avatars), `creeer_tools play` (`--short`, `--check`/`--save-golden` against `tests/scenarios/play.txt`, `--gate`, `--metrics`, `--record`/`--replay`, `--profile`, `PLAY_TRACE`), CI (hashes on every OS; work and bandwidth in `play/tests.rs`) and `cargo xtask perf` (time gates). Found on the way: each client's view cost 3 ms a send (ADR-0008 amended): *crowd* 25.8 → 0.65 ms p50. *crowd* stands at worst 4.0 ms, p99 2.9 ms (gate 2): its slow ticks are the eight ball avatars pressed together leaving the spawn, one group's contact solve (P1, G1.8). A client binary's `--record` comes with G1.10's recorded scenes.

- **Scripted play.** A play scene is a server world plus recorded inputs for N simulated players: walk here, strike there, pick up this. It runs headless, with no client. The inputs are the network's own messages (`AvatarInput`, `Command`, and those G1 adds), so a scene exercises the real server path.
- **What it measures.** Every tick's time (worst, p99, p50), the Newton iterations of the worst tick (the work gate, as in `physics_bench_tests.rs`), every client's bandwidth (through the real replication), and a state hash at the end (determinism).
- **Where it runs.** `creeer_tools play <scene>` and `cargo xtask perf` (time gates); the work gates in CI.
- **Validation:** the same scene gives the same hash twice and on all three OSes; a scene recorded from a live client replays to the same state.

### G1.1 Joints (`creeer_physics`, ADR-0022)

The physics has contacts and welded members, but no articulation. The ragdoll needs it, and so do carrying, ladders, and in G2 the hinges, pins and bearings.

- **Kinds.** A ball joint (three rotations free) and a hinge (one free), each with angle limits (a cone and twist, or a range), joint friction, and an optional drive (a target angle with a strength limit: muscles, later motors). They join two bodies, or a body and the world.
- **Solved within the implicit step.** A joint is part of the step's energy minimisation (ADR-0004/0005), as an exact constraint, not a soft spring tuned to look right. The method (eliminated coordinates, or an augmented Lagrangian on the existing Newton) is decided in the ADR with a spike that measures both.
- **Limits and friction** follow the contacts' approach: limits are one-sided, friction is regularised the way contact friction is.
- **Validation:** a double pendulum against an RK4 reference (period and chaos onset), energy of a joint chain without friction never rising, a limit holding a hanging load at its stop, momentum and angular momentum conserved in free flight, a 15-body chain at rest staying at rest (no creep). The joint's force is checked against the statics.
- **Budget:** a 15-joint ragdoll's step ≤ 0.1 ms.

### G1.2 The character (`creeer_sim::character`, ADR-0023)

It replaces `creeer_sim::avatar` (the ball). The decisions are in ADR-0023 and are measured, not assumed.

- **Standing and walking.** The body is one rigid body: torso and legs as a capsule, with the mass and inertia of a person (body segment data, de Leva 1996) plus the suit and whatever is carried.
  - The legs are a spring-damper between hip and ground. Their stiffness and damping come from human leg stiffness, so the body stands, steps up a kerb and absorbs a landing.
  - Walking is a force through the feet, limited by friction, and its reaction goes onto the ground or the body stood on (a raft pushed back, a crate pressed down).
  - The body is held upright by a balancing torque with a strength limit. A hard enough shove topples it.
  - Speeds from people in work suits: walking 1.4 m/s, running up to 4 m/s, a jump about 0.4 m.
- **Climbing.** Ladders and handholds are joints (G1.1) with the grip's force limit; climbing is pulling on them.
- **Swimming.** Buoyancy from the body's and suit's volume, and drag from the water (ADR-0014). Lifts *avatars in water* (`docs/future-improvements.md`): the character now feels the air and the sea. Drowning comes with G9's needs; until then the suit is sealed.
- **Jetpack and EVA.** Cold-gas nitrogen thrusters (Isp about 70 s, as NASA's MMU and SAFER) with a finite tank: Δv = Isp·g₀·ln(m₀/m₁), refilled at the landing pod (G1.6). In free fall, translation and attitude thrusters (the orbit bubble, ADR-0007).
- **Ragdoll.** Above an impact threshold (an acceleration or impulse the legs cannot take), the capsule hands over to a 15-segment ragdoll on joints, with the same linear and angular momentum to rounding. It gets up again when at rest and able. Getting up is a drive on the joints, a recovery with a time.
- **Injury.** Per body region, from peak acceleration and impulse, with published tolerance data: the head-injury criterion (HIC₁₅) for the head, the Abbreviated Injury Scale bands for the rest. Health is the sum of injuries, which heal slowly. Death drops the inventory as a bag and respawns the player at their pod.
- **Prediction.** Lifts *predicting bumps into things*: the client's prediction world (`creeer_server::predicted`) holds the bodies near its player (within a few metres, mirrored from the snapshots), so walking into a crate is predicted, not corrected. The share of corrections over 1 cm is gated (today 5 % on a lossless link; target ≤ 1 %).
- **Validation:**
  - climbing slopes up to the friction angle and sliding beyond it (μ from the materials);
  - a raft walked on moving back by momentum;
  - a jump's height against the legs' impulse;
  - the jetpack's Δv against Tsiolkovsky;
  - floating at the depth the densities give;
  - HIC of a drop onto a spring against its closed form;
  - a 3 m fall onto concrete, soil and water giving three different injuries;
  - momentum exact across the ragdoll's handover;
  - determinism; a save with characters standing, swimming and ragdolled.
- **Budget:** a standing or walking character ≤ 0.03 ms a tick; a ragdolled one ≤ 0.1 ms.

### G1.3 The look: art direction and the asset pipeline

The look first, so that everything after it is made in the style.

- **`docs/art-style.md` made real** (§11 there):
  - its palette moves into data (`data/art/palette.ron`), tested against the file's tables;
  - its automated style checks run in CI (contrast, chroma, silhouettes, player colours, forbidden effects);
  - the style board (`--style-board`) and its golden images.
- **Blender for authored models.** The Blender MCP is set up here (below, "Blender MCP"). Authored assets are the character, tools, the landing pod, bags and crates. Everything procedural stays procedural: terrain, trees, structures and decoratives come from code.
- **Pipeline.** `.blend` sources live in `art/`, exported glTF binaries (`.glb`) in the base pack (`data/models/`). The renderer loads glTF meshes (the `gltf` crate, pinned). A mesh's material slots name palette materials (`"steel"`, `"suit"`, `"player_accent"`) instead of carrying textures, so a mod can recolour a material once. The validator checks every model: it loads, its material names exist, it stays under its triangle budget, and it is the right size in metres.
- **Renderer additions.**
  - SSAO (ambient occlusion in screen space; half resolution on low settings).
  - Procedural detail and edge wear on meshes, as on the terrain (triplanar noise, plus wear from curvature baked into vertex data at export).
  - Per-vertex material slots (today a mesh vertex is only a position and a normal; colour and roughness come per instance).
- **The character's look.** A segmented suit: each ragdoll segment is a rigid mesh, so the drawn body is exactly the simulated one, with no skinning needed. A helmet, a backpack with the jetpack, and the player-colour accent.
- **Procedural animation.** Feet placed on the ground by IK from the legs' spring, arm swing, tool swings timed to the strikes the server applies. Clients animate from the replicated state and inputs, so nothing extra goes over the network.
- **Icons.** Item icons rendered offscreen from their models (`--icon <model>`), cached per pack's content hash.
- **Validation:** golden screenshots of the character, every tool and the pod (headless, `--planet` views); the validator's checks on a set of broken models; SSAO's cost measured.
- **Budget:** SSAO ≤ 1 ms at 1080p on min-spec projected; a model's load ≤ 5 ms.

### G1.4 Tools, reach and carrying

- **Tools as items.** Hands, a pickaxe, a shovel, an axe and a bow saw. Each is a discrete item (G1.6) with a mass, a head material and a striking energy per swing (a pickaxe about 60 J, an axe about 80 J, from swing speed and head mass). Powered tools come with power in G3–G4.
- **Reach.** Actions work on what the crosshair ray hits, within 2.5 m of the character's hands (the server checks the reach). The F5 sandbox's 500 m commands are kept for creative and test servers only.
- **Carrying and throwing.** A grab is a ball joint (G1.1) between the hands and the body's grasped point, with a force limit from human strength (about 50 kg carried easily, 150 kg lifted briefly, heavier only dragged as friction allows). The grasped body's weight acts on the character, which slows and can topple it. Throwing is an impulse from the arm's stroke, with its reaction on the thrower.
- **Strikes.** A swing hits what the ray finds. The strike's energy goes into what it hits: dug rock (G1.5), cut wood (G1.7), or an impulse on a body.
- **Validation:** a carried 50 kg crate slows walking as its weight on the legs predicts; a 300 kg crate cannot be lifted; throwing conserves momentum; reach is enforced against a cheating client (commands from too far are refused).

### G1.5 Mining with real composition

- **Rock strength data.** `rocks.ron` gains each rock's mechanical properties: uniaxial compressive strength, tensile strength, Young's modulus and dry density. Soils get cohesion and friction angle instead. Sources are rock-mechanics references, cited per entry as the materials do. The validator checks their ranges.
- **Digging.** A strike removes the volume its energy can break: V = η·E_strike / e_s, with e_s the rock's specific energy of excavation (from its compressive strength, the standard rock-cutting relation) and η the tool's efficiency. Picks suit rock, shovels suit loose soil and gravel. The brush is the tool's own small shape at the strike point (a few centimetres in granite, a spadeful in soil). Lifts *small edits vanish at a distance*: the levels of detail keep sub-cell edits visible (a dug pit stays visible from 1 km).
- **Composition.** The dug volume's solid mass is ρ·V·(1 − porosity), and its minerals come from the geology at that point, including ore grades from the deposit models (ADR-0006), integrated over the brush, not taken from its centre alone. The result is a bulk item (G1.6): mass, mineral composition, temperature.
- **Mass conserved.** The mass taken from the terrain equals the items' mass, to rounding. The terrain's removed volume is measured from the same density field as the edit.
- **Loose boulders.** Worldgen places talus under cliffs and boulders on slopes, as hull bodies (ADR-0005) that are scenery until touched (principle 5). A pickaxe breaks a boulder into bulk; a large one can be rolled.
- **Prospecting.** A hand lens shows which minerals a surface carries and roughly how much (by the rock's colours and the assay of a strike's sample). A deposit is found by walking, looking and sampling, not by a map overlay.
- **Validation:** volume per strike against the specific-energy relation for three rocks; the composition of a dug porphyry sample against the deposit model's grade at that point; mass closure over 10,000 strikes; edits visible at distance (golden screenshots); boulders park and wake exactly (ADR-0019).
- **Budget:** a strike (edit, composition, item) ≤ 0.2 ms on the server; the chunk rebuild off the tick (as now).

### G1.6 Items, inventory and the landing pod

- **Items.**
  - **Bulk:** a mass, a composition (minerals or species, mass fractions), a temperature and a particle size. Merging two bulks of any composition conserves mass and each species exactly (the composition is the mass-weighted mix) and energy (the temperature follows the heat capacities, `creeer_materials`).
  - **Discrete:** a part kind, a material, dimensions, a quality, and its plastic deformation if it has any.
- **Capacity by mass and volume, not slots.** The suit's pockets and backpack hold so many litres and the character carries their mass (it walks slower when loaded). Bulk fills by its bulk density.
- **Dropping and bags.** A dropped bulk becomes a bag: a physics body whose mass is its contents. Dropped tools and logs are their own bodies.
- **Containers.** A crate holds items, and its physics mass includes them (it changes as items go in and out).
- **The landing pod.** Each player starts at a landing pod: a container with the starting kit (suit, tools, a little food for G9), a charger for the jetpack's nitrogen, and the respawn point.
- **Transactions.** Every move between inventories, containers and the world is one server-authoritative transaction, atomic and ledgered: mass, species and items conserved, tested over random sequences (proptest).
- **Saves.** New save sections for inventories and containers (save version 7, migration from 6); bags and pods are bodies like any other.
- **Validation:** conservation under random transfers; a crate's mass and inertia following its contents; a full backpack slowing the walk; save and load mid-transfer; a client asking to move what it does not have is refused.

### G1.7 Trees and vegetation

- **Species.** Wood species data in the materials pack: spruce exists (EN 338 C24), and pine, birch and oak are added, each with density, moduli and strengths along the grain from EN 338 and the Wood Handbook. Each species has a growth form: height against trunk diameter (allometry), branching, crown shape, and the foliage's mass.
- **Procedural trees.** A tree is a beam structure (ADR-0004): trunk and branches as members of the species' wood, with tapering sections, and the foliage's mass hung on the branch tips. Trees are placed by biome, slope and altitude, deterministically from the seed. Their shapes are seeded per tree, so no two trees look the same.
- **Scenery until touched** (principle 5). A standing tree is a static instance: no physics per tick. A strike, a cut, a collision or a load turns it into a structure body. It parks again once it has fallen and come to rest (ADR-0019).
- **Felling as it really works.** Each axe strike removes wood from the trunk's section at the cut: the member's section there shrinks by the volume the strike's energy cuts (from the wood's strength across the grain). The tree stands until its own weight, and the wind on its crown, exceed what the remaining section carries. Then it breaks and falls towards the notch side, as a felled tree does. The wind (ADR-0014's uniform wind) sways crowns and can decide the fall.
- **Bucking.** A bow saw cuts a fallen trunk through at a point: the structure splits there, and the logs are bodies (a 3 m spruce log of 30 cm weighs about 90 kg: carried by one person barely, rolled easily). Branches break off into fallen wood. Planks and timber come from G3's sawmill.
- **Decoratives.** Grass tufts, bushes, pebbles and fallen branches: instanced, non-physical, crushed flat where bodies lie on them (a mask, not a simulation).
- **Rendering.** Trees are instanced with their own meshes from their beams (bark on the trunk, foliage clumps on the crown, in the style), with LODs down to impostors at distance.
- **Validation:**
  - a standing tree's stresses under its own weight below the wood's strength for every species and size generated (no tree breaks by itself);
  - the cut depth at which a trunk fails against the analytic remaining-section capacity;
  - the fall direction against the notch for 100 seeded trees;
  - a falling tree's energy within its potential energy;
  - placement determinism (golden hashes);
  - parking after a fall.
- **Budget:** 100,000 standing trees in the loaded 2 km² cost nothing per tick and draw at 60 FPS on min-spec (impostors); a tree turning into a structure ≤ 1 ms, spread so no tick takes more than 2 ms extra.

### G1.8 P1: piles (ADR-0024; performance track)

G1's scenes make piles: logs on logs, boulders, dropped bags, crates. The rock pile's worst tick (18.2 ms, ADR-0017) is measured again here on the real scenes, then cut.

- **A large group's contact solve split across cores.** The group is partitioned into pieces large enough to be worth a core (hundreds of contacts each), each solved by Newton on its own, coupled by an outer iteration on the forces across the cuts until the whole is the same minimum as solved at once (the same tolerance: ADR-0005's convergence check, unchanged). The partition follows the contact graph and is kept from step to step (no repartitioning noise).
- **Fixed with it:**
  - friction on fresh normal forces: no contact's friction rests on a normal force more than 5 % off its own (today 19 % of the pile's steps);
  - edge contacts and face patches made consistent, so rocks no longer rattle awake between them;
  - turn substeps per group, not for the world;
  - faces over curved ground made cheaper.
  
  Each is removed from `docs/future-improvements.md` when done.
- **Validation:** every contact, pile, stack and energy test at its present tolerance; the split solve against the whole solve on the same states (same minimum within the tolerance); golden hashes regenerated only after a change is proven rounding-only or deliberate.
- **Targets:** the rock pile's worst tick 18.2 → ≤ 5 ms, p99 14.2 → ≤ 2 ms; *quarry* and *lumberjack* within their gates; 50 crates on terrain at most 1.2× their cost on a plane.

### G1.9 HUD, the UI theme and first vitals

- **The theme.** The egui theme from `docs/art-style.md` §10, used by every window (the sandboxes' too).
- **HUD.**
  - health per body region;
  - the suit's readouts, measured from the surroundings (pressure, temperature, O₂ outside; the suit's own consumption comes in G9);
  - the jetpack's nitrogen;
  - a compass and altitude;
  - a hotbar of tools;
  - interaction prompts ("cut", "dig", "pick up 42 kg").
- **The inventory window.** Items with their icons, mass, volume and composition. Hovering a bulk item shows its minerals and grades; a part shows its material and dimensions.
- **Cameras.** First person for working, third person for moving about (the character is always visible to others).
- **Validation:** golden screenshots of the HUD and the inventory; UI tests for transfers by drag and drop.

### G1.10 Multiplayer, play scenes and the demo

- **Everything replicated.** Characters (standing, ragdolled, swimming), strikes and their edits, trees as they fall, items, bags and containers, within 256 kbit/s per client in *crowd*.
- **Play scenes gated** (G1.0):
  - *lumberjack*: 4 players fell 20 trees on a slope; logs roll and pile.
  - *quarry*: 4 players dig a 10 m pit with 200 boulders and dropped bags about.
  - *crowd*: 8 players walking, jumping, carrying and throwing crates in one place.
  - *fall*: characters falling, ragdolling and getting up on terrain, crates and each other.
  - *forest*: one player walking through 100,000 trees for 10 minutes (streaming, no tick cost).
- **The demo and playtest.** Walk from the pod to a forest and an outcrop, fell a tree, dig ore, carry both back, save, load; then the same with a second player over the network.

## Blender MCP

**When:** at the start of G1.3, when the first authored models are needed (the character, the tools, the pod). Until then nothing needs it: everything before G1.3 is code and data, and terrain, trees and structures stay procedural after it too.

**What it takes:**
- Blender installed (a current 4.x release);
- a Blender MCP server (the community `blender-mcp` add-on is the usual one; checked at setup time for what is current) added to Claude Code's MCP settings;
- the folders `art/` (sources) and `data/models/` (exports).

The MCP lets Claude build and change models in your running Blender, export them and render previews. You can open and adjust the same files by hand.

**One note:** the add-on runs Python inside Blender that Claude writes. It is your local Blender, and you see the scene as it changes, but it has the access Blender has on your machine.

## Order and dependencies

G1.0 → G1.1 → G1.2 → G1.3, then G1.4 → G1.5 and G1.6 (in either order) → G1.7 → G1.8 → G1.9 → G1.10.

- **Joints come before the character**, because the ragdoll, carrying and ladders need them.
- **The look comes before the content**, so the tools, items and trees are modelled in the style from the start.
- **P1 comes after mining and trees**, so it is tuned on the real *quarry* and *lumberjack* scenes, not only the sandbox's rock pile.

Each sub-milestone ends with `cargo xtask ci` green and a commit pushed to `g1`.

## Decisions to confirm

Recommended here, to be confirmed with the user at the sub-milestone they belong to:

1. **A segmented suit instead of a skinned body** (G1.3): the drawn body is the simulated one, and the renderer needs no skinning. A skinned body looks softer, but costs a skinning pipeline and a mismatch with the ragdoll.
2. **The landing pod as the start** (G1.6): a starting kit and a respawn point that fit a planet with no one on it.
3. **Cameras: first and third person** (G1.9). A higher, Factorio-like overview camera for factories is an idea for G3.
4. **Reach of 2.5 m for survival play** (G1.4), with F5's 500 m commands kept for creative and test servers.

## Budgets (development machine; min-spec projections ×2)

| What | Budget |
|---|---|
| Every play scene's worst tick / p99 | ≤ 5 ms / ≤ 2 ms (aim 2–3 ms worst) |
| A walking character / a ragdoll | ≤ 0.03 / ≤ 0.1 ms a tick |
| A 15-joint ragdoll's step | ≤ 0.1 ms |
| A strike (edit, composition, item) | ≤ 0.2 ms |
| A tree becoming a structure | ≤ 1 ms, no tick more than 2 ms extra |
| 100,000 standing trees | 0 ms a tick; 60 FPS on min-spec |
| SSAO at 1080p | ≤ 1 ms min-spec |
| Bandwidth in *crowd* | ≤ 256 kbit/s per client |
| Prediction corrections over 1 cm | ≤ 1 % of corrections |
| The reference tick (plan.md §5) | still ≤ 10 ms projected |
