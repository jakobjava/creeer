# ADR-0027: Tools, reach and carrying, and the body's motion (G1.4)

**Status:** proposed (2026-10-22), for the user's approval of `docs/g1-plan.md`, G1.4.
**Context:** `docs/g1-plan.md`, G1.4: the engineer's first work. He holds tools and strikes with them, reaches what the crosshair finds, carries and throws bodies. The user's first play (2026-10-22, after G1.3) asked for the motion to be better first: "the animation is very rough and it sometimes glitches, when running the player does not lean forward, also when stopping walking it does some more steps without moving, the arms look robotic, the fingers do not move and are always apart from each other: we can improve these things in 1.4".
**Builds on:** ADR-0022 (joints), ADR-0023 (the character: one rigid body, legs as a strut, balance within its strength), ADR-0025 (prediction of the bodies near the avatar), ADR-0026 (the engineer skinned on the ragdoll's fifteen bones; the gait from measurements, decision 9).
**Evidence:** to come with each sub-milestone (G1.4a–h).

## Where it starts

- **The motion** (`crates/creeer_client/src/animate/`): the gait from measurements (Sekiya's walk ratio, the walk-run change at Froude 0.5, Perry's rockers), feet planted and placed by inverse kinematics, the trunk the simulated body turned to its motion. The arms swing as one rigid piece each against the legs; the trunk does not lean; the gait runs on after the body stops until its phase ends; the hands are one bone each (the ragdoll's), the fingers fixed apart as modelled. A tool's swing has a hook (`Animator::swing`) and nothing behind it.
- **Reach and commands**: the F5 sandbox's commands act at up to 500 m; nothing is held.
- **Items**: none yet (G1.6). The crate is the only item model; tools have none.

## Prior work

Surveyed before the design (`docs/g1-plan.md`, principle 8). Numbers each sub-milestone builds on are confirmed from the papers themselves in its own research, as G1.3g's were.

| Work | What it found or does | What it means here |
|---|---|---|
| Sprenger, Siegenthaler, Kneubuehl and Jackowski, "The influence of striking object characteristics on the impact energy" ([Int. J. Legal Med. 2016](https://www.springermedicine.com/the-influence-of-striking-object-characteristics-on-the-impact-e/21018112)) | 1,170 horizontal strikes by 13 volunteers against a pendulum: men delivered 67–311 J on average, women 30–203 J; for a 1 m object the most energy at about 1.3 kg with its mass in the far quarter; shorter objects more | A strike's energy from the tool's mass, length and balance and the swinger's strength, not a fixed number per tool (the plan's 60 J and 80 J are within the range) |
| Hammer physics in the trade (e.g. a 1.09 kg framing hammer at about 10 m/s, 55 J) | E = ½mv² at the head; head speed falls as the head gets heavier, so the energy has an optimum | The swing modelled as the arm's and tool's rotation, its speed from the arm's torque |
| Cross, "Physics of overarm throwing" ([Am. J. Phys. 2004](https://doi.org/10.1119/1.1634964)), and handball studies of ball mass | Release speed falls with the object's mass as a power law; the arm itself carries an effective mass of about 2 kg | A throw's speed from the object's mass by that law; its reaction on the thrower |
| Knapik et al., load carriage reviews (2004), and the NIOSH lifting equation (23 kg load constant) | Speed falls about linearly with load; a load's cost rises the further it is from the body's centre; hand-carried loads cost more than packed ones | Carrying slows walking by the load on the legs and its moment about the hips, and topples the character when the balance cannot hold it |
| Half-Life 2's physics cannon (Valve, [developer wiki](https://developer.valvesoftware.com/wiki/Weapon_physcannon)); Human: Fall Flat's hands | A held body steered by a force that weakens with its mass; hands that grip any surface by a joint | Taken in kind: a grab is a joint at the hands with a force limit from human strength (G1.1's ball joint), so what is too heavy drags or tears loose |
| Preece et al., "How do elite endurance runners alter movements of the spine and pelvis as running speed increases?" ([Gait & Posture](https://salford-repository.worktribe.com/output/1404607/how-do-elite-endurance-runners-alter-movements-of-the-spine-and-pelvis-as-running-speed-increases)); a 2023 Gait & Posture study of sprint acceleration | Recreational runners lean the thorax further forward as they run faster (3.3–5.6 m/s); accelerating, the trunk leans further and rises as speed builds | The trunk leans with speed and with acceleration, drawn on the trunk's bones (the body itself is the simulated box) |
| Ridge et al., planned and unplanned gait termination ([2013](https://scholarsarchive.byu.edu/facpub/2013), [2016](https://scholarsarchive.byu.edu/facpub/2016)); Jaeger and Vanitchatchavan (1992), ground reaction forces at termination | Healthy people stop in about one step: the leading leg brakes harder, the trailing leg pushes off less; unplanned stops flex the knee more | The gait ends in a stopping step that brings the feet together, then stands: no stride runs on |
| Yegian and Tucker, "Straight arm walking, bent arm running" ([J. Exp. Biol. 2019](https://hsrc.himmelfarb.gwu.edu/smhs_psych_facpubs/1801)); Collins, Adamczyk and Kuo (2009, [Proc. R. Soc. B](https://pmc.ncbi.nlm.nih.gov/articles/PMC2817299)); gait studies of the arm's rotations | The arm is nearly straight walking and bent near 90° running (each the cheaper for the shoulder); the arms swing passively against the legs; walking, the shoulder rotates about 56° and the elbow 25° over a stride | The arms swing from shoulder and elbow with measured amplitudes and phase, the elbow bent with speed |
| The resting hand: hand-surgery literature on the "safe" and resting positions (e.g. MCP 45–90°, PIP 0–20° in the safe position), the "normal cascade" of a relaxed hand | At rest the fingers flex progressively from index to little finger, together, the thumb beside the index | Finger bones in the skin, posed as a relaxed hand at rest and closing on what is held |
| Hand rigs in games (Unreal's mannequin: three bones per finger and the thumb; VR hands that close procedurally on what they hold, as Half-Life: Alyx's) | Fingers posed procedurally from a few curls rather than keyed | Fingers driven by the client's animation alone (not simulated: the ragdoll keeps its fifteen segments) |
| Bernier, "Latency compensating methods in client/server in-game protocol design" (Valve, 2001) | The server judges a hit against where things were when the client fired | A strike or grab is checked by the server against what the client saw, within the reach |
| Minecraft's reach (4.5 blocks to interact, 3 to strike in survival) and anthropometric reach (ANSUR II functional reach about 0.8 m) | A game's reach rounds up a body's | Reach 2.5 m from the hands (the plan's decision 4), the tool's length within it |

## Decision (proposed)

1. **The motion first** (G1.4a–b): the user's notes, each researched and checked in the game, before anything is held. The glitches are recorded and found before anything is smoothed over.
2. **Fingers as bones of the skin, posed by the client** (G1.4b): three bones per finger and the thumb, weighted in the art pipeline from the model's hands, posed procedurally (relaxed, gripping, open); the physics keeps its fifteen segments.
3. **Tools held before items exist** (G1.4c): the four tools modelled in Blender in the style (`art-style.md` §8, "Tools"), held in the right hand, picked by number keys from the sandbox's kit until G1.6's inventory replaces it.
4. **Reach 2.5 m from the hands** (G1.4d), checked by the server against what the client saw; the 500 m commands kept for creative and test servers.
5. **Strikes from a swing** (G1.4e): the strike's energy from the tool's mass and balance and the arm's strength, the swing drawn as it is simulated; its energy goes into what it hits (an impulse now; digging in G1.5, cutting in G1.7).
6. **Carrying and throwing** (G1.4f): a grab is a ball joint at the hands with a force limit from human strength; the load acts on the character, slowing and toppling it; a throw's speed from the object's mass, its reaction on the thrower.

## Measured and rejected

To come.

## Validation

To come.

## Limits

To come.
