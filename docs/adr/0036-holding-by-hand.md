# ADR-0036: Holding by hand: a firm grip, steered by the player

- **Status:** In progress (2026-10-28). The grip, the server's holding, the controls and the drawing are built: the hands drawn where they hold (the arms reaching them, the fingers closed, the body stooping to what is low) and the player's own load drawn where it is steered (decision 5). Waiting on the user's play.
- **Context:** the user's play of G2 (2026-10-28): "this current carrying/hold/grabbing system is absolutely horrible: things float, are not properly grabbed by the hand, start spinning for some reason, go through the player"; building "completely unintuitive, weird, slow (… it is difficult to get the things where you want them)". G2 cannot close until it is fixed. The user's aim: "a realistic system which does not make it hard just because the controls are bad; humans are beings which can manipulate things in space very well if the object is not too heavy". Chosen (2026-10-28): held firmly in the hand; for building, the part held where the player wants it and fastened while held; the key map below.
- **Replaces:** ADR-0027's G1.4f compliant hands (and its G2.9 amendment, raising by one end: now the grip's torque limit).
- **Builds on:** ADR-0023 (struts: capped springs solved with their bodies), ADR-0022 (joints; moved points' work booked), ADR-0024 (the energy guard's books).

## Prior work

- **Half-Life 2** (Source SDK `weapon_physcannon.cpp`, `hl2_player.cpp`): the held object is driven by a velocity controller towards a target pose (at most 1000 in/s, 3600 °/s); it is dropped only on a sustained error (after 1 s, beyond 12 in), never at a limit; hand pickup to 35 kg; the hold distance keeps it in front of the player. It sets the held object's mass to 1 kg so it cannot push the player: not taken (the reaction is real here).
- **Garry's Mod physgun**: the wheel pushes and pulls, E with the mouse turns, a key freezes the object in place: the controls taken (Q for turning; H holds it still).
- **Amnesia / Penumbra**: grab and drag with the mouse, the wheel for distance; reviewers found the gestures "take practice": not taken.
- **Boneworks / HurricaneVR**: hands as force-limited drives; heavy things need two hands; their melee felt "wobbly and laggy" (too soft). Half-Life: Alyx avoided physical hands for want of feedback to the player's own hand, as a mouse has none.
- **Catto's soft constraints** (Box2D v3 "Soft Step"), **Box3D's motor joint** (a velocity motor and a spring, each with its force and torque capped), **PhysX D6 drives**, **Jolt's motor springs**, **MuJoCo welds** (`solref`): a stiffness set by frequency and damping ratio, not N/m, so it behaves the same for every mass; implicit and capped.
- **Haptics' virtual coupling** (Colgate 1995) and the **god-object** (Zilles & Salisbury 1995): the hand's intent kept apart from the object and joined to it by a bounded spring-damper, the intent never running far ahead of a blocked object.
- **Lashing alone:** a square lashing is started with a clove hitch on the upright and the crossbar laid against it; its first wraps hold the crossbar while the rest and the frapping turns are drawn (Scouting Magazine, "Mark II" square lashing, 2019; scouting pioneering guides). No timing of lashing by hand was found (a Marine Corps knot course allows 30 s for a square knot): the tack takes its turns' share of the lashing's estimated time.
- **The hands drawn on what they hold** (surveyed before the drawing, 2026-10-28):
  - *Analytic two-bone IK* is the games' tool for an arm to a held thing: Unreal's Two Bone IK node puts the end bone at an effector (a socket on the object) and turns the chain so its middle joint points at a "joint target", optionally stretching the bones past 90–150 % of their length; Unity's Animation Rigging Two Bone IK Constraint has a target and a "hint" for the elbow, weighted in and out. Taken: an effector and a pole, blended in and out; not taken: stretching (bones keep their length; beyond reach the arm stops).
  - *Tolani, Goswami and Badler* (2000, "Real-time inverse kinematics techniques for anthropomorphic limbs") solve the elbow in closed form from the shoulder–wrist distance and place the arm by a swivel angle about that line, faster and surer than Jacobian or optimisation solvers. Taken, for the legs too: the law of cosines with the bones off the hinge's plane (the elbow and knee bend about x, their bones not in its plane).
  - *Half-Life: Alyx* draws no arms: Valve found no arm drawing that matched every player's real arms ("if you get it wrong, it stands out an enormous amount"); its hands snap to poses per object. Here the arms are the character's own, seen from behind, and must be drawn; the hands' place is the server's, so they cannot drift.
  - *Grasps:* GraspIt-style closing (each joint until its bone meets the object; G1.4b) on handles up to 50 mm across (Kong and Lowe 2005 tested 25–50 mm; 30–40 mm most comfortable); a box without handles held on its sides, the fingers under its bottom (a UFSCar study of industrial load handling: experienced and new handlers alike).
  - *Stooping or squatting:* neither is better for the back (van Dieën, Hoozemans and Toussaint 1999, a review of 27 studies); people mix them (Straker's semi-squat). Taken: the lean shared between the hips and the waist, the hips lowered with it.
  - *Drawing one's own steered load ahead of the server:* ADR-0025's prediction does not reach a load in the hands (the grip is the server's). The god-object's rule (above) bounds the correction: never further ahead of the server's load than the hands are yet to bring it.
- **Human limits**:
  - two hands lift about 58.6 kg to the elbows (ADR-0027);
  - one hand carries comfortably 9–10 kg (men's one-handed carrying);
  - the wrist holds about 11–12 N·m (Delp et al.: radial deviation 11.0, flexion 12.2);
  - forward grip reach is about 0.7 m;
  - stooping and squatting bring the shoulders from 0.818 of the stature down to about 0.43 of it, the trunk leant forward (Winter's segment lengths: hips at 0.23 of it in a squat, the trunk 0.29 of it leant 45°): for a 1.75 m person 0.7 m lower and 0.35 m on, so the hands reach the ground before the feet;
  - peak hand speeds in reaching are 1.5–2 m/s;
  - a braced hand places to about a millimetre, a free one to a few.

## Decision

1. **A grip** (`creeer_physics::world::grip`, solved in the body's group with its contacts and joints, as struts are):
   - **What it is:** a capped spring-damper from the point held and the node's turn to where the hands mean them, its target moving.
   - **Its law:** k·(target − p) + c·(target's velocity − p's): a vector Huber spring, stiff within the cap and constant beyond it. A load too heavy lags or sags; it is not dropped for it.
   - **Its turn:** alike, by three arms on the node's axes.
   - **Stiffness:** by frequency, 8 Hz critically damped (k = m·ω², c = 2mω; the arms' from the largest moment), so it settles in about a tenth of a second whatever it weighs.
   - **Caps:** its pull at most the hands' strength, 575 N with two hands and half with one. Its torque at most the two hands' grip, 57.5 N·m, or the wrist's 11 N·m with one hand.
   - **In the energy guard:** within a step its target stands still, so its spring is a potential, counted by its change as the struts' and contacts' are. The hands' work booked is their damper's drag at the target's own speed.
2. **The hands** (`creeer_server`'s `hands.rs`, `carry.rs`):
   - **One hand or two:** by need. One for what is at most 9 kg and levered at most 11 N·m about the grip (a made tool, always); two for anything else.
   - **What can be grabbed:** anything up to twice their strength (one end can be raised, the other resting where it lies).
   - **Where they mean it:** where the player steers it, held still in the world (steady), or by themselves before the body (elbow height, a forearm out; a tool by the side).
   - **Kept clear of the body, within reach:** at least as far out from its axis as the load needed when taken, and within 0.75 m of the shoulders, which come down by up to 0.7 m and out by up to 0.35 m towards what is held below them (stooping and squatting): what lies before the feet is held where it lies.
   - **The plan:** they bring it there along a plan in the body's frame, critically damped at 3 Hz, its acceleration within their spare strength, at most 2 m/s, never more than 0.25 m ahead of what they hold.
   - **The plan's turn:** alike, its angular acceleration within half of what their torque gives the load's moment, never more than 0.5 rad ahead of the load's.
   - **Supplied as planned:** its weight and the plan's acceleration.
   - **The body:** bears their last pull back at its centre.
   - **Dropped only** when it has been beyond the arms' reach (from as far as it was taken from, closing as it comes in) by 0.35 m for half a second.
   - **Held things pass the body's box.**
   - **Twist:** a slender load does not twist in the fist beyond the grip's friction (G1.7's rule, kept: `twist.rs`).
   - **Hand over hand:** a pole's hands move along it at 0.5 m/s when it lags behind where they mean to be the way the body faces, or when they push it forward (over 40 N) and it does not go. So a pole is walked up into its hole.
3. **Steered by the player** (protocol 30, with ADR-0008's round 31; the grip told, 32):
   - **The hold:** sent with every input (`AvatarInput::hold`: the load's centre from the avatar's, its turn, steady), delta-coded and quantised (1 mm, a quarter milliradian), so the client predicts with what the server applies.
   - **Taken:** it is held where it was, relative to the view.
   - **The keys** (`creeer_client`'s `holding.rs`):
     - the view moves it;
     - the wheel pushes it away or draws it in, 2 cm a notch;
     - Q with the mouse turns it about the view's up and right axes;
     - Q with the wheel rolls it;
     - Ctrl makes all of it, and the view, five times finer;
     - H holds it steady where it is, the view and the other hand free;
     - G puts it down; T throws it.
   - **A hint** of these keys shows while something is held.
4. **Fastening while holding:**
   - **Held steady,** a part is nailed or pegged with the free hand. While that hand works, the other holds it with one hand's strength and torque, so a part too heavy for one hand sags unless it rests on something: tack it where it rests, adjust it, nail the rest.
   - **A lashing is tacked, then finished** (`tacking.rs`; the user chose it, 2026-10-28). Held steady, the free hand makes the lashing's first turns (its `tack` in `data/fasteners.ron`: 2 of the bast lashing's 4, in their share of its time, 15 s), as a lone builder starts a square lashing: laid against the first turns, a crossbar is held by them. Let go, the tack holds the part (half the whole lashing's clamp and hold); J with both hands free draws the rest of its turns (the other 15 s), the joint made anew with the whole lashing where its parts lie. The tack is told from a whole lashing by what its joint keeps (its turns' hold apart), so it is known after a save and wherever its structure goes. The strip is taken by the tack.
   - **Fastened, the hands let go:** the joint holds the part, and a part made a structure has other nodes than the one gripped (held on, it was yanked towards the old node's frame).
5. **Drawn** (the client: `creeer_client`'s `animate/grasp.rs`, `arms.rs`, `holding.rs`, `hands_rest.rs`; the own load in `creeer_server`'s `client/steered.rs`):
   - **The grip told** (protocol 32): `Kind::Character`'s `carries` is a `Carried`: the load, the point held in the frame the load is drawn in (its node 0's, to 1/1024 m) and one hand or two (one too while the other fastens). It is told when taken; again at once when the hands change, and when the point has moved over 1 cm along a pole hand over hand, at most every 6 ticks, as a structure's shape is (`grip_told.rs`).
   - **Where the hands hold** (`grasp.rs`):
     - one hand: the right, at the point held;
     - a pole (its box's least moment under a tenth of its largest: the server's `carry::SLENDER`): along its long axis, 0.4 m apart about the point held (`carry::HANDS_APART`), kept a hand's half breadth within its ends, each to the nearer shoulder; closed round it as G1.4b closes a hand on a handle (to 50 mm across; a thicker one has the palm on its surface, closed on 50 mm);
     - a box: on its sides across the body, the fingers curled under its bottom (`Hold::Edge`: each bone bends until it meets the face beyond the edge) when its bottom is within 0.3 m below the point held and 3 cm off the ground; else flat on its sides at the point held's height (`Hold::Open`), as people take a box up from the ground;
     - the thumb and index towards the body's middle, front and up as near as the pole lets them; the fingers on along the way the arm reaches.
   - **The arms** (`arms.rs`): two-bone inverse kinematics solved exactly by the law of cosines with the bones off the hinge's plane (`skeleton::Chain`, now the legs' too). The hand is turned to hold first, which puts the wrist; the elbow bends (at most 2.5 rad) to the wrist's distance, the arm turned at the shoulder with the elbow towards a pole back, a little out and down. Beyond reach the arm points at it as straight as it goes and stops: no stretching.
   - **In and out:** the arms reach over 0.35 s along a smooth step (at most about 1.7 m/s: peak reaching speeds 1.5–2 m/s), each joint's turn blended so the arm stays one; a hand's hold slides to a new place on the load at 1 m/s and 8 rad/s (hand over hand, a box turned to another face).
   - **The stoop** (`holding.rs`): along the way the server assumes (`reach::stooped`, now shared), only as far as brings each hand within 0.9 of its arm's reach, at most all of it: the trunk leant at the hips (70 %) and the waist (30 %) up to 50°, the hips lowered as far as leaves the legs 3 cm of their shortest reach, the head looking down by half the lean; followed critically damped (12/s).
   - **Facing it:** standing, the body turns to face what it holds once it is over 30° off its front, at 8 rad/s.
   - **The player's own load where steered** (`steered.rs`): drawn at the server's pose relative to the avatar plus the change of the hold since the input the server had applied at the moment drawn. The part the server has applied is released as a model of its hands' plan follows it (the server's law: 3 Hz, at most 2 m/s and 6 rad/s, the spare-strength cap for a body whose mass is known: `carry::PLAN_HZ`, `plan_spare`), stepped a tick at a time on the timeline drawn. Changes to or from steady, and held steady, count for nothing (the server holds a point in the world).

## Measured and rejected

| Tried | Measured | Why not |
|---|---|---|
| G1.4f's soft arms: 445 N/m (an arm's stiffness against unexpected pushes, Perreault), applied as the step's load | A 15 kg box rang at 0.87 Hz, damping ratio 0.47 (about 19 % overshoot, over a second to settle); the user saw held things float, lag and spin | A stiffness in N/m floats more the heavier the load; a load outside the implicit solve cannot be stiff |
| The hands' whole pull booked as their work over the way the point went | A 1 kg box raised 0.22 m had six of its steps redone by the guard | Within a step the target stands still: the spring is a potential; only the damper's drag at the target's speed is the hands' |
| Planning on regardless of the load (the plan led it without bound) | A pole lifted by its end fell behind the plan and was dropped "out of reach"; the work scene's boxes were never thrown | The plan is kept within 0.25 m of what is held (the god-object's rule) |
| Out of reach judged from the arms' reach alone | A pole grabbed where it lay, 1.5 m from the shoulders (the body bent to it), counted as torn from its first tick while its end rose | Judged from as far as it was taken from, that reach closing as it comes in |
| The plan in the world's frame | Walking off with 45 kg, the plan's spare-strength limit (1.27 m/s²) held the load back; the body walked into it, it trailed, and the hands yanked her over from behind (tipped at tick 219) | Planned in the body's frame: the body's own acceleration reaches the load through the arms, capped by the grip (ADR-0027 had learnt this of G1.4f too) |
| The held load colliding with the holder's body | 30 kg carried at 0.40 m/s instead of 1.20: the box rubbed along the body's single box (no chest and arms) | It passes the body; the hands keep it clear of it |
| The turn planned at a set rate (6 rad/s) whatever the load | A 1.4 m branch in one hand fell far behind as the body turned to walk off; the grip swung it round at 14 rad/s | Its angular acceleration within half of what the hand's torque gives its moment, and tied to the load's turn |
| The fist's twist left to the grip's turn springs (`grip_holds_twist` dropped) | That branch, drooping in one hand at the wrist's cap, spun about its own axis at 150 rad/s in a step | The fist holds a handle's own axis by its friction (G1.7's rule kept) |
| Hand over hand towards the point of the pole nearest the hands' target | The hands stayed at the very end of a pole raised overhead (the target above its end), and it stalled at 42° | Along it the way the body faces when the load lags, or when they push it forward and it does not go |
| The arms' reach measured from the standing shoulders alone | A pole lying 0.6 m before the feet, held steady where it lay to be nailed, was lifted 0.58 m in half a second into the arms' reach, off the pole it was to be nailed to ("not on the body as it was"): nothing could be held on the ground or lowered onto it | The shoulders come down and out towards it, as a person stoops and squats |
| A lashing refused while a part is held (it takes both hands) | In survival the only fastener is a lashing (nails need iron, pegs an auger: G3), so no part could be held in place and fastened at all | Tacked by its first turns with one hand, finished with both |
| The arms drawn in G1.4f's fixed carrying pose (upper arms hanging, elbows a right angle) | Its own test held the hands within 8 cm of the default hold only, and nowhere near a load steered elsewhere; the user saw things "not properly grabbed by the hand" | Two-bone inverse kinematics to where the hands hold it: within a micrometre (Validation) |
| The drawn body stooped all the way the server's reach assumes (`reach::stooped`: for anything below the shoulders) | A pole held at the elbows had the body 0.34 m down and 0.17 m on; its wrists came 0.17 m from the shoulders, closer than the bent arm reaches, and the hands were 28 mm off the pole (`docs/adr/0036/stoop-all-the-way.diff`) | The stooped shoulders are the reach's limit, not a posture: the body stoops only as far as the hands need |
| A box on the ground held flat on its sides at its middle's height | 0.28 m box 0.45 m before the feet, stooped all the way: the hands 80 mm short | At the point held's height (high on its sides): reached |
| The own load's correction ended as the server applies the input (the change since the input it last applied, released at once) | Steered 180 mm on the slow link, the drawn load jumped back 175 mm when the input was applied, and came on again with the server's (its hands bring it there over about 0.3 s) | Released as a model of the hands' plan follows it: a frame moves 2.2 mm at most |
| The knee bent by searching (G1.3g: a ternary search for the straightest, then bisection), and the arms so | The straightest found only to 2.1·10⁻⁸ rad (the span is flat at its longest), some 110 evaluations a leg a frame | The law of cosines, the bones off the hinge's plane: exact (the same reach to 6·10⁻¹⁶ m), one arccosine |
| The stoop's lean found by bisection every frame something is held | Holding a box cost 2.5 µs a frame more than walking | Skipped when the hands reach without it, the lean by Newton's method on its closed form: 1.6 µs |
| **The raising test as written before G2.10** (`raising_tests.rs`): the pole laid with its foot 20 cm down the hole and its top on the ground 3 m west; the hands held at head height before the body, the pole turned upright at once | Neither was possible. The pole lay up to 28 cm into the ground past the rim, was pushed out, and came to rest with its foot 7 cm above the mouth, over the hole, not in it. The hold asked the grip, 2.9 m up the pole, to be 3.7 m from its foot: the grip pulled at its most (575 N) and dragged the foot 0.77 m. It passed only because every impact under the person's feet stopped the pole dead (ADR-0005, G2.10); with that fixed, the pole was dragged out and never stood | A slot dug beside the hole along the pole, as linemen dig one to walk a pole up into its hole (the user, 2026-10-29), and a hold within reach (below) |
| A slot 14 cm across, 2 cm wider than the pole | Finer than the terrain's 12.5 cm cells and its rounds draw: the pole wedged in it, its steps finding up to 239 kJ | As wide as the hole (30 cm) |
| The slot rising 3° to the west | It ran 3 m before the pole cleared the ground, and the ground between hid the grip point from the eye ("behind the ground") | Rising 8°: the pole clears the ground 2.1 m out, its middle over the slot's floor |
| The hold turned about the foot towards the hands at head height, past the arms' reach | The hands held where they reached; lifting straight up there, the foot was drawn 0.6 m up the slot | The grip turned about the foot only as far as the arms reach (0.7 m from the stooped shoulders, `raising_tests::reachable_hold`): never farther from the foot than it is along the pole, nor beyond the arms |

## Validation

- **The grip** (`grip_tests.rs`):
  - boxes of 1, 15 and 40 kg settle critically damped, with no overshoot beyond a millimetre (1 and 15 kg within a centimetre of a 0.22 m move at 0.17 s; 40 kg slower, its 392 N weight leaving the hands 183 N);
  - carried at 1.5 m/s it follows within 2 mm;
  - 120 kg is not lifted and nothing is thrown;
  - a 1.2 m stick held at its end turns to the hands' turn within 2° and spins under 3 rad/s while its hands bob 3 cm at 2 Hz;
  - at most one step redone, as a box comes to rest at the guard's nanojoule floor.
- **The hands** (`carry_tests.rs`, `raising_tests.rs`, `loose_tests.rs`, `play/work.rs`):
  - a box is brought to the elbows and carried along, and walking off at once lifts and carries it;
  - 130 kg is refused; 70 kg is held but stays on the ground; 40 kg is held up, and with 30 kg set on it, it sinks and is still held;
  - 30 kg is carried at 1.2 m/s; 45 and 55 kg do not tip the body over;
  - a 3 m, 27 kg pole grabbed at its end is held in both hands, raised by that end, its far end down;
  - lying in a slot dug beside a 30 cm hole, its foot 20 cm down the hole, steered up within reach and walked under, the pole comes to 23° from upright, its foot 0.39 m down the hole (0.45 m at most), the hands 1.6 m along it (G2.10, the test set up anew: below);
  - a branch picked up and carried walking follows the body's half turn in about half a second, steady to a thousandth of a rad/s after;
  - in the work scene every blow falls and no box is dropped;
  - over a slow link, walking with a load is corrected as rarely as before.
- **Fastening while holding** (`tacking_tests.rs`): a pole held where it lies before the feet, not steady, is refused a nail or a lashing ("hold it steady first"); held steady, it stays within a centimetre while a nail is driven in its 6 s, the two poles become one structure and the hands let go. With the stoop taken out, the hands lift it 0.58 m off the other in half a second. Tacked while held steady, it stays within a centimetre for the tack's 15 s; then the hands let go and its joint holds 1,920 N apart. Taken again, it cannot be finished ("both hands"). Put down and finished, 15 s later its joint holds the whole lashing's 3,840 N (`creeer_building`: two turns are half the clamp).
- **The input:** a hold reads back within a millimetre and a quarter milliradian (`inputs.rs`).
- **The grip told** (`carried_tests.rs`, `grip_told_tests.rs`, `replicate/tests.rs`): it reads back within half a millimetre in one hand or two, in 5–7 bytes more than holding nothing; three hands are refused. Taken, it is the point grabbed to a millimetre and not told again while held still; the other hand at work, one hand is told at once, two again after. Walked up 1.47 m hand over hand (the raising test's hold, G2.10; 0.52 m before), it was told 58 times in 360 ticks (at most every 6), at most 42 mm behind (the client slides the hands between).
- **The arms** (`arms_tests.rs`, on the engineer's hands as exported):
  - within a micrometre of where each hand holds, within reach: one hand on a 30 mm stick (15.0 mm from the point held, its radius: the handle's axis on the stick's), two under a 0.28 m box's bottom edges, two on a 60 mm pole 0.400 m apart;
  - a box held on the ground 0.45 m before the feet: both hands flat on its sides within a micrometre, the body leant 49°, the hips 0.49 m down, the shoulders 0.63 m down, both ankles at 0.111 m (the feet on the ground);
  - torn 1.5 m away: 0.88 m short, the arm straight to 1.0000, every bone finite; a hand meant at the shoulder itself, finite too;
  - reaching out and back, the hand at 1.33 m/s at most; an arm reaching its own hand at rest has its elbow 6.2 mm off the model's;
  - under an edge the fingers' bones stay 11.0 mm (a finger's half thickness) below the box's bottom (`hands_tests.rs`); the exact knee is the old search's to 2.1·10⁻⁸ rad, reaching the same to 6·10⁻¹⁶ m (`skeleton_tests.rs`).
- **The cost** (`arms_tests.rs`, release, the least of five rounds): walking 1.4 µs a frame, walking with a box in both hands 3.0 µs: 1.6 µs more a character, 3 % of the animation's budget of 50 µs, 0.01 % of a 60 Hz frame.
- **The own load on a slow link** (`steered_tests.rs`: 100 ms each way, 30 ms jitter, 5 % lost):
  - steered 180 mm and turned 0.3 rad at once, the drawn load is 0.5 mm off where steered and turned 0.300 rad in the same frame; the server's own pose shows it 433 ms later;
  - after, a frame moves it 2.2 mm at most, it is never more than 14 mm from where steered, and at 2.5 s it is drawn where the server has it (to 0.01 mm and 10⁻⁴ rad);
  - pushed 150 mm into the ground (blocked), drawn 145 mm in it, over a centimetre in it for 667 ms: the round trip as drawn (400 ms) and the 283 ms the server's hands would take to bring a 12 kg box so far.

## Limits

- **The body:** bears the hands' pull at its centre, a step late; its moment about the body is not on it, and the body does not lean into a push.
- **The stoop is drawn only:** the server's body (its box and its legs' strut) stands as it was while its hands reach from stooped shoulders; the drawn body stoops and squats to them. The stoop leans along the way the body faces only: what is held low at the side is reached by the arms.
- **The held load passes its holder:** the body is one box, without chest and arms, so a load pressed against it would rub along that box; the hands keep it clear of the body's axis instead.
- **No tiring:** the hands do not tire: a load is held as long as the player holds it.
- **The two hands are one grip:** at one point, the two hands' torque its cap. Two hands at the load's two ends, each with its own grip, are not modelled.
- **Fingers:** they close on a handle up to 50 mm across, curl under a box's edge or lie flat on it; a thicker pole has the palm on it and the fingers closed on 50 mm (their tips in it); the thumb lies straight under an edge and flat. A change between holds (flat on a box's sides to under its bottom as it is lifted) is at once.
- **The hands' places are from the load's box:** a hull (a stone, a branch) is held at the box about it, so its hands may stand a little off its surface; a structure's grip is in node 0's frame as told, its flexing since within the 1 cm the grip is told again by.
- **Which hands work:** the left hand's work while the right holds (fastening) is the whole of "the other hand": no tool in it is drawn yet, and the left arm hangs as it walks.
- **Facing:** the body turns to what it holds standing only; walking, it faces the way it goes, so a load held behind it (walking backwards) is reached for behind. Turned on the spot, the feet pivot until they stray far enough to step.
- **The own load ahead of the server:** the model of the hands' plan has their speed caps and, for a body whose mass is known, their spare strength, but not their torque's cap on turning, the plan's 0.25 m lead or the reach's clamp: where the server's hands do less than the model, the drawn load falls back to the server's over the plan's time (14 mm at most measured). A blocked load is drawn in what blocks it for the round trip and as long as the hands would take to bring it there, not the round trip alone. The avatar is still predicted without the hands' pull on it (pressing a load into the ground lifts the server's body 21 mm that the drawn one is not).
- **Fastening while holding:** checks that the part is held steady, not where the free hand is.
