# ADR-0041: Things as drawn: parts, logs, stones and trees (G2.11, stream A2)

**Status:** in progress (2026-11-14, G2.11 A2).

**Context:** the user's verdict on G2, "looks horrible" (`docs/playability-plan.md`). What a player handles and stands among was drawn plainly: a rod (`post`, `rail`, `pole`) as a square bar, timber and logs as coloured boxes with no grain, end grain or bark, stones and sticks lying about as boxes, a cobble as a dark steel ball, and trees as rods under stacked ellipsoids, a birch's trunk as a white line. `art-style.md` asks for the drawn shape to be the simulated one (§2), curved things round at every distance (pillar 9), and every surface grainy, weathered and used from the palette's patterns (§6), trees with "masses of foliage in clumps, not individual leaves; dark undersides from AO; bark with vertical detail" (§8).

**Builds on:** ADR-0003 (the renderer), ADR-0026 (the look: tokens, the surface's layers, primitives in levels of detail by screen error), ADR-0028 (items), ADR-0029 (what lies about), ADR-0031 (trees).

**Evidence:** `creeer_render::{parts, foliage}`, `shaders/parts.wgsl`, the part path in `shaders/mesh.wgsl` and `globals.wgsl` (`placed`); `creeer_client::{part_meshes, part_view, tree_draw, tree_view, loose_view}`; the views `parts`, `nailed`, `forest` and `tree` (`planet_shot_things.rs`); frames before and after in the stream's report.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Perlin 1985, *An image synthesizer* (SIGGRAPH); Peachey 1985, *Solid texturing of complex surfaces* | Solid textures: a pattern as a function of the point in space, not of a surface's map, so any cut through the solid shows it consistently; wood as rings about an axis (distance from it, perturbed by turbulence, through a sine) | Taken: a part's wood is a solid texture about its pith. A pole's side shows fibres running along it, its ends the rings, a sawn board the rings cut across where it was sawn, with no seams and no texture coordinates |
| Lefebvre & Neyret 2002, *Synthesizing bark* (EGWR); Pixar, *A procedural approach for stylized bark shading* (Elemental, 2023) | Bark from fractures along the stem (strips that split and widen as it grows); stylised bark as procedural furrows with plates between | Adapted: furrows as the crevices of noise stretched along the stem, a few centimetres apart whatever the stem's size, so thin stems' bark is smooth as young bark is |
| Factorio, FFF-86 (2015), FFF-227 (2018), FFF-324 (2019) | Trees as hand-painted sprites: leaves and trunk apart (stages of leaf loss), volumetric leaves with their shadow painted in, leaves moved by their normal map | Taken: foliage as masses whose shading is part of them (baked occlusion: dark undersides and creases), the trunk apart from the leaves |
| Stylised foliage practice (Unreal and Godot community write-ups; *The Witness*): clumps whose normals come from a sphere about them | Leaf clusters lit as one soft volume rather than as cards | Adapted without cards or alpha (none in Creëer's renderer, `art-style.md` §6): clumps are lumpy closed meshes, smooth-shaded, their lumps' creases darker |
| Brucks 2018, octahedral impostors (Fortnite, Unreal); Bruneton & Neyret 2012, real-time forests | Far trees as views baked into an atlas, the nearest blended; forests drawn by shaded volumes far off | Not yet: far trees are one mesh each (a spire or a clump) to 160 m; impostors farther stay a future improvement (below) |
| Hoppe 1997, screen-space error (ADR-0026's levels) | A level of detail chosen by its error on screen | Taken for every new mesh: logs, boards and foliage in levels chosen by their silhouette's error under half a pixel |
| Valheim, Medieval Engineers (players' notes); SketchUp timber-framing forum | Repeating wood textures read as fake; end grain darker than the faces | Taken: no texture repeats (a solid texture in metres, each part its own seed and tone); cut ends in `wood_cut` with their rings |

## Decision

1. **A part is a unit primitive that names a palette token** (`creeer_render::parts`). The sandbox's rounded box and cylinder (ADR-0026 decision 11), their levels of detail and bevels kept, with each vertex naming a token (`vertex::PART` marks it) and how its surface follows its wood (`Grain`: round, sawn, bark). They are drawn by the models' pipeline with the full surface of `art-style.md` §6 (tone, pattern, mottling, age, grime, edge wear), the wear on their rounded edges and their height above their lowest point found in the shader. A part's tone, the seed of its grain and its taper reach the shader as its instance's colour (`Material::part`), which a part's tokens do not use: the instance stays 64 bytes. A part's bevel is `art-style.md` §7's (2 % of its smallest side, at least 5 mm), smaller than the sandbox primitives' 8 %, so a stem drawn in pieces shows no groove where they meet.
2. **Wood is a solid texture about its pith** (`shaders/parts.wgsl`): the Wood pattern's streaks and wandering lines, the lines the crests of rings about the pith; a sawn part's pith beside it by its seed (a board sawn from a log wider than it, a squared beam about it). Bark: plates between furrows along the stem; a species' marks (a birch's dark dashes, `marks` in `flora/trees.ron`) over it in a second token.
3. **Things are drawn as what they are** (`part_view.rs`). An item's rod hull (its sixteen-sided prism) is drawn as the round rod it stands for, in its wood; a block as a sawn board; a stone or a cobble as a speckled ellipsoid in its rock's colour (the server's own ellipsoid); a made tool as its haft and head; a dead branch lying about as a stick with greyed bark. What an item is made of is told by its colour, which the server derives from its material's token (`Densities::colour`): the client finds the token by it (no two materials share a colour; tested).
4. **Trees are drawn where their members are, as what they are** (`tree_draw.rs`). The stem in tapering logs (three near, one far: a linear taper within a centimetre of the stem's curve), each with its species' bark and `wood_cut` ends (a stump's cut, a bucked log's ends), the branches as tapering branches. Foliage as masses: a conifer's (a cone's crown) as tiers drooping from its whorls to pointed tips, others' as lumpy clumps along their branches; at mid distance a crown of tiers or clumps over its outline; far, one spire or one clump. Each foliage mesh carries its baked occlusion (dark undersides, darker creases) and tones per lump or tip (`creeer_render::foliage`).
5. **A body that becomes a structure is told again** (`building_parts.rs`, `make_built`): nailing two parts left clients drawing the first as its item's hull at its first node, the second gone, until it was let go.
6. **Built members are drawn as wood.** A structure with fastened joints (members of radius 0 in its `Frame`) was built by hand, and every part G2 builds with is wood (`building_parts.rs` refuses anything else): its members are drawn round in `timber`, at their radius, each its own tone. Until the protocol tells a member's wood and profile (Limits), a log's bark and a sill's square section are not drawn once built, and a steel frame (no fastened joints) stays a tube.
7. **A broken structure's lone pieces are fitted** (`part_fit.rs`). A built structure collides by each member's halves about its nodes (a round member's half a prism of its section, a board's a box: `creeer_building`'s `assemble`); a piece of one node that breaks free is a rigid body of that hull, told with no colour, and was drawn as its bounding ball: a collapse was a heap of large dark balls (the bridge-site stream's finding). The hull is fitted now: its axes the eigenvectors of its corners' spread (Jacobi's rotations), round about the axis its corners lie most evenly far from (within 1 %, twelve corners at least: a box's eight are as far from each of its axes), else a block, in `timber`.

## Measured and rejected

| What | Measured | Why not |
|---|---|---|
| Foliage drawn with the models' full surface | (filled in below) | |

## Validation

- `parts::tests`, `foliage::tests`, `part_meshes::tests`, `part_view::tests`, `vertex::tests`, `scene::tests`, `primitives::tests` (the shaders' constants against the Rust ones).
- Frames (`creeer_client --planet <parts|nailed|forest|tree|surface|rail|post|low> <file.png>`), before and after.

## Cost

(filled in below)

## Limits

- Built structures' members are drawn from what the frame tells: a radius per member and nothing of its wood or profile, so every built member is round `timber` (a log's bark, a birch rail and a sill's square section are lost once built), and its joints are not drawn: the frame does not say which fastener holds them, so nail heads, a lashing's wraps and a peg's end cannot be. The protocol needs each member's token and profile, and each joint's fastener (stream B's: the protocol); `part_view.rs` then draws them as it draws items.
- A crate item (a block with a wall) is drawn as a solid sawn block: the client is not told it is a container.
- Trees beyond 160 m are not drawn (impostors).

Each is in `docs/future-improvements.md`.
