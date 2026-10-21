# Creëer art style

**Factorio's vibe, in 3D.** This file is the authority on how Creëer looks. Every asset, shader, effect and UI element is made to it. It changes only with the user's approval, recorded in the changelog at the end. plan.md's pillar: *"Factorio-like. Stylised, readable, softly textured, strong colours per material. Real, smooth shapes: never blocky, voxel-like or low-poly. Grounded and industrial, never futuristic. Stylised in its surfaces and palette, not photographic."*

Status: written 2026-10-18, before G1. Its palette and checks are implemented in G1.3 (`docs/g1-plan.md`, ADR-0026); until then the rules apply to whatever is made. Models are made in Blender (`docs/blender-mcp.md`).

**In short, what Creëer is not:**
- **Not blocky or voxel-like.** No Minecraft geometry: no cubes as the world's unit, no stair-stepped ground, no blocky people. Surfaces are smooth and shapes are real (§1, pillar 9).
- **Not futuristic.** No science-fiction look: no glowing panels, holograms, neon or sleek space suits. It looks like the workshops, mines and machines of the last hundred years (§1, pillar 10).
- **Not photographic.** No photo textures. The detail is soft and made by procedure (§6).

## 1. The pillars

1. **Readable first.** At a glance from the play camera you can tell *what* a thing is and *what it is made of*. Readability wins over detail every time.
2. **The material decides the colour.** Granite looks like granite everywhere, and steel like steel. Colours come from the palette (§3), never chosen per asset.
3. **Grounded and earthy.** The terrain is muted and earthy. Objects stand out from it by contrast (§4), not by glowing or outlines.
4. **Soft detail, never photographic.** Surfaces get procedural noise, edge wear and grime. Photo textures are never used.
5. **Weight and contact.** Strong sun shadows and ambient occlusion sit everything firmly on the ground; nothing floats visually.
6. **Chunky, mechanical, believable.** Shapes look like what they are and how they were made: bevels, bolts, welds, rivets, handles. They are slightly chunky in their small details, never cartoonish in their proportions.
7. **Dense and lived-in.** The ground between things is never empty: pebbles, grass, bushes and fallen branches are everywhere.
8. **Real where physics speaks.** Hot metal glows at its real temperature, and water, air and sunlight are the simulation's own (ADR-0003). Stylisation lives in surfaces, palette and readable shapes, never in contradicting the physics.
9. **Smooth, real shapes; never blocky.** Creëer is not a blocky or voxel game, and nothing in it looks like Minecraft. Smooth geometry is where its realism lives.
   - **The ground** is a smooth surface. Its voxels are only how it is stored; it is meshed with Surface Nets and shaded with exact, smooth normals (ADR-0006). A dug hole is a smooth crater, never a cube taken out.
   - **Curved things are round** at every distance (the rules in §7): a pipe, a wheel, a trunk, a person's arm.
   - **Organic shapes are modelled as they are:** people, trees and rocks. There is no faceted "low-poly" look, and no cube heads or limbs.
   - **Building is free-form** (plan.md): parts have their real profiles and lengths. There is no block grid that shapes the world.
10. **Grounded, never futuristic.** The technology is what the player builds from the ground up, and it looks like the workshops, mines, mills and machines of the last hundred years: steel, timber, rivets, welds, paint, rust and grease. There are no glowing panels, holograms, neon, sleek glossy plastics or space-suit chrome. The engineer is a worker in workwear, not an astronaut. Even rockets (G8) look like real hardware bolted together from real parts.

## 1b. References before making

Before a kind of asset, shader, effect or UI is made, study how others did it: Factorio's own art and its developers' write-ups (the FFF blog), and stylised 3D games with readable, material-led looks. Note what was taken from where (in the asset's ADR or this file's changelog). The style is ours; the craft is learned from those who did it first (plan.md §1, principle 12).

What G1.3's survey took from Factorio (ADR-0026, Prior work):
- **Its art is made in Blender** (FFF-146, FFF-218), and so is Creëer's (`docs/blender-mcp.md`).
- **Hand-painted contrast.** Factorio's renders are finished by hand-painted masks "to enforce contrast, make edges clearer, define shape of entities better" (FFF-146). In real time, Creëer gets that from ambient occlusion, edge wear and the contrast rules of §4.
- **The player's colour** is a tint on a desaturated region of the engineer, at half strength (FFF-218). Here it is the `player_accent` material on the jacket (§3.3, §8).
- **Restraint in saturation.** The terrain is a background that should give "a good and comfortable contrast with the entities and the icons"; players warned that too much saturation turns it into a "plastic playground" (FFF-320). This is what §4's limits are for.

