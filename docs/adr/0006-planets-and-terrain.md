# ADR-0006: Planets and terrain

- **Status:** Accepted (2026-10-16)
- **Context:** F3 (planets & terrain). A KSP-scale planet (500 km radius) with smooth voxel terrain that can be dug anywhere; real geology and ore deposits from data; terrain collision for the physics of ADR-0004/0005; radial gravity; day and night; edits saved as deltas. The exit criterion: fly from orbit down to the surface without hitches, and dig tunnels. Budgets from plan.md §5 (chunk throughput ≥ 200 chunks/s on 6 minimum-spec cores, no frame hitches).

## Decision

### The planet and its data (`creeer_terrain::planet`, `data/planets/*.ron`)

- A planet is a RON file with every quantity written with its unit (`"500 km"`, `"9.81 m/s^2"`), parsed and dimension-checked by `creeer_math::units`. Its geology lives in a second file; minerals and rocks are shared (`data/geology/`).
- **Sea level is the equipotential** of the field the physics uses: a point mass μ seen from the frame turning at ω about +Y, μ/r + ½ω²r²cos²φ = μ/R (`datum.rs`). On a sphere, flat ground would be tilted by up to ω²R/2g (0.12° on the reference planet): balls would roll and, from F6, oceans would flow towards the equator. The equator bulges 1,085 m on the reference planet. Heights are measured from this datum.

### Elevation on a cube-sphere (`cube.rs`, `elevation.rs`, `tiles.rs`)

- **Elevation** is a sum of noise layers on the sphere: continents through a curve, ridged mountains in masked belts, hills and detail. Continent and mask values are mapped to the **fraction of the planet's surface** below them (a measured distribution, 33 knots from 8,192 samples), because one low-frequency noise on one planet is lopsided: a coast "at 0.54" means 46 % land for every seed (measured 46.1 %).
- Every evaluation is **band-limited** to the spacing it will be stored at; the octave near two samples per wavelength fades in, so changing resolution moves the surface by less than 5 % of a sample, and LOD levels agree within one sample (tested).
- **Height tiles:** heights are stored on cube-sphere grids, one quadtree per face, 64×64 cells per tile plus a one-sample border taken from the neighbouring face, and interpolated with Catmull-Rom (C1, so contact normals turn smoothly). The face coordinate is warped by s = g·(1.2 − 0.2g²), whose slope falls from 1.2 to 0.6 like the equal-angle map: cells stay within ±9 % of one size, with one division per lookup and no library function. *Measured:* a height costs 29 ns from a tile against 630 ns evaluated directly (21×); a tile takes 3 ms to generate.

### Geology (`geology/`)

- **Rocks** are modal mixtures of minerals (with chemical formulas, `creeer_materials`) plus porosity; their densities are *computed* and checked against measured rock densities, and ore grades against real ones (porphyry Cu ≈ 0.7 %, BIF Fe ≈ 45 %, Ni sulfide ≈ 1.5 %, placer Au ≈ 1.4 g/t, bauxite Al ≈ 28 %).
- **Provinces** (cratons, basins, arcs, orogens, greenstone belts, oceanic crust) are chosen by smooth noise plus bias, minus a penalty outside their elevation range. Each is a column of layers with smoothly varying thickness; folds bend the bedrock but not the soil cover.
- **Deposits** (USGS descriptive models) are ellipsoidal lenses placed by a hash of (seed, kind, cube-sphere cell) when province, elevation and latitude allow; grade falls from 2.5× the average at the core to 0 at the rim. Nothing is stored: geology is a pure function of position and seed (golden hashes in CI).
- The natural surface shows the rock **at the surface** (soil, thinning to bare rock on slopes over 25–40°); only dug walls show the rock at their depth. A coarse chunk's vertices lie up to a cell off the surface, and taking the rock at their own depth made distant land a patchwork of soil and bedrock that changed with the level of detail (a test now checks the surface rock agrees between level 0 and level 9).

### Voxel chunks and meshing (`chunk.rs`, `surface_nets.rs`, `occlusion.rs`)

