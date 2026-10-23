# G2 plan: building system

Status: **in progress** (planned 2026-10-23). Branch `g2`, squash-merged into main as "G2" at its end.

G2's loop: **build a bridge or a tower by hand from the parts you have, load it, and watch it hold or collapse.** The first version is a log bridge over a gorge in survival, lashed and pegged. On a creative server it is nailed timber, shown with the stress overlay.

plan.md's G2, re-planned with the user on 2026-10-23 (Phase B, principle 13, "Precision is technology"): building is by hand, as on a real site. Parts are carried, put in place and fastened. Nothing snaps, previews or undoes. Precision (measuring tools, drawings, CAD) comes with the tools that give it, in G3, G6 and G7/G10.

## Principles

1. **A building is one structure** (ADR-0004). Fastened parts become members of one beam structure: each part is subdivided so it can bend and buckle, and each fastener is a joint element between them. A structure stands or falls by its members' and joints' own laws. Nothing is graph-heuristic, unlike every game surveyed (Prior work).
2. **A joint is its fasteners.** A joint's stiffness, slip, strength and failure come from what holds it: how many nails, of what diameter and steel, into what wood, at which grain angle. These follow the engineering rules (Eurocode 5 for timber, EN 1993-1-8 for steel). One law covers every fastener: elastic, then yielding slip along a backbone curve, then failure. Lashings, pegs and nails are data entries in G2. Welds and bolts are later data entries, each with its own law parameters, and need no change to the system.
3. **Hands are hands.** A part is held by the G1 grab constraint with the character's real strength (ADR-0027). A part too heavy to hold needs props, rope or a second player. Placement is as good as the player's eye. There are no ghosts, snapping, numeric input, undo or "will it hold?" preview, even on creative servers (principle 13).
4. **What a person on site sees.** In survival a structure shows only sag, sway, cracks and failure. The stress overlay is a debug view on creative and test servers.
5. **Fundamentals first** (principles 10–12). Each step builds the generic system with a few data entries that prove it. The user plays as soon as building is playable (after G2.3).
6. **Gated play scenes:** worst tick ≤ 5 ms on the development machine, p99 ≤ 2 ms, ≤ 256 kbit/s per client.

## Prior work (surveyed 2026-10-23, before the design)

**Games.**
- Medieval Engineers shows stress in blocks snapped to a grid.
- Valheim, 7 Days to Die and Rust use support heuristics that decay with distance from the ground.
- KSP, Besiege and Garry's Mod use rigid bodies with joints that break at a force or torque limit.
- Poly Bridge uses its own 2D truss solver, and Dream Car Builder a node–beam soft body.

None of them forms a joint from what fastens it, none runs a real structure solver, and none makes the player hold a heavy part. Sources: Medieval Engineers' Steam page and its developer's blog (2020); the Valheim and 7 Days to Die wikis; the KSP forum on `breakingForce`; Unity's Joint.breakForce documentation.

**Fasteners** (EN 1995-1-1; units N, mm, kg/m³):
- **Embedment strength, unpredrilled nails:** f_h,k = 0.082·ρ_k·d^−0.3.
- **Embedment strength, predrilled holes and dowels:** f_h,0,k = 0.082·(1 − 0.01·d)·ρ_k, with grain angle α through k90 = 1.35 + 0.015·d (softwood).
- **Yield moment:** M_y,Rk = 0.3·f_u·d^2.6.
- **Lateral capacity:** Johansen's six failure modes, eq. 8.6. The rope effect F_ax/4 is capped at 15 % of the Johansen part for smooth nails and 0 % for dowels.
- **Withdrawal:** f_ax,k = 20·10⁻⁶·ρ_k² for smooth nails.
- **Slip moduli:** K_ser = ρ_m^1.5·d^0.8/30 for nails and ρ_m^1.5·d/23 for dowels; K_u = ⅔·K_ser.
- **Load–slip curve:** Foschi (1974), P = (P₀ + K₁·δ)(1 − e^(−K₀δ/P₀)). The CASHEW backbone (Folz & Filiatrault 2001) adds the peak and the fall beyond it.
- **Wooden pegs:**
  - Sandberg, Bulleit & Reid (2000, J. Struct. Eng. 126(6)): 25 mm red-oak pegs, strengths within −16 % to +2 % of yield theory with two peg modes.
  - Schmidt & Daniels (1999): pegged mortise-and-tenon joints in tension.
  - TFEC 1-10: pegs of specific gravity ≥ 0.57; edge distance 2.6·d and end distance 2.8·d for full strength (Nichols 2004).
