# ADR-0043: A client's frame: what its prediction costs, and what it need not do

- **Status:** Accepted (G2.11 stream A5, 2026-11-14; `docs/playability-plan.md`).
- **Context:** the user: the game "plays horrible"; "I really want to keep this game as performant as possible". What a player feels is their own frame, and only the server's tick was gated. Measured in G2.10 (`play::client_probe`): a client's prediction took 44–50 ms a frame in *builders* within CI's 600 ticks (66 and 85 ms at full length), and a fastening made cost *log_bridge*'s clients 382 Newton iterations at once. A frame at 60 Hz is 16.7 ms for everything.
- **Builds on:** ADR-0008 (snapshots, prediction, reconciliation), ADR-0025 (the bodies near the avatar predicted with it; their choice by reach, G2.10), ADR-0021 (the clients' work measured and gated: `Tick::client_work`, `CLIENT_WORK`), ADR-0036 (holding by hand), ADR-0039 (the bridge site).
- **Evidence:** `creeer_server`'s `world/building_parts.rs` (`make_built`: a part made a structure told anew); `predicted/rewind.rs` (setting back, as before, each rewind counted with why and by how much); the probes `predicted/probe_tests.rs` (`client_frames`, `a_lone_avatar`); the bench `client/bridge_bench.rs` and its gate `client/bridge_bench_tests.rs`; `creeer_client --play-bench bridge` (the whole frame, drawn). Tests: `client/told_tests.rs`, `predicted/tests.rs`, `server/held_view_tests.rs`, `server/build_flow_tests.rs`, `server/build_play_tests.rs`, `play/tests.rs` (`CLIENT_WORK`).

## Measured first

The probe `client_frames` (`SCENE=<scene> TICKS=<n> cargo test --release -p creeer_server --lib client_frames -- --ignored --nocapture`) plays a scene in lockstep and takes, per client and tick: the prediction's Newton iterations and those of its replays; whether each snapshot agreed or was rewound, and why (the avatar off, a body off, the bodies chosen changed); by how much and in what (position, velocity, spin, orientation); the bodies predicted and how many were awake. What it found, in order of cost:

1. **A fastening made was never told to the clients.** Parts fastened become one structure under the first part's entity (`fasten`, `make_built`); a client learnt an entity's kind only when it came into view, or when a hand let go of it. Fastened while nobody held them (every play scene's builders; in the game, the part aimed at while the other is held) the clients went on drawing the structure as the lone rigid part it had been, at its node 0, and predicting it so: its partner gone, it fell where the partner had been and disagreed with every snapshot. *log_bridge*: 382 Newton iterations at the nails, then a rewind at every snapshot for two seconds (121 a tick on average over ticks 400–500); *builders*: the same at each of its sixteen nailings, and a player's avatar corrected by up to 1.14 m where it stood on a pole drawn as the lone pole it had been. The player saw the part jump half its length and its partner vanish.
2. **The avatar alone is never mispredicted** (`a_lone_avatar`: standing, walking, running, stopping, jumping every 3 s, lossless: no rewind but the two at joining). In contact it is: walking into a crate, 119 of 150 snapshots rewound. Of all rewinds in *builders*, *crowd* and *work*, nine in ten were for errors under 5 mm, the avatar off in velocity, spin or orientation by a little; each replayed every body near it.
3. **Bodies still on the server rattle on the client.** A body the server holds still but awake (*builders*' crossed poles, *log_bridge*'s stacked sills and plank) is set to the snapshot's quantised pose (up to half a millimetre and 20 µrad off where it rests), stepped from a reset history and moved by millimetres a second: the next snapshot disagrees again.
4. **What remains is the physics of what the avatar touches.** *builders*' worst tick (1,176 iterations at tick 211) is a pole kicked by its player's feet, spinning at 33 rad/s: 681 iterations in the input's own step, as on the server, and its replay on top.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Unreal Engine's networked physics (Chaos, 5.4 on: `FRewindData`, `NetworkPhysicsSettingsResimulation`) | Keeps every physics frame's state for rewinds; resimulates only once an object drifts past per-object thresholds of position, rotation, velocity and angular velocity, and within them applies a *runtime correction* towards the server's state without resimulating (`np2.Resim.RuntimeCorrectionEnabled`, `RuntimeVelocityCorrection`, stability multipliers) | Both measured and rejected here (below): the kept frames predicted contacts worse; the runtime correction changed what a holding player's client sends |
| Valve's Source engine, `cl_pred_optimize 2` | No re-prediction where the server's update shows no prediction error | ADR-0025 already does it; kept |
| Glenn Fiedler, "Networked Physics in Virtual Reality" (2018) | Cubes at rest jittered after quantisation (quantised positions just above a surface fall back; quantisation pushes cubes into each other); cured by quantising both sides, limiting depenetration speed, and forcing a cube to rest once it has not moved for 16 frames | Forcing bodies the server has still to rest on the client was measured and rejected (below). Quantising the server's own state is not open: the server's results may not change |
| Fiedler, "State Synchronization" (2015) | An at-rest flag per body; at-rest bodies cost no velocity bits | The snapshot's `asleep` flag (ADR-0008) |
| Photon Quantum, prediction culling | Entities outside a prediction area are not simulated in predicted frames, only in verified ones | ADR-0025's bubble by reach, as before |
| Unity Netcode for Entities (physics, prediction switching) | Interpolated ghosts are kinematic in the client's physics world; only predicted ghosts are simulated; a switch is smoothed | As ADR-0025; nothing further taken |

## Decision

1. **A part made a structure is told anew** (`make_built` marks its entity changed unless it is in someone's hands: every client is sent its despawn and its spawn as a structure; one in hands is told as the hands let go of it, as before). Two lines in stream B's `building_parts.rs`; the server's world and every play and stress hash are unchanged.
2. **The prediction is set back as before** (ADR-0025): nothing where it agrees, else rebuilt from the snapshot and the inputs since replayed. Nothing the player can touch is interpolated instead of predicted: bumping into things is predicted as before, and nothing the player sees changes but the fastened part drawn as what it is.
3. **Measured and gated** (`client/bridge_bench.rs`): at the bridge site with its footbridge built (ADR-0039), one builder through her own connection walks among the spare timber, takes a handrail post, carries it before her eyes to the bridge's near end, holds it there and puts it down; her prediction's Newton iterations a tick are gated, worst and 99th percentile, lossless and over the slow link. `creeer_client --play-bench bridge` plays the same with the frame drawn, phase by phase. The rewinds are counted with why and by how much (`Corrections`), for the probes. *log_bridge*'s `CLIENT_WORK` gate comes down to what was reached.

## Measured and rejected

| Tried | Measured | Why not |
|---|---|---|
| Rewinding from the client's own world as it was after the snapshot's input, kept for each input pending (Chaos' `FRewindData`): the bodies that agree keep their own state, history, contacts and friction anchors; those that do not are set to the server's | *builders*: rewinds 1,466 → 1,452, its mean work up (34 → 40 iterations a client and tick), p99 306 → 388. Walking into crates, lossless, the worst placement 17 corrections over 1 cm of 163 (gate 10); slow, 31 of 149 (gate 28), the largest 15 cm (gate 14 cm); rebuilt from the snapshot, 8 of 163 and 25 of 149 | The kept anchors and contacts belong to the client's own diverged contact history: mixed with corrected states they predicted the crates worse, and not more often right |
| Small mispredictions moved in place without a replay (Chaos' runtime correction): every body off by at most 5 mm (its velocity's error carried over the inputs pending included), 5 mrad, 5 cm/s, 50 mrad/s moved by its difference as uniform motion, its solver's history moved with it (a new `NodeState::moved_to` in `creeer_physics`), nothing replayed | Cut rewinds by a fifth (*builders* 1,563 → 1,243, 247 moved in place) but not the work's worst (a kicked pole). Over the slow link it corrected walking into crates by up to 17 cm at once (9 cm replayed; gate 14 cm): limited to 4 inputs pending. Then, on a good link, a holding player's client sent other holds: the first of two posts held in its hole stood 65° off upright (gate 18°, `build_flow_tests.rs`); with the forced rest below, a set post's nodes went NaN on the server | The hands are steered relative to the predicted avatar: an avatar moved in place moves what the hands are asked to hold. Two failures: dropped, the simpler way kept (rule 7) |
| Bodies the server has still (asleep, or awake with no motion its quanta show: under about 2 mm/s and 2 mrad/s) put to sleep on the client (Fiedler's forced rest) | Cut the rattle at rest. But walking into crates' median share over 1 cm went to 1.53 % (gate 1.4 %), *build_play*'s second pole was laid 21.9° off the way across (gate 20°) | A body held asleep at the server's quantised pose is met by the player's aim and hands where the server's is not: what the client sends differs, and the server's results with it |
| The history moved with a correction at every rewind, not reset | *builders* 1,466 → 1,455 rewinds; within the crates' ensemble's spread | No gain |
| A builder's bench on a site of its own (a ditch, two sills, a plank, six poles; the pole nailed across, the plank nailed, crates on it) | Its nails were never made under building by hand's blows (stream B's G2.11), and over the slow link the builder walked into the ditch and fell | Built on the bridge site and its built footbridge (ADR-0039) instead, without nailing or loading (*log_bridge* and *builders* hold those) |

A set post's nodes going NaN on the server because of what a client asked of the hands is the server's to refuse (streams A6 and B): whatever a client sends, the server's physics must stay finite.

## Validation

- **Told anew** (`client/told_tests.rs`, the user's case as the scenes make it): two poles crossed, nailed by a command while nobody holds them; the client has the survivor as a structure (it had the lone pole's box), the other gone, the structure drawn within a centimetre of where the server has it and not predicted as a lone body. Without the fix: `Discriminant(0)`, the rigid body.
- **Prediction quality, unchanged gates** (all pass): walking into crates over the eight placements, lossless and over 100–130 ms with 5 % lost (`predicted/tests.rs`); the stick held in first person (`held_view_tests.rs`); over a slow lossy link the avatar rarely corrected (`play_tests.rs`); building as the player builds (`build_flow_tests.rs`, `build_play_tests.rs`).
- **Server unchanged:** every play scene's hash at CI's length checked (`creeer_tools play --short --check`).

## Cost

Per client and tick, lockstep at CI's 600 ticks (`client_frames`; Newton iterations, deterministic; *before* is this branch's g2 base with the change switched off, measured on the same binary):

| Scene | Mean before → after | p99 | Worst | Rewinds | Avatar corrections over 1 cm (largest) |
|---|---|---|---|---|---|
| *builders* (8 players, loose poles, nailing) | 62.1 → 35.9 | 599 → 306 | 1,176 → 1,176 | 1,563 → 1,496 | 64 (1.14 m) → 4 (1.9 cm) |
| *log_bridge* (nailing a stringer, beside it) | 45.4 → 13.7 | 349 → 92 | 393 → 184 | 254 → 200 | 51 (1.3 cm) → 51 (1.3 cm) |
| *crowd* | 6.84 → 6.84 | 53 → 53 | 123 → 123 | 868 → 868 | 19 → 19 |
| *work* | 12.2 → 12.2 | 86 → 86 | 136 → 136 | 1,191 → 1,191 | 4 → 4 |
| the bridge site's builder, lossless | worst 47, p99 47 (alike before and after) | | | | |
| the bridge site's builder, slow link | worst 48, p99 44, 20 inputs replayed at most (alike) | | | | |

*builders*' 1.14 m correction was a player standing where a fastened pole was drawn as the lone pole it had been. *log_bridge*'s 51 corrections over 1 cm are its builders holding still while nailing: the hands' pull on the body is not predicted (ADR-0036, Limits). In time, lockstep on a loaded machine (seven other streams compiling), the most of eight clients a tick in *builders*: p99 180 → 110 ms and the median 8–15 ms, swinging by half between runs: to be measured again on a quiet machine.

**The whole frame** (`creeer_client --play-bench bridge --seconds 25`, drawn at 1600 × 900, the server hosted on its own thread, on the loaded machine, with the in-place moves of the table above still in; `<frames>/frame/after-play-bench-bridge.txt`): at the median the player's acts (aims and placing) took 0.01 ms, the session (the prediction) 0.26, the terrain 1.9, the snapshot drawn 0.9, the scene with the people animated 0.2, the crosshair's aim 0.3, the render 15; at worst 2, 163, 6, 12, 20, 1.8 and 143 ms. The session's worst came walking among the timber in the first seconds, when this machine's hosted server ran behind and every rewind replayed many inputs (1,665 inputs in 190 rewinds over the run); from 9 s the hosted server took 0.2–1.7 s a tick beside the built bridge (stream A6), and the client's frames went on (the session under 8 ms in most seconds) while its render took 15 ms at the median (stream A1's). All of these times are to be measured again on a quiet machine.

## Limits (also in `docs/future-improvements.md`)

- The client steps what its avatar touches as the server does: a pole kicked by the feet costs the client its own step (681 Newton iterations in one, *builders*) and its replays. The cure is the physics' (stream A6), not the prediction's.
- A body in contact with the avatar is mispredicted within a snapshot or two (contact is stiff, and the client has neither the server's friction anchors nor other players' bodies), and each misprediction, however small, replays every body near the avatar.
- Bodies the server holds still but awake rattle on the client and are rewound.
- When the hosted server falls behind (a slow tick), the client's inputs pending grow and every rewind replays them all: on the loaded development machine, walking among the timber, 1,665 inputs replayed in 190 rewinds over 25 s.