- **Planet-centred Cartesian octree:** chunks of 32³ cells, level L with 2^L m cells (level 0: 1 m, 32 m chunks; level 15: one chunk for the whole planet). Cartesian chunks avoid cube-face seams in 3D; the cube-sphere is used where 2D data lives (heights, deposit cells).
- **Surface Nets** (one vertex per crossed cell, quads per crossed edge): smooth, blockless, and cheaper than Dual Contouring. Each chunk owns the edges starting at its samples 0..32 and samples one cell beyond, so neighbours of one level join without gaps (tested watertight).
- **Skirts** fill the cracks between levels: open borders fold 3 cells into the rock, **two-sided**, grouped by chunk face, and drawn only on faces whose same-level neighbour is not drawn. Skirts everywhere poked through thin rock (a cave mouth showed a large flap); one-sided skirts left cracks visible from one side.
- **Occlusion:** per vertex, eight rays in a cone through the chunk's own density grid; for edited fine chunks, three near-vertical rays through the full terrain, because local rays cannot tell a long tunnel from an open trench. A tunnel 12 m down measures openness < 0.15, the floor of a bowl open to the sky > 0.35.

### Edits (`edits.rs`)

- **An ordered log of CSG brushes** (dig/fill spheres and capsules): dig d = max(d, −b), fill d = min(d, b). This is the "delta against the seeded generator": it works at every level of detail, costs ~60 bytes per edit, and saves bit-exactly (versioned binary log with signature; corrupt files are refused).
- A chunk applies an edit only to samples within one cell of its brush; brushes smaller than half a cell are left out of coarser chunks (they would alias into cell-sized bubbles). A spatial hash with a 2 m margin serves point queries, because rock just outside a dug brush has a density set by the brush.

### Streaming (`stream.rs`)

- A chunk splits while the camera is within 2.5 chunk sizes of it; chunks without surface never split. Worker threads (cores − 2) build chunks, most important first (distance per size). **A chunk is shown in place of its children until all of them are built**, and zooming out shows the cached parent: no holes (tested every frame while flying 20 km). Edits mark the chunks they touch stale; the old mesh stays until the new one is ready.

### Rendering (`creeer_render::terrain`, `terrain_pass.rs`, `terrain.wgsl`)

- One vertex/index buffer per chunk, drawn at its camera-relative origin (f64 → f32 per frame), 20-byte vertices (position, normal + openness, sRGB colour + roughness).
- Lighting **per point**: the sun through the atmosphere for the sun's elevation at that point (transmittance table), so the night side is dark and the terminator glows; sky light from a table over sun elevation (integrated once on the CPU); cascaded shadows near the camera (only chunks within the shadow range cast); **aerial perspective** with the sky's own scattering integral (10 steps). The atmosphere functions and the PBR lighting are shared with the sky and mesh passes (refactored without changing a single pixel of existing scenes, checked byte for byte).
- A soft 3D detail texture in chunk-local coordinates with a 32 m period (every chunk origin is a multiple of 32 m: seamless, fixed to the ground), faded where a pixel covers its cells.

### Physics on the planet (`creeer_physics::gravity`, `world/ground.rs`, `world/friction.rs`, `creeer_sim`)

- **Planet gravity** in the rotating frame: Φ = −μ/r − ½|ω×r|² enters the implicit step as a potential (differences computed without cancellation: 4.9 MJ/kg potentials, millijoule steps), Coriolis −2mω×v as a force from the start-of-step velocity (error of order (ωh)²).
- **Ground contacts** through a density field (`Ground` trait; `creeer_sim::TerrainGround` adapts the terrain): spheres project to the surface (two steps); boxes test their corners always and other surface points only where the ground rises into the box beyond the plane through the corners. On flat ground this is exactly the box–plane contact of ADR-0005 (the slide test agrees to 1e-6 m); on a bump the bump is touched.
- **Edits wake** sleeping bodies near them; edits are applied in the simulation tick (server-authoritative).

