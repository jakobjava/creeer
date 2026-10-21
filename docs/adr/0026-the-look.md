# ADR-0026: The look: models, materials, ambient occlusion and the engineer (G1.3)

**Status:** in progress (2026-10-21). The user approved the plan (`docs/g1-plan.md`, G1.3) and chose the jetpack on a harness, the skinned body, and a hood and goggles with the face open. The user then added the base body and wearables (decision 6).
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
| Curvature-based wear: Blender's Pointiness and Dirty Vertex Colors; Substance's curvature masks; Unigine's screen-space curvature | Convex edges chip to the bare material and concave areas gather grime; the masks broken up by noise. Screen-space curvature aliases at distance (reported) | Taken: convexity and occlusion baked per vertex at export, broken up by triplanar noise. Screen-space curvature not taken |
| Blender MCP practice (agent-skills notes on `blender-mcp`) | Exports through the MCP time out; run them in headless Blender | Taken: the MCP for live work, exports by `blender --background` from committed scripts (`docs/blender-mcp.md`) |
| glTF 2.0 (Khronos); the `gltf` crate 1.4.1 | The standard format, with skins, animations and vertex colours; custom attributes need the crate's `extras` feature | Taken; the baked masks go in `COLOR_0`, so no custom attributes are needed |
| World of Warcraft's geosets ([warcraft.wiki.gg](https://warcraft.wiki.gg/wiki/Character_model)) | The player model is divided into pieces of geometry that equipment toggles or swaps | Taken: the base body in regions that worn things hide |
| Bethesda's biped slots (Skyrim; [UESP](https://ck.uesp.net/wiki/Skyrim_bodyparts_number)) | Each armour piece fills numbered body slots; its mesh is partitioned by body part, often carrying the body beneath it | Taken: wearables fill slots. Not taken: carrying a copy of the body in each piece (a body per piece to keep in step) |
| inZOI's masking system ([ModKit](https://mod-docs.playinzoi.com/ModKit/project/CAZ/Guide/03.%20Masking%20System.html)) | Each outfit names the lower layers' regions it covers and the body regions it hides, "to prevent body parts from poking through shirts, pants, shoes" | Taken: each wearable lists the regions it hides, of the body and of the layers beneath |
| Modular characters in practice (Unreal's leader pose; [gamedev.net](https://gamedev.net/forums/topic/710077-how-to-implement-a-modular-clothing-system); [polycount](https://polycount.com/discussion/comment/2636650)) | All parts share one skeleton, animated once; covered meshes are not drawn. Practitioners warn that skin hidden only partly will still clip through clothes in some poses, and prefer replacing whole segments | Taken: one skeleton animated once; whole regions hidden, never a part of one; poke-through measured in extreme poses |

## Decision (proposed)

1. **Models from scripts.** Each authored model is built by a Blender script in `art/`, deterministic, run live through the MCP while it is made and headless (`cargo xtask art`) for export. The `.glb` files go in `data/models/` with a manifest (`models.ron`: each model's class, triangle budget, size and origin), and `.blend` files are not committed (`docs/blender-mcp.md`).
2. **Materials are palette tokens.** `data/art/palette.ron` holds every token of `art-style.md`, tested against the file's tables.
   - Each mesh vertex carries its token's index, its baked occlusion and its convexity.
   - Each instance carries the player's accent, how worn it is, and the overlay value.
   - A token can name the material that wear reveals (painted steel wears to `steel`).
   - Procedural meshes (members, the sandbox's bodies) keep a per-instance material, as now.
3. **Surface detail on meshes**: triplanar noise (±12 %, 5 cm), edge wear from the baked convexity, grime from the baked occlusion, as `art-style.md` §6 says.
4. **Ambient occlusion: GTAO** after XeGTAO, applied to ambient light only (the sun has its shadows), with the multi-bounce fit.
   - How it fits the MSAA forward pass is decided by measuring both options: (a) a depth and normal prepass, as Bevy does, or (b) ambient light written to its own target in the main pass and occluded when composited.
   - Quality presets, with half resolution on low.
5. **The engineer is skinned on the ragdoll's bones.** Fifteen bones, one per segment, so the drawn body is the simulated one bone for bone, smooth across its joints.
   - Dual quaternion skinning, in one compute pass per frame.
   - A ragdoll's segments say whose they are and which segment they are (protocol 7), so the client can draw the person.
   - This replaces `docs/g1-plan.md`'s first decision to confirm (a segmented suit). Smooth surfaces are now a rule (`art-style.md` pillar 9), and rigid segments would show seams and gaps at every bent joint.
6. **A base body and wearables** (the user, 2026-10-21: "in future I want to have many wearables, so there should [be] a 'naked' version that can wear more and more cool stuff as you progress").
   - **One skeleton, animated once.** The base body and every wearable are skinned meshes on the same fifteen bones.
   - **Regions.** The base body and every wearable are divided into regions (head, face, hair, neck, torso, upper arms, forearms, hands, pelvis, thighs, shanks, feet). A wearable fills slots (head, eyes, face, torso inner and outer, hands, legs, feet, back) and lists the regions it hides, of the body and of the layers beneath (geosets, biped slots, inZOI's covering regions).
   - **Drawn as one.** When an outfit changes, the visible regions of everything worn are merged into one index list, skinned and drawn as one mesh.
   - **The starting outfit is made of wearables** (`art-style.md` §8), so the same system carries every later piece.
   - **Gameplay comes later.** Wearables become items in G1.6 (each worn piece's mass on its segments), and their insulation and protection count from G9.
7. **The engineer's look**, as `art-style.md` §8 says: the base body with an open face and the player's skin tone, in the starting outfit. A blockout from the ragdoll's proportions comes first, judged by its silhouette and approved by the user before the detailed models.
8. **Procedural animation on the client**: a few authored poses blended by gait phase and speed, feet placed by two-bone inverse kinematics on the ground the legs' rays find, and the torso exactly the simulated body. Fallen, the bones are the ragdoll's segments exactly. Nothing extra is sent over the network.
9. **Icons and the style board** (`art-style.md` §9, §11), rendered by our renderer.
10. **Smooth geometry everywhere** (`art-style.md` pillar 9, §7): the sandbox's flat-shaded primitives are replaced, and curved meshes get their levels of detail by screen-space error.

## Measured and rejected

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

Planned per sub-milestone in `docs/g1-plan.md` (G1.3a–i). Among them:
- CIEDE2000 against Sharma, Wu & Dalal's test pairs;
- colour-vision simulation against Machado et al. (2009);
- GTAO against analytic occlusion (a sphere on a plane, a 90° crease) and a ray-traced reference;
- dual-quaternion skinning exactly rigid for single-bone vertices;
- the validator refusing a set of broken models;
- the engineer's skin within its ragdoll segments' boxes;
- wearables: nothing beneath pokes through an outer layer by more than 2 mm in the rest pose and in poses at the ragdoll's joint limits, and every hidden region lies inside what hides it.

## Cost

Budgets in `docs/g1-plan.md`. Ambient occlusion ≤ 1 ms at 1080p on min-spec (a GTX 1660-class GPU), measured with GPU timestamps on the development machine and projected. XeGTAO's "High" costs 0.56 ms on an RTX 2060, a GPU somewhat faster than min-spec.

## Limits

To be written as the work finds them; each also in `docs/future-improvements.md`.
