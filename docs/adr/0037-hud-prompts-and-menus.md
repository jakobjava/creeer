# ADR-0037: The HUD, prompts and menus

- **Status:** accepted (G2.11, stream A3, 2026-11-13).
- **Context:** the user, after playing G2: the building system was "so unusable that I can't even tell you what to change"; the game "plays horrible" (`docs/playability-plan.md`). Much of it is that the game does not tell the player what they can do, which key does it, or why something was refused. Keys had piled up over the milestones with nothing on screen explaining them.
- **Builds on:** ADR-0026 (the look; `docs/art-style.md` §10, the UI's palette), ADR-0027 (reach, aiming), ADR-0028 (items and the inventory), ADR-0036 (holding by hand), plan.md principle 13 (precision is technology: the screen shows what a person on site would know).
- **Evidence:** `crates/creeer_client/src/{bindings,bindings_keys,prompts,hud,hud_seen,hud_shot,settings}.rs` and their tests; `crates/creeer_client/src/app/input.rs`; frames `creeer_client --planet <view> <file.png> --hud` (`hud_shot.rs`).

## The audit (bc11c37, before this ADR)

### Every key and button of the planet game

| Input | Where handled | What it did |
|---|---|---|
| W A S D | `fly_camera.rs` | Walk (walking) or fly (flying) |
| Space | `fly_camera.rs` | Jump; flying: up |
| Left Shift | `fly_camera.rs` | Run; flying: ten times faster |
| Left Ctrl | `fly_camera.rs`, `holding.rs` | Jetpack (creative); flying: down. While a part is held: turning, sliding and looking five times finer (and no jetpack) |
| Right Ctrl | `holding.rs` | Finer, while a part is held |
| Right mouse (held) | `app/mod.rs` | Look about: the cursor captured while it is held, walking too |
| Left click | `app/mod.rs`, `planet_view.rs` | Walking: strike with what is in hand, or dig with bare hands, at the crosshair. Flying: the Planet window's sandbox tool (dig, fill, tunnel, crate, box, ball, frame, powerhouse) |
| Wheel | `app/mod.rs`, `holding.rs` | Holding a part: slide it along itself. Flying: flying speed. Solar system: zoom |
| Mouse (Q held) | `holding.rs` | Turn what is held instead of the view |
| V | `app/planet.rs` | Walk the avatar, or fly the camera free |
| F5 | `app/planet.rs` | First person, or the camera behind |
| G | `app/planet.rs` | Take what the crosshair is on in both hands, or put down what they hold |
| E | `app/planet.rs` | Put into a pocket what is held, or the item aimed at |
| F | `app/planet.rs` | Put what is held into the container aimed at, or look into it |
| I | `app/planet.rs` | The inventory window |
| T | `app/planet.rs` | Throw what is held |
| X | `app/planet.rs` | Strip bark from the stick held |
| B | `app/planet.rs`; `app/system.rs` | Bind the stone aimed at onto the stick held; in the solar system, the ship's engine |
| J | `app/planet.rs`, `fastening.rs` | Fasten the parts touching at the crosshair |
| K | `app/planet.rs` | The next fastener |
| U | `app/planet.rs` | Take apart the joint at the crosshair |
| P | `app/planet.rs` | Set the post at the crosshair in its hole |
| R | `holding.rs` | Stand up or lay down what is held |
| Q (held) | `holding.rs` | The mouse turns what is held |
| 1–4, 0 | `app/planet.rs` | A tool of the tools' table in hand (creative); again or 0: away |
| F9 | `app/sandbox_ui.rs` | The profiler |
| M , . B | `app/system.rs` | Solar system: map, time slower, faster, engine |
| Esc, F3 | — | Nothing (F3 and F4 on the Sandbox window's buttons are milestone names) |

H and Alt, named in older comments, were no longer bound. Keys were matched in five files, each by its `KeyCode`; the hints on screen were written out by hand in two more (`holding.rs`, `inventory_ui.rs`), and had drifted from them (the bottom hint and the line under the crosshair disagreed while a part was held).

### Everything on screen during play

| What | Where | Shows | Debug? |
|---|---|---|---|
| Performance window | top left | frame rate, adapter, frame and tick percentiles, a plot | debug |
| Planet window | left | the planet's radius, g and day; leave and orbit buttons; the eight sandbox tools; walk; save, load, remove bodies; machines; skin and hair; edits; time of day and the sun's speed; a geology probe (rock, density, province, deposit grade); altitude, latitude, longitude; health; bodies in view, network, terrain chunks; four lines of keys for flying | debug, but for the skin and hair (a setting) and save and load (the menu's) |
| Crosshair | centre | a cross, a ring in reach, "name · 1.9 m" or "out of reach" | play; the distance is a number no person on site knows |
| Under the crosshair | centre | "Hands: …" with the exact mass; every key that might apply, with a blow's energy in joules; the last note or refusal for 5 s | play, overloaded; joules are not seen |
| Hint while holding | bottom | the holding keys, written by hand | play; a second list disagreeing with the first |
| Work ring | centre | an arc of 18 px under the crosshair's 12 px ring | play; hidden by the crosshair |
| Vitals | bottom left | health, hurt parts, down, the jetpack's gas | play |
| Inventory (I) | window | load in kg, each pocket's litres, items with a take button, the container looked into, the creative kit as buttons | play |
| Profiler (F9) | window | off by default | debug |

The frames of that state: `<frames>/hud/before-*.png` (aim, aim-far, heavy, post, rail, work, inventory). Its cost: the screen laid out and tessellated in 0.57–0.62 ms a frame (p50; p99 0.84–0.93 ms; 1080p, `--hud-frames 600`), most of it the performance plot and the Planet window.

## Prior work

How well-regarded first-person building and survival games tell the player what they can do, and the guidelines written from them. Surveyed 2026-11-13.

| Source | What it does | For Creëer |
|---|---|---|
| **Subnautica** (Unknown Worlds, 2018) | What the crosshair is on is named beside it with the action and its key or button ("Pick up · E"); the hand icon changes; building shows its progress where the player looks; refusals ("Inventory full") at the centre | **Taken:** the name, the action and its key beside the crosshair; progress and refusals where the player looks |
| **Valheim** (Iron Gate, 2021) | Hover text over the object: its name and "[E] Use"; the hammer's build menu (right click) is a grid of pieces by category; a hotbar 1–8; Esc menu with settings and rebinding. Mods most often add more hover detail (time left, fill) | **Taken:** name plus keyed actions; Esc menu with settings and controls. **Not taken:** a piece grid with ghost placement (principle 13) |
| **Space Engineers** (Keen, 2013–19) | A toolbar 1–9; G opens the block palette (categories, search); F1 lays every key over the screen; the block aimed at named with its integrity | **Taken:** a palette for creative parts (C), a controls list generated from the bindings. **Not taken:** ghost blocks, grid, symmetry |
| **Satisfactory** (Coffee Stain) | Q opens the build menu with each piece's size and cost; "[E] Interact" prompts | **Taken:** the kit shows each part's name and size |
| **Minecraft** (Mojang) | Crosshair only and an outline of the block aimed at; first-start toasts ("Move with W A S D"); Esc menu → Options → Controls, every key rebindable; F3 the debug screen, off by default | **Taken:** a short first-start hint; debug numbers behind F3, off by default; rebinding from the Esc menu. **Not taken:** outlines (`art-style.md` §5 forbids them; the name at the crosshair says what is aimed at) |
| **Factorio** (Wube) | Tooltips end in the controls that apply, named from the player's bindings ("[Mouse button 1]: Mine"); every control rebindable | **Taken:** prompts name keys from the one table, never hard-coded |
| **Teardown**, **Half-Life 2** | Minimal HUD; "E" prompts only at what can be used; the tool bar at the bottom edge | The centre stays the world's; prompts appear only when something can be done |
| **Rust**, **Astroneer** | Hold E for a radial menu of what can be done with a thing | **Not taken:** here few actions apply at once and each has its own key; a radial menu adds a step to every one |
| Fagerholt & Lorentzon, *Beyond the HUD* (Chalmers / EA DICE, 2009) | 20+ FPS games studied: the HUD's coherence with the game's theme matters more to immersion than whether it is diegetic; strengthen the player–avatar perceptual link; put information where the player looks | The HUD in the workshop palette of `art-style.md` §10; feedback at the crosshair |
| Xbox Accessibility Guideline 101 (text) | Text at least 26 px tall at 1080p on a TV; handheld at least 14, 18 recommended; players can scale it | Body text 15–17 px at 1080p at desk distance and a UI scale setting (0.5–3) for more |
| XAG 107 (input); Game Accessibility Guidelines (basic: "allow controls to be remapped") | Remapping expected of every release; remapped bindings must update hints, tips and diagrams | One table the input, the prompts and the controls list all read |

Sources: the games as shipped and their controls pages (each game's own wiki documents its keys and menus; the descriptions above are of the games, not checked page by page in this survey); publications.lib.chalmers.se/records/fulltext/111921.pdf (Fagerholt & Lorentzon 2009); learn.microsoft.com/gaming/accessibility/xbox-accessibility-guidelines/101 and /107; gameaccessibilityguidelines.com/allow-controls-to-be-remapped-reconfigured.

## Decision

### 1. One table of controls

`bindings.rs` holds every action the player can take: its id (for the settings file), its name in plain words, its group in the controls list, when it applies (always; on the planet; while a part is held, before the planet's on a key both use; the solar system), whether it lasts while held, and its default keys. `app/input.rs` turns a key, button or the wheel into the actions bound to it and does the first that applies now; `holding.rs` and `fly_camera.rs` take actions, not keys; the prompts and the controls list name keys from the same table. No other file names a key.

- **Rebinding:** an action takes one key; another action that would be active at the same time loses it (shown "unbound"). Esc (the menu) and the wheel (sliding, flying speed) cannot be moved. Ctrl is shared on purpose by the jetpack and "finer" while holding (`SHARED`).
- **Settings file:** only the player's changes, by action id: a version with new actions loads an old file, and an unknown id or key is left out and printed.
- **For stream B:** an action is added, changed or removed in `TABLE` and `Action`, done in `Running::act` (`app/input.rs`), and offered in `prompts.rs`; the controls list, rebinding and the file follow.