Two F2 defects were found and fixed on the way (ADR-0005 amended):
- **Balls could not roll:** friction acted at a sphere's centre. It now acts at the contact point carried by the body's linearised motion (pure rolling slips exactly zero), and the stick offset is carried between steps. Box contacts are unchanged bit for bit.
- **The energy guard rejected steps starting from rest in a varying field** (absolute floor of 1 nJ): the slack now scales with the larger of the energy in motion before and after the step.

## Validation

| Test | Result |
|---|---|
| Circular orbit 20 km up, quarter orbit (22,560 steps) | radius within 1 mm, phase within 1e-6 rad |
| Body dropped 1 km at the equator, 10 s: eastward deflection ω·g·t³/3 | within 2 % (0.95 m) |
| Effective gravity at the equator (μ/R² − ω²R) vs the pole | ratio within 2e-5 |
| Ball rolling down terrain at 15°: a = 5/7·g·sin θ | within 1 % |
| Box sliding on a density-field ramp vs a plane | same motion to 1e-6 m, same contacts |
| Box on a 15 cm bump; ball resting on a planet-sized sphere | bump touched, nothing deeper than 5 mm; sinks m·g/k within 2 % |
| Datum is an equipotential; level ground ⟂ effective gravity | 1e-8 relative; 1e-6 rad |
| Crate dropped on the real terrain; ball rolling downhill; digging under a resting crate | rests, < 5 mm penetration; energy never rises; falls into the hole |
| Rock densities, ore grades, land fraction, every province and deposit kind present | ✓ (`geology/tests.rs`) |
| Tunnel across two chunks | watertight, every wall vertex within 0.15 m of the surface |
| Golden hashes (Windows, Linux, macOS in CI) | elevation, geology, planet physics |

## Measured (dev machine: 16 threads, RTX 4070 SUPER; min-spec projection ×0.5 per core)

- **Chunk build** (heights with cold tiles, geology, mesh, occlusion): 6.9 ms per chunk on one thread; 1,762 chunks/s on 16 threads → **330 chunks/s projected to 6 min-spec cores** (budget 200).
- **Streaming** the surroundings of a camera on the ground from nothing: 1.4 s (5,088 chunks).
- **Descent from 1,100 km to 2 m in 40 s** at 1080p (frames paced at 60 Hz, CPU and GPU serial): **p50 6.8 ms, p99 14.7 ms, worst 25 ms, no frame over 33 ms**; the first frame (pipelines, sky table) takes 0.14 s.
- 50 crates landing on the terrain: 1.4 ms per tick (223 contacts).
- Before the fixes: skirt masks by point lookups took 7.8 ms per frame; every coarse chunk was a shadow caster in all cascades. Both are gone (masks: six lookups per chunk).
- **The perf gate on the dev machine drifts with heat:** during long benchmark sessions F2 metrics read up to 13–15 % slower than a baseline saved minutes earlier. Interleaved runs of the F2 and F3 binaries showed no difference (contact world 0.548 vs 0.555 ms; welded block 2.84 vs 2.75 ms). The contact-world metric itself varied by ±12 % between identical runs; it is now the best of three fresh worlds (±1 %). `Contact` shrank back to 304 bytes (the carried friction points live beside the contacts).

## Consequences and limits

