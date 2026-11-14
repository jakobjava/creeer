# ADR-0037: The ground, its cover, and the light on it (G2.11 A1)

- **Status:** Accepted (2026-11-14).
- **Context:** the user's verdict after playing G2, "looks horrible" (`docs/playability-plan.md`). Frames of 2026-11-13 showed the ground one flat brown, shaded in visible triangles, with nothing on it; shadow edges stair-stepped; things not seated on the ground. `docs/art-style.md` pillars 1, 3, 4, 5, 7 and 9 were not met.
- **Builds on:** ADR-0003 (render architecture, shadows, atmosphere), ADR-0006 (terrain, its levels of detail), ADR-0026 (the look, ambient occlusion), ADR-0031 (trees' ranges), ADR-0029 (loose things).
- **Evidence:** `crates/creeer_render/src/shaders/{ground,terrain,lighting,ao_denoise,ao_sample}.wgsl`, `crates/creeer_render/src/{terrain,shadow_filter}.rs`, `crates/creeer_client/src/{ground_look,terrain_mesh}.rs`, `crates/creeer_art/src/ground.rs`, `data/flora/{ground_layers,decoratives}.ron`. Frames: `creeer_client --planet <view> <png>`; cost: `--planet <view> <png> --frames 120` (GPU time per pass).

## Prior work

| Work | What it does | What it means here |
|---|---|---|
| Factorio, FFF-320 and the terrain of 0.17 (Wube) | Terrain as a muted background with "a good and comfortable contrast with the entities"; too much saturation turns it into a "plastic playground"; decoratives (grass, rocks, decals) everywhere, removable where built on (FFF-343) | The ground stays muted and earthy (palette tokens only); cover is low and sparse enough that things stand out |
| Texture splatting with height blending (Mishkinis, "Advanced Terrain Texture Splatting", Gamasutra/GameDev.net 2013; Unity and Unreal landscape layers) | Several materials laid by weights, the transition decided by each material's height so it breaks up naturally instead of a smooth cross-fade | Adapted: layers (litter, moss, grass) laid by a weight per vertex, their edges decided by noise against that weight (patches, not a cross-fade). No images: the "height" is procedural noise (art-style §6) |
| Horizon Zero Dawn's GPU placement (van Muijden, GDC 2017) | Density maps per asset from the biome's rules, turned into instances by a dithered threshold; data-driven, deterministic, locally stable; ~250 µs a frame for 100,000+ objects | Taken in principle: a decorative's density follows its layer's share of the ground; placement is a pure function of position (stable as the camera moves) |
| Ghost of Tsushima's grass (Wohllaib, GDC 2021) | Grass blades generated per tile on the GPU, instanced, thinned with distance | The scale reference for instanced cover with distance fade; Creëer's cover is far sparser (art-style §8: tufts below the knee) |
| Shadow filtering: Castaño's optimised PCF for The Witness ("Shadow Mapping Summary", 2013; MJP's shadow sample; Bevy's `castano_thirteen`) | A wide Gaussian-like filter from bilinear comparison taps: 16 taps make a 7×7-texel bell whose result is continuous as the point crosses texels | Taken (decision 2) |
| Receiver plane depth bias (Isidoro, GDC 2006) | Each tap of a wide filter compares at the receiver's own depth under it, from the receiving plane's slope | Taken, with the slope from the normal (no screen derivatives in a branch) |
| Percentage-closer soft shadows (Fernando, 2005) | A blocker search sets the filter's width by the light's size: contact hardening | Not now: the sun's 0.53° makes penumbrae of a few centimetres for things on the ground, which the filter's width already gives; noted in the limits |
| Screen-space (contact) shadows (Bend Studio, "Inside Bend: Screen Space Shadows", Days Gone 2019; Unreal's contact shadows) | A short ray towards the light through the depth buffer catches small details and the contact line that shadow maps' resolution and bias lose | Taken (decision 3) |
| Bump mapping unparametrised surfaces (Mikkelsen 2010) | The surface gradient of a height from screen-space derivatives: relief without tangents or textures | Taken for the ground's fine relief (decision 4) |
| Hillaire 2020 (Unreal's sky) | Physically based sky and aerial perspective from transmittance and multiple-scattering tables | Already the game's (ADR-0003); distance reads through it |

## Decisions

1. **The facets were the detail texture** (ADR-0006, found in G2.11). Each terrain chunk now carries where its origin lies in a 1,024 m pattern (`terrain::pattern_offset`), so the ground's procedural detail is fixed to the ground and continuous across chunks of every level.
2. **Shadow edges without stairs.** Each cascade lookup is Castaño's 7×7 filter (16 bilinear comparison taps). Each tap compares at the receiving surface's depth under it (receiver plane bias from the normal), so the normal offset stays one texel and contacts stay dark (a wider offset leaked light under a rail lying on another). `shadow_filter.rs` mirrors the filter on the CPU and tests that an edge is a smooth, monotone ramp about seven texels wide with no jump as the point crosses a texel.
3. **Contact shadows** (things sit on the ground; small things cast their shadows). In the occlusion's last compute pass, each pixel marches 16 steps towards the sun through the screen's depth, up to 0.3 m and at most 40 pixels on screen (longer steps drew a near stick's shadow as stairs). A surface found in front of the ray by less than 6 cm (plus 4 mm per metre away) shades it. The result rides in the occlusion texture's second channel, and `sun_visibility` takes the darker of it and the shadow maps (no change to the mesh shader). Thin things' shadows, which the 7×7 filter alone softens, come back sharp.
4. **The ground's materials** (`ground.wgsl`, `creeer_art::ground`, `data/ground/layers.ron`). Over the rock's own colour (mottled ±12 % at 5 m and 1.2 m, speckled ±9 % at 7 cm and 1.8 cm, broad ±6 % at 40 m) lie up to four layers: litter and moss under trees, grass in the open. Each vertex carries the share of the ground each layer covers (four bytes: vertices grow from 28 to 32 bytes), from the layer's ranges as a tree species' are (rock, slope, height, latitude; "under trees" where a species of `flora/trees.ron` grows) with soft edges; none on ground dug below its natural surface. The shader lays each in patches whose edges come from three octaves of noise against that share (the finest breaks the edges into tufts), as thickly as its `density`, with a grain of its own (blades, cushions, needles), and turns the normal by a fine relief (Mikkelsen's surface gradient) so the sun catches crumbs and tufts up close. Detail finer than a pixel fades to its average. Every octave is one fetch from the 32³ noise volume the parts' surfaces already use (ADR-0026), bound with the atmosphere's tables, with its size rounded so the volume tiles a whole number of times per period (continuous where the pattern wraps). `ground_pattern.rs` mirrors the patches on the CPU, so decoratives stand where their layer is drawn. Layer colours are palette tokens: `bark_spruce` stands in for needle litter and `leaves_spruce` for moss until the palette has tokens of their own (the user's approval). The layers' data lives in `data/ground/`, apart from `flora/`, whose every file is read as tree species.

5. **Decoratives** (`data/ground/decoratives.ron`, `creeer_client::ground_cover`, `creeer_render::cover`, `cover_pass.rs`, `cover.wgsl`): grass tufts (seven tapering, curving blades, 35 triangles) and grit (a dome sunk a third, 40 triangles), drawn instanced within 24 m of the camera. The ground is cut into cells of 4 m on the cube-sphere's faces; what stands in a cell comes from the planet's seed and the cell alone (the same every time, whatever the level of detail), made on a worker thread: so many per square metre, each on the ground as physics has it (the heights at 1 m), a tuft only where the shader draws its layer (`ground_pattern.rs`), nothing on ground steeper than 50° or changed by an edit (an edit forgets the cells it reaches). Each frame the client hands the renderer what stands about the camera, camera-relative, each shrinking into the ground over the last quarter of its reach (600 times its size, at most 24 m: grit goes at 15 m), and tufts pressed flat within 12 cm of a thing lying loose or a body, so what the player needs to find stays in sight. Their limits are data checks (`creeer_art::ground`): nothing taller than 0.35 m, no grit over 3 cm (it would be taken for a stone to pick up). They are drawn in the depth prepass (they take ambient occlusion and give the ground contact shadows) and lit as the parts are; they cast nothing into the shadow maps.

6. **The forest from afar** (distance reads). Trees are drawn within 160 m (`tree_view.rs`); beyond, a forested land was bare floor, so from 300 m up the land read as an empty brown plain. Each terrain vertex now also carries how fully a forest grows there (the best of the tree species' ranges, in the coarse normal's spare byte), and beyond where trees are drawn (blending in from 104 to 168 m) the ground shows the crowns: the mean of the species' foliage colours, darkened by half for the crowns shading one another and the gaps (a conifer forest's albedo is about half its needles' reflectance), mottled crown by crown (5 m) and stand by stand (60 m), seen through the atmosphere's own aerial perspective. The sky itself is the physical model of ADR-0003 and unchanged: clouds, which it lacks, are a limit.

## Measured and rejected

| Variant | Measured | Why not |
|---|---|---|
| A wider normal offset for the 7×7 filter (1.5 + 2.5 texels by the slope) instead of receiver plane bias | No acne, but a thin rail lying across another let light in under itself in a cross-hatched pattern | Contacts must stay dark |
| Contact-shadow rays of a fixed 0.3 m | A stick 1 m from the camera cast its shadow in stairs (steps of ~30 pixels) | The ray is now limited to 40 pixels on screen |
| Contact shadows read with the occlusion's bilinear sampler | A thing's rim took the ground's shadow from its neighbour pixel: a dark line round it (forbidden, art-style §5) | Read unfiltered, each pixel its own |
| The ground's noise computed (hashed value noise, eight hashes an octave, as the old detail texture was) | 17 octaves on a forest floor: the scene pass 1.18 → 2.06 ms (0.23 ms the rock's detail, 0.65 ms two layers); working out the period's wrap once per axis instead of per corner changed nothing | One fetch from the noise volume an octave: 1.08 → 1.41 ms |
| Moss at full density in patches of 2.2 m | The forest floor read as a lawn in camouflage blobs | Density 0.55, patches of 1.4 m, their edges broken by a third octave |

## Cost

Surface view (forest clearing, 1600×900, RTX 4070 SUPER, other agents' builds running: to be measured again on a quiet machine), GPU time per pass, best of runs:

| Pass | Before (2026-11-13) | After |
|---|---|---|
| Scene | 1.19 ms | 1.32 ms with the 7×7 filter (decision 2); 1.41 ms with the ground's materials (decision 4; 1.08 ms without any of the ground's detail) |
| Occlusion's last pass | 0.09 ms | 0.14 ms with contact shadows (decision 3) |
| Depth prepass | 0.51 ms | 0.55 ms with the decoratives (decision 5; the scene pass 1.41 → 1.43 ms) |
| Scene, with the forest from afar | | 1.51 ms (decision 6: two more fetches, only where a pixel lies beyond 104 m) |
| All passes | 4.34 ms | 4.68 ms |

The decoratives' CPU work each frame (gathering the cells about the camera, a few thousand instances) did not show in the frame's time against the noise of other builds (p50 8.5–8.7 ms either way); filling a cell runs on its own thread.

The vertices' four bytes of cover cost 14 % more terrain vertex memory; working them out costs one height lookup a vertex on the mesh workers (a sampler at the chunk's own spacing: one at a metre made metre tiles for every vertex of kilometre chunks, and streaming never settled).

## Limits

- No contact hardening by blocker distance (PCSS): a tree's shadow 10 m from its crown has the same penumbra as a stick's.
- Ground layers do not follow actual trees, only where trees may grow: a clearing reads as forest floor.
