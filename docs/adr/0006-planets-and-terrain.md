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