- **Lighting seams between levels:** each chunk shades with normals from its own resolution, so a faint line can show where a fine chunk meets a coarse one (geomorphing would remove it).
- **Small edits vanish at a distance:** brushes under half a cell are left out of coarse chunks (a 2 m hole is invisible beyond ~250 m); a large pit dug with many small brushes disappears from distant views.
- **One planet in its own frame:** the sun turns about the axis once per sidereal day; orbits, frame switching and the planet's motion are F4.
- **Ground contacts cost more than plane contacts** (each contact point samples the terrain's heights and edits); a large crate samples up to a few hundred surface points when close to the ground.
- **Oceans are dry basins** until fluids (F6). Water would rest level on the equipotential datum.
- **Point-mass gravity:** no oblateness term (J2) and no mass concentrations; the datum is level for exactly that field.

## Amended in F5 (2026-10-16): seamless levels of detail

Two kinds of seam showed in low sunlight: faint lines where levels meet, and straight lines across rolling land. Hunting them also turned up a pinhole crack, which led to the skirts being replaced.

**Creases in the elevation, not level borders.** The straight lines stayed with flat shading turned off, with every level coloured apart and with shadows off; they were creases in the height itself (second difference 700× its median). Two sources: the continent curve and the noise distributions were piecewise linear (a kink along every knot's contour), and ridge crests were V-shaped |n|, sharper than any sample spacing. Both are now smooth: **monotone cubic interpolation** (Fritsch–Carlson, `monotone.rs`: through the same knots, no overshoot, continuous slope), and crests rounded over one sample spacing, |n| → √(n² + ε²) − ε with ε = spacing/wavelength, with the octave weight saturating smoothly. Rounding crests further where the mountain mask fades ("worn foothills") was tried and rejected: over 3,000 random transects the worst 0.1 % improved only from 9.6× to 8.7× the median curvature, too little to justify changing the shape. Golden hashes regenerated: elevation, and planet physics (with the old elevation its old value came back exactly).

**Geomorphing** (after Strugar's CDLOD; `levels.rs`). Every vertex is moved onto its own level's surface (band-limited to its cell) and carries that surface's normal, the next coarser level's normal, and the distance to the coarser surface along its normal. The renderer moves vertices and turns normals towards the coarser level as `smoothstep(4.25 s, 5 s, distance)` (s = chunk size); a coarser neighbour begins no nearer than 5 s (tested), so where two levels meet the finer one already looks exactly like the coarser one. Vertices are 28 bytes (+ coarse normal, morph distance). Fully morphed, a fine chunk lies on the coarse mesh: 1 → 2: 0.30 m mean off it on its own level, 0.008 m morphed (cell 4 m), normals 0.80° → 0.21°; 3 → 4: 0.50 → 0.029 m (cell 16 m), 0.76° → 0.18°. No triangle turns over, as refined or fully morphed (963,581 checked).
- A vertex stays inside its cell (so chunks can be clipped cell by cell), and is not moved where the projection would land on the far side of something thinner than a cell (the normal there faces away) or near an edit's crease, where moving along a normal averaged over both sides cuts the edge off. Without these, flaps covered a tunnel's mouth: refined meshes now fold exactly as many triangles as plain Surface Nets at such edges (27 and 27; 69 without the rule).
- **Exact gradients:** density gradients come from the derivative of the Catmull-Rom heights, the warped cube projection and the datum (implicit function), one height lookup instead of six for central differences. Edits combine by max/min: an edit whose surface stays two cells (scaled by the slope) from the generated one either wins outright (the brush's gradient) or has no part; nearer, the crease may cross the stencil and central differences over one cell are used, as before. All three Jacobians are tested against finite differences.

**Clipping and caps instead of skirts** (`clip.rs`, `caps.rs`). A chunk's mesh ended anywhere in its last cell, and a finer neighbour's began half a fine cell beyond the face: where the ground ran nearly parallel to a face (a horizontal face under level ground), a flat strip was left open that skirts, hanging down, could not close; a CPU ray cast found a 10-pixel gap at a level border. Now:
- Each chunk samples one more layer (35³ samples), builds every quad that reaches into its box, and **clips the mesh exactly at the box**. Two neighbours of one level build the straddling quads from the same world positions (f64, diagonals chosen in world coordinates) and compute every cut point from the segment's endpoints in a fixed order: their borders are the same points, **bit for bit** (tested). A vertex lying exactly on a face is moved one ulp into its own cell, which both neighbours see alike.
- **Caps** cover the rock each face cuts through: from every border piece to the rock end of the sample edge it crosses, from each crossed grid cell's border node to its rock edges, and strips over runs of all-rock cells (a vertex wherever a neighbouring cell needs one, and every corner of the face's rim: no T-junctions). A chunk with its six caps is a **closed solid** (every edge matched, tested on real chunks at levels 0–4 and on cut spheres with the analytic volume within 2 %). Caps are drawn where the same-level neighbour is not drawn, as the skirts were; between two levels any gap lies in the common face, on the rock side of one of them, so that level's cap closes it.
- Cap corners move with the surface as the point they stand for does; corners deeper than three coarse cells (beyond any morph) stay put without field lookups. Corners on box edges are shared between faces.
- **Measured:** 10,251 rays from the air into the rock across level borders (steep and grazing, across and along the border, with the finer side at 0, ½ and full morph) all meet the surface or a cap; 187 of them would slip through without caps. The pinhole is gone from the screenshots.

**Found on the way:**
- **The planet's centre read as air:** the density there has no direction to look a height up in and was NaN; the coarsest chunks around the centre meshed a hidden bubble. The centre is now deep rock (tested).
- **Sky view stepped over thin roofs:** rays sampled 1, 2, 4 … 32 m up and missed a 1.8 m roof between 4 and 8 m, lighting a tunnel floor. Rays now march in steps of half the density, at least 0.25 m (tested with a 1.5 m roof).

**Cost** (interleaved with an F4 build): one chunk with cold tiles 9.1 ms against 6.9 ms (warm tiles: 6.1 against 4.6 ms; the rest is tiles of the coarser level, which streaming shares with the coarser chunks), 1,452 chunks/s on 16 threads → **272 chunks/s projected to 6 min-spec cores** (budget 200; F4: 330). Of the meshing: refining 0.7 ms, caps 0.4 ms, clipping 0.1 ms, the extra sample layer 0.25 ms. Triangles +16 % (caps). The descent benchmark is unchanged: p50 7.0 against 7.2 ms, p99 12.0 against 12.1 ms.

The limit "lighting seams between levels" above is resolved by geomorphing.

## Amended in G1.3c (2026-10-21): rounded edits

G1.3c's check of the terrain up close (ADR-0026, decision 11; `docs/art-style.md` pillar 9, "smooth, never blocky") found a ball dug into the ground drawn with a **zigzag rim**: cell-sized facets, alternately lit and dark, some folded back on themselves.

**The cause:** max/min CSG leaves a crease where a brush meets the ground, and Surface Nets puts one vertex in each cell. A crease that turns by 90° within one cell has no vertex on it, and the cells around it fold their quads across it.

**Prior work.**

| Work | What it does | What it means here |
|---|---|---|
| Quilez, smooth minimum ([iquilezles.org](https://iquilezles.org/articles/smin/)) | `smin(a, b, k) = min(a, b) − h²·k/4`, `h = max(k − abs(a − b), 0)/k`: equal to min where the two differ by k or more, a quarter of k below it at the crease, continuous in slope | Taken, as a smooth max for digs and a smooth min for fills |
| Surface Nets (Gibson 1998) and dual contouring (Ju et al., SIGGRAPH 2002) | One vertex per cell; dual contouring places it by a quadratic error over Hermite data, keeping sharp creases | Not taken: a crease kept sharp would still be drawn with 1 m facets, against pillar 9. A rounded rim is also what loose ground does: it slumps |

**Decision.** Edits combine by a **smooth max (dig) and smooth min (fill)** whose round spans `ROUND_CELLS` = 2 cells of the grid that draws them (`edits.rs`): 2 m on the finest level, wider on coarser ones, so every level stays smooth and geomorphing blends between rounds. Physics reads the finest level's field, so what is drawn up close is what is touched. A round reaches up to its width beyond the brush, so the chunks an edit affects, the samples it changes, the point index's margin (2 m → 4 m) and the crease test of the exact gradients all widen by it.

**The rock on a dug wall:** a vertex showed the rock at its depth when it lay within half a cell of a dig brush, else the rock at the natural surface. On a rounded rim neighbours flipped between the two. Now any vertex below the natural surface shows the rock at its depth, so the layers appear on dug walls as level bands; the mesher no longer walks the chunk's edits for each vertex.

**Measured** (a ball of 4.5 m radius dug 2 m into gentle ground; folds between neighbouring triangles within the rim, `rounds_tests.rs`):

| Edits | Worst fold | Folds over 45°, levels 0 / 1 / 2 |
|---|---|---|
| max/min (before) | 150–154° (flaps) | 52 / 27 / 21 |
| rounds of 2 cells | 44–54° | 6 / 0 / 3 |
| rounds of 3 cells (not taken) | 35–47° | 0 / 0 / 1 |

Three cells would be smoother, but a 3 m round on the finest level turns small holes into bowls. Motion in the stress and play scenes is unchanged (their physics hashes before and after are equal: no body reaches a dug rim there). The `server` stress scene, which digs, runs as before (p50 0.84 against 0.83 ms).

**Limit:** on the finest level a round of 2 m spans about two vertices, so from a few metres away its lighting still shows the triangles as spikes along the rim. Smooth up close needs a finer mesh near the camera (0.5 m cells) or normals per pixel (`docs/future-improvements.md`).

## Amended in G2.9 (2026-10-26): 12.5 cm cells near the player

The user's play: a hand-dug post hole never showed, and the ground stayed solid where it was dug ("there is no visible hole"); the user asked for "highly detailed terrain detail ... so every dig makes a visible difference".

**Prior work** (surveyed for this amendment; marked where unconfirmed):
- Teardown keeps 10 cm voxels, run-length encoded (Gustafsson, Software Engineering Daily 2024).
- Dual Universe keeps 25 cm "precision voxels" in an octree, compressed on the server.
- Valheim first rebuilt its heightmap from every modifier in range, one a blow, and in 2021 changed to a per-zone record (TerrainComp) because of that cost.
- Godot Voxel Tools edits only its finest level and saves only modified blocks.
- Dreams records sculpts as lists of distance-field add and subtract operations (Evans, SIGGRAPH 2015).
- Space Engineers and 7 Days to Die use about 1 m cells (unconfirmed).
- Clipmaps (Losasso & Hoppe 2004) keep a ring of about equal chunks per level, so a finer level costs about one level's chunks, not eight times the chunks.
- Transvoxel (Lengyel 2010) and CDLOD (Strugar 2010) join levels that differ by one, as ours do.

**Decision.**
- Three levels finer than 1 m. Level 0 now has 12.5 cm cells in 4 m chunks; level 3 has 1 m cells (`chunk::METRE_LEVEL`); the coarsest is level 18. With `detail` 2.5, 12.5 cm cells are drawn within about 10 m of the camera, 25 cm within 20 m, 50 cm within 40 m and 1 m within 80 m.
- The generated ground has no detail finer than 1 m, and physics reads it at 1 m (`tiles::FINEST_SPACING`). Heights are sampled at that spacing however fine the cell, so the fine levels draw exactly the ground physics touches; only edits are finer.
- **Rounds of three cells** (`ROUND_CELLS`). Over two cells, a rim on the 12.5 cm level still folded 62°. Over three it folds 47° at most on every level (`rounds_tests`, levels 0–5: 40, 43, 45, 40, 35 and 47°). The reason three cells were rejected above, 3 m bowls round small holes, holds only for a 1 m finest level: the finest round is now 37.5 cm, and physics rounds with it.
- Chunk ids hold 5 bits of level and 19 bits a coordinate: planets up to about 1,000 km in radius.
- An edit marks stale only the chunks of the levels that draw it (`Streamer::invalidate_edit`): a coarse chunk leaves a small brush out, and a blow's dig rebuilt every level's chunk over it.
- **Edits replaced in their place** (`Edits::replace`). A dig spot's one brush grows with every blow (ADR-0032, amended). A server sends the log again from the first edit replaced since; a client changes those edits in place and redraws only where the old and new versions reach (`Connection::edits_dirty`). One brush a blow would put hundreds at one spot, each read at every point near it.

Validation: the closed-mesh, cap, crack-ray and geomorph tests run as before on the old levels (each moved by three). The crack-ray test also crosses the new borders 0→1 and 1→2. The 1 m levels' results are unchanged.
