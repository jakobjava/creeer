# ADR-0022: Joints

- **Status:** Accepted (2026-10-18)
- **Context:** G1.1 (`docs/g1-plan.md`). The physics had contacts and welded members but no articulation. The character's ragdoll needs it (G1.2), and so do carrying (a grab is a joint), ladders, and G2's hinges, pins and bearings.
- **Builds on:** ADR-0004 (the implicit step as an energy minimisation), ADR-0005 (contacts, the energy guard, the lagged refresh of friction's normal forces)
- **Evidence:** `creeer_physics::world::joint` (tests in `joint/tests.rs`, `joint/tests/motion.rs`, `joint/rows.rs`), `creeer_sim::save::joint_tests`, `creeer_tools bench-spikes --only joints`

## Decision

### What a joint is

A **ball joint** (three rotations free) or a **hinge** (one), between two bodies (a node of each) or a body and the world. It has:
- **limits:** a ball's cone and twist range, a hinge's angle range;
- **damping** (N·m·s/rad) and **friction** (N·m) on the two sides' relative turn;
- a **drive** towards a target relative orientation, with a stiffness and a strength.

Bodies joined are stepped in one group, sleep and wake together, and do not collide with each other unless the joint says so. Saves keep joints with their constraint forces and friction's anchors (save version 8), so a loaded world goes on bit for bit.

### Exact constraints in the step's minimisation

A joint is a term of the group's incremental potential, beside the contacts (it rides in the group's `Contacts` coupling):
- **An augmented Lagrangian.** λ·c + k/2·|c|² on its point gap c (and a hinge's axis misalignment, A's axis × B's).
  - The multiplier λ is carried from step to step.
  - It is refreshed where the minimisation converges (`Coupling::update_lag`, the slot friction's refresh uses), until the joint holds.
  - The penalty k is the step's own stiffness of the lighter side (mass, or inertia, over β²) times 100. Each refresh then leaves about 1 % of the error before it.
- **When a joint holds:** its gap is under 1 µm (`HOLDS`, a few of the solver's own 3·10⁻⁷ m), or a refresh would change its force by under 1 % (`SETTLED`).
- **Limits** are one-sided terms of the same kind: (max(0, λ + k·g)² − λ²)/2k for a reach g ≤ 0.
- **Exact rows.** Every scalar (a gap component, an axis component, a limit's reach) comes with its exact derivative by each side's translation and world-frame turn, its "row" (`rows.rs`, checked against finite differences). The energies' gradients are then Σ coefficient·row, their Gauss–Newton blocks Σ weight·row·rowᵀ.
- **Angles** are measured as atan2(axis·(r_A × r_B), r_A·r_B) with exact derivatives: the hinge's angle and a ball's twist about A's axis. A ball's cone is measured by cos(cone) − t_A·t_B.
- **Damping, friction and the drive** act on three points on the joint's frame axes, 0.1 m from its centre (`ARM`). Each pair of points (one on each side) is apart only by the two sides' relative turn. These terms are exact and convex like the contacts':
  - damping: a dashpot on the arms' gaps from x̂;
  - friction: μ·|slip| smoothed, measured from an **anchor**, the relative orientation where it sticks. The anchor follows a slip beyond its stick width, so a load below the friction never creeps (measured from each step's start, a 20 N·m joint let a 9.8 N·m load creep 2.5 mrad in 2 s);
  - the drive: a Huber spring on each arm, linear up to the strength's share and constant beyond.
- **Energy.** Constraints and limits store no energy for the guard (they are ideal); a drive's springs do.

### Three things the scheme needed

1. **A velocity projection** (`project.rs`). Newmark takes a node's velocity from its positions (v = 2·Δx/h − v₀). A velocity across a joint, once there, changes sign every step and never dies away, and the joint's force rang with it. At the end of each substep, the velocities are made consistent with the joints: the least change, weighted by mass and inertia, that leaves every joint's points moving together, its hinge's axes turning together, and its stops not run into further.
   - It is equal and opposite impulses at the joint, so momentum and angular momentum are kept.
   - It solves J·W·Jᵀ·μ = −J·v in blocks of three, with the deterministic sparse Cholesky of the rigid bodies' joint solve (a dense solve took as long as the rest of a ragdoll's step).
2. **Substeps while joints turn fast** (`stepping.rs`, `JOINT_TURN` 0.05 rad per substep, at most 8, per group). Newmark's energy wavers by about (ω·h)²/4 on a joint's nonlinear swing, back and forth, bounded. Beyond the energy guard's slack (10⁻³), the guard took each rise back for a gain and redid it dissipatively. A double pendulum at 60 Hz lost half its energy in three seconds. At 0.05 rad a substep the wavering stays within the slack, as it does for turning boxes (`turn.rs`).
3. **The guard's surplus counts the contacts only** (`Coupling::found_energy`). The guard takes from the motion whatever elastic energy the coupling holds beyond what the last step left in its contacts (contacts found anew). A drive's springs were taken for such energy and removed every step: a driven body came to a dead stop each tick.

Also: joining two bodies resets their solvers' history (an impulse, as an impact's), and redoing a step for boxes' turns restores the joints' multipliers with the bodies.

## Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| Reduced coordinates (a separate articulated-body solver) | Not built | Joints must step jointly with contacts and structures, close loops (a ladder held with two hands), and grab anything; a second solver beside the minimisation would split every group with joints. The double pendulum's RK4 in its two angles is the reference instead | **Revisited (P1):** rejected without a survey of prior work. RedMax (2019) steps the tree in reduced coordinates inside one implicit step with maximal-coordinate constraints where loops close; PhysX and MuJoCo use reduced coordinates for exactly the ragdolls and humanoids that converge slowly here. See the amendment below |
| BDF2 for jointed groups (as contacts) | 0.82 J of 1.6 lost in 3 s at 60 Hz | BDF2's numerical damping at ωh = 0.14 |
| The constraint force at the step's end only, kept out of Newmark's carried acceleration (SHAKE-like) | Energy fell steadily | Its direction is the end state's, not the start's as in RATTLE: dissipative |
| A stiffer penalty (×10⁴), or stiffened to 10× the contacts on a joint's bodies | Ragdoll 2.4 and 2.5 ms p50 against 1.6; three accuracy tests failed (×10⁴) | Stiff, ringing modes; more refreshes, not fewer |
| A joint force's geometric stiffness (λ·∂²c, its positive part) in the matrices | Iterations identical to the digit | No gain |
| Joints' blocks kept within a step (not remade at each iteration) | 0.134 against 0.149 ms | Two accuracy tests failed: the minimisation stops at different states |
| A hold of 10⁻⁷ m | Every step of a ragdoll took the three refreshes allowed | Below what the solver's tolerance lets a gap settle to |
| The multipliers refreshed by Newton's method, by the step's matrix (P1) | Quadratic on its own; on a falling ragdoll iterations unchanged, +18 % time | Friction's normal forces refresh with them and reopen the joints: ADR-0024, its patch kept |

## Amended in P1 (2026-10-18): prior work, not surveyed when this was decided

This decision was made without a survey of prior work (now required: plan.md §1, principle 12; `docs/adr/README.md`). Done in P1 (ADR-0024), it found:
- **Reduced coordinates are the standard for jointed trees** (PhysX articulations, MuJoCo, Featherstone's methods): joints exact by construction, a sixth of the unknowns, large mass ratios tolerated. Here a ragdoll's joints ask for 93 % of its steps' refreshes, and its steps cost 13 Newton iterations.
- **The objection to them (closed loops, grabbing) has a published answer**: RedMax (Wang et al., ACM TOG 2019) keeps reduced coordinates for the tree and maximal-coordinate constraints where loops close and for contact, in one implicit step.

So the joints of this ADR stay, for loops, grabs and machines, and P1 evaluates ragdolls as one articulated body in reduced coordinates beside them (ADR-0024).

## Amended in G1.4f (2026-10-22): a joint's points moved, and its force

Carrying (ADR-0027, G1.4f) moves a joint's point on its side every step: the hands bringing a load in and holding it as the body turns. Three things were found measuring a 5 kg ball hung from a point drawn up at 0.6 m/s (`joint/tests/motion.rs`), each fixed:

- **The force.** `Joint::force` returned the multiplier alone. When the step's three refreshes run out (each step, with a point moved), the force the step applied is the multiplier and the penalty on the gap left, λ + k·g. It is now recorded where the step ends (`Joint::applied`, `project.rs`): not the joint's state (not saved, not compared), none after a load until the next step.
- **The velocity.** Projected so the joint's points move together (decision 4), the bodies lost the velocity the moved point gives them: the joint re-accelerated the ball every step. A joint now takes its points' velocities on their sides (`Joint::moving`, set by whoever moves them; an input, not saved), and the projection keeps the points' relative velocity at that rate.
- **The work.** A moved point does work on the bodies (it lifts the ball). Uncounted, the energy guard took it for the step's gain and removed the ball's kinetic energy every step: it rose 1 cm a step at no speed, held by 229 N against its 49 N weight. The coupling now counts the work of its moved points (`Coupling::driven_work`: the applied force on the gap the moving opened), as the guard counts loads' and torques'. The ball rises at 0.600 m/s, held by 49.03 N.

One more, found carrying in the play scene *work*:

- **Velocities the guard scales down** (`remove_kinetic`) left a joint whose points are moved out of step with them: its load stopped while its point went on, and the next step tore it along (1,000–1,700 N on a 15 kg box). Where a group's points are moved, the joints' velocities are projected again after the guard takes energy.

Joints whose points are not moved are unchanged (the stress and play scenes' hashes hold). (Carrying itself then moved to compliant hands, forces of the arms rather than a joint, ADR-0027: these stay, tested, for a joint whose point is moved, as a machine's.) Bodies let pass may also be let collide again (`World::let_collide`: a load let go).

## Validation

| Test | Result |
|---|---|
| Rows against finite differences (hinge, ball with cone and twist, drive) | 10⁻⁵ relative |
| A pendulum's period (5°, compound) | within 10⁻³ of 2π·√(I/(m·g·L))·(1 + θ²/16) |
| A double pendulum against RK4 in its angles (30°, −20°, 3 s) | within 2·10⁻³ rad at 240 Hz, 2·10⁻² at 60 Hz |
| A falling chain of five | energy never rises (10⁻³ of its kinetic); joints hold within 10⁻⁵ m |
| Two boxes tumbling, joined | momentum within 10⁻⁹; angular momentum within 10⁻⁴ (its gap within `HOLDS` × the joint force: a torque); joint held |
| A chain of 15 at rest | moves less than 10⁻⁵ m; its top joint carries its weight within 10⁻³ |
| A hinge on its stop | rests at the stop within 10⁻⁴ rad, its torque m·g·L·cos θ within 10⁻³ |
| A damped pendulum | decays as e^(−c·t/2I) within 3 % |
| A drive | holds its target within its strength (50 N·m against 9.8), gives way beyond it (5 N·m) |
| Friction | holds a load below its torque (20 N·m against 9.8), slips beyond (5) |
| Joined bodies | do not collide unless the joint says so |
| Saves | read back as written; go on bit for bit; joints follow their bodies when others are removed; the version 8 fixture |

## Cost (development machine, `bench-spikes --only joints`)

A 15-part ragdoll on 14 joints with limits and damping:
- **Tumbling in the air:** 0.15 ms p50, 0.58 worst. Its joints' terms and projection are 0.09 ms of that. The same fifteen bodies free (no joints) take 0.39 ms p50, as fifteen groups. G1.1's plan said 0.1 ms; that is below what the bodies cost at all, so the gate is 0.2 ms.
- **Falling onto the ground:** 1.4 ms p50, 15 worst; the same bodies without joints 0.6 and 20. With joints all fifteen are one group, whose contact solve takes twice the Newton iterations of the separate bodies: the large group's contact solve, P1's (G1.8).

## Limits (`docs/future-improvements.md`)

- A ball's twist is measured about A's axis: it loses meaning as the swing nears 90° (B's reference along A's axis), where its stops are let go.
- A joint between bodies pressed by stiff contacts holds within its 1 % force criterion, not to a micrometre: each refresh gains little against stiffness a hundred times its penalty (a ragdoll's feet on the ground).
- A ragdoll on the ground: its group's contact solve (P1).
