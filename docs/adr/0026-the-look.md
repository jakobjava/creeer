# ADR-0026: The look: models, materials, ambient occlusion and the engineer (G1.3)

**Status:** in progress (2026-10-21). The user approved the plan (`docs/g1-plan.md`, G1.3) and chose the jetpack on a harness (later the same day taken out of the starting outfit: it is made as the player progresses), the skinned body, and a hood and goggles with the face open. The user then added the base body and wearables (decision 7), a Factorio-like weathered look (decision 3) and ageing (decision 4). The engineer's blockout was approved after three rounds (2026-10-21): no hood, the head bare with goggles pushed up; Blender Studio's base mesh fitted to ANSUR II's men of normal weight; thin clothes. The user kept the Monk scale's ten skin tones (tones 1–3 and 9–10 as approved exceptions) and the nine measured hair colours. The detailed engineer was then made over the user's notes, round by round (2026-10-21): the coverall in canvas, the jacket two-tone, real boots, hanging cloth, hair and brows, no beard, the close-up budget raised to 50,000, the colours approved; then fitted and checked in poses (2026-10-22).
**Context:** G1.3 makes `docs/art-style.md` real, so that everything after it (tools, items, trees, the pod) is made in the style from its first version. The user, at its start: the engineer after Factorio's FFF-40 engineer as "a very rough guide (do not copy and leave the hard hat, tools, rucksack, etc. out)"; "i do not want a futuristic art style but something similar to the vibe of factorio"; "i do not want a blocky or voxy like game (no minecraft geometry, i want smooth surfaces ==> realism)".
**Builds on:** ADR-0003 (the renderer: forward, 4× MSAA, HDR, cascaded shadows, PBR-lite, Khronos PBR Neutral), ADR-0018 (data packs and the validator), ADR-0022 and ADR-0023 (joints, the character and its ragdoll).
**Evidence:** to come with each sub-milestone (G1.3a–i).

## Where it starts

- **Meshes** are a position and a normal per vertex (`creeer_render::mesh`). Colour, roughness and metalness come per instance, so a mesh has one material.
- **Characters** are drawn as a box and two beams (`creeer_client::planet_net`). A fallen character's fifteen ragdoll segments become entities of their own, and the client cannot tell they belong to a person.
- **No ambient occlusion and no depth prepass.** One forward pass draws into a 4× MSAA HDR target. The ambient light is hemispheric and unoccluded, so objects do not sit on the ground.
- **Surfaces.** The terrain has triplanar noise; meshes have none.
- **No models, no palette data, no style checks.**

## Prior work

