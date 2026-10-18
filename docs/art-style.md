# Creëer art style

**Factorio's vibe, in 3D.** This file is the authority on how Creëer looks. Every asset, shader, effect and UI element is made to it. It changes only with the user's approval, recorded in the changelog at the end. plan.md's pillar: *"Factorio-like. Stylised, readable, softly textured, strong colours per material. Neither low-poly nor realistic."*

Status: written 2026-10-18, before G1. Its palette and checks are implemented in G1.3 (`docs/g1-plan.md`); until then the rules apply to whatever is made.

## 1. The pillars

1. **Readable first.** At a glance from the play camera you can tell *what* a thing is and *what it is made of*. Readability wins over detail every time.
2. **The material decides the colour.** Granite looks like granite everywhere, and steel like steel. Colours come from the palette (§3), never chosen per asset.
3. **Grounded and earthy.** The terrain is muted and earthy. Objects stand out from it by contrast (§4), not by glowing or outlines.
4. **Soft detail, never photographic.** Surfaces get procedural noise, edge wear and grime. Photo textures are never used.
5. **Weight and contact.** Strong sun shadows and ambient occlusion sit everything firmly on the ground; nothing floats visually.
6. **Chunky, mechanical, believable.** Shapes look like what they are and how they were made: bevels, bolts, welds, rivets, handles. They are slightly chunky in their small details, never cartoonish in their proportions.
7. **Dense and lived-in.** The ground between things is never empty: pebbles, grass, bushes and fallen branches are everywhere.
8. **Real where physics speaks.** Hot metal glows at its real temperature, and water, air and sunlight are the simulation's own (ADR-0003). Stylisation lives in surfaces, palette and shapes, never in contradicting the physics.

## 1b. References before making

Before a kind of asset, shader, effect or UI is made, study how others did it: Factorio's own art and its developers' write-ups (the FFF blog), and stylised 3D games with readable, material-led looks. Note what was taken from where (in the asset's ADR or this file's changelog). The style is ours; the craft is learned from those who did it first (plan.md §1, principle 12).

## 2. The physical shape comes first

The drawn shape matches the simulated shape: a member drawn 20 cm thick is 20 cm thick in the solver. Only details smaller than **2 cm** may be exaggerated for readability: bolt heads, handles, rivets, weld beads, up to 1.5× their real size. Proportions of bodies, members and parts are real.

## 3. The palette

Each token has a base colour (sRGB), a roughness and a metalness. Assets name tokens, never raw colours. In G1.3 the tokens move into data (`data/art/palette.ron`), and a test reads the tables of this file and fails if the two differ. Changing a colour means changing it here first.

### 3.1 Terrain

The rocks' colours live in `data/geology/rocks.ron` and are part of this palette: soil `#6b4f35`, gravel `#8c8174`, granite `#c4a194`, basalt `#3d3e44`, limestone `#c9c3b1`, and the rest. Ores are deliberately a little more saturated than their host rocks, so a prospector's eye can find them (porphyry copper ore `#c0773f`, banded iron `#80352b`, laterite bauxite `#b8552d`). All terrain is dielectric, with roughness 0.85–0.95.

### 3.2 Materials

| Token | Colour | Roughness | Metalness | Notes |
|---|---|---|---|---|
| `steel` | `#8a8d90` | 0.45 | 1 | Bare structural steel |
| `steel_painted` | `#6f7a6c` | 0.60 | 0 | Default paint for built steel (a muted industrial green-grey) |
| `steel_rust` | `#8b4a2b` | 0.90 | 0 | Rust, as wear on bare steel only |
| `aluminium` | `#c9ccd0` | 0.35 | 1 | |
| `copper` | `#c46f3e` | 0.35 | 1 | Patina (`#5f9b84`) only as wear on old copper |
| `titanium` | `#9a958e` | 0.40 | 1 | |
| `timber` | `#c8a16a` | 0.70 | 0 | Sawn spruce; other woods' tones in §3.4 |
| `concrete` | `#a7a39b` | 0.90 | 0 | |
| `glass` | `#a9c3c8` | 0.10 | 0 | Tinted, opaque until transparency exists |
| `ice` | `#d8e8ef` | 0.20 | 0 | |
| `rubber` | `#2b2a29` | 0.85 | 0 | |
| `plastic` | `#d8d2c4` | 0.55 | 0 | Unpigmented polyethylene (G5) |

### 3.3 The character

| Token | Colour | Roughness | Metalness | Notes |
|---|---|---|---|---|
| `suit` | `#d9d4c7` | 0.70 | 0 | Off-white work suit |
| `suit_joint` | `#3a3836` | 0.80 | 0 | Bellows, gloves, boots |
| `visor` | `#4a3418` | 0.05 | 0 | Dark amber, sun-filtering |
| `player_accent` | per player | 0.55 | 0 | Shoulders, helmet stripe, backpack (§3.6) |

### 3.4 Vegetation

| Token | Colour | Roughness | Notes |
|---|---|---|---|
| `bark_spruce` | `#5a4632` | 0.90 | |
| `bark_birch` | `#d6d1c4` | 0.85 | Dark marks (`#3b3631`) as detail |
| `bark_oak` | `#4f4537` | 0.95 | |
| `leaves_spruce` | `#3f5a35` | 0.80 | |
| `leaves_pine` | `#4b6338` | 0.80 | |
| `leaves_birch` | `#6f8a3c` | 0.75 | |
| `leaves_oak` | `#4d6b30` | 0.80 | |
| `grass` | `#6b7a3f` | 0.90 | Never brighter than the objects standing in it |
| `wood_cut` | `#d9b47c` | 0.75 | Fresh cut faces and stumps: a bright tell of where work was done |