## 2. The physical shape comes first

The drawn shape matches the simulated shape: a member drawn 20 cm thick is 20 cm thick in the solver. Only details smaller than **2 cm** may be exaggerated for readability: bolt heads, handles, rivets, weld beads, up to 1.5× their real size. Proportions of bodies, members and parts are real.

## 3. The palette

Each token has a base colour (sRGB), a roughness and a metalness. Assets name tokens, never raw colours. In G1.3 the tokens move into data (`data/art/palette.ron`), and a test reads the tables of this file and fails if the two differ. Changing a colour means changing it here first.

### 3.1 Terrain

The rocks' colours live in `data/geology/rocks.ron` and are part of this palette: soil `#6b4f35`, gravel `#8c8174`, granite `#c4a194`, basalt `#3d3e44`, limestone `#c9c3b1`, and the rest. Ores are deliberately a little more saturated than their host rocks, so a prospector's eye can find them (porphyry copper ore `#c0773f`, banded iron `#80352b`, laterite bauxite `#b8552d`). All terrain is dielectric, with roughness 0.85–0.95.

### 3.2 Materials

The surface columns (§6): **Pattern** is what the renderer draws within the colour; **Wears to** what chipped edges reveal; **Ages to** what age turns it into; **Weathers in** the years that takes outdoors in temperate air (§6b).

| Token | Colour | Roughness | Metalness | Pattern | Wears to | Ages to | Weathers in | Notes |
|---|---|---|---|---|---|---|---|---|
| `steel` | `#8a8d90` | 0.45 | 1 | Metal | | `steel_rust` | 0.5 | Bare structural steel; rusts over in months (ISO 9223 C3: 25–50 µm the first year) |
| `steel_painted` | `#6f7a6c` | 0.60 | 0 | Paint | `steel` | `steel_rust` | 15 | Default paint for built steel (a muted industrial green-grey); paint lasts 7–15 years (ISO 12944, medium) |
| `steel_rust` | `#8b4a2b` | 0.90 | 0 | Stone | | | | Rust: what steel ages to |
| `aluminium` | `#c9ccd0` | 0.35 | 1 | Metal | | | | Its oxide stays silvery |
| `copper` | `#c46f3e` | 0.35 | 1 | Metal | | `copper_patina` | 10 | Brown within months, green in 5–7 years on coasts, 10–14 inland |
| `copper_patina` | `#5f9b84` | 0.80 | 0 | Stone | | | | Patina: what copper ages to |
| `titanium` | `#9a958e` | 0.40 | 1 | Metal | | | | |
| `timber` | `#c8a16a` | 0.70 | 0 | Wood | `wood_cut` | `timber_weathered` | 1 | Sawn spruce, fresh; other woods' tones in §3.4. Greys within a year outdoors |
| `timber_weathered` | `#5e4f3f` | 0.85 | 0 | Wood | | | | Old wood, greyed and darkened: what timber ages to |
| `concrete` | `#a7a39b` | 0.90 | 0 | Stone | | | | |
| `glass` | `#a9c3c8` | 0.10 | 0 | Glass | | | | Tinted, opaque until transparency exists |
| `ice` | `#d8e8ef` | 0.20 | 0 | Glass | | | | |
| `rubber` | `#2b2a29` | 0.85 | 0 | Leather | | | | |
| `plastic` | `#d8d2c4` | 0.55 | 0 | Plain | | | | Unpigmented polyethylene (G5) |

### 3.3 The character

