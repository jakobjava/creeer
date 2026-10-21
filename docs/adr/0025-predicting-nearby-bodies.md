# ADR-0025: Predicting nearby bodies (G1.2e)

**Status:** in progress (2026-10-21).
**Context:** `docs/g1-plan.md`, G1.2e: the client predicts its own avatar alone (ADR-0008), so walking into a crate is learnt from the server a round trip late and shows as a correction. The gate: corrections over 1 cm ≤ 1 % of corrections.
**Builds on:** ADR-0008 (networking: snapshots, prediction, reconciliation), ADR-0023 (the character).
**Evidence:** `creeer_server/src/server/predict_tests.rs` (walking into crates over a slow, lossy link), `predicted.rs`, `creeer_net::snapshot`.

## Measured first

Over 100–130 ms each way with one packet in twenty lost (`predict_tests.rs`, `play_tests.rs`):

| Scene | Corrections | Over 1 cm | Largest |
|---|---|---|---|
| Walking and jumping on bare terrain | 304 | 4 (1.3 %) | 1.4 cm |
| Walking into a row of six crates | 148 | 65 (44 %) | 38 cm |

The avatar alone is nearly within the gate already; what it touches is not predicted, and that is where the corrections are.

## Prior work

| Work | What it does | What it means here |
|---|---|---|
| Unity Netcode for Entities, prediction switching ([docs](https://docs.unity3d.com/Packages/com.unity.netcode@1.3/manual/prediction-switching.html)) | Most objects interpolated (a ping behind the server); those near the local player (what it collides with, what it triggers) switched to predicted (a ping ahead). A switch jumps an object by up to twice the ping between the timelines, so its pose is smoothed over a set time; prediction's cost grows with every predicted object | Taken: a bubble of predicted bodies about the avatar, the rest interpolated, a smoothed switch |
| Unreal Chaos networked physics, resimulation ([docs](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/Chaos/FRewindData/OverlapsAnyResimBubble)) | Client-predicted, server-authoritative physics: on a correction, rewind to the corrected frame (from per-frame history, `FRewindData`) and replay the stored inputs; the "resim bubble" re-simulates only the particles whose state the correction could change | Taken: rewind and replay the bubble together with the avatar; a per-input history for the bodies a snapshot did not refresh |
| Rocket League (Psyonix, Jared Cone, GDC 2018, ["It IS Rocket Science"](https://gamedeveloper.com/design/video-the-rocket-science-behind-i-rocket-league-s-i-physics)) | Server-authoritative physics with client prediction by re-simulation (Bullet, fixed rate) | The same approach shipped at scale; its details not in the sources found |
| Glenn Fiedler, *Networked Physics* (gafferongames.com, 2014–2018) | State synchronisation with priorities; in shared-authority schemes, objects the player touches are taken over by that player | Not taken: the server stays the authority (ADR-0008) |

## Decision (to be measured before it is kept)

1. **Bodies' matter in their spawn.** A rigid body's spawn carries its mass, friction, restitution and surface (E, ν, rounding), and whether it may be predicted (a lone rigid body: not a joint's side, not a character). Protocol 6.
2. **Which a snapshot refreshed.** The client's snapshot keeps the entities its bytes carried; the others hold the state of an earlier snapshot (the server's budget, `creeer_net::snapshot`).
3. **The bubble.** At each reconciliation the prediction world is built afresh: the avatar from the snapshot, and the nearest predictable bodies within a few metres of it (at most a dozen), each from the snapshot where it refreshed it, else from the client's own prediction at the same input (a history by input), else from the snapshot. Bodies asleep there sleep in it (and wake when touched, as on the server). The inputs the server has not applied are replayed on all of it.
4. **Drawn.** Bodies in the bubble are drawn as predicted; a body entering or leaving it slides from where it was drawn over the avatar's smoothing time (`creeer_net::predict::Smoothing`), the rest are interpolated as before.
5. **Other characters and jointed bodies** (ragdolls) are not predicted: their inputs and joints are the server's.

## Validation

To come: walking into crates within the gate (corrections over 1 cm ≤ 1 % on a lossless link; measured over the slow, lossy one); the existing prediction tests unchanged; the client's cost of re-simulating its bubble measured.