- **Lashings:** lashed bamboo cross-lap joints hold about 5.9 kN with three loops, and more loops add no stiffness (BioResources 2024). There is no published square-lashing law. Ours is friction μ·N on the clamp of the wraps' tension, with the capstan law at the turns (derived; marked as such in the ADR).
- **Welds and bolts** (EN 1993-1-8) fit the same shape:
  - fillet weld: f_vw,d = f_u/(√3·β_w·γ_M2);
  - bolt shear: F_v = α_v·f_ub·A_s/γ_M2, with bearing;
  - friction grip: k_s·n·μ·F_p,C/γ_M3.

**Posts in soil:**
- Broms (1964): a short post in sand, free head, fails at H_u = 0.5·γ'·d·L³·K_p/(e + L), with K_p = tan²(45° + φ/2). In clay the resistance is 9·c_u·d below 1.5·d.
- Uplift: Q = β·σ'_v·π·d·L + W (β-method, Fellenius ranges).

**Lifting by hand:**
- The NIOSH lifting equation (Waters 1993): 23 kg recommended.
- HSE guide weights: 25 kg at the waist, 10 kg at the shoulder or above.
- Team lifts: two people manage ⅔ of their summed capacity, three ½.
- Carpenters align members with temporary braces, clamps and draw-bored pegs.

**Structure physics:**
- **Lateral-torsional buckling:** subdivided 6-DOF co-rotational beams capture it when the geometric stiffness keeps the moment–twist coupling. They miss only warping (EI_w). Timber and rods have I_w ≈ 0, so the error there is about zero. Open steel sections err low (conservative): about 17 % for an IPE300 at 6 m. Validated against M_cr = (π/L)·√(E·I_z·G·I_t)·√(1 + π²·E·I_w/(L²·G·I_t)) (Timoshenko).
- **Beam–beam contact:** Wriggers & Zavarise (1997) closest points; Meier, Popp & Wall's all-angle beam contact (2017: points at large angles, Gauss points along the line at small ones, from a potential); Durville's proximity zones. BeamNG has no beam–beam contact at all.
- **Subcycling (P3):** Prakash & Hjelmstad (2004) keep energy exact at the interface by holding continuity at the coarse step. The Gravouil–Combescure method dissipates energy at the interface. Lew et al.'s asynchronous variational integrators (AVI, 2003) are explicit only.
- **SIMD (P2):** 6×6 blocks in a layout interleaved across structures (Gates et al. 2017); faer and pulp in Rust; runtime dispatch with `target_feature`. FMA changes bits, so each platform keeps one path for determinism.

## Sub-milestones

### G2.0 Rest: contacts that let things lie still (ADR-0024's open work)

