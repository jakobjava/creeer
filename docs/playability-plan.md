# G2.11 plan: playability

Status: **in progress** (planned 2026-11-13). Branch `g2`. G2 does not close before this does.

## Why

The user, 2026-11-13, after playing G2: "nothing seems to work and this game has many bugs, looks horrible, plays horrible, is not fun". Asked what to do about it, they decided:

- **Massively improve almost everything the player interacts with.** "The current state is unbearable."
- **No assisted building.** Principle 13 (plan.md) stands: no snapping, grids, numeric input, undo or "will it hold?" preview.
- **The bridge first.** The first thing that must be fun: build a bridge across a gap, load it, watch it hold or break.
- **As performant as possible.** No slow motion, no gate moved. Performance and realism are not traded (plan.md).
- **The rules are not made lighter.** Research first, ADRs, tests, gates, 400 lines a file: as before.
- **Enjenir as an orientation for the building system** (Prior work below).
- The work is done by several agents at once (Opus 5.5), coordinated from the main session.

The review that led here found: effort went into the avatar's hands and not into what is built; nobody looked at what the player sees while building it (tests passed, the game did not play); looks were deferred by rule; play scenes are far over their gates.

## What the player meets today (frames of 2026-11-13, `creeer_client --planet <view>`)

- The ground is one flat brown, shaded in visible triangles, with nothing on it.
- Trees are stacked ellipsoids on sticks; some trunks are drawn white.
- A rod (`post`, `rail`) is drawn as a square bar. Timber has no grain, logs no bark or end grain.
- Shadows have stair-stepped edges.
- In first person the player has no arms or hands. The `avatar` view's camera sits inside the body.
- There is no sound at all (`creeer_audio` is empty).
- Measured in G2.10: a client's prediction costs 44–85 ms a frame in *builders*; *log_bridge*'s breaking tick takes 2 s; loose poles never sleep; a holder slides back about 0.3 m while holding a post through work.

## Goal

**Ten good minutes:** start the game, find a gap, build a bridge across it from logs and timber without fighting the game, walk over it, load it until it breaks, hear and see it break, at a steady frame rate. Each part of that looks, sounds and handles like something made with care.

## Prior work: The Enjenir (PeatyTurf, early access 2023-12)

Surveyed 2026-11-13 from its Steam page, its community threads and a review summary.

- **What it is:** a physics building game. A third-person ragdoll engineer builds structures and vehicles from beams, logs, sheets, ropes, wheels and gears to solve missions, or freely in a sandbox.
- **Placing:** gridless. A piece is chosen from a menu and attached by its end to something that is there ("you can only place it by having something on the end of the beam"). It is scaled and turned by holding a modifier key and dragging (Caps Lock scales; Shift, Alt and Ctrl turn about the three axes). There are guide lines (G), a precision mode and undo (Z). Pieces are not carried into place by the character.
- **Joining:** nails where pieces meet. Nails are what fails.
- **Testing:** the structure is built with the physics off and tested by turning it on.
- **What players praise:** the physics, bracing a structure and testing it against gravity, many solutions to one mission.
- **What players criticise:** building is slow and fiddly (modifier keys; no auto-align or equal spacing; a beam cannot simply be laid across two others; a sheet offsets in one direction only), failures are hard to read ("I don't think I've seen something actually break"), the ragdoll walking.

Sources: store.steampowered.com/app/1800940; steamcommunity.com/app/1800940/discussions/0/4355618325415441334 and …/4029097331154053604; vaporlens.app/app/1800940.

**What it means for Creëer:** Enjenir places pieces without carrying them, which is at odds with G2's "hands are hands". The user was shown both ways of laying a beam across two posts, step by step, and decided (2026-11-13):

- **Parts are carried by hand, as now.** Not placed from a menu.
- **Real pieces only.** A part has the length it was cut to; nothing is pulled to length.
- **Fastened by hand, quickly:** aim where two parts touch, one key, one nail, about a second and a hammer blow. The player chooses where nails go and how many.

