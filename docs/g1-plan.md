# G1 plan: player & interaction

Status: **planned** (2026-10-18). Branch `g1`, squash-merged into main as "G1" at the end.

G1 is the first gameplay milestone. The loop it makes playable: **start with nothing but the clothes you wear, gather stones and wood, make your first tools, fell a tree, dig ore, carry both home, with friends.** It turns F5's test ball into a person with a body, gives that person hands and tools, makes the planet's rock and trees something to take apart, and gives the game its look.

plan.md's G1: the character (G1.1), interaction (G1.2), mining with real composition (G1.3), trees (G1.4), inventory (G1.5), HUD and first vitals (G1.6), performance package P1 (piles). This plan orders and details them, and adds what they need first: joints in the physics core, a harness for play scenes, and the art direction with an asset pipeline.

**Re-planned 2026-10-22, before G1.5** (the user): no landing pod and no starting kit. The player starts in their clothes and makes everything else, from stone tools on. Tools and trees are each one generic system whose attributes make the content. G1 builds only the fundamentals, so that it is playable soon (principles 9–11). G1.5–G1.10 below are the re-planned ones; the order is G1.6 → G1.5 → G1.7 → G1.8 → G1.9 → G1.10 (see "Order and dependencies").

## Principles

1. **A body, not a controller.** The character is a physics body like any other, with real mass. It walks by pushing on the ground through its feet, so whatever it stands on is pushed back. Friction limits the slopes it can climb, and its strength limits what it lifts. No kinematic shortcuts: the server steers every character, and its own client predicts it, with the same code and the same inputs (ADR-0008).
2. **Harm from mechanics.** Injury comes from what the body undergoes (peak acceleration, impulse, the head-injury criterion), not from a height or a speed threshold. A 3 m fall onto concrete hurts more than one onto soil, because the contacts carry the materials' stiffness (ADR-0005).
3. **What you dig is what was there.** A dug volume yields the mass and mineral composition that the geology has at that place: the deposit models' grades, with mass conserved exactly between the terrain and the items.
4. **Trees are structures.** A tree is beams of real wood. It falls the way its cut and its weight make it fall, and it breaks where its stresses exceed the wood's strength.
5. **Cheap until touched.** A forest is scenery: instanced, static and costless per tick. A tree becomes a physics structure only when something cuts it, strikes it or leans on it. The same goes for loose boulders. Cost follows the players (F7).
6. **Factorio's vibe in 3D** (below, "Art direction"). Readable first: every material has its own colour, silhouettes are clear, objects are grounded by soft shadows and ambient occlusion, and shapes are chunky and believable. Every new thing in G1 is built in this style from its first version: no programmer art left at the end.
7. **Gated play scenes** (plan.md, Phase B): every scene's worst tick ≤ 5 ms on the development machine (aim 2–3 ms), its p99 ≤ 2 ms, and its bandwidth ≤ 256 kbit/s per client.
8. **Research before each sub-milestone's design** (plan.md §1, principle 12; added 2026-10-18). Every sub-milestone from here on starts by surveying how others solved what it builds: papers, engines, games (for G1.3, how Factorio and other stylised 3D games achieve their look; for G1.5, how games and simulators model digging and granular material; for G1.7, tree and wood fracture models). The survey goes into its ADR's Prior work section before any design. P1 learned this the hard way (ADR-0024).
9. **Nothing given** (the user, 2026-10-22: "the game should only give you that what you have actually researched"). A survival player has the clothes they wear and nothing else: no pod, no kit, no free refill, no endless source of anything. Whatever they hold, they made from what the world holds. The game is hard and expects real knowledge of chemistry, mechanics and physics. It shows what can be observed and leaves the knowing to the player. Handouts (tools, the jetpack, crates, refills) belong to creative and test servers only.
10. **Fundamentals first** (the user, 2026-10-22: "doing just what is necessary for the game; basic and fundamental gameplay mechanics and systems and not spend hours on little polish things"; "i want to have something playable somewhat soon"). Each step builds what the loop needs, and the user plays it as soon as it is playable. Story, the start's staging, decoratives, the UI's theme, golden images of the UI and other polish wait for G11, recorded in `docs/future-improvements.md`. Performance is a fundamental, not polish: every gate holds (principle 7). **Development time is limited** (the user, 2026-10-22: "i DO NOT have unlimited usage and want to get to the fun part of designing production, buildings, vehicles, etc. soon"). Each step is the smallest version that is real and fast, with the tests that check its physics and its gates, and no more. Whatever else it could have goes to `docs/future-improvements.md`, not into the step. The philosophy (the user, 2026-10-22): "good quality but focused and research driven; no faffing about; the game can always be fleshed out later to make it look more flashy". Research still decides each method, focused on what the step builds.
11. **Flexible, generic systems with attributes** (the user, 2026-10-22: "make a generic system and then the improved world generation can use that system"; "flexible systems with attributes that can be tweaked to create a lot more actual content"). A system takes any material and any shape, and its results come from them. A tool's mass, swing, cut and wear come from the pieces it is made of, not from a list of tools. A tree is a species' attributes (growth form, size, wood, bark and foliage colours), so worldgen can fill biomes with many trees from data. There are few designed parts, in the spirit of Dream Car Builder (rods, plates and panels of any material), not Satisfactory or Minecraft. New content is data in the packs (ADR-0018), not code.
. **Foundations now, refinement later** (the user, 2026-10-22: "you (opus 5.5) lay the foundations of the game and make the actual important design and architecture decisions; and then later one can refine it easily because everything is flexible and data driven (so i can use cheaper models for that)"). Each milestone builds the architecture and the generic systems, with a few data entries that prove them. More content, tuning and polish come later and are data work. Every data file says in its header how to add to it, the validator catches bad entries, and the tests check every entry generically.

## Art direction: Factorio's vibe in 3D

These rules hold for everything that is drawn. From G1.5 on (principle 10), G1 draws only what play needs. Player-made things and trees are drawn procedurally from their pieces and attributes, in their materials' colours, with no model made for each. Decoratives, the UI's theme and the rest wait for G11.

What makes Factorio's look, and how Creëer carries it into 3D: a summary. The rules themselves (palette, limits, checks) are in **`docs/art-style.md`**, the authority, which every asset is checked against.