The user chose this first (2026-10-23). Felled trunks today never rest: 3 of 4 are still awake 25 s after the last blow, at about 7 ms a tick. Every building stands on the ground and holds members on members, so G2 needs rest first.
- Its own survey first: persistent contact manifolds and warm starting (Box2D, Bullet, PhysX, Jolt), sleeping under penalty contacts, and why contacts found anew hold more energy than the step before (the energy guard's zeroing).
- **Gates:**
  - lumberjack's trunks all asleep within 5 s of their last blow;
  - lumberjack's p99 ≤ 2 ms and worst ≤ 5 ms, or the remaining cost measured and recorded;
  - every existing accuracy test within its tolerance.

### G2.1 Joint elements in the structure solver (`creeer_physics::structure`)

A **joint element** between two nodes, in the joint's own frame (normal n along the fastener, two lateral axes):
- **Lateral:** an elastoplastic slip on the lateral slip's magnitude, so it has no direction bias. Elastic at K_u, then along its backbone, Foschi-like up to F_max at δ_max, then failing.
- **Axial:** withdrawal (pull-out) with its own capacity; compression bears on the wood (stiff).
- **Rotational:** from the fastener group's polar spread (Σ K·r²); a single fastener is close to a pin.
- **Stepping:** the history (plastic slip, peak slip) is frozen within Newton and committed after convergence, as members' plasticity is (ADR-0005). The tangent is consistent.
- **Failure:** at δ_u the joint breaks. Its stored energy is booked as dissipated, and the element is zeroed, keeping the sparsity, as broken members do.
- **Energy audit:** kinetic + elastic + dissipated = external work, per step.
- **Tests:** a single nail's load–slip against EN 1995 numbers (a 3.1 mm nail in C24: K_ser ≈ 0.70 kN/mm); withdrawal; a nailed lap joint loaded to failure; energy; save round trip.

### G2.2 Parts and fasteners as data (`creeer_building`)

- **Fastener catalogue** (`data/fasteners.ron`): each entry gives its kind (nail, peg, lashing; weld and bolt later), material and dimensions. Its joint law comes from the rules above, computed from the parts' woods and the fastener's own material. No strengths are hand-written.
- **Members from items:**
  - a felled log (tapered: each element takes the stem's diameter at its middle);
  - a pole or stick (`Form::Rod`);
  - a riven board split from a log with wedges (survival);
  - sawn beams and planks, steel rods and bars (creative until G3).
  - Section properties are computed and tested against section tables.
  - **Panels** are grillages: a grid of beam strips, the engineering grillage method (limits recorded; shells later).
- **Wood as a member material:** E and G along the grain, and tension, compression and bending strengths parallel to the grain (EN 338, and the species data of ADR-0031). Wood is brittle in tension and bending, and yields in compression. Lifts *the sandbox's built-in S235*: members take their materials from the packs.
- **Tests:** section properties, masses, data validation, generic tests over every entry.

### G2.3 Building by hand (server, client)

- **Hold, brace, fasten:**
  - a part is held by the grab constraint, and a brace holds it still against the body;
  - a fasten action with the hands' tool and a fastener from the pockets (hammer and nails; drill or auger and a peg; a lashing by hand) takes time from the work it needs;
  - the joint forms at the contact point between the two parts nearest the crosshair.
- **Merging:** fastening two bodies merges them into one structure body, keeping position, velocity and momentum exactly. A part fastened to a structure joins it.
- **Splitting:** a structure whose joints or members break into pieces becomes several bodies, as fracture does.
- **Cutting:** cut to length with a saw where the player points, or along a mark made against another part (copying its length). Riving a board from a log takes an axe and wedges.
- **Infrastructure:** protocol and save versions raised; server-authoritative transactions (fastener items consumed); crosshair hints.
- **Then the user plays:** fell, buck, carry, prop and lash a small frame; creative: nail one.

### G2.4 Standing on the ground

- **A post dug in and backfilled:** its pull-out, lateral and moment capacity from the soil (Broms and the β-method, with the ground's friction angle, cohesion and density from the geology data of ADR-0032). It is an anchor element to the world with its own law, and fails by pulling out or tipping.
- **A footing resting on the ground:** friction contact only, as today.

### G2.5 Members that collide and buckle sideways

- **Members colliding with each other:** member segments as capsules against capsules (point contact at large angles, Gauss points along parallel ones, from one potential), and against other bodies and the ground. A spatial hash over segments. Segments that share a node or lie next to each other along a member are excluded. Lifts *members colliding with each other*.
- **Lateral-torsional buckling:** a validation test against M_cr (rectangular timber and a rod, I_w ≈ 0: exact; IPE: the warping error recorded). Plus the moment–twist coupling in the geometric stiffness, if the test shows it missing. Lifts *lateral-torsional buckling*.

### G2.6 Taking apart

- **Pulling fasteners:** pull a nail with a claw (work from withdrawal), cut a lashing, knock out a peg. Undamaged fasteners return to the pockets; a bent nail is scrap.
- **Sawing a member** in a structure: it splits there, with the cut's waste (kerf) removed.
- Bent members keep their plastic deformation; salvage keeps it.

### G2.7 P2 and P3 (performance track)

- **P2: the structure solve.** SIMD kernels for the 6×6 blocks, with runtime AVX2 dispatch and NEON on aarch64. One path per platform keeps determinism. Many structures at once: a layout interleaved across structures. Targets as in plan.md.
- **P3: struck structures.** A struck region subcycled in Prakash–Hjelmstad style, so energy is exact at the interface. Simpler first: Newton's work restricted to the struck subgraph. Targets as in plan.md.
- Each starts with its own survey, building on the one above.

### G2.8 Play scenes and the demo

- **Play scenes:**
  - *tower*: a 20-storey steel frame of about 20,000 beams, assembled by script, under wind, at rest;
  - *collapse*: the tower loses a ground-floor column;
  - *drop*: the sandbox's 10 t block on a 40 m bridge;
  - *builders*: 8 players building by hand at once;
  - *log bridge*: lashed and pegged, loaded with crates until it collapses.
- **Demo:** in survival, a log bridge across a gorge, built by hand with lashings and pegs, loaded with crates until it collapses. On a creative server, the same with nailed timber and the stress overlay.

## Order

G2.0 → G2.1 → G2.2 → G2.3 → **the user plays** → G2.4 → G2.5 → G2.6 → G2.7 → G2.8 → the user plays the demo → squash-merge.

## Decided with the user (2026-10-23)

- Building by hand, precision as technology (plan.md principle 13).
- Debug views only on creative and test servers; creative gives materials, not conveniences.
- Holding is fully physical.
- The precision ladder: by eye in G2, measuring tools in G3.6, drawings in G6.3, CAD and design files in G10.1.
- G2's fasteners: lashings, pegs and nails. Welds and bolts must fit the same system without changing it.
- The contact core first (G2.0).
- Panels as grillages of strips.
- Survival wood: logs, poles and riven boards. Sawn timber comes from creative or G3's sawmill.
