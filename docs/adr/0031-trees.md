# ADR-0031: Trees, one generic tree

**Status:** accepted (2026-10-23, G1.7 done).

**Context:** G1's re-plan (`docs/g1-plan.md`, principles 9–11). Trees are the first material a player who has made a stone axe works: felling, limbing, bucking, logs for building and fire. The user asked for one generic system whose attributes make the variety ("make a generic system and then the improved world generation can use that system to have biomes with different trees (shape, colour, size, etc.)"). Trees cover the land in their millions, so they must cost nothing until touched (principle 5), and be the same for every player.

**Builds on:** ADR-0004 (beams: the section mechanics), ADR-0022 (joints: the hinge), ADR-0027 (strikes: a swing's energy), ADR-0029 (the ground's cells), ADR-0030 (edges: what a made tool cuts with).

**Evidence:** `creeer_sim::trees` (`trees/tests.rs`, `trees/tests_cut.rs`), `data/flora/trees.ron`, `spruce_green`, `pine_green`, `birch_green` (`materials/engineering.ron`, with `across`), the example mod's oak (`tests/packs/example/flora/oak.ron`), the axe's edge (`tools.ron`), `creeer_server::world::{trees, felling, bucking}` (`trees_tests.rs`), `Command::StrikeTree`, `Kind::Tree`, `ServerMessage::Trees` (protocol 22), save version 11 (`TREE`), `creeer_client::tree_view`.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Weber & Penn 1995, *Creation and rendering of realistic trees* (SIGGRAPH) | Trees from a few parameters per branching level (counts, angles, taper, the crown's shape), 3–4 levels where others need 9–10; stems as lines and leaves as points far off | Taken: species as parameters (taper, crown outline, branch counts and angles); one level of branches; levels of detail by distance |
| Runions, Lane & Prusinkiewicz 2007, space colonisation | Branches grow towards free space | Not taken: costlier per tree, and not needed for a crown's mass and outline |
| Leonardo's rule; Eloy 2011 (*Phys. Rev. Lett.* 107) | The area of a stem below a fork is the sum of the areas above it; wind stress explains why | Taken: a node's branches share the section the stem loses across it |
| The pipe model (Shinozaki et al. 1964) | Foliage in proportion to the sapwood area that feeds it | Taken: the crown's foliage shared by branch section, its wood by section times length |
| Repola 2008, 2009 (*Silva Fennica* 42(4), 43(4)) | Biomass of the living branches and foliage of Scots pine, Norway spruce and birch from diameter and height | Taken as data: each species' functions (spruce: 33 kg dry needles at 30 cm and 25 m, as the paper's) |
| Wood Handbook, FPL-GTR-282 (2021), Table 5-3a | Green (living) wood: modulus of rupture, stiffness, compression along and across the grain | Taken: red spruce, red pine and paper birch standing in for the European species (MOR 41, 40, 44 MPa; across 1.8, 1.8, 1.9 MPa) |
| McMahon 1973 (*Science* 179); Greenhill's height | Trees stand at a fraction of the height at which they would buckle under their own weight | Taken as a check: every grown tree stands at 0.49–0.73 of it |
| ForestGALES (Gardiner et al. 2000; Hale et al. 2015); Mayhead; Vollsinger et al. 2005 | Wind on a crown as drag on its streamlined area; the stem breaks at its green MOR | Taken: a crown's drag (C_d 0.35) in the cut's check; the planet's air has no wind yet |
| EN 1995-1-1, eq. 6.19 | Bending with compression along the grain: (σ_c/f_c)² + σ_m/f_m ≤ 1 | Taken: what is left of a cut, and every stem section under its own weight |
| Felling practice: Weisgerber 1999 (*An Ax to Grind*, USDA Forest Service); OSHA | Axe felling: a front notch a third to a half in, cut at 45–50°, the back cut above it; the hinge guides the tree until the notch closes | Taken: notches at 45°; the hinge as a joint along its edge, let go when the tree has leant through the notch's opening |
| Saraydar & Shimada 1971; Townsend 1969; Salisbury 1962; Godelier & Garanger 1973 (via Hardy & Sillitoe 2003); Moore's synthesis | Stone axes fell 2.5–4.4 times slower than steel in New Guinea, over 6 in Saraydar & Shimada; 30 cm in about 20 minutes with stone | The validation: a made stone axe against the steel one |
| Glaser, *Unasylva* 8(4) (c. 1954) | Axemen swing 25–35 strokes a minute | With the stone axes' time, the calibration: a steel axe fells 30 cm in about 5 minutes, 150 blows |
| Ennos & Oliveira 2017 (EXARC); Putz et al. 2023 (*Forests* 14) | Wedge splitting and cutting of wood; stone edges blunt, wide-angled | Read: no measured energy per volume of chips was found, nor edge angles; hence one calibrated constant |

## Decision