- **Readability over realism of surface.** You can tell what a thing is, and what it is made of, at a glance from the play camera. Colour belongs to the material: rocks already carry their colours in `rocks.ron`, and parts take the colour of their material. Silhouettes are distinct: no two tools share an outline.
- **A grounded, earthy, industrial palette.** The terrain is muted and earthy (as it is now). Objects sit slightly brighter and more saturated than the ground they stand on, so they read against it. Saturated accents are saved for what matters: hazard yellow and black, glowing hot metal, a player's own colour.
- **Soft detail, no photos.** Surfaces get procedural detail (triplanar noise, as the terrain already has), wear and grime on edges and in creases (from curvature and ambient occlusion), and never photographic textures.
- **Weight and contact.** Strong sun shadows, ambient occlusion (SSAO, new in G1.3) and contact shadows ground everything; Factorio's pre-rendered look owes much of its density to them. The tonemapper keeps authored colours (ADR-0003, Khronos PBR Neutral).
- **Chunky, mechanical, believable shapes.** Things look like what they are and how they are made: bevelled edges that catch the light, bolts, rivets, welds, handles. Small details are slightly exaggerated (thicker handles, chunkier gloves and boots) so they read at a distance.
- **Smooth, real shapes; never blocky** (`art-style.md` pillar 9, the user, 2026-10-21). No Minecraft geometry and no faceted low-poly look: the ground is smooth, curved things are round, and people, trees and rocks are modelled as they are.
- **Grounded, never futuristic** (pillar 10). Workshops, mines and machines of the last hundred years, not science fiction.
- **Dense, lived-in ground.** Pebbles, grass tufts, bushes and fallen branches, instanced in their thousands, so the planet never looks empty between the trees.
- **The engineer.** A worker in padded workwear (Factorio's engineer as a very rough guide, without its hard hat, tools and rucksack), readable as a silhouette, with a jacket in the player's own colour (as Factorio colours its players).
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

**Done** (2026-10-18, ADR-0022):
- ball joints and hinges with limits, damping, friction (anchored) and drives;
- augmented-Lagrangian constraints in the step's minimisation, with exact rows;
- what Newmark and the energy guard needed once they met joints: a velocity projection (sparse Cholesky), substeps while joints turn fast, and the guard's surplus counting contacts only;
- save version 8.

Every validation below is green. Changed from the plan: the budget. A ragdoll in the air steps in 0.15 ms p50, gated at 0.2 ms, because its fifteen bodies free already cost 0.39 ms. On the ground it costs 1.4 ms p50, which is the large group's contact solve, P1's (G1.8).

The physics has contacts and welded members, but no articulation. The ragdoll needs it, and so do carrying, ladders, and in G2 the hinges, pins and bearings.

- **Kinds.** A ball joint (three rotations free) and a hinge (one free), each with angle limits (a cone and twist, or a range), joint friction, and an optional drive (a target angle with a strength limit: muscles, later motors). They join two bodies, or a body and the world.
- **Solved within the implicit step.** A joint is part of the step's energy minimisation (ADR-0004/0005), as an exact constraint, not a soft spring tuned to look right. The method (eliminated coordinates, or an augmented Lagrangian on the existing Newton) is decided in the ADR with a spike that measures both.
- **Limits and friction** follow the contacts' approach: limits are one-sided, friction is regularised the way contact friction is.
- **Validation:** a double pendulum against an RK4 reference (period and chaos onset), energy of a joint chain without friction never rising, a limit holding a hanging load at its stop, momentum and angular momentum conserved in free flight, a 15-body chain at rest staying at rest (no creep). The joint's force is checked against the statics.
- **Budget:** a 15-joint ragdoll's step ≤ 0.1 ms.

### G1.2 The character (`creeer_sim::character`, ADR-0023)

**In progress.**
- **G1.2a done** (2026-10-18): the body (a box from the knees up, 105 kg), its legs (a spring from the hip to the ground a ray finds: `World::ray`, new), walking within the friction cone, balance within the body's strength, reactions on what it stands on (and both stepped as held, so momentum is kept), sleep. The server and the client's prediction use it; protocol 5 (`Kind::Character`). The decisions differ from the plan below: a box instead of a capsule (cost), and no leg compliance from a separate body (a ray-found spring instead).
- **G1.2b done** (2026-10-18): swimming (buoyancy and drag are the sea's own; a stroke of at most 80 N). **G1.2c done:** the jetpack (cold nitrogen, Isp 70 s, 1.5 kN, a 3 kg tank that is the body's mass; attitude jets), the character held (BDF2) while standing and free in the air.
- **G1.2d done** (2026-10-18): knocked down (a lean past 45° or a jolt over 2 m/s), the ragdoll (15 segments, momentum kept exactly), injury from mechanics (pulses on a person's stiffness, HIC for the head), people on the server (health, falling, standing up, death and respawn), the player told (`Own`: health, down), saves (`PEOP`). Its cost on the ground took *crowd* to 58 ms worst: **P1 (G1.8) is brought forward and comes next**, before G1.2e.
- **P1's first pass done** (ADR-0024, "Outcome of the first pass"): the legs became a strut solved in the step (a stiff load on a light body was the crowd's 58 ms tick), falls corrected (injury by region, the ragdoll laid out with the legs it has), about 8 % off a ragdoll's step by exact, profile-found changes. The contact core's redesigns (friction within Newton with the joint tree, contacts that persist) go back to G1.8, where they were planned. *crowd's* CI work gate re-set to its measured 4,730 a tenth above (5,200), by the user's decision; its time gates stay for G1.8.
- **G1.2e done** (2026-10-21, ADR-0025): the bodies near the avatar (12 within 6 m) are predicted with it, set back only where the server says it mispredicted; snapshots list the nearby bodies their budget left out; protocol 6. Walking into crates: corrections over 1 cm 44 % → 0.6 % lossless (3.4 % over 100–130 ms with 5 % loss), the largest 38 → 5.5 cm; on bare terrain none over 1 cm now. G1.2 is complete.
- **Next:** G1.3: the look (art direction and the asset pipeline, with Blender), planned 2026-10-21 (below; ADR-0026).
- **Climbing** waits for something to climb: G2's ladders and handholds.

It replaces `creeer_sim::avatar` (the ball). The decisions are in ADR-0023 and are measured, not assumed.

- **Standing and walking.** The body is one rigid body: torso and legs as a capsule, with the mass and inertia of a person (body segment data, de Leva 1996) plus the suit and whatever is carried.
  - The legs are a spring-damper between hip and ground. Their stiffness and damping come from human leg stiffness, so the body stands, steps up a kerb and absorbs a landing.
  - Walking is a force through the feet, limited by friction, and its reaction goes onto the ground or the body stood on (a raft pushed back, a crate pressed down).
  - The body is held upright by a balancing torque with a strength limit. A hard enough shove topples it.
  - Speeds from people in work suits: walking 1.4 m/s, running up to 4 m/s, a jump about 0.4 m.
- **Climbing.** Ladders and handholds are joints (G1.1) with the grip's force limit; climbing is pulling on them.
- **Swimming.** Buoyancy from the body's and suit's volume, and drag from the water (ADR-0014). Lifts *avatars in water* (`docs/future-improvements.md`): the character now feels the air and the sea. Drowning comes with G9's needs; until then the suit is sealed.
- **Jetpack and EVA.** Cold-gas nitrogen thrusters (Isp about 70 s, as NASA's MMU and SAFER) with a finite tank: Δv = Isp·g₀·ln(m₀/m₁). In survival nobody has one until they make it, and its gas is what they compress themselves (G4–G5). Creative servers hand it out and refill it (G1.6b). In free fall, translation and attitude thrusters (the orbit bubble, ADR-0007).
- **Ragdoll.** Above an impact threshold (an acceleration or impulse the legs cannot take), the capsule hands over to a 15-segment ragdoll on joints, with the same linear and angular momentum to rounding. It gets up again when at rest and able. Getting up is a drive on the joints, a recovery with a time.
- **Injury.** Per body region, from peak acceleration and impulse, with published tolerance data: the head-injury criterion (HIC₁₅) for the head, the Abbreviated Injury Scale bands for the rest. Health is the sum of injuries, which heal slowly. Death leaves what was carried where the body lay, and respawns the player at the world's spawn with nothing (G1.6b).
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

### G1.3 The look: art direction and the asset pipeline (ADR-0026)

**Done** (2026-10-22; begun 2026-10-21, the plan approved by the user; the look reviewed and accepted in G1.3i). The look comes first, so that everything after it is made in the style. Its survey of prior work is in ADR-0026, its rules in `docs/art-style.md`, and how Blender is used in `docs/blender-mcp.md`.

The user's direction at its start (2026-10-21, recorded in `art-style.md`'s changelog):
- **Not futuristic:** the vibe of Factorio.
- **Not blocky or voxel-like:** no Minecraft geometry; smooth surfaces, for realism.
- **The engineer** after Factorio's FFF-40 engineer as a very rough guide, without its hard hat, tools and rucksack. The user chose: the jetpack as two bottles on a harness frame, a skinned and smooth body, and a hood and goggles with the face open (later the hood went: the head is bare, the goggles pushed up).
- **Many wearables in future:** a base body ("naked": in a plain base layer) that wears more and more as the player progresses. The starting outfit is itself made of wearables.

Where it starts: meshes are a position and a normal per vertex with one material per instance; the character is drawn as a box and two beams; there is no ambient occlusion; meshes have no surface detail (ADR-0026, "Where it starts"). Blender 5.2 LTS with MCP for Blender is set up and running (`docs/blender-mcp.md`).

#### G1.3a The palette in data and the style checks

- **The palette.** `data/art/palette.ron` holds every token of `art-style.md` §3 and §10: its colour, roughness, metalness and group (terrain, material, character, vegetation, accent, player, UI), and, for wear, the token beneath it (`steel_painted` wears to `steel`). A test reads the tables of `art-style.md` and fails if the two differ. The validator checks it with the pack (ADR-0018).
- **Colour science** in a small crate, `creeer_art`: sRGB ↔ CIELAB and CIEDE2000.
- **The checks, in CI** (`art-style.md` §4, §3.6, §5):
  - every object token at least ΔE₀₀ 12 from the five grounds;
  - chroma and L\* limits per group;
  - the sixteen player colours at least ΔE₀₀ 8 apart (normal vision);
  - no forbidden effect in the renderer's sources.
- **Approved exceptions:** a miss is kept only by the user's approval, listed in `art-style.md` §4 and `data/art/exceptions.ron`. A check reports an exception once it is no longer needed.
- **The validator** loads the palette and its exceptions with every pack. A mod's misses are warnings, since a mod may choose its own look.
- **Done** (2026-10-21). The first run found:
  - steel, titanium and concrete too close to gravel;
  - rubber too close to basalt;
  - copper and laterite bauxite too saturated;
  - the player colours confused under colour blindness.

  The user kept the original colours, so the misses became approved exceptions. Colour blindness waits until the game is published (`docs/future-improvements.md`; ADR-0026 has what was measured).
- **Validation:** CIEDE2000 against Sharma, Wu and Dalal's 34 test pairs (to 10⁻⁴); Lab against published values (sRGB red, white, mid-grey); every token, player colour and rock colour named in `art-style.md` against the data, both ways; the checks on palettes broken in each way; the base pack passing with its exceptions.

#### G1.3b Models: glTF, materials as tokens, the validator

- **Loading.** The renderer loads `.glb` files with the `gltf` crate (1.4.1, pinned). Meshes, skins and animations are read. Image textures are refused.
- **Vertices** gain the palette token's index, the baked occlusion and the convexity (from `COLOR_0`). A mesh is still one draw. Procedural meshes keep one material per instance through a reserved index.
- **Instances** gain the player's accent colour and how worn they are (new = none, old = full), beside the overlay value.
- **The models' manifest**, `data/models/models.ron`: each model's class, triangle budget, size and origin.
- **The validator** (`creeer_tools validate`) checks every model:
  - it loads;
  - every material is a palette token, and no image is used;
  - it stays within its triangle budget;
  - its size in metres and its origin are as the manifest says;
  - smoothly shaded edges join faces at most 15° apart (`art-style.md` §7);
  - normals are unit, and no triangle is degenerate;
  - a skinned model's bones are the ragdoll's segments, named and placed as `creeer_sim::character::ragdoll` has them.
- **`cargo xtask art`**: runs headless Blender on the scripts in `art/` and writes the `.glb` files. `--check` fails on any difference from the committed files; `--blend` also writes a `.blend` to open by hand (`docs/blender-mcp.md`).
- **The first model is a crate** (`art/crate.py`): timber boards on a steel frame, bevelled, at the sandbox crates' real size. It proves the pipeline from script to screen.
- **Validation:** a set of broken models in `tests/models/`, each refused for its own reason (an unknown token, an image, over budget, wrong scale, a coarse facet, a misplaced bone); the crate drawn in the sandbox; the crate's script rebuilt to the identical file.
- **Budget:** a model's load ≤ 5 ms.

#### G1.3c Surfaces, ageing and smooth geometry

Revised 2026-10-21, after the user saw the first crate in Blender. The user found it "a very boring and blandy monochrome wood texture", and said everything should look like Factorio, "grainy, weathered, used", then "darker and grimier". The user also asked for ageing: "things look older and older". The look was worked out live in Blender (`art/preview.py`) on three crates of different ages (ADR-0026, decisions 3 and 4).

- **Surfaces in the mesh shader** (`art-style.md` §6), the preview's layers in WGSL:
  - tone, pattern, mottling and fine grain;
  - age, grime and rain streaks;
  - edge chips and occlusion.

  The noise is filtered by its screen footprint, so detail finer than a pixel fades instead of shimmering. Vertices gain the part's tone, the height above the base and the grain coordinates; each instance its age and wear; the palette on the GPU its patterns, what tokens wear and age to, and their years.
- **Ageing** (`art-style.md` §6b):
  - the server keeps each object's age: its exposure, its rate and the tick it changed, plus its wear;
  - the rate comes from its surroundings: air with oxygen and water 1, the sea 3, vacuum 0;
  - a world pace (one year of weathering per game day by default) scales it;
  - impact energy beyond about 10 J adds wear;
  - it is saved, and replicated in the spawn and on change, so nothing runs per tick;
  - clients draw it.
- **Validation:**
  - the shader's layers against the Blender preview, side by side in both. Done 2026-10-21: the crate at 0, 1 and 4 years (`creeer_client --style-board`, `cargo xtask art --board`); the preview now reads its "age" as years, as the game does (ADR-0026, Validation);
  - no shimmer: frame-to-frame difference under a slow camera move. Done 2026-10-21: `creeer_client --shimmer`, surfaces 0.079 % against a 0.2 % gate (ADR-0026, Validation);
  - ageing: exposure exact across save and load; rates by surroundings; a crate in the sea ageing three times as fast; a dropped crate's wear against the impact's energy; determinism.
- **Smooth geometry everywhere** (`art-style.md` pillar 9, §7):
  - the sandbox's flat-shaded primitives become smooth ones: boxes with rounded edges and cylinders with rounded rims (true rounds, a radius in metres whatever the scale), and cylinders and spheres with at least 24 segments at their finest. Done 2026-10-21 (ADR-0026 decision 11);
  - members (cylinders) and spheres get levels of detail chosen by screen-space error (half a pixel), not by distance;
  - the terrain's normals and dug craters checked to be smooth at every level of detail, with close-up screenshots. Done 2026-10-21: dug rims were zigzags with flaps; edits now combine by rounds of two cells (ADR-0006, amended in G1.3c); the rims' lighting up close waits on finer terrain (`docs/future-improvements.md`).
- **Validation:** golden screenshots of the crate, members and spheres at noon and in the evening, close and at 20 m; the facets' screen-space error measured on the levels of detail. The error: done 2026-10-21 (`primitives_tests.rs`, `levels.rs`, ADR-0026). The golden screenshots move to G1.3h, whose style board makes every asset's goldens and checks them on each change.
- **Budget:** the F1 mesh budget (10,000 instanced parts at 60 FPS on min-spec) still met; the mesh pass at most 10 % slower than now. For the surfaces: met, +4 to +6 % frame time across the bench views (ADR-0026, Cost).

#### G1.3d Ambient occlusion (GTAO)

- **GPU timestamps** first (wgpu's timestamp queries), shown per pass in the F9 profiler. Every GPU budget from here on is measured with them.
- **GTAO** after XeGTAO (ADR-0026): depth MIPs, the main pass and a spatial denoise, applied to ambient light only, with the multi-bounce fit. It darkens creases by at most 50 % (`art-style.md` §5).
- **How it meets the MSAA forward pass,** decided by measuring both: (a) a depth and normal prepass (as Bevy does), or (b) ambient light written to its own target and occluded when composited. Measured: cost at 1080p, and the quality at MSAA edges.
- **Quality presets** (low, medium, high), with half resolution on low.
- **Validation:**
  - against analytic occlusion: a sphere on a plane (its closed form), and a 90° crease (half the hemisphere, cosine-weighted);
  - against a ray-traced reference of the style board's scene, computed on the CPU (XeGTAO's own method: the error measured and recorded);
  - no flicker under a slow camera move (the frame-to-frame difference measured);
  - the visibility-bitmask variant measured if thin members or branches over-darken.
- **Budget:** ≤ 1 ms at 1080p on min-spec (a GTX 1660-class GPU), projected from the development machine's timestamps.
- **Done** (2026-10-22; ADR-0026 decision 5, Validation, Cost):
  - GPU timestamps on every pass, in the F9 profiler's "GPU passes" and in `--bench`;
  - GTAO from a depth prepass at 4× MSAA, the scene pass testing for equal depth: option (a), 0.07–0.18 ms a frame faster than (b), which was built and measured (`docs/adr/0026/`);
  - XeGTAO's three passes in WGSL, with a 4×4 interleaved pattern and GTAO's own 4×4 bilateral filter in place of XeGTAO's TAA-reliant noise; presets off, low (half resolution), medium and high, in the sandbox's window; with occlusion off no prepass is drawn;
  - validated: the sphere's closed form within 0.011 and the crease by quadrature within 0.026 (reference preset); the shipped preset 0.033 from a ray-traced reference of the crate; no flicker in the image (0.079 %, as before); no over-darkened thin occluders, so no visibility bitmask;
  - cost: the occlusion's passes 0.22 ms at high (about 0.9 ms projected), medium 0.6 ms projected; a frame grows by 0.33–0.53 ms at high with the prepass (1.3–2.1 ms projected), so min-spec runs medium.

#### G1.3e Skinning on the ragdoll's bones, and wearables

- **Decided** (the user, 2026-10-21; it was the first of the "Decisions to confirm" below): the engineer is skinned on fifteen bones that are the ragdoll's segments, so the drawn body is the simulated one bone for bone, and smooth across its joints.
- **Dual quaternion skinning** (Kavan et al. 2007), in one compute pass a frame. The shadow, prepass and colour passes draw the skinned result as a plain mesh.
- **Wearables** (ADR-0026, decision 7):
  - the base body and every wearable are skinned meshes on the same bones, animated once;
  - each mesh is divided into regions (head, face, hair, neck, torso, upper arms, forearms, hands, pelvis, thighs, shanks, feet);
  - a wearable fills slots (head, eyes, face, torso inner and outer, hands, legs, feet, back) and lists the regions it hides, of the body and of the layers beneath;
  - a wearable's data (its model, slots and hidden regions) lives in the pack (`data/wearables.ron`), checked by the validator;
  - when an outfit changes, the visible regions of everything worn are merged into one index list, so a character is still one skinning dispatch and one draw.
- **Drawn as G1.3f modelled him** (ADR-0026, decision 8): two-sided, since his garments are single shells (back faces lit with their normals turned to the eye); the player's colour laid on at half strength over `#5c5a57`, as the preview does; each level chosen by its error against the close-up level, under half a pixel.
- **Ragdolls known to the client.** A ragdoll segment's spawn says whose it is and which segment (protocol 7), so the client draws the person, not fifteen boxes. The protocol also carries each player's outfit, skin tone and hair colour.
- **Validation:**
  - the bind pose reproduces the rest mesh exactly;
  - a vertex weighted to one bone moves rigidly with it (to f32 rounding);
  - the volume of an elbow bent by 90° kept within 2 % (linear blend skinning measured beside it);
  - a ragdoll's drawn bones equal its segments' poses;
  - an outfit's merged mesh draws exactly the visible regions (a test outfit of test wearables), what lies inside a worn wearable left out (the parts `in.<wearable>`, ADR-0026 decision 8);
  - golden screenshots of the base body and the full outfit standing, on all five grounds (moved from G1.3f: the game cannot draw him before).
- **Budget:** skinning 8 characters ≤ 0.05 ms of GPU time; an outfit change ≤ 0.5 ms on the client.
- **Done** (2026-10-22; ADR-0026 decisions 6–8, Validation, Cost):
  - dual-quaternion skinning in one compute dispatch a frame (`skin.wgsl`, `renderer/skin_pass.rs`), each character at the level of detail its error allows (`creeer_art::model_levels`, the levels' errors in `models.ron`), drawn two-sided by the shadow, prepass and scene passes; only what moves is skinned, packed (32 bytes read, 24 written a vertex); the player's colour, skin and hair as each character's own palette entries;
  - outfits: `wearable::visible` merged into one index list a level (`Renderer::add_outfit`); the starting outfit and the base layer alone registered by the client (`people.rs`);
  - protocol 8: a character's look (colour, skin, hair, outfit) and whether it has fallen; each ragdoll segment says whose and which (`Kind::Segment`); the server gives each player a free colour and the starting outfit, the player chooses skin and hair in the planet window (`Command::Look`);
  - validated: one bone moves its vertices rigidly, the rest pose draws the model itself, the GPU skins as the CPU's dual quaternions do (0.7 µm at worst over 55,494 vertices, every character of a frame), an elbow bent 90° keeps its volume (−0.06 %, linear blending −1.70 %), a fallen ragdoll's bones draw each joint at one place, an outfit draws exactly its visible regions; found by drawing him: the vest had a hole over each shoulder (fixed in `art/engineer/outfit.py`);
  - cost: skinning eight engineers close up 0.020 ms of GPU time; an outfit merged in 0.12 ms;
  - the golden screenshots of him standing on the five grounds come with G1.3h's style board check (`--style-board` draws him, base layer and full outfit, on any ground).

#### G1.3f The engineer

- **The look** (`art-style.md` §3.3, §8). The base body: a person with an open face, short hair and the player's skin tone, in a plain base layer. The starting outfit's wearables, as detailed and approved round by round (`art-style.md` §8): a navy canvas coverall, a two-tone work jacket (the player's colour and navy) with a belt and pouch, leather knee pads and elbow patches, thick gloves, laced work boots on rubber soles, and goggles pushed up on the forehead; no hood, the head bare with short hair, clean-shaven (the user, 2026-10-21). There is no hard hat, no rucksack and no tools, and no jetpack: the user, 2026-10-21, "the 'base' player model should not have a jet pack, that is something that you can build if you progress in the game" (G1.6). It is modelled when it can be made.
- **Its survey first** (as every step's): how stylised games with realistic proportions model readable faces without the uncanny valley, and measured human skin colours (for the `skin` tones, within `art-style.md` §4's limits). The skin tones and hair colours go to the user for approval.
- **A blockout first.** Rough shapes on the ragdoll's proportions, the base body and the outfit, judged as a silhouette at 64 px and at 20 m, and shown to the user from the front, side and three-quarter. The detailed models start only after the user approves it.
  - **Done** (2026-10-21, approved after three rounds): Blender Studio's realistic base mesh (CC0) fitted to ANSUR II's men of normal weight at 1.80 m, every garment an offset of the body (`art/engineer/blockout.py`, `body.py`, `physique.py`, `garments.py`; ADR-0026). The user kept the Monk scale's skin tones and the nine measured hair colours.
- **The models** (`art/engineer.py` and the scripts in `art/engineer/`, one model file with every piece: `docs/blender-mcp.md`):
  - built on the bones the ragdoll has (the table that `creeer_tools` writes from the Rust code);
  - each wearable fitted over the body (and over the layers it goes on), its regions and slots named;
  - weights painted by the script;
  - bevels and face-weighted normals;
  - occlusion and convexity baked into `COLOR_0`;
  - in levels of detail chosen by screen error, as the primitives' (the user, 2026-10-21: "different LODs for the player"): close-up about 30,000–40,000 triangles for the body and the outfit as drawn, mid 10,000–15,000, far 2,000–4,000 (play distance and the shadows); a wearable at most 4,000 at the mid level, the others in proportion;
  - **the close-up budget measured first**, before modelling to it: one player close to the camera, and a crowd of players in the factory scene, timed with stand-in meshes of each level's triangle count drawn into the main pass and the shadow cascades, against the frame budgets on the development machine projected to min-spec (skinning's own cost measured with G1.3e). **Measured** (2026-10-21, ADR-0026 Cost): 0.02 ms for one player close up at 40,000 triangles, 0.016 ms a player at 15,000, 0.006 ms at 3,000; the levels set at 40,000 / 15,000 / 3,000; the close-up level raised to 50,000 for the detail (the user, 2026-10-21);
  - **Done** (2026-10-22; ADR-0026 decision 8): `data/models/engineer.glb`, every piece in regions at three levels, skinned on the fifteen bones; what each wearable hides in `data/wearables.ron`. Worn whole it draws 49,848, 14,458 and 2,908 triangles. Each garment is weighted as what lies beneath it; the body's hidden regions end inside the garments' openings. The garments are single shells: G1.3e draws the engineer two-sided.
- **Within its segments** (`art-style.md` §2): every vertex of the body weighted mainly to a bone lies within that segment's collision box grown by 3 cm, and the starting outfit within 5 cm. Where the real shape and a box disagree by more, the box is corrected to the person's dimensions, as a physics change, tested. Bulky later wearables (a pressure suit, armour) change their segments' boxes and masses when they come.
  - **Done** (2026-10-21): the segments measured on the fitted engineer (`art/engineer/segments.py`) and moved to his proportions; within a ragdoll only the arms meet the trunk and head (the user's choice, until G1.8's persistent contacts); ADR-0023, amended in G1.3f. The upper arms and thighs miss the 3 cm on 5–7 % of their vertices (the shoulders' and hips' muscles); the detailed model is checked again.
  - **Checked again** (2026-10-22, `art/engineer/check_fit.py`): the body as before (thighs 6.2–6.5 %, upper arms 2.5–3.1 %). The outfit misses its 5 cm where it stands off the limbs: the knee pads (11–13 % of their points, up to 9.5 cm beyond the shank's box at the front), the elbow patches (38–40 %), the coverall's legs at the shanks (10–11 %), the belt's pouch (25 %). A ragdoll fallen on its knees would sink the pads up to about 4.5 cm into the ground. **For the user to decide:** the boxes corrected to the outfit (a physics change), or the misses kept as recorded (ADR-0026, Limits). **The user chose** the boxes grown to the outfit, keeping each segment's mass (2026-10-22). Measured first: the knee pads' fronts lie 12 cm ahead of the shin box's front face (17 cm from its centre, which is the shin's centre of mass), and the elbow patches 8 cm behind the forearm box's back. A box grown about its centre would stand as far behind the calf; reaching the pads needs a collision box set off its segment's centre of mass, which the physics does not have yet. **Deferred to G1.8** (the user, 2026-10-22), where the contacts are reworked: until then a ragdoll fallen on its knees sinks the pads into the ground (ADR-0026, Limits).
- **Out of scope here:** later wearables (a respirator, insulated suits, armour, a pressure suit for EVA in G8) come with what makes them; a character creator beyond skin tone and hair colour (faces, hair styles, body shapes) is recorded in `docs/future-improvements.md`.
- **Validation:**
  - the validator on every model;
  - no poke-through: nothing beneath an outer layer shows through it by more than 2 mm, in the rest pose and in poses at the ragdoll's joint limits, for the base body alone, the full outfit, and each wearable alone;
  - every hidden region lies inside what hides it (no holes where a region is hidden);
  - **Measured** (2026-10-22, ADR-0026 Validation; the two above): at rest, nothing pokes through and no hidden point is seen. At the limits a person reaches, the joints closed fully leave contact skinning cannot part (the elbow's crook 10.9 mm, the hand bent fully against the cuff 11.5 mm, the hip flexed 80° 10 mm through the belt, the knee's strap 9.5 mm); beyond a person's range (the ragdoll's cones are the same every way) the base layer is up to 11.9 mm off the body. **Decided** (the user, 2026-10-22): the joints closed fully fixed now, not recorded, by edge loops across every joint. Measured first, edge loops could not: undecimated, the elbow's crook came 11.9 mm through, as the skinning itself folds a crook over at its limit. So what lies inside a wearable is hidden while it is worn, the thighs' weight gives way to the pelvis above the hips, the coverall's cuffs are the forearms' and the knee straps the shins', and the loops went where the miss was density, round the boots' bend (ADR-0026, Measured and rejected). **Measured** (2026-10-22): within a person's range nothing pokes through at the close-up level (67 points before, 11.5 mm at worst); the points seen are those seen before and two more with the hips flexed 80°; beyond a person's range 53 points poke through, against 738 (ADR-0026, Validation);
  - golden screenshots of the base body and the full outfit standing, walking and fallen, on all five grounds: **moved** (the user, 2026-10-22) to G1.3e (standing, once the game skins and draws the engineer) and G1.3g (walking and fallen, once he is animated), as the game cannot draw him before;
  - the silhouette at 64 px: **done** (2026-10-22): from the front, side and three-quarters he reads as a person in workwear, the goggles and boots in the outline;
  - the user's approval of the images: **approved** (2026-10-22, "looks good enough for now; we can improve later?"); the look is refined once the game draws him in its light (`docs/future-improvements.md`).

#### G1.3g Procedural animation

- **Its survey first:** gait data, meaning stride length and cadence against speed from published measurements of human walking and running.
- **Poses.** A handful authored in the engineer's script and exported as glTF animations: standing, walking (contact and passing), running, landing, jumping, swimming, hovering on the jetpack, and bracing for a fall. They are blended by gait phase and speed (Rosen 2014).
- **Feet.** The feet are placed by two-bone inverse kinematics on the ground the legs' rays find, and the hips are lowered where a foot cannot reach (Johansen 2009). They turn to the ground's slope.
- **The rest of the body.** The torso is exactly the simulated body. The arms swing against the legs. A tool's swing is timed to the server's strikes (the hook is made here; the swings come with G1.4's tools).
- **Fallen:** the bones are the ragdoll's segments exactly. Getting up blends from the lying pose to standing over the rise time the server sets.
- **On the client only:** it is driven by the replicated state and inputs, sends nothing, and is not part of the simulation's hash.
- **Validation:**
  - feet within 1 cm of the ground on slopes up to the friction angle;
  - no foot sliding while it is planted (its drift measured);
  - stride and cadence against the gait data;
  - golden frames of a walk cycle;
  - golden screenshots of the base body and the full outfit walking and fallen, on all five grounds (moved from G1.3f).
- **Budget:** ≤ 0.05 ms of CPU a character a frame.
- **Done** (2026-10-22; ADR-0026 decision 9, Validation; `crates/creeer_client/src/animate/`):
  - the gait from measurements instead of authored key poses (the user's direction of "realism", and no Blender animation pipeline needed): walking cadence from the walk ratio (Sekiya et al. 1996), running from a stride-frequency fit, the change at Froude 0.5, each foot's share on the ground 60 % walking and 35 % running;
  - feet planted where they land and placed by inverse kinematics on the ground found under them (the knee a hinge about its own axis, so the rest pose is the model's exactly), rolling heel–flat–toe (Perry's rockers) on the sole's edges; the hips lowered as far as a foot needs (Johansen 2009); steps shortened on ground steeper than a stair and the stride hurried when a foot is left behind (setting off, steep slopes);
  - the trunk the simulated body, turned to its motion (the body, a box, never turns); arms swing against the legs; treading water where it is over 0.4 m deep, arms out in the air; a tool's swing hook for G1.4 (`Animator::swing`); fallen, the ragdoll's segments exactly, and standing up blends from them over 0.8 s;
  - validated: standing still is the model at rest; walking and running up and down slopes to the friction angle (walking only near it, where the legs' push just holds the body), every planted sole on the ground to rounding and no touching corner moving; cadence 114 steps a minute and 0.74 m steps at 1.4 m/s, 171 a minute running at 3.5 m/s, and as many steps taken as the gait says; the hips bob 63 mm walking at 1.4 m/s;
  - cost: 0.009 ms a walking character a frame and 0.4 µs a ground query on the planet (about six a frame);
  - drawn on the style board (fallen) and as a walk-cycle sheet (`--style-board <file> --walk <sheet>`); their golden images come with G1.3h's check.

#### G1.3h Icons and the style board

- **Icons** (`art-style.md` §9): `--icon <model>` renders orthographic three-quarter views at 64 and 128 px on a transparent background, cached by the pack's content hash. Silhouettes of any two items must overlap by IoU < 0.8; the test runs for the items there are (the crate and the jetpack now; tools come in G1.4).
- **The style board** (`--style-board`): every model on the five grounds, at noon and in the evening. The HUD and the inventory join it in G1.9.
- **Golden images.** They are compared on the development machine with a perceptual tolerance, because rendering differs between GPUs (`cargo xtask style-board --check`). An intended change is approved by the user with the image.
- **Done** (2026-10-22; ADR-0026 decision 10, Validation, Measured and rejected):
  - icons (`icons.rs`, `icon_image.rs`): nearly orthographic (40 radii away, as the camera has no orthographic projection), rendered four times larger and reduced, transparent by triangulation matting on two backgrounds at a fixed exposure (`Renderer::fix_exposure`), over a soft drop shadow; cached in `target/icons/<pack hash>/`. The jetpack left the starting outfit (a later wearable, not modelled yet), so the crate is the only item yet: the silhouette test runs over every item and tool and gets its pairs with G1.4;
  - the style board (`--style-board <dir> --all`): crates at three ages, a steel member and a painted sphere (the smooth primitives, G1.3c's goldens), the engineer in his base layer, his outfit and fallen, on the five grounds at noon and in the evening, from 20 m on soil, the walk sheet and the icons;
  - golden images (`tests/golden/style-board`, half size, `golden.rs`): CIEDE2000 per pixel, a mean of at most 1.15 and at most 0.1 % of pixels past the least noticeable 2.3. WARP against the RTX goldens differs by a mean of 0.04–0.08; a sun 4° lower and a just-noticeable recolour of the sphere are caught. `cargo xtask style-board [--save]`, and `cargo xtask ci` runs the check on the development machine.

#### G1.3i The review

The style board and a walk in the planet sandbox, shown to the user: the engineer walking, jumping, swimming, flying and falling, crates being pushed, the terrain at noon and in the evening. The look is accepted, or changed, before G1.4 begins.

- **The first review** (2026-10-22): the user: "Looks good, move the spawn; one thing: the skin looks horribly pale, like plastic", then "it looks like the character has holes in the area of the collarbone".
  - **Skin**: ten tones from measured faces instead of the Monk scale's swatches, and skin's own specular; subsurface scattering built, measured to change nothing visible, and set aside (ADR-0026 decision 12, Measured and rejected).
  - **The collarbones**: the jacket's taut neckline left a 6 mm groove over each, a dark slit in the game's light; the band now fades out over 8 cm (ADR-0026, Measured and rejected).
  - **The spawn**: it lies by the pole, where the sun never climbs above 4°. Moving it to flat land at a temperate latitude showed two things. First, a standing character crept downhill at 5 mm/s: the legs stood along the radius, not the plumb line (the planet's centrifugal pull), so it never slept; standing along gravity with the centrifugal pull fixes it. Second, two gates depend on chaotic contact and on where they are measured: walking into crates mispredicts 3–26 of 163 corrections by over 1 cm at the new place (the gate allows 1; even 64 times finer snapshots leave 3 of 8 placements over it), and the crowd scene's worst tick moves across its ceiling with a 0.0004 rad change of up. The user chose to gate both over ensembles and move the spawn (2026-10-22): done. The spawn lies on level ground at latitude 45° (ADR-0023, amended); walking into crates is gated over eight placements of the row and *crowd* over eight headings, each on its median and its worst, a tenth above what they measured (ADR-0025, amended; Baker et al. 2015's ensemble test). On level ground *crowd* is far lighter (worst tick 1.5 ms, 36 ms by the pole) and walking into crates predicts worse (slow link median 16 of 148 over 1 cm, 6.5 by the pole: the box meets the crates face to face; `docs/future-improvements.md`, for G1.8).

**Order:** G1.3a → G1.3b → G1.3c → G1.3f, the engineer (blockout, then the models) → G1.3d → G1.3e → G1.3g → G1.3h → G1.3i. The user, 2026-10-21: "lets finish 1.3c and then just do 1.3f (the engineer) before d and e". The engineer is modelled and approved in Blender first, and drawn in the game once G1.3e skins it. Each step ends with `cargo xtask ci` green and a commit pushed to `g1`.

### G1.4 Tools, reach and carrying

**Done** (2026-10-22, ADR-0027). The user approved the plan and chose, as recommended: the motion first, full finger bones, tools picked by number keys until G1.6, reach 2.5 m from the hands. Its survey of prior work is in ADR-0027. The sub-steps, each ending with `cargo xtask ci` green and a commit pushed to `g1`:

- **G1.4a The body's motion** (the user's notes): the glitches recorded and found (a scripted walk, run, start and stop with every bone's motion checked for jumps, and the cases the user saw replayed); the trunk leaning with speed and acceleration; stopping in a step that brings the feet together, then standing; the arms swinging from shoulder and elbow with measured amplitudes, the elbow bent with speed. Checked against the measurements and in a walk sheet and the planet sandbox.
  - **Done** (2026-10-22; ADR-0027, G1.4a, Measured and rejected, Validation): the probe over the real server and client (`animate/probe_tests.rs`) found G1.3's spikes, up to 48.5 rad/s over the frames beside them (setting off, the jump, landing), and the stride never stopping after the run. Transitions inertialized (Bollo, the offsets from both poses' velocities as Holden), the air found by the body's motion, each step keeping its gait, where a foot lands settled through its swing, the hips on a smooth path coming back up fast enough for the stance knee to straighten (Perry), the run leaning 7.3° and into its acceleration, Murray's arms less the model's own elbow bend. Now: the worst spike 7.4 rad/s, no step in place after either stop, the run leaning 7.4°, the walking stance knee straightening to 1–6°; 0.010 ms a character a frame. The walk and run sheets on the style board. The user's play of it (2026-10-22) found the upper body seen several times on a fast screen, a lean back on stopping and a leg left bent after a jump: fixed the same day (ADR-0027, "After the user's play of G1.4a"; ADR-0025 amended: the avatar drawn between its ticks), the probe now drawing at 60 and 144 frames a second.
- **G1.4b The hands**: finger bones in the engineer's skin (three per finger and the thumb), weighted in the art pipeline and checked by the fit check; posed as a relaxed hand, gripping and open, by the client; the skinning budget kept (0.05 ms for eight engineers).
  - **Done** (2026-10-22; ADR-0027, G1.4b): thirty finger bones found on the posed skin (each finger by the mesh's connectivity past the webs, its joints by Hamilton and Dunsmuir's and Buchholz's proportions) and weighted at the export from each point's hand weight; posed by the client relaxed (Lee et al.'s measures, the fingers together), open, and closed on a handle (all joints closing together until a bone meets it, the thumb over the index); the fit check poses them relaxed and in a fist; both hands posed in 0.001 ms, eight engineers skinned in 0.020 ms as before. The hands on the style board (`hands.png`).
- **G1.4c Tools in the hand**: the pickaxe, shovel, axe and bow saw modelled in Blender in the style (`art-style.md` §8), their icons (G1.3h) and silhouettes distinct; held in the right hand on a bone of the skin; a sandbox kit picked by number keys until G1.6's inventory.
  - **Done** (2026-10-22; ADR-0027, G1.4c): four tools modelled each to a real one (a 5 lb pick mattock, Asaka's long round point shovel, Gränsfors Bruk's American felling axe, Bahco's 24 in bow saw), 882–1968 triangles, their masses from their volumes held to the listings within 15 % and their balances to the table's carry within 1 cm at every build; the tools' table (`data/tools.ron`); carried in the right hand at the balance (an axe "just behind the head", as the Forest Service carries it), the fingers closed on the handle; which tool a player holds the server's (protocol 9, `Command::Wield`, `Kind::Character { wields }`); keys 1–4 and 0; the icons and `tools.png` on the style board.
- **G1.4d Reach**: the crosshair's ray and what it hits, within 2.5 m of the hands, checked by the server against what the client saw; the 500 m commands only on creative and test servers.
  - **Done** (2026-10-22; ADR-0027, G1.4d): reach 2.5 m from the hands (`creeer_sim::reach`, the hands' centre on the body's axis); the crosshair's ray then the eye's, against the bodies as drawn and the terrain, ringed and named in reach; the server keeps the bodies within 8 m of players for a second and judges a reach on the body as it was at the tick the client drew it (a predicted one as it is, by as far as it can have gone), within reach and in sight past the ground; the sandbox's commands a creative server's alone (`--creative`; the local sandbox, the play harness and the tests creative). Tested over a 100 ms, 5 % loss link: a walking player reached as seen 12 ticks before.
- **G1.4e Strikes**: a swing drawn as it is simulated, its energy from the tool and the arm (ADR-0027, Prior work), delivered as an impulse on what it hits and handed to digging (G1.5) and cutting (G1.7).
  - **Done** (2026-10-22; ADR-0027, G1.4e): a tool's spin from its swing-weight about the handle's end (Cross and Bower's law, n = 0.27, its constant ordinary men's measured overhead strikes): the axe 17.3 m/s and 241 J, the pick 15.9 m/s and 338 J; its inertia from the model, checked at every build; the blow 1 s after the swing begins, one every 2 s (30 strokes a minute); an impulse as the collision of the tool's and the body's effective masses, along the head's path; the swing begun at the player's own tick and told to everyone who sees them (protocol 10, `ServerMessage::Swing`), drawn raised, struck and brought back; the left click strikes what is in reach. Tested over a 100 ms, 5 % loss link.
- **G1.4f Carrying and throwing**: the grab as a joint at the hands with a force limit from human strength; the load slowing and toppling the character; throws by the measured speed-mass law, the reaction on the thrower.
  - **Done** (2026-10-22; ADR-0027, G1.4f): a grab held by compliant hands (after the user's play: "the boxes you pick up almost float when moving"): the load's weight and planned acceleration given ahead and an arm's measured stiffness and damping (445 N/m, 0.47 of critical) towards where the hands mean it, its turn kept to the way the body faces, the same force back on the body, the load drawn in the predicted hands; lifted up and then in with half the spare strength, held at the elbows' height clear of the body, turned within that strength; the hands hold up 575 N (58.6 kg, McDaniel et al.): heavier is refused, a pull past it over 0.1 s drops it; thrown at Cross's overarm speed for its mass, the thrower pushed back; no load the hands lift tipped the body over. The physics' joints amended (ADR-0022: the applied force, moved points with their velocities, their work counted by the energy guard). Protocol 11; G grabs and drops, the left click throws; the arms drawn holding it. Walking with a 20 kg load over a 100 ms, 5 % loss link: no correction over 1 cm in 87.
- **G1.4g Play scene and review**: a play scene of players working (reach, strikes, carrying) gated as the others are; shown to the user.
  - **Done** (2026-10-22; ADR-0027, G1.4g): the play scene *work*: four players striking crates with axes and picks, four carrying, throwing and fetching 15 kg boxes, all asked as the game asks; every blow falls, no load is dropped; worst tick 3.0 ms, p99 1.9 ms, 81 kbit/s; gated and its hashes golden. The user played G1.4 and asked for compliant hands, done.

- **Tools as items** (the original plan. Re-planned 2026-10-22: players make tools from pieces, in one generic system (G1.6d). These four steel tools are the creative kit and the test of the generic model). Hands, a pickaxe, a shovel, an axe and a bow saw. Each is a discrete item (G1.6) with a mass, a head material and a striking energy per swing (estimated at first as a pickaxe about 60 J, an axe about 80 J; worked out in G1.4e from measured swings: the pick 338 J, the axe 241 J). Powered tools come with power in G3–G4.
- **Reach.** Actions work on what the crosshair ray hits, within 2.5 m of the character's hands (the server checks the reach). The F5 sandbox's 500 m commands are kept for creative and test servers only.
- **Carrying and throwing.** A grab is a ball joint (G1.1) between the hands and the body's grasped point, with a force limit from human strength (about 50 kg carried easily, 150 kg lifted briefly, heavier only dragged as friction allows). The grasped body's weight acts on the character, which slows and can topple it. Throwing is an impulse from the arm's stroke, with its reaction on the thrower.
- **Strikes.** A swing hits what the ray finds. The strike's energy goes into what it hits: dug rock (G1.5), cut wood (G1.7), or an impulse on a body.
- **The animation, from the user's first play (2026-10-22, after G1.3):** "the animation is very rough and it sometimes glitches, when running the player does not lean forward, also when stopping walking it does some more steps without moving, the arms look robotic, the fingers do not move and are always apart from each other: we can improve these things in 1.4". Each is researched first (measured gait and hand data, as G1.3g's gait was) and checked in the game:
  - **Glitches:** caught in a recording and found, before anything is smoothed over;
  - **Leaning into a run:** the trunk tilted forward with speed and acceleration, as measured in running;
  - **Stopping:** the stride ends in a step or two that bring the feet together, and no stepping in place once the body has stopped;
  - **The arms:** a natural swing (shoulder and elbow phased against the legs, the elbow bent more when running), not a rigid pendulum;
  - **The hands:** fingers at rest curled and together (a relaxed hand), closing on what is held (G1.4's tools). The hands need finger bones in the skin: today the fifteen bones are the ragdoll's, so the hand is one rigid piece.
- **Validation:** a carried 50 kg crate slows walking as its weight on the legs predicts; a 300 kg crate cannot be lifted; throwing conserves momentum; reach is enforced against a cheating client (commands from too far are refused).

### G1.5 Digging with real composition

Re-planned 2026-10-22 (principles 9–11). It comes after G1.6, because the player digs with what they made.

- **Done** (2026-10-23, ADR-0032): one law V = η·E/e_s (e_s the compressive strength, or soil's Mohr–Coulomb passive resistance), η from the head's tip and its hardness against the rock's minerals; abrasive wear by each mineral's hardness and the ground's strength, edges blunting; bare hands in loose ground only. What is dug lies at once as a heap of the ground's composition at its grade; each spot's ledger cuts a pit (0.5–0.6 m) once it holds the smallest the finest grid draws, the field losing exactly what was dug (the user chose this over finer terrain, 2026-10-23). Measured: 0.05 ms a blow, 2.3 ms the rare blow that cuts; mass closed to 10⁻⁹ over 10,000 blows. Save version 12 (`PITS`). Hardness and tensile strength beyond the compressive: hardness per mineral; Young's modulus, fracture toughness and grain were G1.6c's. Not built (ADR-0032, Limits): heads chipping, pits across two grounds closing mass per ground, terrain finer than the 1 m grid.

- **Rock and soil strength data** (written in G1.6c, which needs it first for loose stones). `rocks.ron` gains each rock's mechanical properties: uniaxial compressive strength, tensile strength, Young's modulus, dry density, fracture toughness, grain size and hardness. Soils get cohesion and friction angle instead. Each entry is cited from rock-mechanics references, as the materials are, and the validator checks the ranges.
- **Digging, one law for every tool.** A strike removes the volume its energy can break: V = η·E_strike / e_s. Here e_s is the rock's specific energy of excavation (from its compressive strength, the standard rock-cutting relation). The efficiency η comes from the tool as built (G1.6d): its head's shape (a point, an edge, a blade) and its hardness and toughness against the rock's. There is no table per tool kind. A stone-pointed pick and a steel one follow the same law, and a head too soft for a rock barely marks it and wears away. Bare hands dig loose soil and gravel only. The brush is the head's own shape at the strike point.
- **Tools wear and break.** Each strike takes from the head: wear by the ratio of the hardnesses, and chipping or fracture where the blow passes the head's toughness. Each strike also loads the joint (G1.6d), so a head can break off or fly off.
- **Composition.** The dug volume's solid mass is ρ·V·(1 − porosity). Its minerals come from the geology at that point, including ore grades from the deposit models (ADR-0006), integrated over the brush, not taken from its centre alone. The result is bulk (G1.6a): mass, mineral composition, temperature.
- **What the player sees** (principle 9). A dug bulk shows what can be observed: its mass, its volume and so its density, its colour, grain and lustre. Its minerals and grades are the server's truth, shown on creative servers and in debug views. The player identifies them by tests they build themselves (streak, density, roasting, assay: G3 on). There is no hand lens and no prospecting overlay.
- **Mass conserved.** The mass taken from the terrain equals the items' mass, to rounding. The terrain's removed volume is measured from the same density field as the edit.
- **Not in G1** (principle 10, `docs/future-improvements.md`): small edits kept visible at a distance (a dug pit seen from 1 km).
- **Validation:** volume per strike against the specific-energy relation, for three rocks and two heads (stone and steel); a stone head wearing faster on granite than on sandstone, as the hardnesses say; the composition of a dug porphyry sample against the deposit model's grade at that point; mass closure over 10,000 strikes.
- **Budget:** a strike (edit, composition, item, wear) ≤ 0.2 ms on the server; the chunk rebuild off the tick (as now).

### G1.6 Items, the start, and making tools

Re-planned 2026-10-22 with the user (principles 9–11): no landing pod and no starting kit. A player starts in the clothes they wear and makes everything else from what lies on the ground. **The user plays it at G1.6's end:** spawn with nothing, gather stones and branches, knap a stone, haft it, and strike with your own tool. This is the first playtest of the new start, and it comes before digging and trees are built on it.

#### G1.6a Items and inventory

- **Items.**
  - **Bulk:** a mass, a composition (minerals or species, mass fractions), a temperature and a particle size. Merging two bulks conserves mass and each species exactly (the composition is the mass-weighted mix) and energy (the temperature follows the heat capacities, `creeer_materials`).
  - **Pieces:** a material and a shape. The shape is a parametric solid with dimensions, from which mass, volume and inertia are computed. A piece also has its quality, wear and any plastic deformation.
  - **Assemblies:** pieces joined (a tool, G1.6d; later anything built).
  
  There is no list of authored item types: a stone, a branch, a stone axe and a steel beam are all pieces or assemblies.
- **Where things are carried, by mass and volume.**
  - The hands: G1.4f's grab, one load.
  - The clothes' pockets: their litres from the outfit's real pockets (the coverall's thigh pockets, the jacket's chest pockets, the belt's pouch).
  
  The character carries their mass. There is no pack or bag until a player makes one.
- **Dropped things are bodies:** a piece as its own shape, a bulk as a heap of its volume.
- **Containers are generic:** any item with an inner volume holds what fits in it. The sandbox's crate is one; baskets and pots will be once they are made. A container's physics mass follows its contents.
- **Transactions.** Every move between hands, pockets, containers and the world is one server-authoritative transaction, atomic and ledgered: mass, species and items conserved, tested over random sequences (proptest).
- **Saves.** A new section for what each player carries and each container holds (the next save version, migrating the last). Dropped things are bodies like any other.
- **The inventory window:** a plain egui list (mass, volume, what can be observed) from which a thing is taken into the hand. In survival it replaces the number keys. It shows text, not icons, since made things have no authored models to render icons from.
- **Validation:** conservation under random transfers; a crate's mass and inertia following its contents; full pockets refusing more; save and load mid-transfer; a client asking to move what it does not have is refused.
- **Done** (2026-10-22, ADR-0028):
  - pieces, bulks and containers in `creeer_sim::items`, and pockets in `wearables.ron` (3.6 l in five, as modelled);
  - a piece fits a pocket by each of its edges and the room left (DayZ's rule); bulk in a store is one mix;
  - the pockets' load on the body, apart from the jetpack's gas;
  - E puts away, F puts into a container or looks into it, I shows the pockets with what can be seen of each item; the creative kit (`kit.ron`) dropped from that window;
  - save version 10 (`ITEM`), protocol 14;
  - validated: transfers conserve mass and species to 10⁻¹² (proptest); the world's whole mass unchanged by every move; a save keeps pockets, items and the body's mass.

#### G1.6b The survival start

- **Survival** (the default server, and the singleplayer game's: the user, 2026-10-22; `--creative` gives the sandbox's tools). A new player spawns at the world's spawn in the starting outfit, with nothing else: no tools and no food. They have no jetpack: the character has no tank (its dry mass), and thrust is refused. After a death the player respawns at the world's spawn, and what they carried lies where they fell. Nothing refills: today's respawn refills the jetpack (`world/people.rs`), and that is removed.
- **Creative and test servers** (`--creative`, ADR-0027) keep handing things out: G1.4's four steel tools, the jetpack and its refill, crates, and the 500 m commands. The play scenes use them where they test something else.
- **Wearables as items** wait for the first wearable a player can make (the jetpack, G3 on). Until then the outfit is fixed.
- **Done** (2026-10-22, `world/survival.rs`, `survival_tests.rs`): a survival body is its dry mass, so thrust does nothing; tools refused ("make one"); dead, the player starts again at the spawn with empty pockets and nothing refilled, what they carried lying where they fell; creative refills the jetpack. The singleplayer game is survival; `--creative` gives the sandbox.

#### G1.6c Stones and wood on the ground

- **Loose stone,** from the local geology: the rock and its composition at that place. It lies as cobbles and pebbles on the ground, scree under cliffs and boulders on slopes, with sizes from measured distributions (talus and scree data, found in the survey). Stones are hull bodies, scenery until touched (principle 5): instanced, at no tick cost. They are picked up as pieces. A boulder too heavy to lift is only rolled.
- **Fallen wood:** branches and dead trunks where forest grows (the biomes' rule, which G1.7 places trees by), in the species' wood (G1.7's attributes), dry or decayed (strength reduced as measured).
- **Tool stone** (the user, 2026-10-22): chert (as nodules and beds in limestone) and obsidian (beside volcanic rock) are added to the geology. Good tool stone exists, but has to be found.
- **Breaking.** A stone struck with another, or dropped, breaks by its fracture toughness and the impact's energy, with the pieces' masses conserved. A branch breaks over the knee by its bending strength.
- **Validation:** placement deterministic (golden hashes); masses from the materials; a stone breaking at the energy its fracture toughness predicts; parking and waking exact (ADR-0019); a square kilometre of ground with its stones at no tick cost.
- **Done** (2026-10-22, ADR-0029): stones and branches in cube-sphere cells of about 8 m, from the seed (`creeer_sim::scatter`, `geology/loose.ron`), of the ground's rock or its bedrock, with chert and obsidian as nodules; picked up into the hands (G) or a pocket (E) as items, remembered taken, saved and told to clients; a stone shows its size, mass and colour, not its rock; items drawn in their material's colour. Breaking stones and the rocks' strength data moved to G1.6d, where knapping needs them. Found: the hands lifted a pebble in two ticks and lost it (the arms' own mass now counts, ADR-0027 amended).

#### G1.6d Making tools: one generic tool

- **A tool is an assembly:** a head (a piece of stone now, metal from G3) on a haft (a piece of wood), held by a joint (a binding, or the head wedged into a split haft). Everything it does comes from its pieces:
  - its mass, inertia and balance, from their shapes and densities;
  - its swing's energy, by G1.4e's law from that inertia;
  - how it cuts or digs, from the head's shape (a point, an edge and its angle, a blunt face) and its material (hardness, toughness);
  - how fast it wears and whether it breaks, from the same.
  
  There is no table of tool kinds: "axe", "adze", "pick" and "hammer" are names for shapes, not types.
- **Shaping stone, kept simple.** The head's shape is parametric (length, width, thickness, a point or an edge, its angle). It is drawn procedurally from that shape, in its rock's colour.
  - Knapping: each strike with a hammerstone takes off a flake, its mass from the blow's energy and the rock's fracture toughness. Whether the edge comes out sharp depends on the rock's grain size and isotropy, a data attribute. Fine-grained, brittle, even rocks (chert, flint, obsidian, quartzite) flake to sharp edges; coarse ones crumble.
  - Grinding on an abrasive stone: it removes volume at a rate set by the hardnesses, slowly, as ground stone axes were made.
  
  No fracture is simulated in 3D. The rates come from experimental archaeology (a short survey).
- **Working wood:** a branch broken or cut to length, and split with a stone edge, at rates from the wood's strength.
- **Joining.**
  - A binding of fibrous strips (spruce roots, inner bark or withies: the survey decides which, with their tensile strengths), or the head wedged into a split haft.
  - The joint's strength comes from friction and the binding's tension. A loose joint lets the head turn or fly off.
- **Hands are a tool too:** soft and weak, with nothing sharp.
- **How a player makes one.** Making is actions on what is in the hands and on the ground (strike this with that, grind this on that, bind these together). Each action is a server transaction that takes real time. There is no recipe list and no menu of what can be made: the player has to know how a stone axe is made (principle 9).
- **G1.4's four steel tools** stay as the creative kit, and they become the generic model's test. Built from steel and hickory pieces, the model must give their masses, balances and swing energies as `data/tools.ron` has them (within its 15 % and 1 cm). From G3 the same system takes forged and cast heads.
- **Validation:** a hafted stone axe's mass, balance and swing energy against a measured replica; the steel tools reproduced; a binding failing at its strips' strength; determinism.
- **Budget:** a making action ≤ 0.2 ms on the server; a tool's mesh built ≤ 1 ms on the client.
- **Done** (2026-10-22, ADR-0030):
  - edges as an attribute of any piece;
  - a blow breaks a brittle piece by fracture energy: chert knaps to a sharp flake, granite crumbles, mass kept exactly (each rock's grain size and fracture energy in `rocks.ron`);
  - bark peels from fresh branches as bast strips, which roll up in pockets;
  - heads bind to hafts as one generic assembly whose swing follows from its parts: a steel axe made of its parts swings as the measured one, within 5 %;
  - a blow past a binding's hold takes the tool apart;
  - controls: the left click strikes with what the hands hold, T throws, R strips, B binds;
  - found: a made tool's inertia axes are not principal, and the hands' turn spun it up (Gershgorin's bound now);
  - not built: grinding, a tool drawn by its parts (it is its bounding box).
- **Played** (2026-10-23): fixed after the user's play: stones flickering (cells skipped), silent refusals (told now), carried sticks spinning (hands pull at the centre of mass), loose stones not strikable (`StrikeLoose`), knapping freezing the game (small things at rest laid as scenery, `world/laid.rs`), tools drawn as one box and named "stone" (drawn by pieces, named by shape), binding a lying flake impossible (`BindLying`), and a hint line under the crosshair (what the hands hold, the keys, what came of the last act).
- **Next:** G1.7 (the user, 2026-10-23: before G1.5).

### G1.7 Trees: one generic tree

Re-planned 2026-10-22 (principle 11; the user: "make a generic system and then the improved world generation can use that system to have bioms with different trees (shape, colour, size, etc.)").

- **A tree is a species' attributes,** all in the packs' data:
  - its wood: density, moduli and strengths along and across the grain (EN 338 and the Wood Handbook);
  - its growth form: height against trunk diameter (allometry), the trunk's taper, branching (angles, counts, lengths by order), the crown's shape, the foliage's mass and clumping;
  - its look: bark and foliage colours (palette tokens, checked by `art-style.md`'s rules as any token is), the bark's pattern;
  - where it grows: its ranges of climate, altitude, slope and soil.
  
  One generator turns any species into trees. A new tree is a data entry, not code. G1 ships spruce (its wood, EN 338 C24, exists) and at least two species of different form (a broadleaf and a pine, say), which prove that the system is generic. The improved world generation's biomes then choose among the species by their ranges.
- **Procedural trees.** A tree is a beam structure (ADR-0004): trunk and branches as members of the species' wood, with tapering sections, and the foliage's mass hung on the branch tips. Trees are placed deterministically from the seed, where their species' ranges allow. Each tree's shape is seeded, so no two look the same.
- **Scenery until touched** (principle 5). A standing tree is a static instance with no physics per tick. A strike, a cut, a collision or a load turns it into a structure body. It parks again once it has fallen and come to rest (ADR-0019).
- **Felling as it really works.** Each strike removes wood from the trunk's section at the cut, by the tool as built (G1.6d): its edge and energy against the wood's strength across the grain. So a stone axe fells more slowly than a steel one, by the ratio experimental archaeology measured (found in the survey). The tree stands until its own weight and the wind on its crown exceed what the remaining section carries. Then it breaks and falls towards the notch side, as a felled tree does. The wind (ADR-0014's uniform wind) sways crowns and can decide the fall.
- **Bucking** with an axe (a saw needs metal teeth: G3). The trunk splits at the cut, and the logs are bodies (a 3 m spruce log of 30 cm weighs about 90 kg: one person barely carries it, and rolls it easily). Branches break off as pieces: hafts and firewood (G1.6).
- **Drawn plainly** (principle 12): the trunk and branches as their members (tapered cylinders in the bark's colour) and the crown as a rough mass in the foliage's colour, instanced, with levels of detail and impostors only as far as the frame budget needs them.
- **Not in G1** (principle 10): grass tufts, bushes, pebbles and branches as decoratives (G11).
- **Validation:**
  - no generated tree breaks under its own weight;
  - the cut depth at which a trunk fails, against the remaining section's capacity;
  - the fall towards the notch;
  - stone and steel felling times against the measured ratio;
  - placement deterministic;
  - a species added as data only, with no code changed.
- **Budget:** 100,000 standing trees in the loaded 2 km² cost nothing per tick and draw at 60 FPS on min-spec (impostors). A tree turning into a structure takes ≤ 1 ms, spread so that no tick takes more than 2 ms extra.
- **Done** (2026-10-23, ADR-0031):
  - species as data (`flora/*.ron`): spruce, pine and birch, their green woods from the Wood Handbook and their crowns' biomass from Repola; the example mod adds an oak with no code;
  - one generator: a tapered stem, branch nodes sharing its section (Leonardo's rule), the crown's biomass shared by the pipe model; trees placed cell by cell where their ranges allow, scenery until struck;
  - a blow takes out V = k·E/(σ⊥·(tan(θ/2) + μ)) by its edge and energy, k calibrated so a steel axe fells 30 cm of spruce in about 150 blows; a made stone axe is then 3.2–3.4 times slower, as measured (2.5–4.4);
  - a cut is the stem's section less its notches, checked by EN 1995's rule with green strengths; it goes over towards the notch, a rigid body on a hinge joint along its hinge's edge, let go when the notch closes;
  - limbing (branches off thinnest first, laid as pieces of their wood: hafts, firewood) and bucking (cut through, two bodies), mass kept exactly with the chips;
  - trunks are boxes about what moves (walked into, a falling tree lands on its stump), laid the same in the avatar's prediction;
  - drawn plainly (rods and rough masses, finer the nearer, to 160 m); the crosshair says a tree's girth and how far it is cut; protocol 22, save version 11 (`TREE`).
- **Not built** (ADR-0031, Limits; `docs/future-improvements.md`): felled parts as beam structures (they are rigid hulls), branches touching the ground, standing trees moved or broken by what hits them, wind on the planet, impostors (the forest costs 2.7 ms a frame).
- **Next:** G1.8 (P1, piles: the user, 2026-10-23), then G1.5 (digging). **G1.8 done** (2026-10-23, below): next G1.5.

### G1.8 P1: piles (ADR-0024; performance track)

**Done as a targeted pass** (2026-10-23, the user's choice after its survey and first measurements; ADR-0024, "Second pass"). The gates below are not met, and what they need is measured and recorded:
- **Kept:**
  - Turn substeps only for boxes the step can bring into contact (and bodies in hands).
  - Turn substeps and the turn check's redos per group: a group stepped beside one that substeps goes bit for bit as alone.
  - *crowd* over eight headings: p99 median 13.5 → 11.5 ms; worst tick median 46.8 → 25.9 ms, worst of the eight 286 → 67 ms.
  - The rock pile unchanged: its worst ticks are one group's iterations.
- **Measured, not kept:** every friction variant within reach, each within ±10 % (patch beside the ADR). The bound is friction off: −42 % iterations, the worst step halved.
- **Found:**
  - The ragdoll bench had been broken since G1.3f (its pairs passing are not saved).
  - The play trace counted the clients' steps.
  - Three gates were single chaotic samples (the rocks' work ceiling re-set on the evidence).
- **Left** (`docs/future-improvements.md`):
  - the turn limit as a box's corner sweep, proven on small fast boxes (for hulls it failed);
  - cheaper substeps;
  - friction within Newton with a residual merit, then the joint tree;
  - large groups across cores;
  - edge and face contacts consistent;
  - faces over curved ground cheaper.

The plan as it stood:

G1's scenes make piles: logs on logs, boulders, dropped bags, crates. The rock pile's worst tick (18.2 ms, ADR-0017) is measured again here on the real scenes, then cut.

**Its first pass, brought forward after G1.2d, has ended** (ADR-0024, "Outcome of the first pass", 2026-10-18): the legs' struts, and about 8 % off a ragdoll's step by exact, profile-found changes. Measured on the way, and the facts this milestone starts from: the joint tree alone is 6.5 % slower, as friction refreshes with the joints (the refreshes, 35–43 % of a ragdoll's iterations, wait on both); a see-saw between a face and crossed edges keeps some ragdolls under a crate from resting; a ragdoll slumps on for want of passive joint resistance. What follows is what remains, at its place in the order.

**Revised 2026-10-18, after its survey of prior work** (ADR-0024, "What others found"; the survey came late, after weeks on methods the literature had settled):
1. **Ragdolls as one articulated body in reduced coordinates** (the root's pose and the joints' angles), within the implicit step, beside the maximal-coordinate joints of ADR-0022 that stay for loops, grabs and machines (RedMax's hybrid). First as a measurement against the recorded ragdoll states, then built if it pays.
2. **Friction updated within Newton's iterations**, for realism (no friction on stale normal forces) and iterations.
3. **The split across cores** (below) for large piles, by consensus ADMM with a per-body adaptive penalty, as published work does for hundreds of bodies and more; not for a ragdoll's group of tens.

- **A large group's contact solve split across cores.** The group is partitioned into pieces large enough to be worth a core (hundreds of contacts each), each solved by Newton on its own, coupled by an outer iteration on the forces across the cuts until the whole is the same minimum as solved at once (the same tolerance: ADR-0005's convergence check, unchanged). The partition follows the contact graph and is kept from step to step (no repartitioning noise).
- **Fixed with it:**
  - friction on fresh normal forces: no contact's friction rests on a normal force more than 5 % off its own (today 19 % of the pile's steps);
  - edge contacts and face patches made consistent, so rocks no longer rattle awake between them;
  - turn substeps per group, not for the world;
  - faces over curved ground made cheaper.

  Collision boxes set off their bodies' centres of mass (the knee pads drawn sinking into the ground, deferred here from G1.3f) are polish: they moved to G11 on 2026-10-22 (principle 10, `docs/future-improvements.md`).
  
  Each is removed from `docs/future-improvements.md` when done.
- **Validation:** every contact, pile, stack and energy test at its present tolerance; the split solve against the whole solve on the same states (same minimum within the tolerance); golden hashes regenerated only after a change is proven rounding-only or deliberate.
- **Targets:** the rock pile's worst tick 18.2 → ≤ 5 ms, p99 14.2 → ≤ 2 ms; *quarry* and *lumberjack* within their gates; 50 crates on terrain at most 1.2× their cost on a plane.

### G1.9 The HUD: what play needs

Re-planned 2026-10-22 (principle 10).

- **Done** (2026-10-23): the vitals at the bottom left (`inventory_ui::vitals`): health, what each of the body's parts has taken (head, torso, arms, legs: the server keeps each impact's harm on the part its segment belongs to, `ragdoll::part`, healed in their shares; `ServerMessage::Own { hurt }`), down, and the jetpack's gas only while there is gas. The crosshair's keys carry what can be felt: "G: take in the hands (0.4 kg)" (laid things, item bodies and loose stones and branches have their mass), "click: strike (tool, 1.6 kg, 160 J)" and "click: dig (…)" with the held thing's swing energy (`ItemView::swing`). Protocol 23. Hands, pockets and the crosshair were G1.6's; transfers are tested there (`server/items_tests.rs`). A loaded person's loss is put on its torso (the parts are not saved).

- **Shown:**
  - health per body region;
  - what is in the hands and in the pockets;
  - a crosshair;
  - interaction prompts with real numbers ("strike: stone axe, 1.1 kg, 160 J", "pick up: 12 kg");
  - the jetpack's gas, only while one is worn (creative servers for now).
- **egui's own look,** with a few of the palette's colours.
- **Not in G1** (G11, `docs/future-improvements.md`):
  - the full UI theme and font (`art-style.md` §10);
  - icons in the inventory;
  - a compass and altitude;
  - golden screenshots of the HUD;
  - a first-person camera.
  
  The suit's readouts come with G9's needs.
- **Validation:** UI tests for transfers.

### G1.10 Multiplayer, play scenes and the demo

- **Everything replicated.** Characters (standing, ragdolled, swimming), strikes and their edits, made tools, loose stones, trees as they fall, items and containers, within 256 kbit/s per client in *crowd*.
- **Play scenes gated** (G1.0):
  - *crowd* and *work*, as now;
  - *toolmaking*: 4 players gathering, knapping and hafting;
  - *lumberjack*: 4 players fell 20 trees on a slope, with stone and steel axes; logs roll and pile;
  - *quarry*: 4 players dig a 10 m pit among loose boulders and dropped heaps;
  - *fall*: characters falling, ragdolling and getting up on terrain, crates and each other;
  - *forest*: one player walking through 100,000 trees of several species for 10 minutes (streaming, no tick cost).
- **The demo and playtest.** Spawn with nothing but clothes. Gather stones and branches, and make a stone axe and a pick. Fell a tree, dig ore, and carry both back in hands and pockets. Save and load. Then the same with a second player over the network.

## Blender MCP

**Set up and running** (2026-10-21): Blender 5.2 LTS with MCP for Blender (add-on 1.8), registered with Claude Code for this project only, telemetry off, its asset libraries and model generators off. How it is used, its conventions, safety and troubleshooting: **`docs/blender-mcp.md`**.

In short:
- each authored model is a Blender script in `art/`, run live in your Blender through the MCP while it is made (you can watch and comment);
- exports run headless (`cargo xtask art`) into `data/models/`;
- the `.glb` files are committed, the `.blend` files are not.

Terrain, trees and structures stay procedural.

**One note:** the add-on runs Python inside Blender that Claude writes. It is your local Blender, and you see the scene as it changes, but it has the access Blender has on your machine.

## Order and dependencies

G1.0 → G1.1 → G1.2a–d → G1.8's first pass (P1, brought forward: the ragdoll on the ground needed it) → G1.2e → G1.3 → G1.4, then (re-planned 2026-10-22) G1.6a → G1.6b → G1.6c → G1.6d (the user plays it) → G1.5 → G1.7 → G1.8 → G1.9 → G1.10.

- **Joints come before the character**, because the ragdoll, carrying and ladders need them.
- **The look comes before the content**, so the tools, items and trees are modelled in the style from the start.
- **Items and making come before digging and felling,** because in survival the player digs and fells only with what they made.
- **P1 comes after mining and trees**, so it is tuned on the real *quarry* and *lumberjack* scenes, not only the sandbox's rock pile.
- **The user plays each step once it is playable** (principle 10), from G1.6's end on.

Each sub-milestone ends with `cargo xtask ci` green and a commit pushed to `g1`.

## Decisions to confirm

Recommended here, to be confirmed with the user at the sub-milestone they belong to:

1. **A skinned body on the ragdoll's bones** (G1.3). **Decided by the user, 2026-10-21** (it replaced "a segmented suit instead of a skinned body"). The fifteen bones are the ragdoll's segments, so the drawn body is still the simulated one, bone for bone, but smooth across its joints. Rigid segments would show seams at every bent joint, against smooth surfaces (`art-style.md` pillar 9). It costs a compute skinning pass, and the ragdoll's segments must be known to the client (protocol 7).
2. **The landing pod as the start** (G1.6). **Decided by the user, 2026-10-22: no pod.** A survival player starts in their clothes alone ("clothes only, make tools"), respawns at the world's spawn, and nothing refills for free (G1.6b, principle 9). A staged arrival and its story are for G11.
3. **Cameras: first and third person** (G1.9). Third person for now; first person moved to G11 (principle 10). A higher, Factorio-like overview camera for factories is an idea for G3.
4. **Reach of 2.5 m for survival play** (G1.4), with F5's 500 m commands kept for creative and test servers.
5. **The jetpack's look** (G1.3f). **Decided by the user, 2026-10-21:** two small nitrogen bottles on a harness frame, a wearable on the back slot, so the thrust visibly comes from somewhere. It is not a rucksack, which the user asked to leave out. Later the same day the user moved it out of the starting outfit: it is made as the player progresses (G1.6b; its gas is compressed by the player, G4–G5).

## Budgets (development machine; min-spec projections ×2)

| What | Budget |
|---|---|
| Every play scene's worst tick / p99 | ≤ 5 ms / ≤ 2 ms (aim 2–3 ms worst) |
| A walking character / a ragdoll | ≤ 0.03 / ≤ 0.1 ms a tick |
| A 15-joint ragdoll's step | ≤ 0.1 ms |
| A strike (edit, composition, item, wear) | ≤ 0.2 ms |
| A making action (knapping, grinding, binding) / a made tool's mesh | ≤ 0.2 ms server / ≤ 1 ms client |
| Loose stones on 1 km² | 0 ms a tick |
| A tree becoming a structure | ≤ 1 ms, no tick more than 2 ms extra |
| 100,000 standing trees | 0 ms a tick; 60 FPS on min-spec |
| SSAO at 1080p | ≤ 1 ms min-spec |
| A model's load | ≤ 5 ms |
| Skinning 8 characters / animating one | ≤ 0.05 ms GPU / ≤ 0.05 ms CPU a frame |
| The mesh pass with surface detail | the F1 budget met; ≤ 10 % slower than before G1.3 |
| Bandwidth in *crowd* | ≤ 256 kbit/s per client |
| Prediction corrections over 1 cm | ≤ 1 % of corrections |
| The reference tick (plan.md §5) | still ≤ 10 ms projected |
