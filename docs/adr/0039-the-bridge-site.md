# ADR-0039: The bridge site: a place to build a bridge, load it and see why it broke

- **Status:** In progress (G2.11 stream C, 2026-11-13; `docs/playability-plan.md`)
- **Context:** the user found G2 "not fun" and chose the bridge as the first thing that must be fun: build a bridge across a gap, walk over it, load it until it breaks, in ten good minutes. There was nowhere to do it: the player started on flat ground with no gap, had to know a key and a kit index to get parts, had nothing made for loading a bridge, and when something broke nothing showed what failed or why.
- **Builds on:** ADR-0033 (fastened joints), ADR-0036 (holding by hand, first-person placing), ADR-0027 (what a person carries), ADR-0028 (the creative kit), ADR-0032 (digging: terrain edits), plan.md principle 13 (precision is technology: debug views are a creative server's information).
- **Evidence:** `creeer_server`'s `world/site.rs` (the site), `world/site_watch.rs` (what failed, the overlay's numbers), `play/bridge_demo.rs` (the scene); `creeer_physics`'s `structure/carried.rs` (what each element carries); `creeer_client`'s `bridge_site.rs` (`--bridge`), `stress_view.rs` (the overlay, O), `planet_shot_bridge.rs` (its frames); `data/kit.ron`. Tests: `world/site_tests.rs`, `world/site_watch_tests.rs`, `play/bridge_demo_tests.rs`, `creeer_sim`'s `items/tests.rs`.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Poly Bridge (Dry Cactus, 2016) and Poly Bridge 2–3 | A level is a gap, a budget and vehicles that drive across when the player runs the test. The first levels are short gaps a few members span. Stress View colours each member by its load towards red; members that fail break visibly and the vehicles fall | Taken: a short gap first, a test the player runs by loading it, members coloured by their load. Not taken: the budget, the build-then-run split (Creëer's bridge is always simulated) |
| The Enjenir (PeatyTurf, 2023; `docs/playability-plan.md`) | Missions set a goal at a prepared place; joints are nails, and nails are what fail. Players praise testing a structure against gravity and complain that failure is hard to read ("I don't think I've seen something actually break") | Taken: a prepared place with a clear goal, nails failing. Its weakness is where Creëer's solver is strong: every member's and joint's force is known, so a failure is told with what it was and what it carried |
| Structure Collapse (a browser bridge simulator) | Beams coloured cyan → yellow → red by stress; a static weight and a wrecking ball as tests | Confirms the overlay's reading; a heavy test load as the finale |
| BTCV, *Footpaths* (conservation handbooks), "Footbridges" | Sawn-timber footbridges by span: three 150 × 75 mm beams for 3.0 m at a 750–900 mm deck; decking 150 × 50 × 1125 mm, four to the metre, 50 mm over the outer beams; handrail posts 100 × 75 mm cantilevered from the beams at up to 1 m; rails 75 × 50 mm to 2.1 m between posts; log beams of 250 mm for 5 m | Taken: the kit's timber is this bridge's (`data/kit.ron`) |
| Proof-load testing of footbridges (practice: sandbags, water tanks, concrete blocks set by crane) | A bridge is tested with known weights set on it in steps | Taken: 1 t test weights set from the kit, as a crane would set them |
| HSE manual handling guide weights; NIOSH (Waters 1993); ADR-0027 | 25 kg at the waist comfortably; a person lifts about 45 kg at most | Each part and carried load is at most 25.5 kg |
| Viridis (Smith & van der Walt 2015) | A sequential colour scale whose brightness rises evenly | The overlay's scale (already `creeer_render::heatmap`) |

What a well-built footbridge carries: three 150 × 75 mm C24 beams over 2.7 m between their bearings break in bending at about 3 t at their middle (M = f_m·b·h²/6 = 6.75 kN·m each; P = 4·M/L), and sag about 18 mm (L/150) just before. Real footbridges are built to carry a crowd: people and carried stones sag such a bridge by millimetres, they do not break it. So the site offers both what a person carries (granite blocks, gravel sacks, a crate that holds blocks, a boulder to roll) and what a crane sets (test weights of 1 t). A lighter build (two beams, beams laid flat, planks alone) breaks under the carried loads.

## Decision

