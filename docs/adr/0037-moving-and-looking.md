# ADR-0037: Moving and looking, as the player meets them

- **Status:** in progress (G2.11, stream A4, 2026-11-13).
- **Context:** the user, after playing G2: "plays horrible", "many bugs" (`docs/playability-plan.md`). The character (ADR-0023) had been validated by physics tests; nobody had measured what the player's eye and hands get while moving. This ADR measures it through the real client, sets targets from human data and other games, and records the fixes.
- **Builds on:** ADR-0023 (the character: one body on a legs' strut, the walk a force at the feet), ADR-0025 (prediction and its smoothing), ADR-0027 (the gait drawn), ADR-0036 (holding).
- **Evidence:** `creeer_server/src/play/walking.rs` (the courses and the frame-by-frame harness), `play/walking_tests.rs` (the measurements, by hand), `play/walking_gates.rs` (the gates); `creeer_sim/src/character/legs.rs`, `feet.rs`; `creeer_client/src/planet_camera.rs`; frames `--planet avatar|third-crates|first-down`.

## How it was measured

A player is played over the real server and client (`play::Play`) on a course east of the spawn: level ground, logs of 10, 20 and 30 cm, a 15 cm sill, a 20 cm deck, mounds of 11–30°, a plank bridge (150 mm, *log bridge*'s) and a 0.6 m deck, each a board nailed to two sills. The client draws at 144 frames a second (the avatar between its ticks, as the game does), over a lossless link and a slow one (100–130 ms each way, 5 % lost). Per tick: the server's body (centre, velocity, lean, down or not). Per frame: the first-person eye. The eye's *jerk* is its second difference between frames (mm: how far a frame is off the straight line between its neighbours; a stutter shows as millimetres, smooth motion as hundredths).

## What the player met (2026-11-13, before)

| | Measured | People (Prior work) |
|---|---|---|
| Walking as the game walks (W: 3.0 m/s asked) | 90 % of it in 0.80 s and 1.2 m; stops in 0.77 s and 1.14 m | 3.0 m/s is a jog; people walk at 1.3–1.5 m/s |
| Running (Shift: 6.5 asked, 4.0 the most) | 90 % of 4 m/s in 0.98 s, 1.9 m; stops in 1.03 s, 2.0 m | |
| Walking at 1.4 m/s | 90 % in 0.57 s; a right angle in 0.65 s, about-turn in 0.93 s | steady speed within about two steps (1 s) |
| The eye, level ground, both links | jerks under 0.5 mm; its height steady within 1 cm walking | |
| Turning on the slow link | 3.9 mm jerks, one correction of 2.4 cm | |
| Over a 10 / 20 / 30 cm log at 1.4 m/s | the eye rose 5 / 18 / 32 cm and fell again; the body rose at up to 1.7 m/s and fell at 1.8 | the centre of mass rises a few cm stepping over an obstacle |
| Onto a 15 cm sill / a 20 cm deck | the eye rose 14 / 26 cm (6 cm over its mark), rising at 1.1–1.6 m/s | stair ascent raises the centre at 0.3–0.5 m/s |
| Over the plank bridge and the deck | knocked down stepping off the deck, and standing on it with a box | |
| Standing on 11–30° slopes | no creep; one stand at a mound's foot crept 0.3 m in 4 s | |
| The `avatar` frame (third person) | taken from inside the head | |

## Prior work

| Source | What it found or does | For Creëer |
|---|---|---|
| Bohannon, "Comfortable and maximum walking speed of adults aged 20–79 years" (*Age and Ageing* 26, 1997) | Comfortable walking 1.27–1.46 m/s, the most 1.75–2.53 m/s | A walk is about 1.4 m/s |
| Hreljac, "Preferred and energetically optimal gait transition speeds in human locomotion" (*Med. Sci. Sports Exerc.* 25, 1993); Thorstensson and Roberthson (1987) | People change from walking to running at about 2.0 m/s (1.9–2.2) | 3 m/s is a run; the gait drawn (ADR-0027) runs above Froude 0.5 (about 2.2 m/s) |
| Sekiya and Nagasaki, the walk ratio (step length over cadence, about 0.0065 m·min, 1998) | A step at 1.4 m/s is about 0.7 m | The legs look for the ground half a stride, 0.35 m, ahead and behind |
| Brenière and Do (1986); Jian, Winter et al., "Trajectory of the body COG and COP during initiation and termination of gait" (*Gait & Posture* 1, 1993) | Steady walking speed is reached within one to two steps; a stop takes about one step (Ridge et al. 2013, ADR-0023) | 0.57 s to a walk is a person's; the walk's response is kept |
| Morin and Samozino, sprint force–velocity profiles (*Int. J. Sports Physiol. Perform.* 11, 2016) | The most horizontal push 6–10 N/kg in athletes, less in others | The walk's 420 N (4 m/s² of 105 kg, 5.3 N/kg of the 80 kg person) is a person's with a 22 kg pack: kept |
| Chou, Kaufman et al., "Motion of the whole body's center of mass when stepping over obstacles of different heights" (*Gait & Posture* 13, 2001); Patla and Rietdyk, "Visuomotor coordination for obstacle avoidance" (1993) | Stepping over obstacles of up to 15 % of the stature (26 cm), people slow a little and their centre rises only a few centimetres more than in level walking: the feet step over, the body does not climb | A log under the hips is stepped over, not stood on |
| Riener, Rabuffetti and Frigo, "Stair ascent and descent at different inclinations" (*Gait & Posture* 15, 2002) | Climbing stairs, the centre of mass rises at about 0.3 m/s on the whole, at most about 0.5 m/s | A step up rises at about 0.5 m/s |
| Hirasaki, Moore, Raphan and Cohen, "Effects of walking velocity on vertical head and body movements during locomotion" (*Exp. Brain Res.* 127, 1999); Pozzo, Berthoz and Lefort, "Head stabilization during various locomotor tasks in humans" (*Exp. Brain Res.* 82, 1990) | The head bobs about 2 Hz, 18–90 mm across studies, faster walking more; the head's pitch and the eyes' reflexes keep the gaze on its target, so the seen world hardly moves | The eye goes where the body's head goes, with no bob added (a camera's bob is not what the stabilised eye sees, and sickens some players) |
| Valve, Source engine, `C_BasePlayer::SmoothViewOnStairs` (the SDK) and prediction-error smoothing (`cl_smoothtime` 0.1 s); id Software, Quake III's `CG_StepOffset` (a step's height smoothed over 200 ms) | Kinematic characters snap up steps; the view is smoothed after them (Source: at most 150 units/s, about 3.8 m/s, and never more than a step behind) and after prediction corrections | Corrections already slide over 0.1 s (ADR-0025); a step here is the legs', smoothed in the physics (below), so the view needs none of its own |
| Toyful Games, "Making a physics-based character controller in Unity" (*Very Very Valet*, 2022) | A body floating on a ray's spring above the ground, an upright torque, a walk as a capped force towards the speed asked | The same family as ADR-0023's character; their one ray climbs what passes under it too |
| Unity CharacterController (`stepOffset`), Unreal (`MaxStepHeight`, 45 cm), Jolt `CharacterVirtual` (`WalkStairs`, `StickToFloor`) | A capsule moved and snapped up and down steps | No legs: nothing to measure a person's push against; the steps' heights are a game's (a person steps 15–20 cm without thinking) |
| Epic, Unreal's `USpringArmComponent` (a sphere of 12 cm swept from the pivot, the camera put at the hit); Unity Cinemachine's collider and deoccluder (pulled in before an obstacle, damped back out) | A third-person camera never inside things: pulled in at once, let out gently | The third person's boom (below) |
| Steve Swink, *Game Feel* (2009), after Card, Moran and Newell's perceptual processor (100 ms) | A response within about 0.1 s feels immediate | The walk moves the predicted avatar in the input's own tick (2 ticks to 0.1 m/s) |