| Token | Colour | Roughness | Metalness | Pattern | Notes |
|---|---|---|---|---|---|
| `coverall` | `#8c99a6` | 0.85 | 0 | Fabric | Quilted cool grey-blue work coverall: arms, legs, collar |
| `leather` | `#4b2c22` | 0.70 | 0 | Leather | Dark oxblood-brown: boots, gloves, belt, straps, knee and elbow pads |
| `lens` | `#2a3b33` | 0.05 | 0 | Glass | Smoky green glass of the goggles |
| `player_accent` | per player | 0.75 | 0 | Fabric | The work jacket (§3.6) |
| `skin` | per player | 0.55 | 0 | Plain | One of the ten skin tones below, the player's choice |
| `hair` | per player | 0.70 | 0 | Plain | One of the nine hair colours below, the player's choice |
| `base_layer` | `#797e86` | 0.85 | 0 | Fabric | The base body's plain vest and shorts: mid grey, apart from every skin tone (the user's choice) |

**Skin tones.** The Monk Skin Tone Scale (Ellis Monk with Google, 2022; skintone.google, CC BY 4.0), ten tones spanning human skin, lightest first: `#f6ede4` 1, `#f3e7db` 2, `#f7ead0` 3, `#eadaba` 4, `#d7bd96` 5, `#a07e56` 6, `#825c43` 7, `#604134` 8, `#3a312a` 9, `#292420` 10. Tones 1–3 are lighter than §4's L* 92, and 9–10 nearly grey (chroma 6.5 and 3.8): display swatches rather than measured skin. The user kept them (approved exceptions, §4).

**Hair colours.** Measured, not painted (real hair measures darker than painted game hair: its shine comes from the light). Dark brown to red and blond from Mengel-From et al. (2009), who measured 107 Scots with a spectrophotometer; black and light brown from a cosmetics journal's samples; grey from a natural grey sample: `#2c2928` black, `#3c332d` dark brown, `#4f3f35` brown, `#5f4836` light brown, `#765f49` dark blond, `#90785c` blond, `#5e4031` auburn, `#7b563f` red, `#968a72` grey.

