# ADR-0033: Fastened joints

- **Status:** In progress (G2.1 built 2026-10-23; G2.2 adds the laws from the fasteners' data)
- **Context:** G2 (`docs/g2-plan.md`). Buildings are built by hand: parts are fastened where they touch, with nails, pegs and lashings, and later bolts and welds. A building stands or falls by its members and its joints. Each joint's stiffness, slip, strength and failure must come from what fastens it.
- **Builds on:** ADR-0004 (the implicit structure step as an energy minimisation), ADR-0005 (members' plasticity and fracture)
- **Evidence:** `creeer_physics::structure::fastened` (law → equivalent member), `element.rs` (a joint's measures), `plastic.rs` (withdrawal), tests `structure/fastened_tests.rs` and `element_tests.rs` (gradients of a joint against finite differences)

## Prior work

The survey is in `docs/g2-plan.md` ("Prior work"); what decided this ADR:

| Source | What it does | For Creëer |
|---|---|---|
| KSP, Besiege, Garry's Mod | Rigid parts on joints that break at a force or torque limit | Rejected: a limit is not a law; nothing slips or yields first |
| Valheim, 7 Days to Die, Rust | Support values that decay along a graph from the ground | Rejected: no forces at all |
| EN 1995-1-1 §7.1, §8.2–8.3 (Eurocode 5) | Dowel-type fasteners: slip modulus K_ser (Table 7.1), the lateral capacity by Johansen's yield modes (eq. 8.6), withdrawal (§8.3.2), their interaction (§8.3.3, eq. 8.27–8.28) | Taken: the law's numbers (G2.2), the interaction's form |
| Foschi (1974); Folz & Filiatrault (2001, CASHEW) | A nail's load–slip backbone: P = (P₀ + K₁δ)(1 − e^(−K₀δ/P₀)), then a peak and a fall | Adapted: elastic at K, yield, a linear rise to the peak, slip at the peak until failure (the members' own hardening law) |
| Multibody bushings (e.g. MSC Adams' BUSHING) | A 6-DOF spring between two frames, its offsets measured in the first frame | Taken for the joint's measures (below) |
| Sandberg, Bulleit & Reid (2000); Schmidt & Daniels (1999) | Pegged timber joints: yield theory with peg modes within −16 %/+2 % | The same law shape fits pegs (G2.2) |
| EN 1993-1-8 | Welds and bolts: capacity, bearing, friction grip | They fit the same law shape: later data entries |

## Decision

**A fastener is a short member.** It runs from one member's axis to the other's, across the faces where they touch. So a joint is an `Element` between a node of each member, of length L (the distance between the two axes), with a section and material made equivalent to its law (`JointLaw::equivalent`). Members' whole machinery then serves joints too: the step, its matrix and factorisation, plasticity within the step, fracture, damping, saving and splitting a structure. No second element type runs beside them.

- **Slip across** is the short member's shear. A lateral offset δ gives the force 12·EI/(L³·(1 + Φ))·δ. With Φ = 12·EI/(κGAL²) = 100 (`SHEAR_DOMINATED`) that is the slip modulus: κGA/L is set to K·(1 + 1/Φ), exactly.
  - Its end moments are F·L/2, so the members' plastic hinges cap F: plastic modulus Z = F_y·L/(2σ).
  - The flow stress then hardens to the peak at the peak's slip, holds there, and the joint breaks at its last slip. Elongation = plastic slip/L, since both ends' hinges turn by δ_p/L, each straining by half of it.
  - These are Johansen's ductile modes, as the members' own plasticity.
- **Along the fastener:**
  - In compression the joint bears on the wood: stiff, with the bearing capacity as its axial capacity (A = bearing/σ).
  - In tension the fasteners pull out. This is brittle: after each step a joint with (F_ax/R_ax)^e + (F_la/R_la)^e ≥ 1 breaks at once (EN 1995-1-1 §8.3.3; e = 1 for smooth nails, 2 for others), its slip force F_la taken from its end moments.
- **About the fastener:** GJ/L is its twist stiffness, yielding at its torque. One nail is close to a pin; a group holds Σ K·r².
- **A joint measures as a bushing, in the first member's frame.**
  - Its stretch is the separation along the fastener: d·t_a − L, with t_a member a's axis carried by its node's turn.
  - Its other measures use d/L where a member uses the unit chord.
  - Measured with the chord, a joint slipping 5 mm read as a stretch of δ²/2L, and its slip force turned with the chord. Both became 65 N of pull on a 600 N withdrawal: a nailed joint failed by withdrawal at a third of its last slip. Two members slipping past each other do not move apart.
  - Exact gradients (d(d·t_a) = t_a·dd + δθ_a·(t_a × d)) are checked against finite differences. Rigid motions measure nothing.
- **No mass, no buckling, no shape.** Density 0 (a nail's grams are the members'), never split by `buckling.rs`, extent 0 (members' collision balls and drawing ignore it).
- **Saves:** a member's `crooked` byte becomes a flags byte: bit 0 crooked, bit 1 a joint's capacities follow. Members save bit for bit as before, old saves load unchanged, and no save version is raised.

## Measured and rejected

| What | Measured | Why rejected |
|---|---|---|
| A separate joint element type beside the members | Not built | Every pass (evaluation, assembly, factorisation, plasticity, fracture, save, splitting) would need a second path. The equivalent member needs only its measures and a withdrawal check |
| The chord's stretch and direction for a joint (a plain short member) | A nailed joint (7·10⁵ N/m, 1,300 N peak, 15 mm last slip) failed by withdrawal at 4.8 mm of slip: 65 N of pull from the chord tilting 3° | Two members slipping past each other do not move apart: the bushing measures |
| Asymmetric axial stiffness (bearing stiffer than withdrawal) | Not built | A kink in the energy at zero; withdrawal's slip before its peak is small. One axial stiffness, the bearing's, with withdrawal as a capacity |

## Validation

`structure/fastened_tests.rs`, with a law of a few nails in softwood (orders of EN 1995-1-1's values):

| Test | Result |
|---|---|
| Slip under 200 N, bearing under 500 N (statics) | F/K to 10⁻⁹ |
| Twist under 1 N·m | T/K to 10⁻⁹ |
| Ramped to 0.9 of its yield | No yield; elastic slip within 2 % |
| Ramped to 1.5 of its peak | Fails at its last slip, within 15 % (the ramp's last tick) |
| Pulled to 0.9 / 1.1 of its withdrawal; pressed with 10× | Holds / lets go / holds |
| Pull and slip each at 0.6 of capacity | Smooth nails (e = 1) let go; e = 2 holds |
| Undamped elastic swing, 4 s | Energy within 10⁻⁶ |
| Save and load | Bit for bit; a member's flags byte is a bool's |

**Stiff joints and the step.** A joint of 7·10⁵ N/m across and 5·10⁶ along, on 10 kg, rings at ω·h ≈ 4.4 and 12 at 60 Hz. As with stiff members, the step cannot resolve it, and the energy guard's dissipative fallbacks take its ringing. The same section as a plain member lost even more (99.6 % against 93 % of a 0.2 m/s swing in 2 s). In a building, joints sit between members of hundreds of kilograms, and the step's damping of vibrations it cannot resolve is the solver's (ADR-0004), not the joint's.

## Limits

- A joint's rotation about its lateral axes (members rocking on each other) is resisted by its equivalent bending stiffness and capped at F_y·L/2, not by the members bearing on each other. Members colliding with each other come in G2.5.
- The backbone is piecewise linear (elastic, linear hardening, plateau), not Foschi's exponential, and has no falling branch (failure at the last slip). There is no cyclic pinching (CASHEW).
- Withdrawal is brittle at its capacity, with no slip before it.
