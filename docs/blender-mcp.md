# Using Blender with the MCP

Creëer's authored models are made in Blender: the engineer, what the engineer carries (the jetpack, tools), the landing pod, bags and crates. Claude drives a running Blender through **MCP for Blender**, an MCP server with a Blender add-on. You can watch the scene change, and open and change the same models yourself. Whatever is procedural stays in code: terrain, trees, structures and decoratives (`docs/g1-plan.md`, G1.3).

The rules every model follows are in `docs/art-style.md`, the authority. The decisions behind this pipeline are in ADR-0026.

## The set-up

Set up on 2026-10-21, at the start of G1.3:

| Piece | What | Where |
|---|---|---|
| Blender | 5.2 LTS (5.2.2). Blender 4.2 is installed too, without the add-on | `C:\Program Files\Blender Foundation\Blender 5.2\` |
| The MCP server | `mcp-for-blender` (formerly `ahujasid/blender-mcp`), run by `uvx` (uv 0.13.4) | `%USERPROFILE%\.local\bin\uvx.exe` |
| The add-on | "Interface: MCP for Blender" 1.8 (protocol 14), for Blender 5.2 only | `%APPDATA%\Blender Foundation\Blender\5.2\scripts\addons\blender_mcp.py` |
| Claude Code | Registered for this project only (local scope), with telemetry off | `claude mcp add blender -e DISABLE_TELEMETRY=true -- uvx.exe mcp-for-blender` |

The add-on's asset libraries (Poly Haven, Sketchfab, Poly Pizza) and model generators (Hyper3D, Hunyuan3D, Meshy, Tripo) are all off (below, "What it is not used for").

## Starting a session

1. Open Blender 5.2.
2. Press N in the 3D view to open the sidebar. In its "MCP for Blender" tab, press **Start MCP Server** (port 9876, on this machine only).
3. Start Claude Code in the repository. The `blender` server connects to it, and Claude checks the connection with `get_addon_status` (the Blender version and the add-on's protocol).

If the add-on is out of date: run `uvx mcp-for-blender install-addon`, then switch the add-on off and on (Preferences → Add-ons) or restart Blender, and start the server again.

## How a model is made

1. **The script is the source.** Each model has a Python script in `art/` (for example `art/engineer.py`) that builds it from an empty scene: its meshes, modifiers, bones and weights, materials named after palette tokens, and its baked occlusion and wear. Fixed seeds make it deterministic, so the same script always gives the same model. A change to a model is a change to its script, which can be read, reviewed and diffed. `.blend` files are build products and are not committed: binary files can't be reviewed or merged.
2. **Iterating live.** While a model is being made, Claude runs its script, or parts of it, in your Blender (`execute_blender_code`), looks at the result (`look`: from several sides, close up, and as a 64 px silhouette) and changes the script. You can watch, orbit and comment ("the boots are too big"). At each checkpoint Claude shows you renders, and a model is final only when you approve it.
3. **Your own changes.** You can change a model by hand in Blender at any time. Then tell Claude what you changed: it reads the scene (`get_scene_info`, or Python) and writes the change into the script. A change that only lives in a `.blend` file is lost at the next build.
4. **Exporting is headless, not through the MCP** (long operations time out over it). `cargo xtask art [name]` runs `blender --background --factory-startup --python art/build.py -- <name>`: a clean Blender runs the model's script and writes `data/models/<name>.glb`. The `.glb` is committed, because the game loads it and CI's validator checks it. `cargo xtask art --blend <name>` also writes `target/art/<name>.blend`, to open by hand.
5. **Checked.** `cargo xtask art --check` rebuilds every model and fails if a committed `.glb` differs from what its script makes. It runs on the development machine, where Blender is installed; CI has no Blender and runs the validator on the committed files.
6. **Compared with the game.** `cargo xtask art --board <out.png>` renders the style board in Blender (`art/board.py`): the crate at 0, 1 and 4 years weathered on soil, with the game's camera and sun, in the preview's surfaces. Set it beside `creeer_client --style-board <out.png>`: the layers should sit in the same places and age alike. The colours differ by the renderers' lighting and tone mapping.

**The Blender version is pinned** (5.2 LTS). It is recorded in every `.glb` (`asset.generator`), and `xtask art` refuses another version, so a new Blender cannot change a model unnoticed. Blender is found through the `BLENDER` environment variable, or else at its default install path.

## The engineer's base mesh

The engineer's body starts from Blender Studio's Human Base Meshes v1.4.1 (CC0; the user's choice, ADR-0026), its realistic man.

- **Fetched, never committed.** `art/base_meshes.py` downloads the bundle once from its pinned address on mirror.blender.org (50 MB), checks its SHA-256 and unpacks only its `.blend` into `target/art-cache/`. A bundle that does not match is deleted, and nothing is built from it.
- **Only mesh data is taken.** Scripts append the objects they need (`bpy.data.libraries.load(…, link=False)`), never open the file itself, and check that what they took has no animation or drivers. Its text blocks (a README, and a stale licence text naming another rig) are never loaded, and with auto-run off nothing in it runs.
- **The blockout** (`art/engineer/blockout.py`, with `body.py`, `physique.py` and `garments.py`) runs in a live session and exports nothing: `exec(open("art/engineer/blockout.py").read())` builds the scene "engineer blockout" in about 11 s and prints its measures against the surveys as `art:` lines.

## The engineer: building and checking

- **Built** by `cargo xtask art --blend engineer` (about 50 s): `art/engineer.py` runs `art/engineer/models.py`, which builds the pieces (`outfit.py`), details them (`detail.py`, `boots.py`, `gear.py`, `goggles.py`, `hair.py`), weights them (`skinning.py`), divides them into regions (`regions.py`; what a wearable over a piece encloses is that wearable's, `inside.py`) and writes every piece at three levels into `data/models/engineer.glb`, and the same into `target/art/engineer.blend` for the checks below. The `art:` lines print each level's triangles.
- **Seen as the game draws it.** `exec(open("art/engineer/review_model.py").read())` in a live session builds the scene "engineer level 0" from `target/art/engineer.blend`: the full outfit, what it hides (`data/wearables.ron`, and the parts inside each worn wearable) left out, in the preview's surfaces, aged three years, on the style board's soil under its sun. `blender --background --factory-startup --python art/engineer/render_review.py -- <out dir> [<level>]` renders it headless from seven sides (front, back, side, above, the chest, the legs, the head) as PNGs, for the user's review.
- **The fit checked.** `blender --background --factory-startup --python art/engineer/check_fit.py -- [<level>]` poses the exported model by its skin at rest, at each of the ragdoll's joint limits (an arm stopped where its box meets the trunk's, as the ragdoll stops it: `reach.py`) and with the fingers relaxed and in a fist (G1.4b, `hands.pose`), for the base body, the full outfit and each wearable alone. It prints each pose's points poking through a layer over them, hidden points seen, and parts in contact (apart, not failed), and each piece's share outside its segments' boxes; it exits 1 on any miss. About 30 s a level. ADR-0026's Validation gives its results and Limits what it allows.

## Conventions

- **Units and axes.** Metres. Blender's +Z up becomes +Y up on export (the glTF exporter's default), which is the engine's +Y up.
- **Origin** at the model's natural rest point: a tool's grip, a crate's base centre, the engineer's feet.
- **Materials.** One Blender material per palette token, named exactly after the token (`coverall`, `leather`, `steel`). Their colours in Blender are only for previewing: the game takes each token's colour from `data/art/palette.ron`. Image textures are never used (`docs/art-style.md` §6).
- **Vertex colours** (`COLOR_0`): red is the baked ambient occlusion, green the edge wear (the convexity it follows, and on the engineer where workwear wears through), blue each part's tone, alpha the height above the model's base (`art/bake.py`).
- **The engineer's bones** are the ragdoll's fifteen segments and thirty finger bones (G1.4b, `art/engineer/hands.py`: three a digit, `<digit>.<1–3>.<side>`, drawn only), the segments named after them (`pelvis`, `torso`, `head`, `upper_arm.L`, …) and placed at the joints of `creeer_sim::character::ragdoll`. The script reads that table from a file that `creeer_tools` writes from the Rust code, and the validator checks the exported bones against it, so the two cannot drift apart.
- **Shapes.** Bevels, face-weighted normals, the 15° facet limit, triangle budgets and silhouettes: `docs/art-style.md` §7.

## What it is not used for

- **Asset libraries and AI model generators.** They stay off. Their models carry photo textures and arbitrary topology (against `docs/art-style.md` §6–7), and every asset's licence would need checking. They may be used as references to look at, never as assets.
- **Nothing in the game depends on the MCP.** It is a tool for making the scripts. Anyone can rebuild every model with Blender alone.

## Safety

- `execute_blender_code` runs any Python inside Blender, with the access Blender has on your machine. Every script Claude runs is shown in the conversation, and the scripts that build models live in the repository.
- The add-on's socket has no authentication. It listens on this machine only (localhost): never open port 9876 to a network, and stop the server when you are not modelling.
- The add-on offers a safe mode (`BLENDER_MCP_SAFE_MODE=1` in the server's environment) if you want to restrict it further.
- Telemetry is off (`DISABLE_TELEMETRY=true`); the add-on's content collection is opt-in and stays off.
- Don't open `.blend` files from unknown sources with "Auto Run Python Scripts" on. The one outside file the scripts use, the base mesh bundle, is checked against its pinned SHA-256 and only appended from (above).

## Troubleshooting

- **"Blender could not be reached":** the server is not started in Blender's sidebar, or another Blender instance holds port 9876.
- **Protocol mismatch:** update the add-on (above, "Starting a session").
- **Exports time out:** export with `cargo xtask art`, not through the MCP.
- **Removing it:** `claude mcp remove blender -s local`, then switch the add-on off in Blender.
