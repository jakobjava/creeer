# ADR-0025: Predicting nearby bodies (G1.2e)

**Status:** accepted (2026-10-21).
**Context:** `docs/g1-plan.md`, G1.2e: the client predicts its own avatar alone (ADR-0008), so walking into a crate is learnt from the server a round trip late and shows as a correction. The gate: corrections over 1 cm ≤ 1 % of corrections.
**Builds on:** ADR-0008 (networking: snapshots, prediction, reconciliation), ADR-0023 (the character).
**Evidence:** `creeer_server/src/predicted.rs` and `predicted/nearby.rs`, their tests `predicted/tests.rs` (walking into crates, lossless and over a slow, lossy link), `creeer_net::snapshot` (`stale`).

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

## Decision

1. **Bodies' matter in their spawn.** A rigid body's spawn carries its mass, friction, restitution and surface (E, ν, rounding), and whether it may be predicted: a lone rigid body, not a joint's side (a ragdoll's), not held (a character's). Protocol 6.
2. **Which a snapshot left out.** A body missing from a snapshot's bytes was either unchanged (the client's copy current) or left out by the budget (its copy behind), and the client could not tell. Of the bodies within `predict::NEARBY` (6 m) of the client's avatar, the server lists those the budget left out (`Snapshot::stale`), their room kept in the budget.
3. **Set back only where mispredicted.** The client keeps its prediction after each input (the avatar's and the bubble's motions). A snapshot that agrees with it at its input, within twice the quanta (positions 2/1024 m, velocities and spins 2/256, a milliradian), with the same bodies near, changes nothing. Chaos resimulates only on a misprediction too.
4. **The bubble.** Otherwise the prediction world is built afresh: the avatar from the snapshot, and the nearest predictable bodies within 0.95 × `NEARBY` of it (at most `MOST_NEARBY`, 12), each from the snapshot, or for one it listed as stale from the client's own prediction at that input. Bodies asleep there sleep in it (`Body::sleep_now`) and wake when touched. The inputs not yet applied are replayed on all of it.
5. **Drawn.** Bodies in the bubble are drawn as predicted. One coming in slides from where it was interpolated, one going out from where it was predicted to where it is interpolated, and each correction slides, all over the avatar's smoothing time.
6. **Not predicted:** other characters (their inputs are their own) and jointed bodies (ragdolls: the client has no joints).

## Validation (`predicted/tests.rs`, `server/play_tests.rs`)

| Scene | Before | Now |
|---|---|---|
| Walking into six crates, lossless | — | 1 of 163 over 1 cm (0.6 %, gate ≤ 1 %), the largest 1.8 cm |
| Walking into six crates, 100–130 ms, 5 % loss | 65 of 148 (44 %), the largest 38 cm | 5 of 148 (3.4 %), the largest 5.5 cm |
| Walking and jumping on bare terrain, same link | 4 of 304, the largest 1.4 cm | none over 1 cm, the largest 1 mm |

On bare terrain the gain is the set-back's: setting the avatar to the quantised state at every snapshot was itself a correction of up to a millimetre or two each time. CI gates the first two rows (lossless ≤ 1 % and < 3 cm; slow and lossy ≤ 5 % and < 10 cm) and the third (≤ 1 % and < 2 cm, tightened from 5 % and 10 cm). Crates pushed and come to rest are drawn within 1 cm of where the server rests them.

## Cost

The client's time setting back and replaying, per snapshot (about 30 a second), walking into the crates: 0.36 ms on a lossless link, 1.95 ms on the slow one (5.3 ms when it set back at every snapshot); on bare terrain 0.01 ms. A mispredicted snapshot over a slow link replays about fifteen inputs of the avatar and the crates it pushes: a few milliseconds on the client's main thread.

## Limits (also in `docs/future-improvements.md`)

- Other players' characters and ragdolls are not predicted: walking into another player is corrected a round trip late.
- A body at the bubble's edge, or pushed by something outside it, is predicted as if nothing outside touched it, until the server says otherwise.
- The replay's cost grows with the round trip and with the bodies pushed: about 2 ms a snapshot averaged over the slow link, a few on a misprediction.
- Walking into crates on level ground mispredicts more than on a slope (the character's box meets them face to face), and replays cost more for it: 4.5–7.4 ms a reconciliation averaged over the slow link at the spawn (G1.3i, amended below).

## Amended in G1.3i (2026-10-22): gated over an ensemble

Moving the spawn to level ground (ADR-0023, amended) failed the lossless gate: 3 of 163 corrections over 1 cm. Walking into the crates is chaotic: moving the row by a centimetre gave anywhere from 3 to 26, so one placement was one sample, and the old gate had passed on it by luck. The prior work is the climate modellers': Baker et al., "A new ensemble-based consistency test for the Community Earth System Model" ([GMD 2015](https://gmd.copernicus.org/articles/8/2829/2015/)), judge a chaotic model by an ensemble of runs, not one. The gates are now over eight placements of the row (0.56–0.63 m apart, 3.00–3.14 m ahead): the median and the worst share of corrections over 1 cm, and the largest correction, a tenth above what was measured.

| Eight placements, corrections over 1 cm (largest) | Lossless, of 163 | Slow and lossy, of 145–149 |
|---|---|---|
| Before G1.3i, the spawn by the pole | 0–1, median 0.5 (1.6 cm) | 3–10, median 6.5 (5.1 cm) |
| The plumb line and the avatar's sleep, by the pole | 0–2, median 0 (1.8 cm) | 0–6, median 3.5 (5.8 cm) |
| **The spawn moved to level ground (gated)** | **0–4, median 2 (2.9 cm)** | **5–25, median 16 (12.8 cm)** |
| The same, the crates turned 0.4 rad about the vertical | 0–1, median 0 (2.0 cm) | 1–11, median 6.5 (12.5 cm) |

Gates: lossless median ≤ 1.4 %, worst ≤ 2.7 %, largest < 3.2 cm; slow and lossy median ≤ 12 %, worst ≤ 19 %, largest < 14 cm. The changes of this amendment predict better where they were measured before; the level ground predicts worse. Its cause, measured: on level ground the character's box (it never turns) meets the upright crates face to face, a contact whose force a quantum of tilt redistributes; on the old slope the crates leaned, and turned crates predict as well as there. Measured and rejected: snapshots 4 to 64 times finer (still 3–4 of 8 placements over the old gate: the divergence grows from any difference), crates rebuilt at the server's exact rest pose (no change), the body as a 16-sided bevelled prism (worse: 0–9 lossless, 20–38 slow). A character whose contacts with what it pushes are well conditioned waits for G1.8's contacts (`docs/future-improvements.md`).

The client's replay cost over the slow link rose with the mispredictions: 4.5–7.4 ms a reconciliation averaged over the walk at the new spawn, against 2–4 ms by the pole (Limits).
