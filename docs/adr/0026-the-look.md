# ADR-0026: The look: models, materials, ambient occlusion and the engineer (G1.3)

**Status:** in progress (2026-10-21). The user approved the plan (`docs/g1-plan.md`, G1.3) and chose the jetpack on a harness, the skinned body, and a hood and goggles with the face open. The user then added the base body and wearables (decision 7), a Factorio-like weathered look (decision 3) and ageing (decision 4).
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
| Visibility bitmask: Therrien et al., "Screen Space Indirect Lighting with Visibility Bitmask" ([2023](https://arxiv.org/abs/2301.11376)) | Replaces GTAO's two horizon angles by a bitmask, so light passes behind thin occluders instead of over-occluding | The variant to measure if thin geometry (members, branches) is over-darkened |
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
| 9-slice scaling ([Wikipedia](https://en.wikipedia.org/wiki/9-slice_scaling)); "Cheap round-edged box" ([three.js forum](https://discourse.threejs.org/t/cheap-round-edged-box-vertex-shader/8066)) | Corners keep their size while the rest stretches. In 3D, a vertex shader places each vertex at its inner box's corner (from its signs) plus its normal times the radius, and it works with instancing; a guard is needed when the box is smaller than the radius | Taken: primitives bevelled in the vertex shader, so a bevel keeps its size in metres under any instance scale; the radius limited by the smallest side |
| Face-weighted normals on hard surfaces (Polycount practice, [thread](https://polycount.com/discussion/comment/2577165)) | A one-segment chamfer whose sides carry the faces' own normals shades like a rounded edge, and can stand in for a baked normal map on clean hard edges | Taken for authored models' small bevels. Not for the primitives: their 2.5 cm chamfer strayed beyond `model_check`'s smoothness rule (see "Measured and rejected"), so they get true rounds, the faces' normals at their sides |
| Hoppe, "View-Dependent Refinement of Progressive Meshes" ([SIGGRAPH 1997](https://hhoppe.com/proj/vdrpm/)) | Refines a mesh where its geometric error, projected to the screen, exceeds a tolerance; geomorphs hide the pops | Taken: levels chosen by their projected error. Not needed: geomorphs, as the error stays under half a pixel |
| Nanite (Karis, Stubbe and Wihlidal, SIGGRAPH 2021), as described by secondary sources | Clusters chosen so their error projects to about a pixel; for shadows, a texel of the map drawn to. A metric too strict costs time, one too loose pops | Taken: the error bound in pixels. Half a pixel here, since only silhouettes show it |
| Cesium 3D Tiles ([CesiumJS](https://cesium.com/learn/ion-sdk/ref-doc/Cesium3DTileset.html)) | Each tile's geometric error projected to the screen; refined above a maximum (16 pixels by default) | The same measure; its tolerance suits streamed terrain, not silhouettes |
| Geometric specular anti-aliasing: Kaplanyan et al., "Filtering Distributions of Normals for Shading Antialiasing" (HPG 2016); Tokuyoshi and Kaplanyan, "Improved Geometric Specular Antialiasing" (I3D 2019); as Unity's HDRP ([`CommonMaterial.hlsl`](https://github.com/Unity-Technologies/Graphics/blob/master/Packages/com.unity.render-pipelines.core/ShaderLibrary/CommonMaterial.hlsl)) and Filament implement it | The normal's change across a pixel, from screen derivatives, widens the specular lobe: α² grows by twice the variance, capped. HDRP: the variance "should be at most 0.5² = 0.25", a Gaussian pixel of σ = ½ pixel | Taken, at that variance, for every mesh; both pipelines |
| Centroid interpolation (Direct3D, Vulkan and WGSL's `@interpolate(…, centroid)`) | With multisampling, a pixel only partly covered by a triangle is shaded at its centre, often outside the triangle, where attributes are extrapolated; centroid sampling evaluates them within the covered samples | Taken for normals: a bevel thinner than a pixel was shaded with normals extrapolated past its faces' |

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
5. **Ambient occlusion: GTAO** after XeGTAO, applied to ambient light only (the sun has its shadows), with the multi-bounce fit.
   - How it fits the MSAA forward pass is decided by measuring both options: (a) a depth and normal prepass, as Bevy does, or (b) ambient light written to its own target in the main pass and occluded when composited.
   - Quality presets, with half resolution on low.
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
8. **The engineer's look**, as `art-style.md` §8 says: the base body with an open face and the player's skin tone, in the starting outfit. A blockout from the ragdoll's proportions comes first, judged by its silhouette and approved by the user before the detailed models.
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

**Still to come:**
- colour-vision simulation against Machado et al.'s published examples (set aside with the colour-blind mode);
- GTAO against analytic occlusion (a sphere on a plane, a 90° crease) and a ray-traced reference;
- dual-quaternion skinning exactly rigid for single-bone vertices;
- the engineer's skin within its ragdoll segments' boxes;
- wearables: nothing beneath pokes through an outer layer by more than 2 mm in the rest pose and in poses at the ragdoll's joint limits, and every hidden region lies inside what hides it.

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

## Limits

To be written as the work finds them; each also in `docs/future-improvements.md`.

- **Rounds are drawn, not simulated.** A box's collision shape keeps its sharp edges, so its drawn corners lie up to (√3 − 1) times the bevel radius (at most 1.8 cm) inside the box that collides.
- **One rule for every part's rounds:** 8 % of the smallest side, at most 2.5 cm. A part cannot ask for sharper or softer edges until the instance carries its own radius.
- **Closer than about 0.2 m,** the finest box (8 segments) may show its facets by more than half a pixel.
- **The sandbox's convex hulls** (rocks, a wedge, a prism) are flat-shaded with sharp edges. They are drawn at their own size, so the scaling trick does not apply: rounding them needs each hull's inner hull, its faces moved in by the radius.