1. **A species is data** (`flora/*.ron`, merged by name across packs): its green wood (an engineering material with its strength across the grain), where it grows (latitude, elevation, slope, grounds), its density of stems and sizes, Näslund's height curve, the stem's taper, the crown's ratio, outline and width, its branching, Repola's biomass functions, and its colours. Spruce, pine and birch ship; the example mod adds an oak with no code.
2. **One generator** (`grow.rs`): the stem tapers as r(z) = r₁.₃·((H − z)/(H − 1.3))^taper; branch nodes along the crown share what the stem loses in section (Leonardo); their reach follows the crown's outline; the crown's fresh biomass is shared by the pipe model. A seed makes each tree its own. The stem's mass and centre are integrated in closed form, so cut spans add up exactly.
3. **Placed as the loose things are** (`place.rs`): cell by cell from the planet's seed, each species by its ranges and a Poisson draw, sizes a power law (N(> d) ∝ d⁻²), none within 1.5 m of another in its cell, and none within 30 m of the spawn (a clearing: a player starts in the open; with trees about it, the play scenes walked into trunks, the crowd's median work rose from under 183 to 2,566 and the avatar's prediction, which the test clients do not give trunks, was corrected by up to 0.97 m). Ids carry their cell (with a bit apart from the loose things' and the laid items'), so a tree is found again from its id. Standing trees are scenery: nothing is simulated or sent.
4. **A cut is a section** (`section.rs`): the stem's disc less its notches, the weight above bearing eccentrically (its line through the centre of mass, moved by the wind's moment), the stress linear across what is left, checked by EN 1995's rule with green strengths. Past the middle, the weight bears beside what is left and the tree goes over towards the notch; it pivots on the middle of the edge that bears most.
5. **A blow takes out wood by its edge and its energy** (`chop.rs`): an edge driven across the grain wedges the fibres apart against their strength across the grain, so V = k·E/(σ⊥·(tan(θ/2) + μ)), μ = 0.3, k = 0.26 calibrated once (a steel axe of 25° fells a 30 cm spruce in about 150 blows). Nothing without an edge cuts wood. The energy is what the swing's effective mass carries into wood that does not give (½·m_eff·v²). Blows from within 45° deepen one notch (its floor level, its roof at 45°); from elsewhere they begin another.
6. **Felled, a rigid body on a hinge** (`felling.rs`, `felled.rs`): the part above the cut is one rigid body (its stem's hull, its centre of mass and inertia with its crown's mass where it hangs: `Hull::about`, `Body::rigid_with`), its base shaped as the notch's roof, held at the hinge's middle by a hinge joint across the way it goes. The hinge lets go when the tree has leant through the notch's opening (45°), as the notch closes. The notch's wood lies as chips.
7. **Limbing and bucking** (`bucking.rs`): a blow on a felled part near a node whose branches are still on goes into them, thinnest first, each off once a notch through it is paid for, and lies beside the trunk as a piece of the species' wood (its mass the branch's, needles and all: a haft, firewood). Else it goes into a cut across the stem, from the striker's side; when less than 2 % of the section is left, the last fibres tear and the part is two. The kerf is a slice of the stem of exactly the chips' mass, so pieces and chips are the part to rounding. A part changed is a new body moving as the old one did.
8. **Trunks are walked into** (`trees.rs`): every 10 ticks, the trunks within 12 m of each player and 4 m beyond each body awake are boxes (a square of the disc's area, 3 m tall) at the end of the physics world's fixed shapes; felled trees' stumps to their cut. Clients lay the same about their avatar for its prediction. A fixed box beyond a shape's reach is skipped exactly (`detect.rs`).
9. **Told and drawn** (principle 9, 12): clients are told which trees are cut and felled (`ServerMessage::Trees`) and draw them, and felled parts (`Kind::Tree`: the tree, its span, its branches off, its centre and hull) as the forest grows them, plainly: rods and rough masses, branch by branch within 10 m, a stem and three layers within 50 m, one mass to 160 m. The crosshair says a tree's girth and how far it is cut, never its species.

## Measured and rejected

| What | Measured | Why not |
|---|---|---|
| The felled part on a ball joint at the corner bearing most | A 20 cm spruce notched from the east and cut from the west fell north, its top 2.5 m east of its stump | One corner of a straight hinge: the tree swung about it. The middle of the hinge's edge and a hinge joint along it: its top 19.5 m east |
| Chips laid beside two pieces of the whole stem | Bucking and limbing made 2 kg of wood | The kerf counted twice: now a slice of the stem of exactly the chips' mass |
| A stick carried into a trunk (or a crate) | Spun about its own axis to 450–525 rad/s and dropped: the contact's friction on a moment a six-hundredth of the others, which the arms' spring cannot hold | Not a tree's fault but the hands' (G1.4): the fist now holds a handle's own axis by the grip's friction (ADR-0027, amended): 6.4 rad/s, still held |
| Every near tree's branches and foliage drawn branch by branch, to 220 m | 32,000 instances, frames 11.5 ms against 3.7 ms bare (1600 × 900, the development machine) | Foliage a mass per whorl, branches within 10 m, far trees one mass, thin far stems left out: 7,500 instances, 6.4 ms |

## Validation

- Every species at nine sizes and six seeds stands under its own weight (the most used 0.59) and in a 10 m/s wind; each stands at 0.49–0.73 of its Greenhill buckling height.
- A crown's mass is its biomass to rounding; a node's branches have the section the stem loses across it.
- What a notch leaves against the circular segment's closed forms (area, centroid, second moment, EN 1995's rule) within 0.5 %; a notch through holds πR³.
- Chopped from one side, a 30 cm spruce, pine and birch each go over with the notch 89–90 % through, pivoting on what is left's edge towards the notch; notched from both sides alike, a wind decides the way it goes.
- A steel axe fells a 30 cm spruce in 148 blows (4.9 minutes); a made stone axe (a knapped chert core's 70° edge on a 65 cm stick) takes 3.2 times as many at 15 cm and 3.4 at 30 cm (measured 2.5–4.4; its 30 cm in 16.6 minutes against about 20).
- In the world: a character walking into a trunk stops beside it; a 20 cm spruce notched from the east (12 blows) and cut from the west (11) falls east, lying flat with its top 19.5 m east, its hinge let go, the part's mass that of the tree above the cut; bucked and limbed, its pieces, branches and chips weigh what it weighed (to 10⁻⁹); a save keeps the cuts, the felled and the parts, bit for bit.
- Trees stand where their ranges allow (0.054 a square metre about the spawn), the same twice, each found by its id; the example mod's oak grows from data alone.

## Cost

- Standing trees: nothing per tick. Laying the trunks every 10 ticks: a few cells' trees about each player.
- Chopping: arithmetic per blow; a felled part is one rigid hull. The worst step while 20 cm spruces were felled and fell: 2.0 ms.
- Drawing: 6.4 ms a frame at the spawn against 3.7 ms bare (7,500 instances; the development machine). At the minimum specification's projection this misses 60 FPS: impostors are the step that brings it back (below).

## Limits

- Felled parts are rigid: a log does not bend, and its branches do not touch the ground (the stem's hull alone collides) or break when it lands. The plan had trees as beam structures; standing trees' strength is beam theory at their sections, but a felled tree as a structure body would have needed broken members to split it into bodies, and its branch tips' contacts cost; deferred.
- Standing trees are fixed boxes: one struck by a falling tree, or a crate, does not move or break (a falling tree hangs on it); crowns do not sway, and the planet's air has no wind yet to decide a fall.
- Height curves, tapers, crowns, branching and stem densities are estimates through typical Nordic trees (the published curves are stand-dependent); the green woods are North American stand-ins; the steel axe's 25° edge and the chopping constant rest on felling times, not on a measured energy per volume.
- Edges do not dull or chip on wood; a binding is not loaded by chopping.
- A cut is at one height per tree; a back cut at another height is taken as the same cut.
- Trees are drawn as rods and masses, with no impostors: a forest costs 2.7 ms a frame on the development machine.
- The forest knows no climate beyond latitude, nor biomes; tree cells are blurred to the ground at their middles, as the loose things'.

Each is in `docs/future-improvements.md`.

## Found on the way

- **The trunks' first laying took 9 ms** in the stress scene's server (eight players' surroundings coming into reach at once, the cells' ground generated at first touch). Cells are now made four a tick from a queue (`make_cells`; about 20 µs each on ground already generated), cells near a terrain edit made again at once, and the cells made and waiting are saved (`TREE`), so a loaded world goes on as it would have. A cell wholly in the spawn's clearing is skipped, and the ground is found only for trees that may stand.
- **Crates put down inside trunks.** The stress scene's yards were put down before any trunk was laid, and the trunks then appeared inside the crates (its median tick 0.7 → 1.7 ms, its worst 9 ms). Nothing may be put down in a trunk now (`clear_of_trunks`: crates, boxes, balls, frames, kit items: "a tree stands there"); the yards leave those spots out. After: median 0.8 ms, worst 1.2 ms (120 ticks), as before G1.7.
- **Ageing's pruning took 14 ms every 600 ticks** with 100,000 parked bodies (a set of every parked id): only the aged entities missing from the world are now looked for among the parked (`ageing.rs`), the same result. The stress scene's server is within its ceilings again (worst 6.9 ms of 8.1, first 2.7 of 7.5); it was over them before G1.7 (15.7 ms).
- **The stress and play goldens** were stale since G1.6 (its save version 10): the commit before G1.7 gives the stress server's hash `01c358c9a7e6301a`, and G1.7 without trees and with save version 10 gives the same, so G1.7 moves nothing but through its trees; the play scenes' work is unchanged to the iteration (crowd 3,355 at tick 1,533; work 362 at tick 1,776). Renewed for version 11 and the forest. The crowd scene's tick gates (worst 5 ms, p99 2 ms) fail as they did before G1.7 (worst 19 ms, p99 11.6 ms): P1's (G1.8).