| Work | What it does | What it means here |
|---|---|---|
| Factorio, FFF-146 "The GFX workflow" ([2016](https://factorio.com/blog/post/fff-146)) | Entities are modelled and rendered in Blender, in layers. The renders are finished with hand-painted masks "to enforce contrast, make edges clearer, define shape of entities better" | Taken: Blender as the modelling tool. Their painted contrast has to come, in real time, from ambient occlusion, edge wear and the palette's contrast rules (`art-style.md` §4) |
| Factorio, FFF-218 ([2017](https://factorio.com/blog/post/fff-218)) | The engineer merged into one Blender file of 21 scenes; scripts link shared meshes and materials so edits propagate. The player's colour is a tint mask over desaturated areas, at half strength; the default colour is orange | Taken: scripted sources; the player's colour as a material on a neutral region (the jacket) |
| Factorio, FFF-320 "Color correction" ([2019](https://factorio.com/blog/post/fff-320)) | More light and a little saturation, while "the terrain is a background that should provide a good and comfortable contrast with the entities and the icons"; players warned of a "plastic playground" | Confirms `art-style.md` §4: chroma caps, objects contrasted against the ground |
| Factorio, FFF-40 ([2014](https://factorio.com/blog/post/fff-40)) | The engineer's renders: padded, layered workwear, goggles, hard hat, a rucksack with tools; a "solitary explorer" | The user's rough guide for the engineer, without the hard hat, tools and rucksack |
| Foxhole (Clapfoot; [concept art devblog](https://www.foxholegame.com/post/the-concept-art-of-foxhole-inferno)) | Facilities "old, well used, all metal"; original designs rather than historical copies | The tone of `art-style.md` pillar 10: grounded, used, industrial, not futuristic |
| Stylised PBR: *Agents of Mayhem* (Volition, GDC 2017); Riot's character shape language ([80.lv](https://80.lv/articles/character-design-shape-language-and-readability/)) | Full PBR with the style in colours and shapes; silhouettes judged zoomed out from large blockouts | Taken: the lighting stays physical (ADR-0003) and the style lives in palette and shapes; blockout and silhouette first |
| GTAO: Jimenez et al., "Practical Real-Time Strategies for Accurate Indirect Occlusion" (SIGGRAPH 2016 course) | Horizon-based, cosine-weighted visibility that matches ray-traced ambient occlusion, with a multi-bounce fit for bright albedos | Taken |
| XeGTAO (Intel, 2021, [MIT](https://github.com/GameTechDev/XeGTAO); archived) | GTAO in three compute passes (depth MIPs, main, denoise). "High" (18 samples a pixel) costs 0.56 ms at 1080p on an RTX 2060. Tuned by searching heuristics for the lowest error against ray-traced references. Optional bent normals (+25 %) | Taken as the reference implementation; its tuning against ray-traced references taken as our validation |
| Bevy's SSAO (MIT/Apache) | XeGTAO ported to WGSL on wgpu, after a depth and normal prepass; paired with TAA for noise | Shows that the method maps onto our API; a reference for the port |
| Visibility bitmask: Therrien et al., "Screen Space Indirect Lighting with Visibility Bitmask" ([2023](https://arxiv.org/abs/2301.11376)) | Replaces GTAO's two horizon angles by a bitmask, so light passes behind thin occluders instead of over-occluding | The variant to measure if thin geometry (members, branches) is over-darkened. Not needed: the factory's rods and pipes show no dark halos (G1.3d) |
| Screen-space occlusion in forward renderers (practitioners' threads: [gamedev.net](https://gamedev.net/forums/topic/717004-msaa-resolve-of-normals/), [Khronos](https://community.khronos.org/t/tips-on-implementing-ssao-using-msaa/112588); Unity's URP and Toon RP docs) | Occlusion belongs on ambient light only, so it must be known while shading: a depth (or depth and normal) prepass, which a forward renderer often draws anyway against overdraw; the alternative is to keep ambient light apart and occlude it afterwards | Both measured (decision 5): the prepass is faster here and exact per pixel |
| Keller and Heidrich, "Interleaved Sampling" (EGWR 2001); GTAO's own denoise (Jimenez et al. 2016, §5: a 4×4 noise pattern, a 4×4 spatial filter, then temporal) | Neighbouring pixels take different, complementary samples from a small tile; a filter over the tile gathers them all | Taken: XeGTAO's 64×64 Hilbert noise and its 3×3 denoise lean on temporal anti-aliasing, which the game does not have |
| Filament's ambient occlusion ([documentation](https://google.github.io/filament/Filament.html)) | A material's own occlusion and the screen's combined by the lesser of the two, not their product, so a crease is not darkened twice | Taken: the models' baked occlusion and GTAO by the lesser |
| GPU timestamp queries (WebGPU `timestamp-query`; wgpu's [example](https://wgpu.rs/doc/wgpu_examples/timestamp_queries/index.html)) | Each pass writes a timestamp at its start and end into a query set, resolved into a buffer and read back later; an optional feature | Taken for every pass, read back frames later so nothing waits (G1.3d) |
| Kavan et al., "Skinning with Dual Quaternions" ([I3D 2007](https://users.cs.utah.edu/~ladislav/kavan07skinning/kavan07skinning.html)); TOG 2008 | Blends rigid transforms without linear blend skinning's volume loss at bends and twists, at about 7 more vertex-shader instructions; no scale or shear | Taken: the bones are rigid segments, with no scale |
| Unreal's Physics Asset ([docs](https://dev.epicgames.com/documentation/unreal-engine/physics-asset-editor-in-unreal-engine)); Unity's ragdolls | A skinned mesh's bones each carry one physics body; the ragdoll moves the bones, and the skin follows | Taken: the ragdoll's fifteen segments are the skeleton's bones |
| Unreal's GPU skin cache | Skins each mesh once a frame in a compute pass; every later pass (shadows, depth, colour) draws the result | Taken: one compute pass skins the characters, and every pass draws them as plain meshes |
| Rosen, "An Indie Approach to Procedural Animation" (GDC 2014, *Overgrowth*) | Interactive animation from very few key poses, blended procedurally with springs and physics | Taken: a handful of poses authored in Blender, blended by gait phase and speed |
| Johansen, *Automated Semi-Procedural Animation for Character Locomotion* ([Aarhus, 2009](https://runevision.com/thesis/)) | Cycles analysed at design time; at run time the legs are fitted by inverse kinematics so the feet land on uneven ground (the "footbase", ground height) | Taken: the feet placed on the ground the legs' rays find, by two-bone inverse kinematics |
| Curvature-based wear: Blender's Pointiness and Dirty Vertex Colors; Substance's curvature masks; Unigine's screen-space curvature | Convex edges chip to the bare material and concave areas gather grime; the masks broken up by noise. Screen-space curvature aliases at distance (reported) | Taken: wear on the bevel faces and occlusion baked at export, broken up by noise. Screen-space curvature not taken |
| Factorio's look, as players describe it ([forum](https://forums.factorio.com/viewtopic.php?p=511707)) | Renders "distressed" after rendering; a hand-painted layer of grime over parametric textures; weathered where mods look "shiny and new" | Taken as the goal: grime, wear and age as layers over every material |
| Procedural wood: Modo's Wood procedural ([Foundry](https://learn.foundry.com/modo/content/help/pages/shading_lighting/shader_items/wood.html)); plank shaders (Mankala's write-up) | Rings distorted by waves, plus grain noise; for planks, noise stretched along the board and ring bands from it | Taken: thin wandering grain lines (crests of a sine displaced by slow noise) over streaks stretched about 1:20 along each part |
| Procedural dirt (Blender geometry-nodes AO, Octane's Dirt, practitioners' guides) | Dirt from baked occlusion and position, edge wear from convexity, each a layer broken by noise; rust where water collects | Taken: the ground's splash by height and facing, dirt in creases from the occlusion, rust and patina first where dirt holds water |
| Atmospheric corrosion: ISO 9223 (corrosivity categories) and ISO 12944 (paint durability) | First-year loss of carbon steel: C2 1.3–25 µm, C3 25–50, C4 50–80, C5 80–200, CX up to 700. Paint systems last 7–15 years at medium durability | Taken: bare steel weathers in about half a year at C3, paint in 15; the sea three times faster |
| Wood weathering (lignin photodegradation studies; natural weathering of 16 species) | Yellowing or browning within days to weeks of sun, then a grey cellulose layer 2–5 mm thick over months; denser woods change more slowly | Taken: timber greys in about a year outdoors |
| Copper patina (Copper Development Association) | Russet to chocolate brown within months; green in 5–7 years on coasts and in industrial air, 10–14 rural, over 30 in dry air; moisture's dwell time decides | Taken: copper greens in about 10 years |
| ISO 6272 falling-weight test | Coatings judged by the drop energy they take without cracking, typically of the order of 10 J | Taken: impacts beyond about 10 J chip edges |
| Blender MCP practice (agent-skills notes on `blender-mcp`) | Exports through the MCP time out; run them in headless Blender | Taken: the MCP for live work, exports by `blender --background` from committed scripts (`docs/blender-mcp.md`) |
| glTF 2.0 (Khronos); the `gltf` crate 1.4.1 | The standard format, with skins, animations and vertex colours; custom attributes need the crate's `extras` feature | Taken; the baked masks go in `COLOR_0`, so no custom attributes are needed |
| World of Warcraft's geosets ([warcraft.wiki.gg](https://warcraft.wiki.gg/wiki/Character_model)) | The player model is divided into pieces of geometry that equipment toggles or swaps | Taken: the base body in regions that worn things hide |
| Bethesda's biped slots (Skyrim; [UESP](https://ck.uesp.net/wiki/Skyrim_bodyparts_number)) | Each armour piece fills numbered body slots; its mesh is partitioned by body part, often carrying the body beneath it | Taken: wearables fill slots. Not taken: carrying a copy of the body in each piece (a body per piece to keep in step) |
| inZOI's masking system ([ModKit](https://mod-docs.playinzoi.com/ModKit/project/CAZ/Guide/03.%20Masking%20System.html)) | Each outfit names the lower layers' regions it covers and the body regions it hides, "to prevent body parts from poking through shirts, pants, shoes" | Taken: each wearable lists the regions it hides, of the body and of the layers beneath |
| Modular characters in practice (Unreal's leader pose; [gamedev.net](https://gamedev.net/forums/topic/710077-how-to-implement-a-modular-clothing-system); [polycount](https://polycount.com/discussion/comment/2636650)) | All parts share one skeleton, animated once; covered meshes are not drawn. Practitioners warn that skin hidden only partly will still clip through clothes in some poses, and prefer replacing whole segments | Taken: one skeleton animated once; whole regions hidden, never a part of one; poke-through measured in extreme poses |
| Blender Studio, Human Base Meshes v1.4.1 ([demo files](https://www.blender.org/download/demo-files/); CC0) | Realistic and stylised men's and women's bodies, heads, hands and feet: quad topology, closed volumes, multires levels, UVs. The realistic man is 1.69 m tall, in an A-pose, with separate eyes; he is an athlete (shoulders 0.53 m across once scaled, muscles defined) | Taken (the user's choice over modelling the body ourselves): the engineer's body, fitted. Downloaded once from a pinned address and checksum, never committed (`art/base_meshes.py`); only its mesh data is appended, with scripts off. Its bundle carries a stale text naming another rig's CC BY licence; the README and the download page say CC0 |
| MakeHuman / MPFB (CC0 assets, GPL code) | Parametric bodies from sliders (weight, muscle, proportions) | Not tried: the user chose the base mesh. Its sliders would set a build directly where we fit one; a candidate for the players' body shapes later |
| SMPL and SMPL-X (Max Planck Institute) | Bodies learned from thousands of scans (CAESAR), shaped by a few parameters | Not usable: their licence is for non-commercial research only |
| ANSUR II (US Army, 2012; Gordon et al. 2014; [public data](https://www.openlab.psu.edu/ansur2/)) | 93 measures of 4,082 men and 1,986 women, with weight and stature | Taken: the engineer's heights (all men's means) and his build (the men of normal weight, BMI 18.5–25: 1,061), at 1.80 m |
| NHANES 2015–2018 anthropometric reference data (Fryar et al. 2021, [CDC](https://stacks.cdc.gov/view/cdc/100478)) | US civilians measured at rest: men of 20 and over 1.753 m tall on average, their median mid-upper-arm girth 34.4 cm | A cross-check: civilians' arms at rest are near ANSUR's normal-weight men's flexed (33.0 cm at 1.80 m) |
| Baran and Popović, "Automatic Rigging and Animation of 3D Characters" ([SIGGRAPH 2007](https://people.csail.mit.edu/ibaran/papers/2007-SIGGRAPH-Pinocchio.pdf)), as Blender's automatic weights | Skin weights from heat diffusing from each bone through the mesh, smooth across the joints | Taken for the blockout's fit and pose: one bone per limb, its weights kept on the body and on every garment copied from it |
| Taubin, "A Signal Processing Approach to Fair Surface Design" (SIGGRAPH 1995) | Smoothing in pairs of passes, one averaging and one slightly un-averaging, so small features fade without the shrinkage of plain averaging | Taken: garments smoothed off the body's relief; far stronger (pass band near 1 m) where cloth hangs, at the seat and over the chest |
| Garments from the body in practice (sculpting's mask-and-extract, shrinkwrapped shells; cloth simulation in Marvelous Designer or Blender) | A first garment is a shell of the body grown outward; detailed ones are simulated from patterns | Taken for the blockout: each garment an offset of the fitted body, so it fits and poses with it. Simulation is a candidate for the detailed models |
| Paraskevas et al., "Study of the carrying angle of the human elbow joint in full extension" (Surg. Radiol. Anat. 2004; 600 students) | The forearm's outward angle from the upper arm, palms forward: 11.0° in men, 15.1° in women | Measured and not taken: with the palms to the thighs the forearm lines up with the upper arm |
| 9-slice scaling ([Wikipedia](https://en.wikipedia.org/wiki/9-slice_scaling)); "Cheap round-edged box" ([three.js forum](https://discourse.threejs.org/t/cheap-round-edged-box-vertex-shader/8066)) | Corners keep their size while the rest stretches. In 3D, a vertex shader places each vertex at its inner box's corner (from its signs) plus its normal times the radius, and it works with instancing; a guard is needed when the box is smaller than the radius | Taken: primitives bevelled in the vertex shader, so a bevel keeps its size in metres under any instance scale; the radius limited by the smallest side |
| Face-weighted normals on hard surfaces (Polycount practice, [thread](https://polycount.com/discussion/comment/2577165)) | A one-segment chamfer whose sides carry the faces' own normals shades like a rounded edge, and can stand in for a baked normal map on clean hard edges | Taken for authored models' small bevels. Not for the primitives: their 2.5 cm chamfer strayed beyond `model_check`'s smoothness rule (see "Measured and rejected"), so they get true rounds, the faces' normals at their sides |
| Hoppe, "View-Dependent Refinement of Progressive Meshes" ([SIGGRAPH 1997](https://hhoppe.com/proj/vdrpm/)) | Refines a mesh where its geometric error, projected to the screen, exceeds a tolerance; geomorphs hide the pops | Taken: levels chosen by their projected error. Not needed: geomorphs, as the error stays under half a pixel |
| Nanite (Karis, Stubbe and Wihlidal, SIGGRAPH 2021), as described by secondary sources | Clusters chosen so their error projects to about a pixel; for shadows, a texel of the map drawn to. A metric too strict costs time, one too loose pops | Taken: the error bound in pixels. Half a pixel here, since only silhouettes show it |
| Cesium 3D Tiles ([CesiumJS](https://cesium.com/learn/ion-sdk/ref-doc/Cesium3DTileset.html)) | Each tile's geometric error projected to the screen; refined above a maximum (16 pixels by default) | The same measure; its tolerance suits streamed terrain, not silhouettes |
| Geometric specular anti-aliasing: Kaplanyan et al., "Filtering Distributions of Normals for Shading Antialiasing" (HPG 2016); Tokuyoshi and Kaplanyan, "Improved Geometric Specular Antialiasing" (I3D 2019); as Unity's HDRP ([`CommonMaterial.hlsl`](https://github.com/Unity-Technologies/Graphics/blob/master/Packages/com.unity.render-pipelines.core/ShaderLibrary/CommonMaterial.hlsl)) and Filament implement it | The normal's change across a pixel, from screen derivatives, widens the specular lobe: α² grows by twice the variance, capped. HDRP: the variance "should be at most 0.5² = 0.25", a Gaussian pixel of σ = ½ pixel | Taken, at that variance, for every mesh; both pipelines |
| Centroid interpolation (Direct3D, Vulkan and WGSL's `@interpolate(…, centroid)`) | With multisampling, a pixel only partly covered by a triangle is shaded at its centre, often outside the triangle, where attributes are extrapolated; centroid sampling evaluates them within the covered samples | Taken for normals: a bevel thinner than a pixel was shaded with normals extrapolated past its faces' |
| The donkey jacket (British workwear since 1888; the OED's 1929 "leather shoulders and back") | A dark canvas work coat whose shoulders and back are a panel of their own (leather, later PVC, orange on council workers') | Taken: the jacket's yoke a navy panel over the shoulders and down the back, its body in the player's colour |
| Canvas work trousers (Carhartt's duck canvas, double knees) | Heavy cotton duck, a second layer sewn over the knees where work wears through | Taken (the user's choice over quilting): the coverall in navy canvas, double layers over the knees and the seat |
| Quilted insulated workwear (US patent 4,460,645; Dickies' diamond-quilted jacket) | Padding held by stitched lines about 7.5 cm apart, in diamonds | Tried on the coverall and rejected by the user (Measured and rejected) |
| Work boots as boot makers describe them | A toe box under a toe cap, a heel counter, eyestays with punched eyelets and speed hooks, a padded collar, the upper on a welt over a lugged rubber sole | Taken: the boots built as made (`art/engineer/boots.py`, `last.py`) |
| American Optical's welding goggles (1930s) | Leather cups with steel rims round green lenses and steel side shields, a bridge between the cups, a strap round the head | Taken: the goggles pushed up on the forehead, their strap a flat band (`goggles.py`, `gear.py`) |
| Real-time clothing in practice (polycount; character artists' breakdowns) | Large and medium folds in the geometry; fine creases and the weave in the surface | Taken: hang, folds, stacks, seams and sewn-on panels in the geometry (`relief.py`); the weave, wear and grime in the shader |
| The draped figure's seven folds (Bridgman) | Pipe, zigzag, spiral, half-lock, diaper, drop and inert folds, each from how the cloth is held | Taken: pipe folds where the belt gathers the jacket, zigzag stacks above the cuffs and the boots, creases in the crooks |
| Garland and Heckbert, "Surface Simplification Using Quadric Error Metrics" (SIGGRAPH 1997), as Blender's Decimate | Edges collapsed in the order of the squared distance they move the surface by | Taken: each piece decimated whole to each level's triangles, so its regions meet without cracks; refined afterwards where its facets strayed |
| Copying skin weights from a body (Blender's Data Transfer modifier; Maya's Copy Skin Weights) | A garment's points take the weights of the body nearest them, and move as the skin beneath them | Taken, layer by layer: each garment weighted as what lies beneath it |
| Lewis, Cordner and Fong, "Pose Space Deformation" (SIGGRAPH 2000) | Corrective shapes, blended in by the pose, mend what skinning gets wrong at a bent joint | Not taken yet: the layers in a crook are hidden instead (decision 8, Regions) |
| Vaillant et al., "Implicit Skinning: Real-Time Skin Deformation with Contact Modeling" (SIGGRAPH 2013) | Linear blending and dual quaternions cannot model the contact in a joint's crook. Implicit surfaces fitted to the parts push the skinned vertices out of each other; the method fails where the first skinning self-intersects badly | Not taken: a cost per vertex every frame, for every player. The crook's lower layers are hidden instead |
| Helper (corrective) joints: US patent 11,151,766, "Photoreal character configurations for spatial computing"; riggers' practice ([tech-artists.org](https://www.tech-artists.org/t/advice-on-gettting-good-joint-deformation-for-game-models/11146)) | Extra joints driven by the skeleton's angles carry what corrective shapes would, cheaper than blend shapes at run time | Not taken yet: the bones are the ragdoll's fifteen; a client-side joint driven by an angle is recorded (`docs/future-improvements.md`) |
| Edge loops at joints in practice ([polycount](https://polycount.com/discussion/comment/1402970); [Blender Artists](https://blenderartists.org/t/character-for-animation-topology/563479)) | Two or three loops across each bending joint keep a bent elbow or knee's volume and shape | Chosen by the user for the joints closed fully, and measured first: density cannot part layers that the skinning itself folds over. Taken only where the miss was density, the boots' bend (Measured and rejected) |
| Mohr and Gleicher, "Deformation Sensitive Decimation" (2003); DeCoro and Rusinkiewicz, "Pose-independent Simplification of Articulated Meshes" ([I3D 2005](https://gfx.cs.princeton.edu/pubs/DeCoro_2005_PSO/skeleton.pdf)) | Quadric error summed over example poses, or weighted by how likely each pose is, so decimation keeps the triangles a joint needs when it bends | Not taken: undecimated, the crooks fold as badly. The levels keep their rest-pose quadrics |
| Leaf et al., "Bolt: Clothing Virtual Characters at Scale" ([2025](https://arxiv.org/abs/2504.17614)) | Weights carried from body to garments by the nearest point fail where cloth layers lie close in a pinch (the armpit); they take the body point along the cloth's normal, falling back to the nearest | Agrees with what was measured here: by ray and by nearest point each fail somewhere. The layers' order is kept by hiding what is enclosed |
| The American Academy of Orthopaedic Surgeons' normal joint ranges (*Joint Motion*, 1965) | The shoulder flexes 180° and extends 60°; the hip flexes 120°, extends 30°, opens 45° and closes 30°; the ankle bends up 20° and down 50°; the elbow 150°, the knee 135° | Used to tell the ragdoll's limits a person reaches from those no one does (Limits) |
| glTF 2.0's `doubleSided` materials | Back faces drawn, lit with their normals turned to the eye, for thin surfaces such as cloth | Taken for G1.3e: the engineer's garments are single shells |

## Decision (proposed)

1. **Models from scripts.** Each authored model is built by a Blender script in `art/`, deterministic, run live through the MCP while it is made and headless (`cargo xtask art`) for export. The `.glb` files go in `data/models/` with a manifest (`models.ron`: each model's class, triangle budget, size and origin), and `.blend` files are not committed (`docs/blender-mcp.md`).
2. **Materials are palette tokens.** `data/art/palette.ron` holds every token of `art-style.md`, tested against the file's tables.
   - Each mesh vertex carries its token's index, its baked occlusion and its convexity.
   - Each instance carries the player's accent, how worn it is, and the overlay value.
   - A token can name the material that wear reveals (painted steel wears to `steel`).
   - Procedural meshes (members, the sandbox's bodies) keep a per-instance material, as now.
3. **Surfaces: grainy, weathered, used** (`art-style.md` §6; revised 2026-10-21 after the user saw the first crate: "a very boring and blandy monochrome wood texture", "it should look like factorio … grainy, weathered, used", then "darker and grimier").
   - **What a model carries,** baked by its script: occlusion, edge wear on its bevels, each part's tone, each corner's height above its base, and grain coordinates in metres along each part's grain. A per-face value on large faces, since per-corner values drew diagonals.
   - **Per token,** in the palette: a pattern (wood, paint, metal, stone, fabric, leather, glass, plain), what wear reveals, what age turns it into, and the years that takes.
   - **The shader's layers:** tone, pattern, mottling and fine grain (filtered so nothing shimmers), age, grime, rain streaks, edge chips, occlusion on ambient light. They are worked out live in Blender (`art/preview.py`, the same layers as nodes), on three crates side by side, new, one year and four years weathered.
4. **Ageing** (the user: "the game should have an aging mechanic, things look older and older"; `art-style.md` §6b).
   - **Each token weathers at its material's real rate** outdoors in temperate air (ISO 9223 C3): timber in about a year, bare steel in half a year, paint in 15, copper in 10.
   - **Surroundings set the rate:** air with oxygen and water 1, the sea 3, vacuum 0.
   - **A world pace** makes it visible: one year of weathering per game day by default.
   - **Impacts beyond about 10 J chip edges** (ISO 6272).
   - **The server keeps each object's age** as its exposure, its current rate and its wear, changed only on events. It is saved and replicated, so it costs nothing per tick, and clients draw it.
5. **Ambient occlusion: GTAO** after XeGTAO, applied to ambient light only (the sun has its shadows), with the multi-bounce fit. Built in G1.3d (2026-10-22; `renderer/ao_pass.rs`, `ao_textures.rs`, `shaders/ao_*.wgsl`, `gtao.wgsl`).
   - **A depth prepass, chosen by measuring both options** (Measured and rejected): the meshes and the terrain are drawn depth-only into the multisampled depth first; GTAO runs on it; the scene pass then tests for equal depth (`@invariant` positions), so each pixel is shaded once, and the meshes' and terrain's shaders read the occlusion at their pixel. Option (b), ambient light in a target of its own occluded afterwards, cost 0.07–0.18 ms more a frame. With occlusion off, no prepass is drawn.
   - **Three compute passes, XeGTAO's, ported to WGSL:** the depth's sample 0 as view-space depth in five MIPs (the fifth its own dispatch: WebGPU allows four storage textures a stage); the main pass, normals from the 32-bit depth; then the filter.
   - **Noise and filter for a game without temporal anti-aliasing:** the slices turn by 16 points of the R2 sequence laid over each 4×4 tile in Bayer order, and a 4×4 bilateral mean (against each pixel's own plane) takes in all sixteen, as GTAO's paper filters its 4×4 pattern. XeGTAO's 64×64 Hilbert noise and 3×3 denoise left grain without TAA.
   - **Applied:** the multi-bounce fit per colour channel, no darker than half (`art-style.md` §5), and a model's baked occlusion where that is darker (the lesser of the two, as Filament combines them).
   - **Settings:** XeGTAO's tuned constants (radius multiplier 1.457, falloff 0.615, distribution power 2, MIP offset 3.3, final power 2.2); a radius of 1 m. Presets: low (one slice of two steps, half resolution), medium (two of two), high (three of three, the default), and off; the sandbox's window switches them. A reference preset (16 × 16, full-resolution depth only) exists for validation.
   - **GPU timestamps** (`renderer/timing.rs`): every pass is timed where the adapter allows it, read back a few frames later, shown in the F9 profiler and printed by `--bench`.
6. **The engineer is skinned on the ragdoll's bones.** Fifteen bones, one per segment, so the drawn body is the simulated one bone for bone, smooth across its joints.
   - Dual quaternion skinning, in one compute pass per frame.
   - A ragdoll's segments say whose they are and which segment they are (protocol 7), so the client can draw the person.
   - This replaces `docs/g1-plan.md`'s first decision to confirm (a segmented suit). Smooth surfaces are now a rule (`art-style.md` pillar 9), and rigid segments would show seams and gaps at every bent joint.
7. **A base body and wearables** (the user, 2026-10-21: "in future I want to have many wearables, so there should [be] a 'naked' version that can wear more and more cool stuff as you progress").
   - **One skeleton, animated once.** The base body and every wearable are skinned meshes on the same fifteen bones.
   - **Regions.** The base body and every wearable are divided into regions (head, face, hair, neck, torso, upper arms, forearms, hands, pelvis, thighs, shanks, feet). A wearable fills slots (head, eyes, face, torso inner and outer, hands, legs, feet, back) and lists the regions it hides, of the body and of the layers beneath (geosets, biped slots, inZOI's covering regions).
   - **Drawn as one.** When an outfit changes, the visible regions of everything worn are merged into one index list, skinned and drawn as one mesh.
   - **The starting outfit is made of wearables** (`art-style.md` §8), so the same system carries every later piece.
   - **Gameplay comes later.** Wearables become items in G1.6 (each worn piece's mass on its segments), and their insulation and protection count from G9.
8. **The engineer's look**, as `art-style.md` §8 says: the base body with an open face and the player's skin tone, in the starting outfit. A blockout comes first, judged by its silhouette and approved by the user before the detailed models (approved 2026-10-21, `art/engineer/blockout.py`).
   - **A real man of normal weight.** Blender Studio's realistic base mesh (CC0), fitted to ANSUR II at 1.80 m: heights through measured landmarks to all men's means (his legs were 4.7 cm short), arms lengthened to the wrists' height; the build to its men of normal weight, region by region, measured on the posed body (`body.py`, `physique.py`).
   - **Garments are offsets of the body,** so they fit him and pose with him: cut along planes, pushed out, smoothed off his relief and pushed back out of what they are worn over (`garments.py`).
   - **The ragdoll follows the person,** not the reverse: its segments are moved to his proportions (G1.3f).
   - **Levels of detail** (the user, 2026-10-21): the detailed engineer in levels chosen by screen error, as decision 11's primitives: close-up about 30,000–40,000 triangles, mid 10,000–15,000, far 2,000–4,000 for play distance and the shadows. Detail can only be geometry here (no image textures, no normal maps), and players multiply the cost (the main pass and up to four cascades each), so the close-up budget is measured before it is modelled to: one player close up and a crowd in the factory scene, with stand-in meshes of each level's triangle count.
   - **The detailed model** (G1.3f): built by `art/engineer.py` from the scripts in `art/engineer/` into one file, `data/models/engineer.glb`, every piece divided into regions at three levels and skinned on the fifteen bones (`docs/blender-mcp.md`). The outfit as `art-style.md` §8 says, approved round by round.
     - **Levels.** Each piece decimated whole by quadric error to its share of the budget (the close-up level all of it, mid 34 %, far 6.5 %), its rims thickened, its real edges (over 30°) shaded hard, and the close-up level refined until every smooth edge keeps §7's millimetre. Small hardware stays whole: buckles, eyelets and the zip's pull close up only; the larger (the belt's buckle, the boots' collars) mid way too; straps and soles decimated with the cloth beyond the close-up level. The close-up budget was raised to 50,000 for the detail (the user, 2026-10-21).
     - **Regions** (decision 7). Each face goes to the segment it weighs most on. The body's hidden regions end inside the garments' openings, 2 cm into the neckline, 3.5 cm into the sleeves and 3 cm into the trouser legs, where its faces are cut, so a look into an opening finds skin (`regions.py`, `outfit.openings`). What each wearable hides is `data/wearables.ron`, checked against the model (`creeer_art::wearable`).
     - **What lies inside a wearable is hidden while it is worn** (`inside.py`; the user chose edge loops, and measured they could not do this: Measured and rejected). A piece's faces that a wearable over it encloses at that level take the region `in.<wearable>`. A face is enclosed when that wearable lies over each of its corners, along the skin's normal, and every line of sight within 60° of that normal, from 3 mm beneath, meets it as that level draws it. So the coverall's sleeves inside the jacket's are hidden, and its legs inside the boots, the gloves' cuffs inside the sleeves, and the skin under the vest and shorts. At a joint's limit the skinning folds its crook over itself, and the layers there cross whatever their density. Hidden, the one beneath is not drawn to show through, and the outfit draws fewer triangles.
     - **Weights.** Each garment is weighted as what lies beneath it, so dual quaternions move it as the skin under it whatever the distance between them: the base layer and gloves as the nearest skin; the coverall as the nearest skin on its own limb (a hanging sleeve's inside by the arm, not the torso beside it); the jacket as the coverall; the belt and its pouch as the jacket. The boots are stiff (the foot's below the ankle bone, the shin's above, bending over 4 cm, cut along the bend round the posed ankle), the knee pads rigid on the kneecaps (half the thigh's turn, half the shin's) and their straps the shins', the heel the foot's. Above each hip joint the thigh gives way to the pelvis over 5 cm, so a belt rides on the hip bone. The coverall's sleeves are the forearms': the hand turns inside the cuff.
     - **Drawn two-sided** (G1.3e): the garments are single shells, thick only at their edges.
9. **Procedural animation on the client**: a few authored poses blended by gait phase and speed, feet placed by two-bone inverse kinematics on the ground the legs' rays find, and the torso exactly the simulated body. Fallen, the bones are the ragdoll's segments exactly. Nothing extra is sent over the network.
10. **Icons and the style board** (`art-style.md` §9, §11), rendered by our renderer.
11. **Smooth geometry everywhere** (`art-style.md` pillar 9, §7): the sandbox's flat-shaded primitives are replaced, and curved meshes get their levels of detail by screen-space error.
    - **Rounds built in the vertex shader.** The box, the cylinder and the sphere are unit shapes, scaled per instance. A vertex at `p` with normal `n` is drawn at `p·s + (n − 2p)·r`: its corner of the inner shape plus its normal times the bevel radius `r`. Every vertex of a round thus lies on the true round, whatever its normal: the box's edges and corners and the cylinder's rims are rounds of `k` segments a quarter turn, and on the sphere `n = 2p` and nothing moves. So a round keeps its size in metres however a part is scaled: `r` is 8 % of the smallest side, at most 2.5 cm. The shadow pass places vertices the same way.
    - **Levels by half a pixel.** The box comes sharp and rounded by 1 to 8 segments; the cylinder and the sphere in levels of 6 to 128 segments around, the cylinder's rims as round as its sides need (none at 6 and 8, up to 13 at 128). Each instance is drawn with the coarsest level whose silhouette error projects under half a pixel at its nearest point: the sagitta `R(1 − cos(π/n))` of the radius, about twice that on the sphere for its faces' centres, the deepest facet of a rounded corner of the bevel radius, and `(√3 − 1)·r` for the sharp box's corners. A switch moves the silhouette by less than half a pixel, so nothing pops and no geomorphs are needed. Rounds of 8 segments stray 0.12 mm along the edges at 2.5 cm, within `model_check`'s rule for authored models (1 % of the radius or 1 mm).
    - **Shadows at their own level.** In each cascade, an instance is drawn at the coarsest level under half a texel of the map. The coarser caster lies within the drawn shape, so it shades nothing on it.
    - **Highlights filtered by the pixel.** Specular anti-aliasing widens each highlight by the spread of normals within the pixel, and normals are interpolated at the centroid, so a bevel thinner than a pixel neither sparkles nor shows normals extrapolated past its faces'.

## Measured and rejected

### Baking and building the crate (G1.3b, 2026-10-21)

| Tried | Measured | Why rejected, and what replaced it |
|---|---|---|
| Occlusion and convexity baked per vertex | On the crate (788 triangles) the mean occlusion was 0.25 and wear covered 89 % of it: every vertex lies on an edge or under another part, and its value smears across whole faces | Per face corner, each sampled a fifth of the way to its face's centre |
| Corners sampled where another part covers them | Corner plates 6 mm thick over the battens' ends turned 1 m battens black (occlusion 0 at all four corners); a 3 mm "covered" test missed them | A corner covered within 1 cm is sampled further in (a fifth, then a third, then halfway) |
| Per-corner values on large faces, triangulated | Dark diagonals across boards: three corners alike and one different, interpolated over two triangles | One value (the corners' mean) on faces over 25 cm², baked before triangulation; height stored instead of a finished grime value, so it interpolates exactly |
| Curvature wear on smooth edges using vertex normals | Plates' flat faces read fully worn: their vertex normals average in the plates' sides | Wear on bevel faces only; wear on smooth curves waits for the character |
| Python sets of Blender faces and edges | Builds differed run to run (sets iterate in memory order) | Ordered de-duplication; `PYTHONHASHSEED` fixed by `xtask art` |
| Triangulation left to the exporter | One build in five differed, in 30 bytes of indices | A Triangulate modifier with fixed rules, after the bake |
| Byte equality for `art --check` | Still one build in six differed: Blender's normals vary in their last bits between runs, and the exporter shares vertices by them | Geometry compared: every triangle's corners quantised (10 µm, 10⁻⁴, 10⁻³), sorted, equal to one unit. A 2 mm change in the boards' gaps is caught |
| Blender's Wave texture for wood grain | Its band direction and distortion drew zigzags or ran across the boards | Grain lines written out: the crests of a sine across the grain, displaced by slow noise along it (also what the game shader computes) |

### Drawing the surfaces (G1.3c, 2026-10-21)

Measured with `creeer_client --bench 600 --view factory` (10,015 parts, 1080p, an RTX 4070 Super), each run alternated with a build from before the surfaces (2.20 ms p50). The rejected versions are commit `090d95d`'s `mesh.wgsl`, `surface.wgsl` and `scene.rs`.

| Tried | Measured | Why rejected, and what replaced it |
|---|---|---|
| Value noise computed from hashes: eight hashes and seven interpolations a sample | 2.64 ms p50, 20 % over before; the noise on procedural parts alone 0.14 ms | One fetch a sample from a 32³ noise volume, its fraction warped by a quintic (Quilez's texture interpolation). Octaves finer than a pixel are their mean, without a fetch |
| One pipeline for models and procedural meshes, branching per fragment | The full surface's registers and outputs slowed every procedural part, with no model on screen: about 0.12 ms | Two pipelines: procedural meshes with a lean vertex output and mottling only; models with the full surface |
| The instance grown by 16 bytes for its age and wear (80 bytes) | 0.08 ms: each instance is copied for the camera and for every shadow cascade it reaches | Packed into the 4 bytes roughness had: roughness and wear 8 bits each, the years a half float. 64 bytes again |
| Bevels shaded as interpolated at the pixel's centre, without specular anti-aliasing | In the close-up view, a dotted bright line along a beam's top edge: chamfers 9 mm wide, under a pixel there, caught the sun with normals extrapolated past their faces' | Normals interpolated at the centroid, and highlights widened by the pixel's spread of normals: the line is continuous, the highlight a rounded edge has |
| Edge chips as specks (noise of 30 cells a metre, 4 octaves) revealing fresh `wood_cut` at 70 %, 0.2 smoother than the surface | On the one- and four-year crates of the style board: bright yellow dots along every board edge, glinting; they read as glitter, not wear | Patches from noise of 9 cells a metre, 3 octaves; the revealed token's own roughness and metalness; what is revealed fades with the object's age, from 70 % fresh to 15 % (`art-style.md` §6) |
| The box's and the cylinder's edges as one 45° chamfer of the bevel radius, shaded with the faces' normals | A chamfer is the chord of its quarter round: up to 29 % of the radius off it, 7.3 mm at 2.5 cm, several pixels a metre away. By `model_check`'s measure it strays 3.5 mm, over its 1 mm | True rounds of `k` segments in levels of detail, from a sharp box far away to 8 segments (0.12 mm) close by |
| Shadows drawn at the camera's level | Close-up 1.83 ms against 1.66 before the surfaces (+10 %); horizon p99 +14 %: near parts drew 128-segment casters into four cascades | Each cascade at its own level, under half a texel |
| Each pass's levels gathered by sorting the batch once for the camera and once per cascade | Factory 2.44 ms, from 2.07: five passes over 10,000 instances on the CPU, each with a sort | One pass over the instances: each converted once, put in a bucket per pass and level, the buckets appended as one draw each. Factory 2.12 ms |

### The engineer's blockout (G1.3f, 2026-10-21)

Each round was judged by the user on renders (the review page) and measured against the surveys. The first, procedural figure is commit `30a8fd8`; its later head is `docs/adr/0026/procedural-head.patch`.

| Tried | Measured | Why rejected, and what replaced it |
|---|---|---|
| A procedural figure: primitives merged by a voxel remesh, a hood, then a head shaped from a sphere | The hood read as a balaclava; the face, rebuilt, still "looks very cursed"; the chest massive beside a small head (the user) | Blender Studio's realistic base mesh, fitted |
| Garments as plain offsets of the skin (1.2 cm) | Every muscle showed through, like a superhero's suit | Smoothed, then pushed back out of the body where they sank in |
| Plain (Laplacian) smoothing of the garments | A limb's loop of 30 vertices loses about 2 % of its radius a pass; after 150 the shrinkwrap only rebuilt the offset body, relief and all | Taubin's paired passes |
| Taubin's passes alone at the seat and chest (pass band about 30 cm) | The seat and its cleft still showed ("I do not want to see the bum so accentuated"); a 20 cm relief fades by under 10 % in 200 passes | A far stronger pass (pass band near 1 m) over the hips and trunk, arms aside; on the body's chest, plain averaging, its shrinkage undone by the girth fit |
| The strong pass reaching the armpits in the A-pose | It bridged the arms and the trunk into a web; posed, the jacket's back read as a cape | Weighted down by four times the arm's weight |
| The build at all of ANSUR's men's means | "Way too buff, too broad": shoulders 0.534 m across; their mean carries a belly (waist 0.335 m, chest 0.297), and under narrower shoulders the hips read wide | ANSUR's men of normal weight, one group for every measure: shoulders 0.491, waist 0.302 × 0.208 |
| The base mesh's trapezius kept | A lifter's slope: the tape over the neck's base read 0.857 m against 0.422 | The slope lowered by up to 5 cm where neck and shoulder meet |
| The forearms at men's carrying angle (11°) | The hands hung 0.28 m from the middle, held out | Palms to the thighs, the forearms in line with the upper arms: hands at 0.23 m |
| Clothes 3.7 and 4.6 cm off the skin (coverall, jacket), then 2.2 and 3.2 | "Really really thick", then "still too thick" | 1.4 and 2.2 cm |
| A rolled collar, a tube 3.2 cm thick | "This weird sausage like feature" | A stand collar: a band 3.5 cm high leaning in to 8 mm off the neck |
| Knee and elbow pads as ellipsoids | Thinner clothes buried them to crescents. The first conforming patches took the garments' inner shells too (pads on the backs of the knees) and lost their tops | Octagons cut from the limb's own outer surface, chosen by its weights |
| Boots as offsets of the feet | Big leather feet, toes and all | Each foot's convex hull, subdivided |
| Solidify with even thickness | Spikes a metre long where bisected slivers meet | Its thickness clamped |

### The engineer in detail (G1.3f, 2026-10-21)

Each round was judged by the user on renders of the exported model as the game draws it (`art/engineer/render_review.py`). The code of rejected versions is beside this ADR (`docs/adr/0026/`).

| Tried | Measured | Why rejected, and what replaced it |
|---|---|---|
| The blockout's outfit, smooth and in one colour each | "Very boring: no detail, flat, monochrome", and "a very strange surface" (the user): the levels overlapped, and torn shading | Pockets, patches, seams, folds, zip, buckles, straps, laces and soles in the geometry; the crate's surface layers, with the wear and stains of clothing, in the shader |
| Diamond quilting on the coverall, after insulated workwear (`diamond-quilting.py`) | "I hate the diamond shapes" (the user) | Navy canvas with double layers over the knees and the seat (the user's choice) |
| A pale grey-blue coverall (`#8c99a6`) | "Darker and grimier", "a lot less monochrome" (the user) | Navy `#4a5a73` wearing to `#6b7a8e`; stains; each cut piece its own tone (approved) |
| An olive field jacket (`#3b3f2c`) with a buttoned front and flap pockets | "Not like a military uniform" (the user) | A two-tone work jacket after the donkey jacket: the player's colour on its body, navy on its yoke, lower sleeves, cuffs, hem and collar; a zip; plain banded pockets |
| The blockout's boots, each foot's convex hull | "Triangles on flip flops" (the user): low at the toes, a wedge on a flat sole | Each grown round its foot as a last is made (`last.py`) |
| Boots of ellipsoids merged | A clown's shoe: the toe a ball apart from the instep, the big toe through its side | The foot closed into a solid with a toe box and a shaft, grown 11 mm |
| Boots grown as a ball round each of the skin's points (`boots-of-points.py`) | Every toe showed as a lump | The same |
| The soles and the belt as curves converted in pieces | Decimation kept every edge between a curve's caps and sides as open: torn shading | Each merged into one mesh first |
| Panels raised by extrusion, the original faces kept, their steps smooth | Zigzag shading from slivers' normals, smudges along the steps | Normals from before the cut, points within 2 mm of a plane snapped to it, the original faces deleted, the steps shaded hard; a flush panel with a groove where its planes lie oblique (the yoke) |
| The yoke bounded by the arms' weights | A weight's edge follows the faces, and frayed its sides | Bounded by planes alone |
| Cloth following the body in | "Perfectly morphed around the body, no hanging things"; "hang more in between the legs [...] (same is true for the bum)"; the shoulders' "back facing collar bones" (the user) | The sleeves and legs hung nearly straight from their widest point, the seat and crotch spanned, the jacket spanned from the shoulders |
| The legs hung from the hip | A fin of cloth along the crotch | Hung from below the crotch, the crotch spanned across |
| Hair as ridged relief | "Horrible hair" (the user) | A smooth shell combed from the crown, its strands in the surface (the Hair pattern, along its grain) |
| A light beard (`light-beard.py`), then a fuller one | "The beard looks bad, remove it" (the user) | Clean-shaven |
| The close-up level decimated alone | About 1,000 smooth edges past §7's millimetre: faces several centimetres wide bent 20–30° across folds | Refined until every smooth edge keeps it, the new points put back on the full surface |
| Refinement judging each hard edge as hard | A hard edge whose corners have no other keeps one normal there and shades smooth: left straying | Edges judged as shading treats them |
| Buckles and the zip's pull in coarse steps | Their tubes' facets strayed past §7 | The belt's buckle in finer steps; domes at the full resolution |
| The close-up budget at 40,000 | The detail did not fit | 50,000 (the user's choice; one player close up costs 0.02 ms at 40,000) |

### The engineer's fit (G1.3f, 2026-10-22)

Measured with `art/engineer/check_fit.py` on the close-up level, in 37 poses for ten outfits. Patches of the rejected versions are beside this ADR.

| Tried | Measured | Why rejected, and what replaced it |
|---|---|---|
| The garments weighted as the skin they were offset from | Their weights stayed behind where the cloth was spanned, smoothed and hung: a raised arm drew the neckline 12 mm into the neck | Each garment weighted as what lies beneath it |
| Every garment from the skin, the jacket too | Where the jacket and coverall hang apart they found the skin in different places: the coverall 12 mm through the jacket at the hips' limits | The jacket as the coverall: 4.7 mm at most |
| From the skin where a ray in along the normal meets it (`weights-by-ray.patch`) | The hips worse (11.9 mm); 34 of the coverall's points seen at a shoulder's limit | The nearest skin |
| The jacket from the coverall's nearest face (`jacket-weights-by-face.patch`) | The elbow's crook unchanged (10.7–11.0 mm); new pokes at the shoulders (7.3) and the hips (11.4) | Its four nearest points |
| The base layer by the skin on its own limb | 2 mm off the skin, where the skin beneath is another limb's (the groin, the armpit): 9.3 mm short of the body at the hips' limits, 7.0 at the shoulders' | The nearest skin, whatever its limb: 5.8 and 4.2, both beyond a person's range |
| The belt as the skin's pelvis and torso | It stayed while the jacket's hips turned with the thighs, 12 mm through it | As the jacket, its trunk and hips |
| The cloth under the belt as the trunk alone (`belt-held-weights.patch`) | The layers beneath still turned with the thighs and parted from it at the belt line: 11.8 mm | As before |
| The heel as bone heat left it (59 % the foot's) | It turned with the shin and came 1.7 cm out of the boot's heel | The foot's below the ankle bone, over 3 cm |
| The boots as the skin beneath them | The shaft took the instep's weight 2.5 cm above the ankle; its long faces tilted with the foot, 10 mm into the coverall tucked inside | Stiff leather, bending over 4 cm at the ankle, and cut along the bend's edges at every level. Cut and not weighted again, the new loop took its edges' weights in proportion (0.6 the foot's at the bend's top): 12 mm |
| Knee pads as the coverall | They folded with the knee to its limit and came apart over it (11.6 mm) | Rigid on the kneecaps: half the thigh's turn, half the shin's |
| Knee pads pushed out of the coverall point by point | Each tip crumpled over the knee patch's step: facets 2 mm off their curve | Smoothed into a plate and moved out whole |
| The knee strap at the joint line | It crumpled into the fold of the knee closed fully | Round the calf's top, under the knee's pit |
| Straps kept above the cloth's outline | Averaged, the outline's peaks sank (a knee patch's step 3.9 mm through the strap); clamped to it, the strap kinked (facets 1.1 mm off); widened over 45°, it stood off the leg like a hoop | Smoothed round, the cloth gathered under it as a buckled strap gathers it |
| The coverall's sleeves 1.5 cm above the wrists (`sleeves-above-wrist.patch`) | The gloved hand bent fully still met the cuff (11 mm), and the forearm was seen at rest past the cuff's wider mouth | At the wrists |
| The hidden regions to fixed lines: the body's neck to 1.585 m; the base layer hiding the torso, arms and thighs | The throat missing inside the collar's front; the vest's short sleeves and the shorts leave the shoulders, arms and thighs bare | Each region ends inside its garment's opening; the base layer hides the pelvis alone |
| The belt ringed round the jacket before its detail | The jacket's folds and the coverall's ease came 7 mm through it; the pouch, 5 mm off the belt, had the jacket puffing through its back; the keeper's back 4 mm into the jacket | The cloth gathered under the belt; the pouch clear of the cloth behind it; the keeper round the belt alone |
| The coverall's legs tucked to the boots' rims | 5.6 mm through the padded collars | Inside the collars, easing out over them |
| The goggles over the hair as it grew | Locks 3.9 mm through the rims; then a round strap 1 cm off the hair read as a hat's brim | The hair pressed flat under the cups and under a flat strap |
| Holes as a layer over within 4 cm on the same limb; then as the first face met from inside; then both | Loose cloth hangs 8 cm off the shank; a sleeve lowered from the A-pose passes into the garment's torso or rests against the hip; a ray from one armpit crossed to the other's sleeve | A line of sight out, past everything drawn |
| Arms at the shoulder's cone in every direction | Swung across and through the chest, where the ragdoll's arms cannot go (they collide with the trunk) | Stopped where their boxes meet the trunk's |

### The engineer's joints closed fully (G1.3f, 2026-10-22)

At the limits a person reaches, the layers came through each other in the crooks: the elbow 10.9 mm, the knee's strap 9.5, the wrist 11.5, the hip flexed 10.3 (through the belt), the foot pointed 3.0. The user chose edge loops across every joint (2026-10-22), over the 2 mm rule held short of the limits. Measured first, with `check_fit.py` (37 poses, ten outfits) on builds from `0026/joint-experiments.py`.

| Tried | Measured | Why rejected, and what replaced it |
|---|---|---|
| Edge loops across every joint, at their limit: nothing decimated (439,890 triangles close up) | As bad or worse: the coverall 11.9 mm through the jacket in the elbow's crook (102 points), the hip 11.0, the wrist 11.3, the knee's strap 7.7. Only the ankle's 3 mm went | At the elbow closed fully the skinning folds the crook over itself: the turn's change along the arm times the distance from its axis passes 1 at a third of the crook's points, 7.6 at worst (`0026/crook-fold.py`). Density cannot part what the skinning folds. What lies inside a wearable is hidden instead (decision 8) |
| The boots cut along their bend round the fitted body's ankles (as committed in G1.3f) | The feet stand 7 cm further apart before the legs are drawn in: the planes reached a few faces, 230 still spanned the bend, and the coverall came 3 mm through them with the foot pointed | Round the posed ankles: the one place loops were the answer. 650 triangles close up |
| The thighs' weight above the hip joints as bone heat left it | A third of the weight at the belt line, 13 cm above the joint: the hip flexed turned the belt 10.3 mm into the jacket | Given way to the pelvis over 5 cm. Over 4 cm, the left hip came 7.5 mm through the jacket's hem; over 6, 6.8 mm on both sides; over 10, the fold moved up under the hem (11.7 mm) |
| Hiding a face where a ray along its normal met the wearable within 6 cm, 3.5 cm from its open edges | The hands hanging by the thighs, the gloves' fingers by the legs and the flanks under the arms were hidden: 223 of their points seen at rest | Every line of sight to meet the wearable |
| Asking at each face's middle | The corners nearer an opening were seen: 50 of the coverall's at the boots' collars, at rest | At every corner |
| Only where the wearable over a point is on its limb (or its limb's root on the trunk) | The sleeve's inside, resting on the jacket's side, met the torso first, and its faces in the crook stayed drawn | No such rule: enclosed, a hand by the thigh is not |
| Looking out along the point's own normal | A crease's tilted normal looked down the sleeve and out of its mouth: 7 points in the crook stayed drawn, 9.6 mm through the jacket | Along the skin's normal, as the check looks |
| Cones of 45° (the check's), 50° and 70° | 45°: 672 hidden points seen over the poses, the gloves' cuffs at rest; 50°: 361; 70°: too little hidden, the elbow's crook 11.7 mm through again | 60°: 177 |
| Looking out from the point itself | A layer touching the one over it at rest (0.5 mm in the crook) looked out past it | From 3 mm beneath |
| Hidden as the whole garments enclose, at every level | The far level's coarser garments leave gaps between their faces: 1,512 hidden points seen over the poses and outfits | As each level draws them: 1,153 (the committed model's 1,117) |
| The thighs' skin kept drawn inside the shorts | 160 pokes with the hips flexed (11.1 mm), 933 beyond a person's range | Hidden too |
| The knee pads 0.3 the thigh's and 0.7 the shank's | No change: the knee's 9.5 mm was the strap's | As before; the strap the shank's |
| The coverall's cuffs as the skin beneath | The hand bent fully put the glove's cuff 11.5 mm through the sleeve | The forearms' |
| The elbows' patches 4 mm off the jacket | A facet 1.08 mm off its curve; and at 3 mm the jacket no longer came through, once the check took normals from the whole piece | 3 mm, as before |

### Ambient occlusion (G1.3d, 2026-10-22)

How GTAO meets the multisampled forward pass, measured both ways as planned, plus a third; frame time p50 with the high preset (1080p, RTX 4070 Super, three interleaved rounds):

| Option | Factory | Close-up | Why not |
|---|---|---|---|
| **(a) Depth prepass at 4× MSAA, the scene pass testing for equal depth** | 2.71–2.78 ms | 2.03–2.04 ms | Taken |
| (b) Ambient light written to a second multisampled target in the scene pass, occluded when composited (`0026/ao-ambient-target.diff`) | 2.83–2.85 ms | 2.19–2.21 ms | 0.07–0.18 ms slower: the second 4× target costs more than the prepass, and the composite cannot see albedo except through the target's alpha |
| (a′) Depth prepass at one sample, the scene pass unchanged (`0026/ao-single-sample-prepass.diff`) | 2.78 ms | 2.13 ms | Slower than (a): the scene pass still shades every overdrawn pixel |

| Tried | Measured | Why rejected |
|---|---|---|
| XeGTAO's noise (a Hilbert index over 64 × 64 pixels) with its 3×3 denoise, two passes | Grain everywhere in the occlusion, clumped round contact shadows | XeGTAO's noise is built to be averaged over frames by TAA; two 3×3 passes cannot gather it |
| A 4×4 Bayer pattern with the 3×3 denoise, two passes, then three | A visible 4-pixel cross-hatch; three passes softer but still blotchy | A 5×5 tent does not cover the 4×4 tile evenly |
| A 2×2 pattern with two 3×3 passes | Banding and blotches | Four turns of three slices give too few directions |
| An infinite wall as the crease's closed form (half the sky) | GTAO 0.59–0.64 at the crease against 0.52–0.54 | Not GTAO's error: its falloff fades far horizons, so it never sees an unbounded wall. The crease is tested on a block within the radius instead (Validation) |
| The visibility bitmask (Therrien et al. 2023) | Not built: no dark halos behind the factory's rods and pipes | Nothing over-darkened to mend |

### Measured and set aside: colour-blind player colours (G1.3a, 2026-10-21)

The first run of the style's checks found the sixteen player colours of `art-style.md` §3.6 confused under simulated colour-vision deficiencies (Machado et al. 2009, `creeer_art::vision`). For deuteranopes, blue and violet are ΔE₀₀ 1.3 apart, violet and indigo 1.3, and orange and ochre 1.7. For protanopes, violet and indigo are 2.9 apart. In normal vision the closest pair is blue and indigo, at 8.5.

| Option | Result |
|---|---|
| Krzywinski's colour-blind palettes (12 and 15 colours, after Wong 2011) | Far apart for deuteranopes, but neon (luminous magentas and greens): against `art-style.md` pillar 10 |
| A greedy max-min selection (Glasbey et al. 2007) under all three visions, chroma ≤ 50, every colour ΔE₀₀ ≥ 12 from the grounds | Sixteen colours at least 11.1 apart, but chosen without regard to today's colours |
| Today's colours nudged, each to the nearest colour that keeps every pair ≥ ΔE₀₀ 10 under all three visions (orange fixed) | Achieved at 10.0. Changed: blue `#5587af`, yellow `#ebcd5a`, violet `#644ba5`, teal `#289b78` (jade), rose `#dc5569`, lime `#b9f0aa` (mint), sienna `#a55028`, green `#378250`, ochre `#d7af78` (tan) |

The user kept today's colours and set colour blindness aside until the game is published (`art-style.md` changelog). The nudged set is the starting point for a colour-blind mode then (`docs/future-improvements.md`). The check now holds the player colours to ΔE₀₀ 8 in normal vision.

### The other first-run misses, kept by the user

Steel, titanium and concrete against gravel (ΔE₀₀ 10.3, 7.5, 11.5), rubber against basalt (7.8), and the chroma of copper (50.7) and laterite bauxite (55.8). Changes as small as those limits ask were measured (for example steel `#96999c` at 12.2), and the user kept the original colours. They are listed as approved exceptions (`art-style.md` §4, `data/art/exceptions.ron`). The checks report an exception once it is no longer needed.

## Validation

**Done so far (2026-10-21):**
- **Colour (G1.3a).** CIEDE2000 matches all 34 of Sharma, Wu and Dalal's pairs to 10⁻⁴. The palette, player colours, rock colours and approved exceptions match `art-style.md` both ways, and the base pack keeps the style with its exceptions.
- **Models (G1.3b).** The crate reads with 788 triangles in its two tokens, and models broken in each way are refused for their reason.
  - Its exported masks are the baked values unconverted: alpha equals each vertex's height over 2 m, and Blender's occlusion 0.6875 and 0.6458 at two corners read back exactly.
  - Its grain runs along its boards: u's gradient on their faces is (1, 0).
  - `xtask art --check` passes on unchanged scripts and catches a 2 mm change.
- **Drawing (G1.3c).** A snapshot without looks (the planet's) draws the 1 m crate as its model, from its base. Meshes whose vertices name tokens are drawn by the model pipeline, the rest by the plain one. The noise volume is the same on every run, and evenly spread. An instance's packed roughness, wear and years read back exactly, and years beyond 65,000 are capped below infinity.
- **Smooth geometry (G1.3c).**
  - Every level of the rounded box and of the cylinder is closed (each edge used once each way) and convex, its faces outward, at scales from 1 m cubes to 1 cm × 2 m rods. The box has 8k² + 24k + 12 triangles.
  - A round keeps its size in metres: on a 0.1 × 3 × 0.4 m box, the faces are inset by the same 8 mm from both edges, and every rounded vertex lies on its corner's sphere of that radius to 10⁻⁶.
  - Each level's error is measured on its drawn geometry: the cylinders' chords at their sagitta to 10⁻⁵; the spheres' deepest faces within their bound and above 90 % of it; the rounded box's deepest corner facet at the error used for it to 10⁻⁴. The cylinder's rims never stray further than its sides. At 24 segments the facets lie within 1 % of the radius.
  - Each instance gets the coarsest level under half a pixel, and the next coarser exceeds it; a cylinder lying along the view is drawn as finely as its near end needs. A cascade takes the coarsest level under half a texel, never coarser for finer texels.
  - The shaders' round constants and placement match `primitives.rs` (checked against `globals.wgsl`).
- **No shimmer (G1.3c; `creeer_client --shimmer`, and a test wherever there is a GPU).**
  - **The measure:** crates of three ages from 3 to 35 m, pipes, balls and a long ground, rendered while the camera turns a twentieth of a pixel a frame. A pixel flickers when the root mean square of its second differences in time exceeds 2 levels of 8. At that speed a well-filtered image changes almost linearly, while detail popping between pixels jumps.
  - **Edges set apart:** the same motion rendered with every surface one colour (the engineering overlay) finds the edges, where 4× multisampling steps as it crosses its samples. Pixels within 2 of them are left out, and the surfaces are judged on the rest.
  - **The measure tested first,** on rows sliding as the camera turns: broad shading does not flicker; stripes of 0.7 pixels point-sampled flicker everywhere; the same stripes faded to their mean as the shader fades them, and full-contrast stripes of 3 pixels, do not.
  - **Result (1280 × 720, 32 frames):** the surfaces, 89 % of the image, flicker on 0.079 % of their pixels (p99.9 at 1.8 levels); the gate is 0.2 %. The edges, rendered in one colour, flicker on 1.64 % of the image.
  - **The check catches shimmer:** with the noise's and the grain lines' footprint fade switched off, the surfaces flicker on 3.48 %, 44 times as much, the distant ground and the crate faces most.
  - Left for later: the edges' crawl itself (an antialiasing matter; recorded in `docs/future-improvements.md`).
- **The shader against the preview (G1.3c).** The crate at 0, 1 and 4 years, drawn by the game (`creeer_client --style-board`) and by Blender with the preview's nodes (`cargo xtask art --board`), same camera and sun. The layers sit in the same places and age alike: each board's tone and grain lines, the one-year crate's grey patches and stains, the old crate darkened and streaked, worn edges in patches, the painted corner plates. The patterns differ in detail (Blender's noise is not ours). Blender's colours are paler and cooler (its AgX tone mapping desaturates; its sky light is a uniform blue): a difference of lighting, not of surface.
  - Found on the way: the preview read its "age" as one number from 0 to 1 for everything, while the game reads years, with grime over two of them and each token weathering at its own rate. The preview now reads years too.
- **Ageing (G1.3c).**
  - A year of exposure per game day in air, and three in the sea.
  - A 200 kg crate dropped from 3 m is worn 0.091, which is 4.5 kJ beyond the coating's 10 J (its bottom fell about 2.3 m). One placed gently is worn 0.
  - Ages survive a save exactly (version 9, its fixture), and a client is told every crate's age and its wear when it changes (protocol 7).
  - Ageing changes no motion. The stress scene `server` and the play scene `crowd` hash the whole save, so their goldens changed with version 9's ages section (and CI's stress job failed from 090d95d until they were renewed). Their physics alone, the save's world, hashes the same before ageing (e5a02a1) and after, at both lengths: `server` 2453dd84… (120 ticks) and 8c52bd7b… (1,200), `crowd` 2beb8e52… (300) and 0bf54b69… (1,800).

- **The engineer (G1.3f).** `model_check` passes the model: each level within its budget, each piece's and the character's worn whole; at most four bones a vertex, its weights summing to one; its bones at the ragdoll's joints; its close-up facets within §7's millimetre; every wearable a piece of it. `xtask art --check` rebuilds it identically.
  - **The fit** (`art/engineer/check_fit.py`): 37 poses (at rest, and each of the ragdoll's joint limits alone, an arm stopped where its box meets the trunk's), for the base body, the full outfit and each wearable alone over the base body, skinned with dual quaternions.
    - **At rest:** nothing pokes through by more than 2 mm and no hidden point is seen, at the close-up level. At the mid level, one hidden point in each of three outfits; at the far level, the seam between the boots and their soles lets 7–9 hidden points of the feet be seen. Each coarser level is held to its own error against the close-up one (mid 0.9–7.4 mm, far 2.4–36 mm by piece), under half a pixel wherever it is drawn.
    - **At the limits a person reaches** (2026-10-22, with what lies inside a wearable hidden): nothing pokes through at the close-up level, against 67 points before (11.5 mm at worst). The points seen are those seen before (the coverall's neck with the shoulder raised forward, the base layer's crotch with the hips flexed, a foot in the boots alone with the knee or ankle closed), and two more with the hips flexed 80°: a point of the buttock inside the shorts, one of the pelvis under the jacket's hem. At the mid level 34 points poke through, against 218 (the elbow's patches 5.0 mm; one of the coverall's in the left elbow's crook, 10.7); at the far level 16, against 90.
    - **Beyond a person's range** (the ragdoll's cones are the same every way): 53 points poke through, against 738 (the hips opened or closed 80°, the body 7.6 mm through the shorts and the coverall 5.2 mm through the jacket); the ankle bent up 34° shows 3–6 hidden points of the foot.
    - **Found on the way:** the check had read `data/wearables.ron` as hiding nothing, so no hole had been looked for. With it read, the throat was missing inside the collar's front, and the base layer hid what its sleeves and legs leave bare. Later (2026-10-22): the boots' bend was cut round the wrong ankles (Measured and rejected), and the check took each part's own normals, which lean off the piece's at a part's edge (now the whole piece's).
  - **Within its segments** (at rest): the body's thighs 5.0–5.4 % outside their boxes grown by 3 cm and its upper arms 2.8–3.4 %, the shoulders' and hips' muscles, as the blockout's. The outfit's misses (5 cm) are what stands off the limbs: the knee pads 13–16 % of their points, the elbow patches 41–42 %, the coverall's legs 9–10 % at the shanks, the belt and its pouch 11 % at the pelvis (Limits).

- **Ambient occlusion (G1.3d, 2026-10-22;** `tests/gpu_ao.rs`, `tests/gpu_ao_traced.rs`, wherever there is a GPU).
  - **The port against closed forms,** with the reference preset (16 slices of 16 steps, full-resolution depth, power 1, no filter), its value read back and compared at the floor's pixels found by casting each pixel's ray on the CPU:
    - a sphere of 0.5 m resting on a plane, against its form factor R³/(d² + R²)^{3/2}: within 0.011 at every ring from 0.6 to 1.5 m (0.803 against 0.804 at 0.6–0.8 m). The high preset at power 1 reads 0.91 there: GTAO's few steps miss part of the horizon, which XeGTAO's final power fits (0.91^2.2 = 0.81);
    - the floor at the foot of a block 0.6 × 0.3 m (the crease), against its visibility by quadrature over the hemisphere: within 0.026 from 1 cm to 25 cm (0.545 against 0.538 at 1–3 cm). The block lies within the radius: an unbounded wall would be faded by GTAO's falloff (Measured and rejected).
  - **Against a ray-traced reference of the style board's crate,** traced on the CPU as XeGTAO's references are (rays of the 1 m radius, 256 a pixel, the share that leave it unmet), every third pixel of the crate and the ground about it (4,924 pixels). Mean absolute error, and the mean signed (positive: too open):

    | Preset | All | On the crate |
    |---|---|---|
    | high (shipped) | 0.033 (+0.021) | 0.054 |
    | medium | 0.042 (+0.038) | 0.074 |
    | low | 0.055 (+0.055) | 0.087 |
    | high at power 1 | 0.052 (+0.051) | 0.106 |
    | reference at power 1 | 0.036 (+0.033) | 0.079 |

    XeGTAO's final power earns its place: without it the high preset's error rises by half. The test holds the high preset under 0.05.
  - **No flicker** (`creeer_client --shimmer`, 1280 × 720, 32 frames): the shaded surfaces flicker on 0.079 % of their pixels, as before occlusion (gate 0.2 %). The occlusion map alone, in 8-bit levels of occlusion, on 0.37 % (p99 1.06 levels, near its own rounding); the test holds it under 1 %.
  - **Thin occluders:** no dark halos behind the factory's rods and pipes in the occlusion map, so the visibility bitmask was not built.

**Still to come:**
- colour-vision simulation against Machado et al.'s published examples (set aside with the colour-blind mode);
- dual-quaternion skinning exactly rigid for single-bone vertices.

## Cost

Budgets in `docs/g1-plan.md`. Ambient occlusion ≤ 1 ms at 1080p on min-spec (a GTX 1660-class GPU), measured with GPU timestamps on the development machine and projected. XeGTAO's "High" costs 0.56 ms on an RTX 2060, a GPU somewhat faster than min-spec.

**The surfaces (G1.3c), measured:** within the plan's 10 % in every view, at p50 and at p99. Frame time p50, before → with the surfaces (1080p, RTX 4070 Super):

| View | Before | With | Change |
|---|---|---|---|
| factory (10,015 parts) | 2.19 ms | 2.29 ms | +4.6 % |
| closeup | 1.67 ms | 1.77 ms | +6.0 % |
| overview | 1.85 ms | 1.94 ms | +4.9 % |
| horizon | 1.47 ms | 1.55 ms | +5.4 % |
| physics | 1.63 ms | 1.70 ms | +4.3 % |

Of the factory's 0.10 ms, the mottling on procedural parts takes about 0.08 ms and the larger vertex (40 bytes, from 28) about 0.02 ms.

**Smooth geometry (G1.3c), measured** with the surfaces, against the same build from before them. Far parts drop to sharp boxes and a few segments, and shadows to their texels' needs, so the look as a whole now costs less than the flat-shaded sandbox did in most views:

| View | Before the surfaces | With surfaces and smooth geometry | Change |
|---|---|---|---|
| factory | 2.20 ms | 2.12 ms | −3.6 % |
| closeup | 1.67 ms | 1.71 ms | +2.4 % |
| overview | 1.86 ms | 1.75 ms | −5.9 % |
| horizon | 1.47 ms | 1.44 ms | −2.0 % |
| physics | 1.62 ms | 1.58 ms | −2.5 % |

p99 likewise: the close-up 2.00 ms against 1.91, the rest at or below before.

**The engineer's levels (G1.3f), measured before modelling** (`creeer_client --bench 600 --view <v> --players <glb> <n>`): the approved blockout's outfit decimated to each level's triangle count, drawn where players stand, in the main pass and every shadow cascade. Frame time p50 (1080p, RTX 4070 Super):

| Players | Factory view, 16 players | Close-up view, 1 player 3 m away |
|---|---|---|
| None | 2.13 ms | 1.74 ms |
| 3,000 triangles each | 2.23 ms (+0.006 ms a player) | 1.75 ms |
| 15,000 | 2.38 ms (+0.016) | 1.75 ms |
| 40,000 | 2.72 ms (+0.037) | 1.76 ms |
| 80,000 | 3.05 ms (+0.057) | 1.78 ms |

The cost follows the triangles, shadows included; one player close to the camera costs 0.02 ms at 40,000 and 0.04 ms at 80,000. Close-up detail is cheap, as one or two players are ever that close; a crowd is what the far levels are for. **The levels: close-up 40,000, mid 15,000, far 3,000.** Eight players, one close up and seven at the mid level, add about 0.13 ms (0.26 ms projected to min-spec); 80,000 close up would still fit, if the close-up level's facets show at the nearest camera.

**The engineer as modelled (G1.3f).** The close-up level was raised to 50,000 for the detail (the user, 2026-10-21; at 40,000 a close player measured 0.02 ms). Worn whole, the model draws 48,790, 14,276 and 2,941 triangles at its three levels; the base body in its base layer 31,500, 10,877 and 2,477 (2026-10-22: what lies inside the worn wearables hidden, 1,700 triangles close up, and the boots' bend cut all round, 650).

**Ambient occlusion (G1.3d), measured** with GPU timestamps and frame times (`creeer_client --bench 300 --view <v> --ao <preset>`; 1080p, RTX 4070 Super; two interleaved rounds against the build before it, `91caa3d`). Frame time p50:

| View | Before | Off | Low | Medium | High |
|---|---|---|---|---|---|
| factory | 2.13–2.22 ms | 2.26–2.34 ms | 2.46 ms | 2.66 ms | 2.68–2.73 ms |
| closeup | 1.72 ms | 1.73–1.74 ms | 1.85 ms | 2.01–2.02 ms | 2.06–2.07 ms |
| overview | 1.76–1.77 ms | 1.83–1.91 ms | 2.04–2.05 ms | 2.20 ms | 2.24 ms |
| horizon | 1.46 ms | 1.49 ms | 1.61 ms | 1.77–1.79 ms | 1.79–1.80 ms |
| physics | 1.59 ms | 1.63–1.64 ms | 1.75–1.76 ms | 1.86–1.87 ms | 1.88–1.89 ms |

Off draws no prepass; what remains of its few hundredths is the timestamps and the occlusion's constants. GPU time of the occlusion itself (the factory, high): depth MIPs 0.029 ms, the main pass 0.147, the filter about 0.04; medium's main pass 0.096, low's 0.020. The prepass takes 0.04–0.14 ms and saves the scene pass 0.02–0.06 ms by shading each pixel once.

**Projected to min-spec** (a GTX 1660 class GPU): ×4, a margin over PassMark's G3D and memory bandwidth, both about 2.6× between the two cards (nominal FP32 is 7×, inflated by Ada's dual issue). The occlusion's compute: high about 0.9 ms, medium 0.6 ms, low 0.3 ms, each within the 1 ms budget; with the prepass and its barriers, a frame grows by 0.33–0.53 ms here at high (1.3–2.1 ms projected), 0.3–0.45 ms at medium. Min-spec is meant to run medium. XeGTAO's own high costs 0.56 ms on an RTX 2060.

## Limits

To be written as the work finds them; each also in `docs/future-improvements.md`.

- **Ambient occlusion without temporal anti-aliasing.** Each pixel's 18 samples (high) are gathered from its 4×4 tile by the filter, so occlusion under thin, close occluders shows a faint 4-pixel texture, and within about 4 pixels of a crease the filter's window (two pixels one way, one the other) reads up to 0.04 too open. TAA would let XeGTAO's own noise average over frames (`docs/future-improvements.md`).
- **Normals from depth.** GTAO takes each pixel's normal from the depth, so it sees a cylinder's true facets where the shading sees smooth normals: faint vertical bands in its occlusion on curved parts. A normal written by the prepass would mend it, at a colour target's cost.
- **Screen space.** What the camera does not see does not occlude: an overhang's underside, the far side of a part. GTAO assumes each surface continues behind itself as a heightfield, so it over-occludes behind thin objects (not seen in the sandbox) and its occlusion fades out at the radius (1 m).

- **Rounds are drawn, not simulated.** A box's collision shape keeps its sharp edges, so its drawn corners lie up to (√3 − 1) times the bevel radius (at most 1.8 cm) inside the box that collides.
- **One rule for every part's rounds:** 8 % of the smallest side, at most 2.5 cm. A part cannot ask for sharper or softer edges until the instance carries its own radius.
- **Closer than about 0.2 m,** the finest box (8 segments) may show its facets by more than half a pixel.
- **The engineer's blockout is not a model.** The whole skin stays under the clothes, garments are open shells, the pads flat octagons, the hair a cap, the gloves follow the fingers. Its fit misses two targets: the thighs are 4.5 % under their girth (fitted band by band, the blend into the hips takes some back), and the tape over the neck's base still reads 0.55 m against 0.422, as its plane still crosses the shoulders' tops at the 5 cm cap.
- **The engineer's joints closed fully.** Skinning cannot part what a closing joint presses together; the layer beneath is hidden where a wearable encloses it, so it does not show. The skin and the lowest layer still fold in the crook, and with the hips flexed 80° two hidden points are seen (the buttock inside the shorts, the pelvis under the jacket's hem). At the mid level, the elbows' patches 5 mm and one point of the coverall in the left elbow's crook 10.7 mm through what is over them. Corrective shapes or joints would part them (`docs/future-improvements.md`).
- **What lies inside a wearable is that wearable's alone.** A part `in.<wearable>` is hidden while that wearable is worn, whatever else is: another wearable's list of regions does not reach it. So the skin under the vest shows only if the vest is off and the coverall on, which no outfit wears yet.
- **The ragdoll's cones go beyond a person's range** (the shoulder 92° and the hip 80° every way, the ankle 34° up): there the clothes fit worse (the body 7.6 mm through the shorts with the hip opened 80°), and the body itself takes poses no one takes.
- **The segment boxes against the outfit.** The boxes keep each limb's cross-section and mass, centred on its centre of mass, so the knee pads' fronts stand 12 cm ahead of the shin box's front face and the elbow patches 8 cm behind the forearm box's back: a ragdoll fallen on its knees sinks the pads into the ground. Likewise, by less, the coverall's legs and the belt's pouch (Validation). A box grown about its centre would stand as far out on the other side; reaching the pads needs a box set off its body's centre of mass, a physics feature deferred to G1.8 (the user, 2026-10-22).
- **The garments are single shells**, thick only at their edges: G1.3e draws the engineer two-sided, or a look into a collar or a sleeve sees through its far side.
- **The far level's soles** are decimated apart from the boots, and the seam between them opens up to the level's error: under half a pixel where it is drawn.
- **The fit is checked one joint at a time.** Two joints at their limits together (a shoulder raised while the spine bends) are not posed.
- **The hair is a shell** with the strands in its surface: its outline is smooth, with no strands against the sky.
- **The sandbox's convex hulls** (rocks, a wedge, a prism) are flat-shaded with sharp edges. They are drawn at their own size, so the scaling trick does not apply: rounding them needs each hull's inner hull, its faces moved in by the radius.
