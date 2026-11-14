# ADR-0042: Moving and looking, as the player meets them

- **Status:** Built (G2.11 stream A4, 2026-11-13/14): decisions 1, 2, 4 and 5 in; two limits found and left open, measured (logs climbed, decision 2; the soles giving way standing, decision 3); the speeds the user's to choose. Waiting on the user's play.
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
| First person stooping to what the hands hold (stream B's own body, 2026-11-14) | the camera at the standing eyes looked down on one's own head, shoulders and back | people see their arms and knees |
| *bridge_demo* (A6's and C's measurements) | the first person stepping onto the nailed deck pulled two nails out and broke it; 947,754 Newton iterations in a tick | a person presses a deck with about their weight, footfalls 1.1–1.5 body weights |
| Holding a post through its 8 s of backfilling (ADR-0036) | its holder slid back 0.3 m | a person stands |

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

### 2. How fast the legs straighten onto a body (`legs.rs`)

The legs are one ray down from the hips: whatever passes under the hips is stood on at once, and the legs' spring finds itself pressed by its whole height and pushes with up to its 3,600 N (3.5 body weights). The body popped onto every deck at 1.6 m/s and 6 cm over its mark, and pressed the deck it stepped onto with 2.79 body weights for a tick or two (changing at 84 body weights a second). On *bridge_demo*'s nailed deck that pulled two nails out (1,240 and 1,337 N against their 706 N: A6's measurement), the stringers broke within four ticks, and the pile of pieces with the person in it froze the game (947,754 Newton iterations in one tick).

- **Onto a body of half a person or more** (50 kg: a deck, a plank bridge with its sills, a big log, a crate) found suddenly under them, the legs press at most 3.5 cm beyond what the weight presses them, so the body rises at about k·3.5 cm/c, 0.49 m/s (0.42 measured), a person's pace stepping up (Riener et al.), and not over its mark. Coming down onto them (a landing, v down) they are met as far as they are pressed, up to v·0.5 s more: a landing is not softened.
- **The ground and light bodies as before.** On the fixed ground and on a lighter body (a pole, a box, a ragdoll's limb) the legs are ADR-0023's, bit for bit (the walk's cuts too): steps in the terrain are rare and its slopes are walked as before, and a person's weight presses a light body into what is under it. Every gate that has no such body stood on plays as before.

| Measured (1.4 m/s) | Before | After |
|---|---|---|
| Onto a 20 cm deck: the body's rise; the eye over its mark | 1.57 m/s; 6 cm | 0.42 m/s; 2.6 cm |
| The legs' push stepping onto a nailed deck / walking along / standing | 2.79 / 1.20 / 1.00 body weights | 1.51 / 1.04 / 1.00 |
| Its change stepping on | 84 body weights a second | 28 |
| *bridge_demo*'s footbridge, the first person stepping onto it (`walking_loads.rs`, `measure_the_footbridges_nails`) | two nails pulled out (1,240 and 1,337 N against their 706 N: A6), broken within four ticks | the most pulled nail 603 N (0.91 of what pulls it out), whole |
| The same, walking over it | (broken) | nails pulled 100–520 N; broken 4 s on by a person falling through its deck (below) |
| Over the plank bridge and the deck with a 15 kg box | knocked down stepping onto the deck | keeps her feet, the box held |

(The first pass, the cap on everything over 20 kg, had the footbridge whole with people walking over and back, at most 420 Newton iterations a tick: C's `what_the_bridge_costs`. Measured again on G2.11's merged state with the law as built, 2026-11-15: as in the table.)

**Found, not mended: falling through a deck's gaps.** *bridge_demo*'s deck boards are 150 mm wide every 250 mm: a tenth of a metre between them, two-fifths of the deck. The one ray under the hips passes through a gap: over a stringer it finds the stringer's top 5 cm down, so a person walking along a stringer bobs at every board (±0.3–0.7 m/s) and climbs each; between stringers (75 mm wide, 0.4 m apart) it finds nothing. Walking, the next board comes within a few ticks; stopping with the hips over a gap between stringers, the body falls freely, 0.37 m in 0.3 s, until its own shape strikes the deck at 2.6–2.9 m/s, and the struck stringers break (tick 580, 4.7 s after the walk began; the pieces then cost 0.2–1.7 million Newton iterations a tick). With A6's held-back wood patch (ADR-0005) the first step pulls a nail with 496 N (0.94), two members break in the first steps, and the whole deck breaks struck at tick 567, both people falling into the gully. A foot is 0.26 m long and spans such a gap. The way: the feet's own footprint, not one point (`docs/future-improvements.md`, *One leg*).

**Found, not mended: logs and sills are climbed.** With one ray under the hips a 10, 20 or 30 cm log lifts the eye 5, 18 or 32 cm as it passes under them, and a 15 cm sill 14 cm, the body hopping at up to 1.8 m/s. Five rays along the way (under the hips, a quarter and half a stride ahead and behind) and their middle by height stepped over every one (the eye 0.3–0.7 cm up, never slowed), as people step over obstacles (Chou et al.); it moved the prediction's gate against crates over a slow link (Measured and rejected), and is left with its test ignored (`stepping_gates.rs`).

### 3. Found, not mended: standing, the soles give way

Standing, the walk is a damper of 420 N·s/m towards no speed, put on as a load: any steady push moves the body at push/420. A holder pulled 14 N by the post she held creeps 3 cm/s and slides back 0.3 m through its 8 s of backfilling (ADR-0036's unexplained limit, now explained); on a mound's foot one stand crept 0.3 m in 4 s. People's soles hold by friction, and what the feet resist without stepping is the weight's moment over the feet's front (the ankle strategy, Winter 1995: about m·g·0.2 m over the centre's 1 m, 100 N). Soles holding the body where it stands (a damper of 63,000 N·s/m solved in the step, within 100 N and the walk's damper at the step's speed, within the friction) mended it (the holder within 2 cm), but moved B's building tests and the prediction's gates: Measured and rejected below. The test stands, ignored with its reason (`carry_walk_tests.rs`).

### 4. A ragdoll laid out clear of what struck it (`ragdoll.rs`)

A ragdoll's segments were laid out where the body stood, cleared of the ground only: a felled player's head lay 13 cm inside the crate that hit him (A6, G2.11), and the contacts threw it out (now pushed out). Each segment's deepest corner in another body is now put 1 mm clear of it along the way out, as of the ground.

### Not changed: the speeds and the walk's response

- **The speeds are a choice for the user.** W asks 3.0 m/s (a jog: people walk at 1.3–1.5 m/s and change to running at about 2.0, so the drawn gait runs); Shift asks 6.5 and the body runs at its most, 4.0 m/s (a run with a 22 kg pack, Knapik's load carriage). Games' walks are faster than people's (Half-Life 2's 190 units/s is 3.6 m/s); a realistic walk would be 1.4 m/s with W and a run with Shift. Left as it is, to be asked.
- **The walk's response is a person's.** 90 % of a 1.4 m/s walk in 0.57 s (within two steps), a right angle in 0.65 s, moving within two ticks of the key; its push at most 420 N (4 m/s² of 105 kg, 5.3 N/kg of the person: within sprinters' and others' horizontal force), braking at the same until stopped (from 3 m/s in 0.77 s and 1.14 m). Kept.

### 5. The cameras (`creeer_client`'s `planet_camera.rs`)

- **Third person on a boom.** From the eyes to where the camera wants to be (4.5 m behind the avatar's centre along the view, 1.2 m above it), five rays (the boom's and four 0.2 m round it, so the near plane stays clear) against bodies, the terrain and trees; pulled in at once before what is between, let back out over 0.3 s; nearer the eyes than 0.7 m the player's own body is not drawn. (Before: kept 1.5 m above the ground and nothing else; behind crates the player was hidden, and the `avatar` frame was taken from inside the head.)
- **First person, standing:** the eyes where the drawn head's are (stream B draws the body from within, its head sunk): 9 cm before the head's centre, followed over 0.08 s, a drawn head more than a metre off not believed. The drawn body turns to face where the eyes look beyond 30°. Placing aims from those eyes (`Connection::eyes`), so the crosshair is where the part goes. (Left at the standing eyes, the frames looked down on the top of one's own head, shoulders and back as the body stooped to what it held.)
- **First person, down:** at the ragdoll's head where it lies, the own body not drawn. (Before: 1.5 m above the pelvis.) No lifting off the ground: the eyes are where the head is.
- **No bob added.** The eye goes where the body's head goes, the hips' own bob of the drawn walk with it; the view's turn is the mouse's alone, so the far world stays still, as a gaze keeps it (Hirasaki et al.; Pozzo et al.).

## Measured and rejected

| Tried | Measured | Why not |
|---|---|---|
| First person's eyes brought to the drawn head's only along the line of sight (the crosshair's ray unchanged) | Stooped far forward, the camera stayed 0.26 m behind and 0.12 m above the drawn eyes: the frames still showed one's own head and shoulders from above (B's *bridge* 26) | The eyes go where the drawn head's are, and placing aims from there |
| The eyes at the drawn head's centre | Looking down standing, the camera looked into the collar | 9 cm before it, where a face's eyes are |
| The legs' extra push stepping up capped at 1.5 cm (1.44 body weights at once) | Rises at 0.21 m/s: a 20 cm deck took a second | 3.5 cm: 1.51 measured, within people's footfalls |
| The cap on every body, light ones too | *builders* at CI's length (debug): its third player on the crossed poles at the start took 1,504 Newton iterations a tick against 780; a client's prediction 1,533–1,624 at most (ceiling 1,294), 138,000 in all against 86,000 before A4 | Light bodies as before (below 20 kg): 1,031 at most, a client's 652, 50,148 and 64,558 in all (72,280 and 86,074 before A4) |
| The cap on the fixed ground too, with the slope's rise under the hips as the body goes taken as the damper's (so a body running up a slope would not sink into it) | On bodies, a pole's round top tilted its normal from one millimetre to the next: client and server pushed differently, a client's 106,000–138,000 Newton iterations in all in *builders*; on the ground alone, *walking_into_crates_is_predicted*'s slow link over its gate (median share over 1 cm 0.17 against 0.156) and *work*'s client 156–162 (ceiling 150) | The fixed ground as before; the cap on bodies stepped onto |
| The walk's cuts by `clamp_length_max` (a rewrite) | Its rounding differs from `normalize() * most`: *builders*' tick 225 2,060 Newton iterations (ceiling 1,134), the scene's chaos | Cut as before, bit for bit |
| Standing, the soles holding the body where it stands (decision 3), on every body | *builders*' tick 5 (a player landing on a 2.5 kg pole) 1,152 Newton iterations (ceiling 1,134) | Not on light bodies, then not at all (next row) |
| The same on the ground and on what is stepped onto | The holder through the backfilling within 2 cm, but B's `two_posts_set_in_their_holes_…` held its post 40° off upright (the hands and a body that no longer gives), *builders* 1,227 at tick 219, *walking_into_crates_is_predicted*'s slow link over its gate | Not taken: the limit stays open (the hands' pull and the planted body need to be tuned together, with stream B) |
| Five rays along the way walking, their middle by height (logs and sills stepped over) | *walking_into_crates_is_predicted*'s slow link: its worst placement 0.25 of corrections over 1 cm (gate 0.19), median 0.149–0.162 (0.156); leaving out what the rays meet moving faster than 0.3 m/s (the crates pushed) changed nothing | Not taken: logs and sills are climbed (decision 2, open); a gait's own feet, predicted, are the way (`docs/future-improvements.md`, *One leg*) |
| The cap on bodies over 20 kg | Stream B's 27 kg poles laid across a gap were stood on and turned 27–42° off the way across (`two_poles_carried_and_laid_across_a_gap`, gate 20°) | Half a person, 50 kg |

## Validation

- `feet_tests.rs`: on a node of two members the one stood on carries the balance whole.
- `play/walking_gates.rs`: over a plank bridge and a deck, on, along and off, with and without a 15 kg box: never down, the box held.
- `play/stepping_gates.rs`: a 20 cm step at under 0.6 m/s, the eye under 4 cm over; a nailed deck pressed with at most 1.7 body weights stepping on, 1.3 walking, its weight standing; logs and a sill stepped over (ignored: the open limit).
- `play/walking_loads.rs` (by hand): the legs' push on a deck, what a person costs it, and `measure_the_footbridges_nails` (*bridge_demo*'s nails under people walking over, in release; `TRACE=<tick>` follows each person).
- `ragdoll_tests.rs`: a ragdoll laid out clear of a crate 13 cm into where its head stood. `server/carry_walk_tests.rs`: holding a post through its backfilling (ignored: the open limit).
- `planet_camera.rs`: the boom out in the open, pulled in before a wall at once and let out without a jump, the own body hidden at the eyes; the eyes following a stooping head and not one 300,000 km off.
- Frames (`<frames>/walk`): `avatar`, `third-crates`, `first-down` before and after; B's `--build-play bridge` after.

## Limits

- Logs, sills and post holes under the hips are climbed or fallen into, not stepped over; a gap narrower than a foot (between deck boards) is fallen through (decision 2).
- Standing, any steady push moves the body (decision 3).
- Stepping down off a deck the body still drops its height freely (2.13 body weights landing on the ground); a descent braked on the trailing foot is not modelled.
- Walking, the legs press steadily with the body's weight: a gait's two footfalls a stride (1.1–1.5 body weights each) are not in the physics.
- Looking straight down standing, the collar's opening shows the sunk head's stretched neck (B's drawing).