1. **The site** (`world/site.rs`, `ServerWorld::prepare_bridge_site`): at the spawn (flat ground in its 30 m clearing), a gully is dug 4 m east of the start: 2.3 m across at its rims, 1.8 m deep, 12 m long, round-bottomed (a capsule whose circle cuts the ground 2.3 m apart: radius 1.27 m, its axis 0.53 m down; its walls 65° at the rims), with a slope out at its south end (16°). A 3 m stringer spans it with 35 cm on each bank. Terrain edits as tools make them, saved with the world. The timber lies in rows a few steps north of the start, the loads a few steps south, each a kit item laid on the ground, clear of it and of each other (each rests on the ground alone, so all of it sleeps within 3 s and a tick of the site at rest costs no Newton iterations).
2. **Started straight there:** `creeer_client --bridge`, or "Bridge site (creative)" in the Sandbox window: a creative singleplayer game of that world, walking, in first person, looking across the gully, mid-morning.
3. **The parts and loads** (`data/kit.ron`, appended so the kit's indices stay): stringer 3.0 m × 150 × 75 (14.2 kg), deck board 1125 × 50 × 150 (3.5 kg), sleeper 1.2 m × 100 × 150 (7.6 kg), handrail post 1.3 m × 100 × 75 (4.1 kg), handrail 3.0 m × 75 × 50 (4.7 kg), brace 1.0 m × 38 × 100 (1.6 kg), all C24; granite block 320 × 150 × 200 (25.5 kg), gravel sack (25 kg), load crate 800 × 500 × 500 holding sixteen blocks (400 kg; 18 kg empty), granite boulder 500 × 350 × 450 (109 kg: within the hands' grab, too heavy to lift), test weight 1.0 × 0.6 × 0.63 m of granite (1 t, set from the kit). Loads are a little over 0.3 m long, so they stay bodies where they are put (smaller items at rest are laid as scenery, ADR-0030).
4. **What each element carries** (`Structure::carried`, read only): a member's axial force, largest bending moment and share of its capacity (as `utilization`); a joint's pull and slip force (measured as `Fastened::withdrawn` measures them), its share of what breaks it — the fasteners' interaction (F_ax/R_ax)^e + (F_la/R_la)^e, or its slip's way to its last slip, whichever is more — and how far it has slipped.
5. **What failed** (`world/site_watch.rs`): on a creative server alone, after each step and before a broken structure falls apart, each awake built structure is read; an element broken since the step before is a failure, kept (the last 16) with its tick, place and words: "the joint between a stringer and a deck board pulled apart: 1.9 kN of pull, 0.3 kN across", "a stringer broke: 6.8 kN·m bending". The parts are named by their sections and woods against the kit's items. The first failure of a collapse (others within 2 s of it are its own) is told as a note to every player within 40 m.
6. **Seen** (`stress_view.rs`; O on a creative server this process hosts): every structure drawn by the overlay, built members coloured by their share of what breaks them (0–100 %, blue through grey to red: viridis' dark purple for a lightly loaded bridge did not read on brown ground), each joint a 10 cm ball in the same colours, with the legend. A failure is marked by a red ball where it was, and told at the top of the screen ("First to fail: …", with how many followed) and beside its mark for a minute, overlay or not. The host publishes what the client asks for after each tick (`Host::inspection`); a survival server publishes nothing, and survival players see only sag, sway and the break.
7. **The scene** (`play/bridge_demo.rs`, *bridge_demo*): the site, its three stringers and twelve deck boards assembled across the gully by the scene's world, as the tower is, a 100 mm nail at each of the 36 crossings, driven down through the board, with the joints a player's J makes (`ServerWorld::fasten_now`; `nail_within` closes the loops a player's J cannot close yet), each stringer's ends on the highest ground under their bearings. After 3 s Ada and Ben walk over and back, carry two granite blocks each onto the deck, Ada sets twelve more from the kit, then 1 t test weights on the middle every 4 s. 4,200 ticks; CI plays its first 150 (the site and the bridge settling unbroken: its hash and work). The frames of the bridge loaded and broken come from the client's own game (`creeer_client --planet bridge --built --blocks <n> --weights <n>`): with people on the deck the scene takes seconds a tick (Cost).

## Measured and rejected

| Tried | What happened | Why rejected |
|---|---|---|
| A round boulder (a ball) in a dug hollow | It rocked in the hollow, still at 6 mm/s after 3 s (a ball rolls without loss) | A lump (an ellipsoid) lies still where it is put; the kit takes `Lump` forms |
| The deck boards' row 0.23 m apart from 1.25 m west | The last board (1.28 m east, 3.45 m north of the start) lay still and never slept: 7 Newton iterations a tick, 1,233 in 3 s | Moved (0.22 m apart from 1.3 m west): all sleep. The board at rest that never sleeps is reported to stream A6 (tick time) |
| The scene's bridge nailed with the stringer named first (`fasten(stringer, board)`) | The nail is driven from the part named first: up through the 75 mm stringer, it reaches 25 mm into the board, under the 8 diameters a smooth nail needs to hold against pulling (EN 1995-1-1 §8.3.2), yet over the 6 accepted (`would_hold`). All twelve such joints let go as the bridge settled 1 cm onto its banks ("pulled apart: 0.0 kN across") | The board is named first: down through the board, 50 mm into the stringer. In the game J names the part aimed at: a player aiming at the stringer under a board makes the same weak joint (stream B) |
| The scene's bridge put on the ground under its ends' middles, then lifted clear of the ground by a centimetre (`lift_clear`) | Laid on the ground line, its ends were in the higher ground under their bearings; lifted clear, it dropped 1 cm onto its banks: at tick 34 two joints pulled apart and a stringer broke between two boards, carrying 0.1 kN and 0.1 kN·m the tick before | Each end rests on the highest ground under its bearing (sampled across and along it): it settles unbroken. That a 1 cm drop broke a nailed deck goes to stream A6 |
| Loads set from the kit 0.3 m above the deck (as the kit's buttons drop them) | The first 25 kg granite block, landing from about 0.2 m, broke a 150 × 50 mm deck board spanning 0.4 m ("a deck board broke: 0.0 kN·m bending"). A rigid block on an elastic board peaks near 40 kN for 61 J (√(2Ek), k = 48EI/L³); a real board crushes locally under a stone's corner and survives | The frames and the scene set loads down a centimetre above where they rest. That a struck timber member breaks this readily goes to stream A6 (contacts' compliance on wood); a failure struck from below half its share is now told as "broke when struck (… the moment before)" |
| The scene's deck boards laid loose on the stringers and nailed by the players one by one, as a player builds (each board to the outer stringers) | From the fourth board on (tick 560) ticks of 0.3–4.9 s, up to 172,538 Newton iterations; p50 103 ms over CI's 600 ticks. A player's J nails each later board to one stringer only (the loop is refused), so boards hung on one nail and the middle stringer lay loose under them | The scene's world assembles and nails the bridge; the loose deck's cost goes to stream A6 with this reproduction (the script: `docs/adr/0039/loose-deck-nailed-by-players.rs.txt`) |

## Validation

| Test | Result |
|---|---|
| The gully's depth at its middle and 4 m along it; inside and beyond its rims | 1.8 m within 0.1; falls away inside, the bank as it was beyond |
| Every part and load within 4.5 m of the start, on the near bank; all asleep after 3 s; a second of the site at rest | No Newton iterations |
| The site saved and loaded | The gully and every part as they were |
| The kit: each part and carried load ≤ 25.6 kg, the boulder within the grab, sixteen blocks in a load crate | As listed |
| Two poles nailed: the shares, then the nail severed | Shares in [0, 1); one failure, "the joint between a … and a …", with kN, at the crossing; Ada told |
| The same on a survival server | Nothing shown or told |
| *bridge_demo*'s world | The three stringers and twelve boards one structure, 36 joints |
| A collapse told (the client) | Its first failure, with how many followed within 2 s; none after a minute |
| *bridge_demo* at CI's length (release) | The bridge settles on its banks unbroken, nothing told |
| *bridge_demo* at full length (by hand, release) | Not played: people on the deck take seconds a tick (Cost). Its breaking is shown by the frames instead: the built bridge loaded from the kit in the client's own game (`--planet bridge --built --blocks --weights`) |

## Cost

Measured 2026-11-14 on the development machine while eight other streams built and tested on it (94 % load): the times are to be measured again on a quiet machine; Newton iterations are exact.

| What | Newton iterations | Time |
|---|---|---|
| The site at rest (the plain site's 50 items, after 3 s) | 0 a tick | — |
| *bridge_demo*'s world settling, nobody on the bridge | 10,240 in its first second (780 at most in a tick), 1,642 in its second, 42 in its third, then 0: the bridge asleep | — |
| A person stepping towards the bridge | 1,710 at most in a tick | — |
| People on its deck (`what_the_bridge_costs`, by hand) | 228,742 in one tick, 1.76 million in that second; the bridge's own body 65,099 of them, the rest the people standing on a structure of 125 elements | ticks of 1–10 s |
| *bridge_demo* at CI's length (150 ticks: the settling, before anyone walks onto the bridge; release, two runs) | 618 at most (tick 0); a client's 4 | p50 27.8 / 3.7 ms, p99 387 / 214 ms, worst 400 / 286 ms |

The scene is far over the play gates (worst ≤ 5 ms, p99 ≤ 2 ms), and not hidden: its work ceiling is what it measures, and the people on the bridge go to streams A4 (walking on what is built) and A6 (tick time). Its full length (4,200 ticks) cannot be played in reasonable time while people stand on the bridge. The watch reads only awake built structures, on creative servers only.

## Limits

- A joint taken apart on purpose (U) or a member sawn through is told as a failure too (the watch sees an element broken, not why).
- The words name the parts by their sections; a joint is "the joint" (which fastener made it is not kept by the structure: nails, pegs and lashings are told alike).
- The overlay and the failure marks reach a client only from a server its own process hosts: a dedicated creative server's players get the note in words, not the overlay. A message for them is the protocol's (stream B).
- Play scenes cannot carry and place parts by hand (`Act` has no hold): the scene's bridge is laid and nailed by its world.
- A player's J cannot nail two parts that are already one structure (`touching_part` looks at other bodies only): once the stringers are joined through one board, every further board can be nailed to one stringer only. A deck needs its loops closed; stream B's.
- The overlay's joint balls lie mostly inside the members they join and show little; built members are drawn as round bars, deck boards too, and a broken structure's one-node pieces as dark balls (the drawing of parts and pieces: streams A2 and A5).
- A well-built footbridge does not visibly sag under what people carry (about 18 mm at L/150 before it breaks): the sag reads near the break only, or in lighter builds.