## Targets

- **The walk:** the speeds a person walks and runs at; a response a person's (90 % of a walk within about two steps), at once in the first tick.
- **The eye:** no jerk over a millimetre on level ground, either link, beyond what a correction brings; no rise over a narrow thing the feet step over beyond a few centimetres; up a step at a person's 0.5 m/s, no overshoot past a couple of centimetres.
- **Bridges:** walked on, over and off, a load in the hands or not, never knocked down for no reason; standing on them still.
- **Third person:** never inside the player's body, the ground or things.
- **Down:** the first-person eye in the head where it lies.

## Decision

### 1. The balance on a nailed part (`feet.rs`)

A structure's node has a shape for each member it joins (a nail's node: the board's and the sill's). The feet took the node's first, so on a nailed deck the face they stood on was read off the sill's: the soles' patch lay beside them, the balance's torque was carried the wrong way or not at all, and a player walking off the deck leant over at 2.8 rad/s and was knocked down (standing on it with a box, at 0.8 rad/s). Now the node's shape whose surface the ray's point lies on. Walking over both bridges and off them: never down.

### 2. Where the feet stand and how fast the legs straighten (`legs.rs`)

(Being written.)

## Measured and rejected

| Tried | Measured | Why not |
|---|---|---|

## Validation

## Limits
