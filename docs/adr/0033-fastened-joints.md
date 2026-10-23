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

## G2.2: the laws from the fasteners (2026-10-23)

`creeer_building` (`fasteners.rs`, `law.rs`; tests `tests.rs`) works out each joint's `JointLaw` from its catalogue entry (`data/fasteners.ron`) and the two parts' woods:

- **Nails and pegs** (dowel-type, EN 1995-1-1):
  - **Embedment:** f_h = 0.082·ρ·d^−0.3 driven, 0.082·(1 − 0.01·d)·ρ bored.
  - **Yield moment:** M_y = 0.3·f_u·d^2.6 for a nail; a peg's plastic moment, f_m·d³/6.
  - **Lateral yield:** the least of Johansen's six modes (eq. 8.6, without its 1.05 and 1.15).
  - **Rope effect** (smooth nails, modes c–f): min(F_ax/4, 15 %).
  - **Withdrawal:** 20·10⁻⁶·ρ²·d·t_pen, scaled from 8·d to full at 12·d, capped by head pull-through 70·10⁻⁶·ρ²·d_h². Pegs: a tenth of their slip capacity (an estimate).
  - **Slip moduli:** ρ_m^1.5·d^0.8/30 and ρ_m^1.5·d/23 (Table 7.1), with ρ_m = √(ρ₁ρ₂).
  - **Penetration:** a nail must reach 6·d into the second part. A peg, which runs through, must reach past the shear plane by its own diameter.