So Creëer takes from Enjenir its feel, not its placing: gridless free-form building, a part put against what is pointed at, full control of how a part is turned, structures that fail at their fasteners, and failure that is easy to read (Enjenir's weakness, Creëer's solver's strength).

## Streams

Each stream is one agent in its own git worktree. Streams A1–A7 do not depend on the building design and started at once; B and C started after the user's answer above.

| # | Stream | The player's problem | Owns (other streams keep out) |
|---|---|---|---|
| A1 | **Ground, sky, light** | Flat brown faceted ground, nothing on it, stair-stepped shadows | `creeer_render` terrain, shadow, atmosphere and their shaders; `creeer_client/src/terrain_mesh.rs`; ground cover |
| A2 | **Things** | Square rods, plain timber, lollipop trees, white trunks | `creeer_client/src/{models,hulls,tree_draw,tree_view,loose_view}.rs`; `creeer_render` primitives and meshes; `art/` |
| A3 | **HUD, prompts, menus** | The player cannot tell what they can do, what a key does or why something was refused | `creeer_client/src/{planet_ui,inventory_ui,control_panel}.rs`, `creeer_ui` |
| A4 | **Moving and looking** | Walking, jumping, the cameras, standing on what was built, sliding while holding | The character on the server (`world/people.rs` and what moves it), `creeer_client/src/{planet_camera,fly_camera}.rs`, `animate/` gait |
| A5 | **Frame time** | Stutter while building: the client's prediction 44–85 ms a frame | `creeer_server/src/{client,predicted}`; `creeer_client/src/{play_bench,planet_net}.rs` |
| A6 | **Tick time** | The game freezes 2 s when a plank breaks; loose poles never rest | `creeer_physics`; `docs/g2p-plan.md` (written at its start, as plan.md asks) |
| A7 | **Sound** | Silence | `creeer_audio`; its hooks in the client |
| B | **Building by hand** | Taking, carrying, placing, turning and fastening a part without fighting the game; the player's own arms and hands in first person | `creeer_server/src/world/{hands,carry,building*,tacking,grip_told}.rs`, `creeer_server/src/client/{place,hold,steered,aim}.rs`, `creeer_client/src/{holding,held,held_load,fastening}.rs`, `animate/` arms and hands, `creeer_building`, the protocol and save versions |
| C | **The bridge site** | Nowhere to build a bridge, nothing to load it with, no way to see why it broke | A start straight at a prepared gap with parts and loads; `data/kit.ron`; the stress overlay and what shows a failure; a `bridge_demo` play scene |

## Rules for every stream

The project's rules hold as written (plan.md §1, §6, Phase B's principles; `docs/art-style.md` is the authority on looks). In short, and with what working side by side adds:

