# G2P plan: tick time (the A6 stream of G2.11)

Status: **in progress** (written 2026-11-13, at the start of the stream, as plan.md's G2P asks). Branch `g2`. This plan covers the parts of G2P that G2.11 brought forward (`docs/playability-plan.md`, stream A6): P3's *log_bridge* breaking tick and P1's loose poles at rest. The rest of G2P (P2's second pass, P1's ragdolls and piles, *builders*' and *drop*'s worst ticks) keeps plan.md's order and is planned when it starts.

## Why

The first thing that must be fun is a bridge loaded until it breaks. In *log_bridge* (two sills, a 150 × 50 mm spruce plank nailed across them, 200 kg crates dropped on its middle), the game froze for 2.6 s at the moment it broke: tick 941, 445,513 Newton iterations, then 47,618 and 12,627. The plank was said to burst into about 19 pieces of 1–32 kg spinning at up to 831 rad/s. A loaded timber plank that fails in bending breaks at one or two sections and its halves fall. So the breaking tick was first examined as a physics question: if the model is wrong, fixing it is the speed fix, validated against an analytic or published case; if it is right, the cost needs a method (subcycling, ADR-0035).

## The gates

- The breaking tick and the ticks after it within the play gates: worst ≤ 5 ms, p99 ≤ 2 ms (plan.md, Phase B, principle 4). If out of reach in this pass: as far as it goes, the remaining cost measured and its cause named.
- Loose poles at rest (G2P, P1): `rim_tests.rs`' pole at rest over a whole second at 10°, 20° and 30°; *builders*' first 100 ticks under 5 ms; *lumberjack* no slower; `rest_tests.rs`' lone trunk asleep.
- Throughout: every test, tolerance and gate as it is; golden hashes renewed only for an intended change, explained.

## Prior work (surveyed before any change)

### Timber beams failing in bending

| Source | What it says | For Creëer |
|---|---|---|
| *Wood Handbook* (FPL-GTR-282, 2021), ch. 5; ASTM D143 | The modulus of rupture (MOR) is the stress at failure computed as if the beam were elastic to the end: the moment at failure is MOR · bh²/6 for a rectangle. Small clear specimens fail in static bending as simple, cross-grain, splintering or brash tension, in compression or in horizontal shear: at one section, near the largest moment | A plank's failure moment is MOR·W_el. Creëer caps a member's moment at σ·Z_pl (bh²/4) with σ = MOR for green wood: 1.5 times the moment a plank breaks at (`structure/plastic.rs`). Recorded below |
| Thelandersson & Larsen (eds.), *Timber Engineering* (Wiley, 2003); Madsen & Buchanan (1986), *Can. J. Civ. Eng.* 13 | Timber in tension and bending is linear to brittle failure; compression parallel to the grain yields first. A beam breaks at its weakest section near the largest moment; strength falls with size (Weibull) | One or two breaks, not a burst |
| Gordon & Jeronimidis (1974), *Nature* 252:116; Jeronimidis (1980); Atack (1961) as cited by Nairn | Work of fracture across the grain about 10⁴ J/m² (pine 9,200 J/m²); along the grain about 100 times less (100–300 J/m², spruce green) | A 150 × 50 mm plank broken across takes about 75 J to break; the plank stores about 150 J at failure under a central load (P²L³/96EI at P = 4·MOR·W_el/L). Breaking takes about half: there is little left to throw pieces |
| Audoly & Neukirch (2005), *PRL* 95:095505; Heisser et al. (2018), *PNAS* 115:8665 | A brittle rod bent and released at one crack sends a flexural wave that bends it harder elsewhere: further cracks (dry spaghetti breaks in three or more) | A cascade needs stored energy many times the fracture work, released at once. Not timber's case (above) |

### Fracture in implicit and beam solvers

| Source | What it does | For Creëer |
|---|---|---|
| Element deletion ("erosion": LS-DYNA's `*MAT_ADD_EROSION`, Abaqus' element deletion) | An element past its failure measure is removed with its stored energy | Creëer's brittle members do this: zero stiffness at the end of the step that made them flow (`plastic.rs`) |
| Hillerborg, Modéer & Petersson (1976); Bažant & Oh (1983) | Cohesive and crack-band models: a crack dissipates the fracture energy G_f per area of crack whatever the mesh | The regularised way to make fracture take its energy; not needed while breaking dissipates the member's own stored energy and the plank breaks at one or two sections (measured below) |
| Armero & Ehrlich (2004), *CMAME* 193:3129; Ehrlich & Armero (2005), *Comput. Mech.* 35:237 | Beams with softening plastic hinges: the hinge's moment falls to nought while dissipating the fracture energy, dynamics well posed | The method should Creëer's breaks need a fracture energy (Limits) |
| Rots (2001), sequentially linear analysis; Schlangen & van Mier (1992), lattice models | One element fails at a time, the structure solved again before the next | Creëer breaks every member that flowed in a step at once. With 0.5 ms impact substeps two members broke in different substeps (measured below): the load was redistributed between them |
| Hall (2006), "Problems encountered from the use (or misuse) of Rayleigh damping", *EESD* 35:525 | Stiffness-proportional damping over-damps high frequencies and puts spurious forces in a structure as it fails | Measured below: most of the moment that broke the plank's members was their dashpots' |

### A body put into the world inside another

| Source | What it does | For Creëer |
|---|---|---|
| Bullet's split impulse (Coumans) | A penetration is corrected by a pseudo-velocity that moves the bodies apart and is then dropped: no velocity remains from it | The method taken (below): the step moves the bodies apart, the velocity it gave is taken back |
| PhysX `maxDepenetrationVelocity`; Box2D v3 `contactPushMaxVelocity` (3 m/s) | The speed an overlap's push-out may give is capped | The same aim, as a cap |
| Li et al., "Incremental Potential Contact" (*SIGGRAPH* 2020) | Barriers: an overlap can never arise, a start inside one is invalid | Creëer's penalty contacts allow it; a spawn inside a body is the spawner's fault, but the physics must not turn it into energy |
| Unity `Physics.ComputePenetration`, engines' spawn checks | A spawn place is tested and moved clear before a body is put there | The ragdoll's layout should be (reported to the character's stream) |

### Loose poles at rest

The survey for P1's rest is ADR-0005's ("Friction anchors loaded beyond sticking": Li et al. 2020, Kikuchi & Oden 1988, Wriggers 2006, Box2D's persistent manifolds) and ADR-0024's (persistent manifolds in Bullet, Box2D, Jolt; Gregorius 2013, 2015). Its starting point is `docs/adr/0005/anchors-kept-and-clamped.diff` with its tests.

## Part 1: *log_bridge*'s breaking tick

### Found: the 2.6 s tick was energy from nothing, not the plank's fracture

Measured with `play/break_probe.rs` (each body's mass, speed, spin and energy tick by tick) and a temporary trace of the energy guard's books in every substep (not kept).

- **What broke it.** The first crate (200 kg, 1 m) landed on the plank at 6.1 m/s, on the head of the builder standing at the stringer. He was knocked down: his body became a ragdoll of 14 segments (`creeer_sim::character::ragdoll::fall`). The segments are laid out from the standing pose; only the ground is cleared. **Its head (7.25 kg) was laid out 13.4 cm inside the crate.**
- **The energy audit across the breaking tick** (every body but the standing players):

  | | before the tick | after it |
  |---|---|---|
  | the world's energy | 4,299 J | 388,487 J |
  | the crate | 1,582 J, 4.0 m/s | 217,754 J, 43.9 m/s |
  | the ragdoll's segments | 705 J, 3.9 m/s | 155,800 J, up to 115 m/s |
  | the bridge | 59 J (elastic), one body | 2,600 J in 6 pieces |
  | the contact crate–head at the step's start | 221.8 MJ | |

  The "19 pieces of 1–32 kg" were the ragdoll's 14 segments and the bridge's 5 pieces: the plank was smashed by the crate and the segments flying through it.
- **Why the energy guard let it through.** The guard takes energy that contacts hold beyond what the last step left from the motion (ADR-0005, F6). It knows the last step's books only for bodies it stepped: new bodies have none (`left_by_group`), and an impact's step drops them (`set_left(None)`). The tick was an impact on a structure (0.5 ms substeps), its contacts found 222 MJ, and the guard counted them as energy the step had.

So the model was wrong, but not in its fracture: in its energy books for a body put into the world inside another.

### Kept: what a body new to the world holds when put in pressed never becomes motion (`Body::born`, `structure/owed.rs`)

A body put in since the last step (`World::add`) is born. What its contacts hold when first measured is owed: each contact owes the least of what it held then and holds now (what it gains after is the motion's). The energy guard counts only what is still owed at a substep's end, and takes the velocity the overlap gave back out of what the substep gained (a split impulse: the bodies are moved apart, not thrown apart). A copy of a body as it is elsewhere (a client's prediction) is added as not new (`World::add_copy`). Details and validation: ADR-0005, amended in G2.11.

**After it.** The scene plays otherwise from its first tick: a sill laid level by its middle on the slope held 580 J in the ground and was thrown by it at the first tick; now it is pushed out, and the sills are laid along the ground anyway (`play/log_bridge.rs`). The first crate again lands on the builder at the stringer and knocks him down (tick 938), and his head is again laid inside it, now pushed out, not thrown. Crate, man and plank stop together in tick 941 (64,269 Newton iterations, where the old tick took 344,750). At tick 946 the plank breaks **at one section, in its middle**, under the crate lying on the man; its halves fall, the nails pull out of the sills (947), and the falling halves break again at their ends (948).

The energy audit, every body but the standing players (`break_probe.rs`):

| tick | 940 | 941 (crate stops) | 945 | 946 (the break) | 947 | 948 |
|---|---|---|---|---|---|---|
| the world's energy (J) | 4,383 | 1,315 | 1,274 | 1,263 | 1,291 | 1,245 |
| the fastest body (m/s) | 4.0 | 0.18 | 0.51 | 0.66 | 2.2 | 1.0 |

It falls but for 28 J at tick 947, as the halves come off the sills (2 % of what is in play; before the change: +384,000 J in one tick).

### Measured, the realism of the break itself (recorded; not changed in this pass)

- **The plank breaks at a third of its strength, by its dashpots.** The members' material damping (stiffness-proportional, c = 0.02 s for green spruce) acts in series with the plastic slider: a brittle member breaks when spring and dashpot together reach its capacity. In a 0.5 ms impact substep γ = c/β ≈ 61. At tick 946 the plank's middle member broke with 901 N·m in its spring, 23 % of the 3,844 N·m it may carry, under a crate and a man lying still on it (about 2.5 kN; a central load breaks it at 5.1 kN by the *Wood Handbook*'s MOR). In the first trajectory two members broke at 41 % and 12 %. c = 0.02 s is ζ = c·ω/2 ≈ 1.2 at this plank's first mode (19 Hz), where timber has about 1–2 %: a value that suits trees' slow sway. With c a hundred times smaller the plank broke into fewer pieces and the ticks cost the same: it changes how and when it breaks, not the cost.
- **A plank holds 1.5 times its real bending moment** (σ·Z_pl with σ the MOR, which is defined on W_el: above).
- Both change every wooden structure and tree, and need their own validation (the ASTM D143 three-point bending test against the *Wood Handbook*'s MOR; a damping ratio from measured timber members). Recorded in `docs/future-improvements.md` for G2P.

### The cost that remains (measured after the fix)

Newton iterations summed over the bodies (deterministic); times on a machine shared with seven builds, to be measured again on a quiet one.

| | before | after |
|---|---|---|
| the tick the crate lands on the man on the plank (941) | 344,750 (2.0–4.7 s here) | 64,269 (0.37 s) |
| the plank's break (946 after; 941 before) | (the same tick) | 11,496 (0.09 s) |
| ticks after, as man, crate and pieces fall into the gap | 3,500–31,000 | 9,500–68,000 (0.03–0.25 s) |
| ticks 997–1007, the man and the crate lying on the pieces | 800–3,600 | 126,000–174,000 (1–2 s) |

Not met: the gates (worst ≤ 5 ms, p99 ≤ 2 ms). What the remaining cost is, measured with the physics' profile (`break_probe.rs`):
- **Every point impact on a structure is stepped in 34 impact substeps** (0.5 ms). A ragdoll's segment or the crate rocking on a piece of the plank closes faster than 5 cm/s, and the whole group (the man's 14 segments, the crate, the pieces: 16–17 bodies, 150–300 contacts) is stepped in substeps, about 250 Newton iterations each, as ADR-0024's ragdoll under a crate takes. Faces (prisms) are judged by whether the impact can yield a member at all (ADR-0005, G2.3: a hundredth of the least yielding energy at the struck node); points are not, because the save fixtures held their motion then. They no longer do (the user, 2026-10-26). Judging points alike is the next step (a hand at 0.3 m/s brings mJ, a member stores tens of J at yield).
- **Fast-spinning fragments put their groups in turn substeps** (up to 16 world steps a tick, and a redo): single nodes of the plank, 0.9–4.3 kg, spinning at 15–100 rad/s once both their members broke.
- **The man under the crate** is ADR-0024's open case (a ragdoll and a crate: 40 iterations a substep).

Subcycling the struck region (ADR-0035) is not what this scene needs: the plank is 12 nodes, struck whole.

## Part 2: loose poles at rest

(Not started in this pass: see the report and `docs/future-improvements.md`.)