### 3.5 Accents

Saturated colour is reserved for what matters. Nothing else may be as saturated as these.

| Token | Colour | Use |
|---|---|---|
| `hazard_yellow` | `#e8b400` | Danger markings, with `hazard_black` stripes |
| `hazard_black` | `#22201e` | |
| `hot` | from temperature | Glowing metal, embers: blackbody colour from the real temperature (Planck), strength from the emitted radiance, above about 525 °C (Draper point) |
| `signal_green` / `signal_red` | `#5fbf3a` / `#d8402c` | Lamps and indicators that are on |

### 3.6 Player colours

Sixteen accents, told apart by people with the common colour-vision deficiencies (checked against simulated deuteranopia and protanopia in the test): `#e07b39` orange, `#3d7fd9` blue, `#d9c23d` yellow, `#8e5bd6` violet, `#3dbfae` teal, `#d64f6e` rose, `#7fbf3f` lime, `#e0e0e0` white, `#a0522d` sienna, `#5a6fd6` indigo, `#c9a0dc` lilac, `#2f8f4f` green, `#e8a0a0` pink, `#8a8f99` slate, `#c08a2a` ochre, `#3a3a3a` charcoal.

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

## 5. Light and the frame

- **Sun and sky** come from the atmosphere model (ADR-0003). Warm sun and cool ambient are the simulation's, not graded.
- **Tonemapper:** Khronos PBR Neutral, fixed. Colours stay as authored, and no colour grading or LUTs are applied.
- **Shadows** are strong and crisp near the camera, softening with distance (cascades).
- **Ambient occlusion** (SSAO, G1.3) is the main tool for grounding: contact darkening under and between things, at most 50 % darkening in creases.
- **Bloom** only on emissive surfaces (hot metal, lamps), subtle.
- **Forbidden effects:** film grain, chromatic aberration, vignette, lens flares, depth of field in play, motion blur by default, outlines, cel shading, and fog beyond the atmosphere's real scattering.

## 6. Surfaces

- **Procedural detail.** Triplanar value noise modulates the base colour: terrain ±35 % (as now), meshes ±12 %, vegetation ±20 %. The noise scale is fixed in world metres (≈ 0.5 m for terrain, ≈ 5 cm for parts) so detail reads the same everywhere.
- **Edge wear.** Convex edges lighten towards the bare material underneath (painted steel shows `steel`), in a band 1–3 cm wide, from curvature baked into the vertices at export. How much wear a part has is a parameter (new = none, old = full), and may later follow the part's real history.
- **Grime.** Concave areas darken with AO, up to 30 % beyond the lighting's own.
- **No textures from images**, except the UI's icons (themselves renders, §9) and fonts.

## 7. Shapes and models

- **Bevel every hard edge**: 1–2 % of the part's smallest dimension, at least 5 mm, at most 3 cm. Bevels catch the light and give the Factorio "rendered" look.
- **Show how it was made**: welds as beads, bolted joints with bolt heads, plates with their thickness visible, castings rounded, machined faces flat and slightly shinier (roughness −0.1).
- **Distinct silhouettes**: every tool and item is recognisable as a black silhouette at 64 px and at 20 m in third person. The test: the icon silhouettes of any two items overlap by IoU < 0.8.
- **Triangle budgets** (LOD 0):

| Asset | Triangles |
|---|---|
| The character (all segments) | ≤ 15,000 |
| A tool | ≤ 2,000 |
| An item, bag or crate | ≤ 1,000 |
| The landing pod | ≤ 20,000 |
| A tree (LOD 0, generated) | ≤ 8,000, impostors beyond 150 m |
| A decorative | ≤ 200 |

- **Scale** is in metres, with +Y up, origin at the part's natural rest point (a tool's grip, a crate's base centre); checked by the validator.

## 8. Subjects

- **The engineer.** A chunky work suit and helmet: wide shoulders, thick bellows joints, boots, a backpack with the jetpack's tanks. It reads as a person in a suit from 50 m. The body is made of rigid segments, one per ragdoll segment (`docs/g1-plan.md`, decisions). The player accent goes on the shoulders, the helmet stripe and the backpack. No face is shown behind the visor.
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
3. **Models use tokens only.** The validator (`creeer_tools validate`) refuses a model with raw colours, unknown tokens, too many triangles, or the wrong scale or origin.
4. **Automated style checks** in CI:
   - the ΔE contrast against the grounds (§4);
   - the chroma and L\* limits (§4);
   - icon silhouettes distinct (§7);
   - player colours distinguishable under simulated colour-vision deficiencies (§3.6);
   - the forbidden effects absent from the renderer's settings (§5).
5. **The style board.** A headless scene (`--style-board`) renders every asset on the five grounds, at noon and in the evening, plus the HUD and an inventory window. Its golden images are checked on every change to an asset, a shader or the palette. A difference is either a fix or an intended change, approved with the image.
6. **Review at every milestone's end.** The user looks at the style board as part of the demo. New subjects (machines in G2–G3, vehicles in G6) get a §8 entry here before their first model.
7. **Changes need the user's approval**, recorded below with the date and the reason.

## Changelog

- 2026-10-18: written, before G1 (the user: "i want a factorio like vibe art style wise").
- 2026-10-18: §1b, references before making (the user: "proper research is really important: record that in all docs").