Buckles, rivets and the goggles' rims are `steel`; the jetpack's bottles are `steel_painted` on a `steel` frame with `leather` straps (§3.2). Against the five grounds (§4), the smallest ΔE₀₀ are 16.9 for `coverall` (gravel), 13.5 for `leather` (soil) and 13.0 for `lens` (basalt), computed 2026-10-21 with CIEDE2000 (checked against Sharma, Wu & Dalal's test pairs). G1.3a's automated checks confirm them.

### 3.4 Vegetation

| Token | Colour | Roughness | Pattern | Notes |
|---|---|---|---|---|
| `bark_spruce` | `#5a4632` | 0.90 | Wood | |
| `bark_birch` | `#d6d1c4` | 0.85 | Wood | |
| `bark_birch_marks` | `#3b3631` | 0.90 | Wood | Birch bark's dark marks, as detail |
| `bark_oak` | `#4f4537` | 0.95 | Wood | |
| `leaves_spruce` | `#3f5a35` | 0.80 | Plain | |
| `leaves_pine` | `#4b6338` | 0.80 | Plain | |
| `leaves_birch` | `#6f8a3c` | 0.75 | Plain | |
| `leaves_oak` | `#4d6b30` | 0.80 | Plain | |
| `grass` | `#6b7a3f` | 0.90 | Plain | Never brighter than the objects standing in it |
| `wood_cut` | `#d9b47c` | 0.75 | Wood | Fresh cut faces and stumps: a bright tell of where work was done |

### 3.5 Accents

Saturated colour is reserved for what matters. Nothing else may be as saturated as these.

| Token | Colour | Roughness | Pattern | Wears to | Use |
|---|---|---|---|---|---|
| `hazard_yellow` | `#e8b400` | 0.60 | Paint | `steel` | Danger markings, with `hazard_black` stripes |
| `hazard_black` | `#22201e` | 0.60 | Paint | `steel` | |
| `hot` | from temperature | | Plain | | Glowing metal, embers: blackbody colour from the real temperature (Planck), strength from the emitted radiance, above about 525 °C (Draper point) |
| `signal_green` / `signal_red` | `#5fbf3a` / `#d8402c` | 0.30 | Glass | | Lamps and indicators that are on |

### 3.6 Player colours

Sixteen accents, told apart at a glance: in normal colour vision, any two are at least ΔE₀₀ 8 apart (the closest, blue and indigo, are 8.5 apart). A colour-blind mode waits until the game is published (`docs/future-improvements.md`). The colours: `#e07b39` orange, `#3d7fd9` blue, `#d9c23d` yellow, `#8e5bd6` violet, `#3dbfae` teal, `#d64f6e` rose, `#7fbf3f` lime, `#e0e0e0` white, `#a0522d` sienna, `#5a6fd6` indigo, `#c9a0dc` lilac, `#2f8f4f` green, `#e8a0a0` pink, `#8a8f99` slate, `#c08a2a` ochre, `#3a3a3a` charcoal.

### 3.7 Data overlays

Viridis (sequential) and cool-warm (diverging) are used for engineering overlays only (ADR-0003), never as decoration. Under an overlay, surfaces turn matte and parts without data recede to dim grey.

## 4. Value, saturation and contrast

Measured in CIELAB, automated in tests over the palette.

- **Objects against the ground:** every object token differs from each of the five most common grounds (soil, gravel, granite, basalt, grass) by ΔE₀₀ ≥ 12. If a token fails against one ground, its edge wear and AO must carry the contrast, and the style board (§11) shows it.
- **Saturation limits (chroma C\*):**
  - terrain ≤ 35, ores ≤ 55;
  - materials, character and vegetation ≤ 45;
  - only accents (§3.5) and player colours may exceed 45.
- **No pure black or white** in the scene: L\* between 8 and 92 for every base colour. Darkness comes from lighting, not from the palette.
- **Approved exceptions.** A colour that misses a limit stays only with the user's approval, listed here and in `data/art/exceptions.ron` (a test checks that the two agree). The checks report an exception once it is no longer needed. For contrast, the token's edge wear and occlusion must carry it, and the style board shows that they do.

| Subject | Limit lifted | Why | Approved |
|---|---|---|---|
| `steel` | contrast against gravel | Original colour kept; edge wear and reflections carry it (ΔE₀₀ 10.3) | 2026-10-21 |
| `titanium` | contrast against gravel | Original colour kept; edge wear and reflections carry it (ΔE₀₀ 7.5) | 2026-10-21 |
| `concrete` | contrast against gravel | Original colour kept; edge wear and occlusion carry it (ΔE₀₀ 11.5) | 2026-10-21 |
| `rubber` | contrast against basalt | No colour above pure black reaches 12 on basalt (ΔE₀₀ 7.8) | 2026-10-21 |
| `copper` | chroma | Original colour kept (chroma 50.7) | 2026-10-21 |
| laterite bauxite | chroma | Original colour kept (chroma 55.8, against an ore's 55) | 2026-10-21 |
| skin 1 | lightness | The Monk scale's tone kept (L* 94.2) | 2026-10-21 |
| skin 2 | lightness | The Monk scale's tone kept (L* 92.3) | 2026-10-21 |
| skin 3 | lightness | The Monk scale's tone kept (L* 93.1) | 2026-10-21 |

## 5. Light and the frame

- **Sun and sky** come from the atmosphere model (ADR-0003). Warm sun and cool ambient are the simulation's, not graded.
- **Tonemapper:** Khronos PBR Neutral, fixed. Colours stay as authored, and no colour grading or LUTs are applied.
- **Shadows** are strong and crisp near the camera, softening with distance (cascades).
- **Ambient occlusion** (SSAO, G1.3) is the main tool for grounding: contact darkening under and between things, at most 50 % darkening in creases.
- **Bloom** only on emissive surfaces (hot metal, lamps), subtle.
- **Forbidden effects:** film grain, chromatic aberration, vignette, lens flares, depth of field in play, motion blur by default, outlines, cel shading, and fog beyond the atmosphere's real scattering.

## 6. Surfaces: grainy, weathered, used

Factorio's things look worked and old: grain in everything, dirt where it gathers, edges chipped, metal rusting, wood greyed. Creëer gets there with procedural layers in the renderer, all of them in world metres, and none from images. The user, 2026-10-21: "it should look like factorio (it is difficult to describe: grainy, weathered, used, etc.)", "darker and grimier", and "the game should have an aging mechanic, things look older and older".

**What a model carries** (baked by its Blender script, `art/bake.py`):
- baked occlusion (one value per large face, per corner on small ones);
- edge wear (the bevel faces);
- each part's tone;
- each corner's height above the model's base;
- grain coordinates in metres, along each part's grain.

**The layers, in order** (the renderer's mesh shader; `art/preview.py` previews them in Blender):

1. **Tone.** Each part (a board, a plate) is ±18 % in value from its token's colour. No two boards are alike.
2. **Pattern.** Each token has one (§3's tables), drawn along the part's grain:
   - *Wood*: thin dark grain lines, wandering, a few centimetres apart, over soft streaks;
   - *Paint*: a fine orange peel;
   - *Metal*: brushed along the part;
   - *Stone*: speckle at two scales;
   - *Fabric*: a fine weave;
   - *Leather*: pebbling;
   - *Glass* and *Plain*: none.
3. **Mottling and grain.** Over everything: mottling ±15 % at about 30 cm, and fine grain ±10 % at a few millimetres. Detail finer than a pixel fades out (filtered noise), so nothing shimmers at a distance.
4. **Age.** Each token weathers at its material's real rate (§6b): wood greys all over, and more in patches; steel rusts and copper greens in patches, first where dirt holds water. Each part's age differs a little from its object's (±0.3, by its tone).
5. **Grime.** The ground's splash up to 35 cm from the bottom, nothing on what faces up, everything that faces down. Dirt in creases (from the occlusion). Both gather with age: a new object is clean. A film of dirt (up to 30 % darker) and rain streaks down the sides come with age too.
6. **Edge wear.** Patches a few centimetres long along the bevels, never a continuous line (no outlines, §5). They reveal the token beneath with its own finish (`wears_to`: painted steel shows bare `steel`, timber shows `wood_cut`). They grow with age and with use (§6b), and what they reveal weathers and dirties too, lagging the surface: fresh on a new object, mostly its surface on an old one. Never specks or glints.
7. **Occlusion.** The baked occlusion darkens ambient light only; the sun has its shadows.

- **Roughness** follows the layers: grime is rougher, and chipped edges smoother.
- **No textures from images,** except the UI's icons (themselves renders, §9) and fonts.

### 6b. Ageing

Things get older as the world runs. A placed crate is new; after a few game days its timber has greyed, and its plates have rusted where dirt holds water. Each token weathers at its real rate, given in §3's tables as the years it takes outdoors in temperate air (ISO 9223's category C3). Some examples:
- **Timber** greys in about a year: UV breaks down its lignin within days, and rain washes it out.
- **Bare steel** rusts over in about half a year: C3 corrodes 25–50 µm in the first year.
- **Paint** lasts 7–15 years (ISO 12944, medium durability).
- **Copper** greens in about 10 years: 5–7 on coasts and in industrial air, more than 30 in dry air.

What drives it:
- **Surroundings** set the rate: in air with oxygen and water, the C3 rate; in the sea, three times it (marine air and water are C5 and worse); in vacuum, none (wood's greying by UV alone in vacuum is a future improvement).
- **Pace:** real rates would be invisible within a play session, so a world setting runs weathering faster than time. The default is one year of weathering per game day (6 hours); the server sets it.
- **Use:** impacts chip edges. An impact's energy beyond what a coating resists (about 10 J, as ISO 6272's falling-weight test measures) adds wear.

The server keeps each object's age (its exposure so far, the rate it ages at now, and its wear) and saves it. Clients draw it, and it costs nothing per tick: the age at any moment follows from the rate.

## 7. Shapes and models

- **Bevel every hard edge**: 1–2 % of the part's smallest dimension, at least 5 mm, at most 3 cm. Bevels catch the light and give the Factorio "rendered" look.
- **Smooth curves** (pillar 9). A curved surface's facets stay within 1 % of its radius of the true surface at LOD 0: a full circle has at least 24 segments, so the faces on either side of a smoothly shaded edge differ by at most 15°. At every distance, the facets stay within half a pixel on screen: a level of detail is chosen by that screen-space error, never by a fixed distance. The validator checks the 15° on every model.
- **Smooth shading, real edges.** Normals are smooth across curved surfaces. An edge is shaded hard only where the object has a real edge, and that edge is bevelled. Hard-surface models use face-weighted normals (Blender's Weighted Normal modifier), so flat faces stay flat and their bevels soft.
- **No faceted organics.** People, trees and rocks never show flat facets at play distance. Rocks keep their real fracture edges crisp (a crease angle), but their faces are curved. Boulders' collision hulls (G1.5) need enough points for that, and their cost is measured there.
- **Show how it was made**: welds as beads, bolted joints with bolt heads, plates with their thickness visible, castings rounded, machined faces flat and slightly shinier (roughness −0.1).
- **Distinct silhouettes**: every tool and item is recognisable as a black silhouette at 64 px and at 20 m in third person. The test: the icon silhouettes of any two items overlap by IoU < 0.8.
- **Triangle budgets** (LOD 0):

| Asset | Triangles |
|---|---|
| The character as drawn (the base body and everything worn, without hidden regions), in levels of detail chosen by screen error | close-up 40,000; mid 15,000; far, at play distance and in shadows, 3,000 (measured: ADR-0026, Cost) |
| A wearable | ≤ 4,000 at the mid level; at the others in proportion |
| A tool | ≤ 2,000 |
| An item, bag or crate | ≤ 1,000 |
| The landing pod | ≤ 20,000 |
| A tree (LOD 0, generated) | ≤ 8,000, impostors beyond 150 m |
| A decorative | ≤ 200 |

- **Scale** is in metres, with +Y up, origin at the part's natural rest point (a tool's grip, a crate's base centre); checked by the validator.

## 8. Subjects

- **The engineer.** A worker in rugged, padded workwear (pillar 10), not an astronaut. The user's rough guide is Factorio's engineer as first shown in FFF-40 (2014): its layered, padded workwear, chunky gloves and boots, and the player's colour on the jacket. It is not copied, and its hard hat, tools and rucksack are left out.
  - **A base body that wears things** (the user, 2026-10-21: many wearables in future, "a 'naked' version that can wear more and more cool stuff as you progress"). The engineer is a base body plus whatever it wears. Every piece of clothing and gear is a separate wearable model on the same bones. What a player wears shows their progress, as Factorio's armour does.
  - **The base body.** A real man of normal weight, with an open face, short hair and a skin tone the player chooses: Blender Studio's realistic base mesh (CC0), fitted to ANSUR II's men at 1.80 m (heights to all men's means, the build to the men of normal weight; ADR-0026). It wears a plain base layer (a vest and shorts), so "naked" is the body without any gear.
  - **The starting outfit** is made of wearables: a quilted `coverall`, a work jacket in the player's colour (`player_accent`) with a belt, `leather` knee and elbow pads, thick gloves, laced boots, and goggles (`lens`) pushed up on the forehead. The jacket closes at the neck with a stand collar. No hood: the head is bare, with short hair. No jetpack: the player makes one as they progress (the user, 2026-10-21).
  - **Later wearables** come with what the player can make: for example the jetpack (two small nitrogen bottles on a harness frame, on the back), a respirator for dust and gases, insulated or fire-resistant suits, armour, and a pressure suit for vacuum (EVA, G8), in the style of real ones. Each has real properties (mass, insulation, protection, pockets) in the simulation, from G1.6 (items) and G9 (needs) on.
  - **The face** is the base mesh's, a real face at a level of detail soft enough to read at play distance, with no fine detail that would look uncanny up close.
  - **Proportions.** Those of a real man of normal weight (§2), neither broad nor muscular, in workwear that sits close: the coverall about 1.4 cm off the skin, the jacket 2.2 cm. Cloth hangs from the chest and the seat rather than following the body in. Gloves and boots are chunky. They read as a person in workwear from 50 m and as a silhouette at 64 px.
  - **The bones** are the ragdoll's fifteen segments, one bone per segment, so the drawn body is the simulated one (`docs/g1-plan.md`; ADR-0026). The body and every wearable are skinned on them, smooth across the joints, never a set of separate blocks.
  - **Things held** are separate models attached to a bone: from G1.4, the tool in hand.
- **Tools.** Wooden handles (`timber`, darkened with use), forged steel heads (`steel` with wear), simple and sturdy. Their outlines differ: pick, axe, shovel and saw are unmistakable.
- **Trees.** Masses of foliage in clumps, not individual leaves; dark undersides from AO; bark with vertical detail. Stumps and cut ends in `wood_cut`. Species are told apart by shape first (spruce cone, pine umbrella, birch's white trunk, oak's broad crown), colour second.
- **Machines and structures** (G2 on): painted steel by default, bare where machined or worn, hazard stripes where they move or burn.
- **Decoratives.** Muted and low, never competing with objects: grass tufts below knee height, pebbles in the ground's own rock colour.

## 9. Icons

- Rendered by our own renderer from the item's model (`--icon`), never drawn.
- Orthographic three-quarter view: azimuth 35°, elevation 30°; key light from the upper left, fixed ambient.
- Transparent background, a soft drop shadow, no outline; 64 px and 128 px.
- Bulk items: a heap of the material in its own colour; ores show their host rock with the ore's colour flecked through.

## 10. UI

In Factorio's spirit, not a copy of it.

| Token | Colour | Use |
|---|---|---|
| `ui_panel` | `#2e2b29` | Window background (warm dark grey) |
| `ui_inset` | `#232120` | Slots, fields, lists |
| `ui_bevel_light` | `#4a4643` | Top and left edges of raised elements |
| `ui_bevel_dark` | `#191817` | Bottom and right edges |
| `ui_text` | `#e8e2d6` | |
| `ui_text_dim` | `#a8a196` | Labels, units |
| `ui_highlight` | `#f0a030` | Selection, the active item, focus |
| `ui_good` / `ui_warn` / `ui_bad` | `#7fb24a` / `#e8b400` / `#d0452f` | States |

- **Raised and bevelled** buttons and panels with a 1 px light and dark edge; no rounded pill shapes, no glassy blur.
- **Dense tooltips with real numbers:** mass, volume, composition, temperature, strength, in SI units with the unit dimmed. Numbers in tabular figures.
- **One font family**, a clear sans with tabular figures (chosen and bundled in G1.9).
- **The HUD** stays in the corners and edges; the centre of the screen belongs to the world.

## 11. Keeping the style from drifting

1. **This file is the authority.** Plans and code refer to it; nothing restates it differently.
2. **Palette in data, checked against this file.** `data/art/palette.ron` (G1.3) is tested against the tables here. The UI theme's constants are tested the same way.
3. **Models use tokens only.** The validator (`creeer_tools validate`) refuses a model with raw colours or image textures, unknown tokens, too many triangles, facets over 15° on smoothly shaded edges (§7), or the wrong scale or origin.
   - **Models come from scripts.** Every authored model is built by a Blender script in `art/`, which is its source; the exported `.glb` is regenerated from it (`docs/blender-mcp.md`). A change to a model is a change to its script, which can be read and reviewed.
4. **Automated style checks** in CI:
   - the ΔE contrast against the grounds (§4);
   - the chroma and L\* limits (§4);
   - icon silhouettes distinct (§7);
   - player colours told apart (§3.6);
   - the approved exceptions still needed (§4);
   - the forbidden effects absent from the renderer's settings (§5).
5. **The style board.** A headless scene (`--style-board`) renders every asset on the five grounds, at noon and in the evening, plus the HUD and an inventory window. Its golden images are checked on every change to an asset, a shader or the palette. A difference is either a fix or an intended change, approved with the image.
6. **Review at every milestone's end.** The user looks at the style board as part of the demo. New subjects (machines in G2–G3, vehicles in G6) get a §8 entry here before their first model.
7. **Changes need the user's approval**, recorded below with the date and the reason.

## Changelog

- 2026-10-18: written, before G1 (the user: "i want a factorio like vibe art style wise").
- 2026-10-18: §1b, references before making (the user: "proper research is really important: record that in all docs").
- 2026-10-21, at the start of G1.3, on the user's instructions:
  - **Not blocky** (pillar 9, §7's smoothness rules). The user: "make sure in the art-style.md that it is clear that i do not want a blocky or voxy like game (no minecraft geometry, i want smooth surfaces ==> realism)". The pillar's "neither low-poly nor realistic" became "real, smooth shapes; stylised surfaces, not photographic".
  - **Not futuristic** (pillar 10). The user: "i do not want a futuristic art style but something similar to the vibe of factorio".
  - **The engineer** (§3.3, §8): workwear after Factorio's FFF-40 engineer as "a very rough guide (do not copy and leave the hard hat, tools, rucksack, etc. out)". The off-white suit, bellows joints, helmet and visor were replaced by `coverall`, `leather`, `lens` and the jacket in `player_accent` (roughness 0.75, a fabric). The body is skinned on the ragdoll's segments instead of drawn as rigid segments, for smooth surfaces (ADR-0026).
  - **§1b** gained what the survey took from Factorio; **§11.3**: models come from scripts in Blender (`docs/blender-mcp.md`).
  - **The user's choices on the engineer** (2026-10-21): the jetpack as bottles on a harness frame; the body skinned and smooth; a hood and goggles with the face open (so no respirator in the starting outfit). Then: "in future I want to have many wearables, so there should [be] a 'naked' version that can wear more and more cool stuff as you progress". §8 now has a base body (base layer, open face, skin tone and hair chosen by the player) and wearables on the same bones; §3.3 gained `skin`, `hair` and `base_layer`.
- 2026-10-21, G1.3a: the checks' first run over the palette.
  - **The user kept the original colours** of players and materials ("keep the original colours for maximum realism"). The misses are listed as approved exceptions (§4's table).
  - **Colour blindness is not considered for now** (the user: "right now lets not consider colour blind people, we could do that later if i ever want to publish it"). §3.6 no longer promises that the player colours are told apart under colour-vision deficiencies. A colour-blind mode is in `docs/future-improvements.md`, with what was measured.
  - **Tokens made explicit.** The patina and the birch marks became tokens (`copper_patina`, `bark_birch_marks`), and the accents got a roughness, so the data and these tables match one to one.
- 2026-10-21, G1.3b, after the user saw the first crate in Blender:
  - **§6 rewritten: grainy, weathered, used.** The user: "a very boring and blandy monochrome wood texure: in general (everything in the game) it should look like factorio (it is difficult to describe: grainy, weathered, used, etc.)", then "darker and grimier". The ±12 % noise became seven layers: tone, pattern, mottling and grain, age, grime, edge chips, occlusion. They were worked out live in Blender on three crates.
  - **§6b, ageing.** The user: "the game should have an aging mechanic, things look older and older". Each material weathers at its real rate (ISO 9223, ISO 12944, wood and copper weathering), set by its surroundings and a world pace.
  - **§3's tables** gained pattern, wears to, ages to and weathers in; `timber_weathered` (`#5e4f3f`) is new.
- 2026-10-21, G1.3c: **§6's edge wear toned down.** The chips on aged crates read sparkly (bright fresh-wood specks, shinier than the wood); the user: "tone them down, make it look good". Wear now comes in patches a few centimetres long, takes the revealed token's own roughness and metalness, and weathers with the object.
- 2026-10-21, G1.3f: **the jetpack left the starting outfit.** The user: "the 'base' player model should not have a jet pack, that is something that you can build if you progress in the game". It is a later wearable (§8), as the bottles on a harness frame the user chose.
- 2026-10-21, G1.3f: **the engineer's blockout**, approved after three rounds of the user's notes (§3.3, §8; ADR-0026):
  - **No hood** ("the face is covered with some weird balaclava like thing, remove that and model a proper head"): the head is bare with short hair, the goggles pushed up on the forehead; `coverall`'s row names the collar instead.
  - **A prebuilt body** ("please use a proper reference or something prebuil"): Blender Studio's realistic base mesh (CC0), fitted.
  - **An average man** ("too muscular", "still way to buff, to broad", "the hip is too wide"): fitted to ANSUR II's men of normal weight. §8's "bulky clothes, slightly broad at the shoulders" became thin workwear on a man neither broad nor muscular.
  - **Thin clothes** ("really really thick", "still too thick"), a seat that is not accentuated, and a stand collar instead of a rolled one ("remove this weird sausage like feature").
  - **The base layer is mid grey** (the user's pick), `#797e86`: nudged ΔE₀₀ 1.6 from the first proposal `#7c8187` to keep ΔE₀₀ 12 from gravel.
  - **Levels of detail for the engineer** (§7's budgets; the user: "i think we should have different LODs for the player"): detail can only be geometry here (no image textures, no normal maps), so quilting, laces, buckles and gloved fingers are triangles. More near the camera, few at play distance (about 70 px tall) and in shadows; the close-up budget is measured before modelling to it.
  - **Skin tones and hair colours** (§3.3; the user: "skin tone and hair is good"): the Monk scale's ten tones, its lightest three as approved exceptions (§4), and nine measured hair colours (`data/art/people.ron`).