1. **Research first.** Before designing, survey how others solved it (papers, engines, games), briefly and to the point, and record it as the Prior work of the ADR you write or amend. One ADR per decision of weight; small fixes need none.
2. **Look at what the player sees.** Before changing anything, render frames of the present state; after, render them again and read them. Work is not done because its tests pass: it is done when the frame, or the played script, shows it. Frames go in `<frames>/<stream>/` as `before-*.png` and `after-*.png` (`<frames>` is given in your brief).
3. **Never loosen a test, tolerance or gate.** If one stands in the way, stop and report it. Golden hashes are regenerated only for an intended change, explained in the commit message (plan.md §6). Style-board goldens changed on purpose are saved (`cargo xtask style-board --save`) and listed in your report: the user approves the looks at the check-in.
4. **Performance is part of every change.** Measure the frame or tick before and after. Other streams compile while you measure, so wall-clock times are noisy: compare Newton iterations and other deterministic counts first, take times as the best of several runs, and say which numbers the coordinator must measure again on a quiet machine.
5. **Files:** at most 400 lines per `.rs` and `.wgsl` file. Data first: what a modder would change lives in `data/`.
6. **Deferrals and failures are written down:** a limit left goes in `docs/future-improvements.md` (your area's section only); an idea measured and rejected goes in the ADR's Measured-and-rejected table.
7. **If an approach fails twice, stop,** write down what was measured, choose the simpler way and say so in the report.
8. **Stay in your stream.** Other streams' files (table above) are changed only where a few lines cannot be avoided; list those lines in your report. Do not edit `plan.md`, `docs/g2-plan.md` or this file. Only stream B changes the network protocol or the save format.
9. **Commits:** small, on your worktree's branch, each with the tests of the crates it touches passing; `cargo xtask ci` green before your last commit. Messages start `G2.11 <stream>:`. Do not push, merge or rebase: the coordinator merges.
10. **Disk and machine:** the user's disk is watched. Prefix every cargo command with `CARGO_INCREMENTAL=0`, build release only when you need frames or timings, start no further agents.
11. **Report** when done (and only then): what changed in the player's words, the frames to look at, numbers before and after, what is not done and why, your commits, and any file where a merge may conflict.

## State of the streams

Kept by the coordinator as streams are merged.

| Stream | State |
|---|---|
| A7 Sound | Merged 2026-11-14 (ADR-0037). 16 reference sounds rendered for the user's ear |
| A6 Tick time | First pass merged 2026-11-14: the "breaking tick" was a felled player's ragdoll laid inside a crate (ADR-0005 amended). Going on: two defects in how wood breaks, the pile that does not converge, loose poles (`docs/g2p-plan.md`) |
| A3 HUD | Merged 2026-11-14 (ADR-0038). Its volume setting drives the sound |
| B Building by hand | First pass merged 2026-11-14 (ADR-0036 amended): a part taken stays as it lay and is carried low; a pole laid across a gap; one nail a key press, timed by its blows; the own body drawn in first person. Open: parts caught on others, one nail only between parts already joined, the nail driven through the part aimed at, no hammer drawn, no plank on edge |
| C The bridge site | Merged 2026-11-14 (ADR-0039): `--bridge`, the kit's footbridge timber and loads, the stress overlay (O), what failed first, the `bridge_demo` scene |
| A1 Ground, sky, light | Merged 2026-11-14 (ADR-0040): the facets (a detail texture breaking at chunk borders), smooth shadow edges, contact shadows, the forest floor's layers, ground cover, the forest from afar |
| A2 Things | Merged 2026-11-14 (ADR-0041): round rods, wood with grain and cut ends, trees as trees, built members and broken pieces as timber |
| A6, second pass | Merged 2026-11-14 (docs and probes): the two defects in how wood breaks fixed and validated as a patch, held back until the energy guard's hole is closed and a made member starts in its sag; the bridge's freeze traced to its nails pulled by the first person stepping on |
| A5 A client's frame | Merged 2026-11-14 (ADR-0043): a part fastened told to clients as the structure it became; a client's work in builders 40 % less, log_bridge 70 % less; gated at the bridge site. Not met: the whole frame within 16.7 ms at worst |
| D Determinism | Merged 2026-11-14: the simulation is deterministic; the stray hash was a world whose tree data failed to load, dropped without a word. Trees that do not load now stop the world |
| Q The play-through | First pass merged 2026-11-14 (ADR-0038 amended): the ten minutes played through the client with the whole screen drawn (`--build-play site`), the screen's seams fixed, the rest handed on (below) |
| B2 Building, second pass | Merged 2026-11-15: every crossing nailed, also within one structure; several nails add up; the nail driven from the player's side; a nail in a held part holds at its last blow; Z rolls a part; scenes' players steer what they hold |
| A4 Moving, second pass | Merged 2026-11-15 (ADR-0042): the cameras; the legs press a deck with a person's weight: the footbridge holds under the first step |

**Stopped for the night on 2026-11-15, each with work in its worktree (branch `worktree-agent-<id>`), to be resumed by a message to the agent:**
| Stream | Agent id | Committed there, not on `g2` | In hand |
|---|---|---|---|
| A4, third pass | `ad6a804a2cb8db3b3` | nothing | the feet's footprint: walking the footbridge's deck without falling through its gaps (8 files uncommitted) |
| B2, third pass | `aa619e762443efa77` | nothing | a stringer laid across the site's gully in the game itself (gate: `--build-play site`, ten of ten on both links); a held part must not strike what it is laid against (14 files uncommitted) |
| N (A6's stream, taken over) | `a70d27039d743ffa3` | A6's third pass (how wood breaks, validated; the energy guard's hole; members starting in their sag), `g2` merged in, a nail in withdrawal yielding within the step (12 commits) | the nail law's tests; then re-measuring the gate `a_builder_at_the_bridge_keeps_her_client_within_its_work`, which is what keeps the wood fix off `g2`; then the piece that falls through the ground, the pile, loose poles |
| Q, second pass | `a416ece14d84d8024` | a collapse heard; a body resting on a structure drawn where the server has it; a kit part put on the highest thing under it; where "Leave" goes (12 commits) | lone pieces heard; then CI and its report |
| L2 Looks at the site | `a34bf0243cc078ccf` | the gully's saw-tooth and checker (colour edges between vertices, not shadows); the loads drawn as what they are (3 commits) | what drawing costs at the site; the overlay's grey world |

None of the committed work above was run through CI by the coordinator: merge only what an agent reports as checked.

**What the play-through found (Q, 2026-11-14), handed on:**
- *Building (B2):* laying a 3 m stringer across the site's 2.3 m gully mostly fails (6 of 13 attempts over four runs, 0 of 4 in the last) and can pull the player in: aimed over the gap the ray meets the floor beyond reach, slid out from the bank the stringer tips down the wall. A part taken by its end stands up in the hands. A board's second nail is refused. Prompts offered that will be refused (J, U, K on a lone part; P and T on a held stringer).
- *Character (A4):* sliding on 1.2 m after letting go of W, into the gully; tripping on a stringer's end; no way out of the gully by walking at its wall; looking down at a part held low, the view is from above the stooped body.
- *Physics (A6):* the built bridge breaks under the first step; a 25 kg block carried over the deck breaks a board "when struck"; under test weights the deck is drawn whole while the weights are drawn under it.
- *Sound:* a collapse makes no sound (a structure that splits into new pieces is never cracked).
- *Looks:* a saw-tooth edge on the rim's shadow on the gully's floor and a checker pattern on its steep wall; loads are plain boxes; built members round bars and broken pieces blobs until the protocol tells their profile.

**The blocker for the ten minutes** (found by C): people standing on the nailed bridge cost up to 228,742 Newton iterations a tick (ticks of 1–10 s). A6's next item after the wood defects; A4 measures the character's part.

**What the frames still show wrong** (the coordinator's own look, 2026-11-14): in first person the player sees their own head and shoulders from above when the body stoops (A4); a built deck is drawn as grey round bars and a broken structure's pieces as large dark balls (A2).

**Wanted from the protocol** (only stream B changes it; to be scheduled with B's report):
- For sound (ADR-0037, Limits): each loaded structure's most-loaded members and their share of strength (creaking); a member's material; which element broke; the target of a `Swing` and a `Fastening` note to nearby clients (other players' blows).
- For the HUD (ADR-0038, Limits): which bodies are fastened ("Take apart" is offered on every part today); whether a block is a container; whether a post stands in a hole; whether the server is creative (the kit is offered on survival servers and refused there).
- Pausing a singleplayer world behind the Esc menu (a server matter).

**For the user to decide at the check-in:**
- The crosshair's plate sits beside the screen's centre, which `docs/art-style.md` §10 keeps for the world: approve, or move it.
- The looks' new golden images (A1, A2).

## Check-in

When the streams are merged the user gets: a build to play, a short note of what to try, and before/after frames. Their word decides what is good. Feedback is taken in before anything further is built on top.