- **Lashings** (derived; no published law was found):
  - n turns press the parts together with 2n times their tension, and they slip by friction on that. The slip starts at the hand-drawn tension (a quarter of the cord's strength) and peaks at the cord's strength as slipping draws it tight.
  - Pulled apart, the 2n strands break.
  - Stiffness: the strands' E·A over a turn's length.
- **Shared by all:** bearing as compression across the grain over the contact area, f_c,90·A. Axial stiffness from the two woods' E₉₀ = E₀/30 (EN 338) in series over the distance between the axes.
- **Wood's density** is EN 1995's: at 12 % moisture. Green woods' `density` holds their water (800 kg/m³ for spruce), so they now carry a `seasoned` density: red spruce 450, red pine 515, paper birch 620 kg/m³ (Wood Handbook Table 5-3a, specific gravity at 12 % × 1.12).
- **The catalogue:** `nail_75` (3.1 × 75 mm), `nail_100` (4 × 100 mm), `peg_25` (birch), `bast_lashing` (spruce bast, four turns). Its header says how to add entries; `creeer_tools validate` checks them.

| Test | Reference | Result |
|---|---|---|
| 3.1 mm nail in C24, slip modulus | 708.7 N/mm (Table 7.1) | within 0.1 % |
| 4 × 100 mm nail through 50 + 50 mm C24 | mode (f) 1,097 N; withdrawal 706 N (head 1,000 N); rope 165 N | within 0.2 % |
| 75 mm nail with 15 mm in the second part | under 6·d | refused |
| Two nails 4 cm off the middle | twice the slip capacity; twist (2·0.04² + 2·d²)/d² = 202 × one | exact |
| Bast lashing, four turns | breaks at 3,840 N apart; slips at 0.25 to 1 × 0.4 × that | exact |
| Every catalogue entry on green spruce | valid, and its law builds an equivalent member | all |
| Green spruce's density for fastening | 450 kg/m³ seasoned | exact |

## G2.3: building by hand (2026-10-23, in progress)

**Growing a structure in place** (`creeer_physics::structure::grow`):
- `insert_node(e, t)` puts a node into a member as it lies now, on the cubic through its ends and their tangents. Its plastic deformation is shared, and mass and momentum are kept.
- `append(other)` takes another structure in as it is: moving, bent, loaded.
- `join(a, b, law)` makes a joint between two nodes as they lie. A joint keeps b's rest frame relative to a's (`Fastened::to_b`), so a joint made between nodes that have turned apart measures nothing.
- Building onto a loaded structure keeps its sag (tested): nothing springs back.

**Parts** (`creeer_building::assembly`): a wooden rod or block item, or a felled log with no branches on, becomes a member where it lies, moving as it moved.
- Split into elements at its joints and at least every metre.
- Collides as a prism round its axis (a twelve-sided one for round parts, a box for boards), split in halves on each element's two nodes.

**The fasten command** (`Command::Fasten { target, fastener }`, protocol 26; `creeer_server::world::building`, `building_parts`):
- The aimed part and the part touching it nearest the aim are fastened after the fastener's time (`seconds` in `data/fasteners.ron`: estimates), if the player's hands still reach the point (kept in the part's frame while the work goes on).
- Both hands must be free: a part is propped or held by someone else.
- On a creative server any fastener is at hand. In survival a lashing takes a bast strip from the pockets; nails need iron and pegs a bored hole (G3).
- The second structure is taken into the first; the joint goes between nodes put into both members where they touch.
- The joint's law comes from the two members' woods and thicknesses. The bearing area is taken as the narrower member's thickness squared (an estimate).

**Records:** each built structure keeps the items it was made of (for salvage, G2.6) and each element's wood, saved in the section `BILT` (save version 13).

**Replication:** `Frame` carries each member's own radius (joints 0, not drawn).

**Found: face patches on multi-node structures.** Until now structures collided only as balls on their nodes. Hulls and boxes on a structure's nodes diverge in its first step on the ground, while rigid bodies with the same shapes are fine.
- **What was found so far:** a face's corners have no stiffness of their own, only coupled blocks built for rigid bodies; and a lifted corner's friction gave negative stiffness in a node's block.
- **Status:** both are addressed in the structures' contact blocks, and the divergence remains. It is being worked on (`faces_on_structures_nodes_repro`). The tests that need it are ignored until then.

Client: J fastens what the crosshair is on with the chosen fastener; K chooses the next.

## G2.4: posts in the ground (2026-10-23, the law)

`creeer_building::soil` makes a post's buried length a row of soil springs, one per node: each a joint to a fixed node in the ground beside it (`Structure::anchor`). This is the Winkler model of laterally loaded piles (Reese & Van Impe 2011).
- **Across:** subgrade reaction n_h·z/d (Terzaghi 1955: 2.2 to 7.0 MN/m³ from 30° to 40°) plus 67·c/d (Davisson 1970). It yields at Broms' ultimate pressure 3·K_p·σ'_v·d, plus 9·c·d below 1.5·d, over the node's span.
- **Along:** skin friction by Fellenius' β-method, β = (1 − sin φ)·tan(⅔φ), plus half the cohesion. Pressed, the foot bears N_q·σ' + N_c·c (Meyerhof; `tip`).
- **Soil never snaps:** no brittle withdrawal, and a spring flows on at its pressure.

**Found on the way, fixed for every joint:**
- A joint's slip and its bearing do not share a section. The members' N–M interaction (N/N_p)² + |M/M_p| ≤ 1 had cut a soil spring's hold as soon as the post pressed along it: skin friction is far below the pressure across. Joints have their own interaction, the fasteners'.
- A joint fails by its slip only. Its bearing crushing and its turning flow on; counted, a turned post's springs broke long before they slipped.
- The withdrawal interaction breaks *beyond* 1, not on it. A joint at its peak slip (ratio 1 to rounding) broke at the first trace of pull.

**Validation** (`soil_tests.rs`): a 150 mm timber post, 0.8 m in sand (1,800 kg/m³, 30°), loaded across 1.6 m above the ground.
- The reference is the rigid short post's limit with the soil's pressure on both sides of where it turns (Brinch Hansen 1961), 548 N. Broms' toe force gives 848 N, the cruder bound.
- At 0.9 of the limit it leans 15 cm and comes to rest; at 1.1 it is pushed over.

**Measured and rejected:** a ground node 5 mm instead of 5 cm from its post node (to shrink any shear the turning post's node might cause in the equivalent member). No change in the collapse load; the cause was the three rules above.

**Not yet:** setting a post in a dug hole and backfilling it, in the game.
