# ADR-0005: Contacts, rigid bodies, impacts and plasticity

- **Status:** Accepted (2026-10-16)
- **Context:** F2 (physics core). Structures are stepped implicitly as an energy minimisation (ADR-0004). They must collide with each other, with loose rigid objects and with fixed geometry; stand, slide and stack; crash; yield; and break. The goal is physical accuracy at a 60 Hz tick, validated against analytic results, within the performance budgets of plan.md §5.

## Decision

**Everything is one implicit minimisation per group of touching bodies.** Contacts, friction, plasticity and damping are terms of the same incremental potential that the structure solver minimises (ADR-0004). Rigid bodies are structures with one node and no members, so one solver handles both.

### Contacts (`world/contact.rs`, `detect.rs`, `narrow.rs`, `sat.rs`)

- Shapes: spheres and boxes on nodes, plus fixed planes and boxes. A structure's joints collide as spheres as thick as their thickest member. Box–box contacts use SAT with face clipping.
- Contacts are found at the start of a step, with a speculative margin of twice the distance a body can travel in the step. Normal and contact points are then frozen for the step.
- **Normal force: a stiff penalty**, ½·k·min(g, 0)² with k = 1e8 N/m (1 mm per 100 kN), plus a dashpot c = k·h on the rate of penetration. The dashpot absorbs the contact's own vibration (about 1 ms for steel), which a 60 Hz step cannot resolve.
- **Friction: an anchor**. A contact sticks as a stiff spring to an anchor point on the other body, out to 0.1 mm; beyond that it slides with exactly μ·λ, and the anchor follows. λ is lagged and refreshed during the minimisation, as in IPC (Li et al. 2020). Measuring from an anchor rather than from the start of each step means a sticking contact never creeps.
- **Groups:** bodies joined by contacts are solved together; groups run in parallel.
- **Scheme per group:**
  - free bodies use Newmark (ADR-0004);
  - groups with contacts use BDF2, because the trapezoidal rule never damps a contact's unresolvable rocking (Newmark chatter was measured on a resting box);
  - impact steps use backward Euler.
  The cantilever frequency error is −3.5% in contact groups (BDF2), within ±5%.

### Impacts of rigid bodies (`world/impact.rs`)

Newton's impact law, e = restitution, applied as velocity impulses:
- **Before the step:** inelastic speculative impulses. A closing contact may approach no faster than gap/h. Without this, the step's prediction put a ball 20 cm inside a crate, and the minimisation needed 300–800 iterations to push it out.
- **After the (inelastic, backward Euler) step:** the rebound, −e·v⁻. It is solved by sequential impulses that only push, and capped so the total energy cannot exceed its value before the step, less the plastic work done in the step.

### Impacts that deform structures (`world/stepping.rs`)

A 60 Hz step spreads the stopping force over 16 ms, which keeps stiff members below yield: the crash energy would vanish as numerical damping instead of crushing anything. So a group with an impact on a structure is stepped in **substeps of at most 0.5 ms**, until no member flows plastically any more. In those substeps:
- contacts drop their dashpot, because their vibration is resolved;
- contacts on structures get no restitution impulses, because the structure's elastic rebound is resolved directly;
- BDF2 is used, keeping the integration history across the substeps, because BDF2 is exact for the constant force of plastic flow.

### Fast-turning boxes (`world/mod.rs`)

A box that turns 0.3 rad in a step (a crate struck by a ball, at 18 rad/s) moves its corners out of the frozen contact manifold and into neighbours without being noticed. While any box would turn more than **0.05 rad per step**, the world splits the step into substeps (at most 16), finding contacts again in each one.

### Plasticity and fracture (`structure/plastic.rs`)

- **Incremental elasto-plastic potential.** Each member's measures are split into elastic and plastic parts by a convex minimisation (soft thresholding at the capacities):
  - axial N_p = σ_y·A;
  - torque T_p = τ_y·Z_t;
  - end moments M_p = σ_y·Z, with the axial interaction (N/N_p)² + |M/M_p| ≤ 1.
  Plastic hinges therefore form and run *within* the step.
- **Material damping is in series with the plastic slider.** It damps elastic vibration, but no strain rate, however high, lifts a member's force above its capacity. Completing the square, the series model is the same soft-thresholding problem for v = s − γ/(1+γ)·ŝ with the capacities divided by 1 + γ. The velocity reference continues the plastic flow (p̂ = p + τ·ṗ) just as the scheme's x̂ continues the motion.
- **Fracture:** a member breaks when its accumulated plastic strain passes the material's elongation at break (S235: 26%).

### Convergence (`structure/minimise.rs`, `rigid_system.rs`)

- A structure's factorised matrix includes the contacts' stiffness at its touching joints. It is refactorised only when that stiffness changes materially (other joints touched, or a block changing by more than half).
- Sliding contacts use the Coulomb secant μ·λ/slip instead of the sticking stiffness.
- The rigid bodies of a group share one sparse matrix, with each contact coupling the two bodies it joins (−β²·J_aᵀ·H·J_b), factorised with the deterministic sparse Cholesky.
- The iteration limit is a safety net of 500. Steps iterate until converged; typical steps need 2–20 iterations.

### Other

- **Sleeping:** a whole group falls asleep after 60 still steps. It wakes on contact with an awake body, or when its load or velocity is changed from outside. Per-body sleep caused wake loops.
- **Air drag** −½ρC_dA·|v|·v is treated implicitly, with |v| taken from the start of the step.

## Validation

| Test | Result |
|---|---|
| Resting ball sinks by m·g/k | within 1% |
| Bounce heights e² for e = 0, 0.3, 0.6, 0.9 | e within 0.02 |
| Head-on collision: momentum; relative speed | momentum to 1e-9; e·v exactly |
| Box on a ramp: sticks below tan φ = μ, slides with g(sin φ − μ cos φ) above | ✓ |
| Stack of boxes, tumbling box | stable; energy never rises |
| Pendulum at 60° | within 0.2% of the elliptic solution |
| Terminal velocity (tanh curve); projectile with drag vs RK4 | within 0.5% |
| Rigid body over 10 s: momentum / energy drift; Dzhanibekov flip | 1.2e-4 / 3e-4; ✓ |
| **Crush:** 1 t mass on a 200×5 mm tube into a wall, δ = (½Mv² − N_p²/2·(1/k + 1/k_c))/N_p | 5, 10, 20 m/s: within 1% (test: 2%) |
| **Tearing:** the same at 30 m/s (32% strain needed) | breaks at the elongation (26%) within 2% |
| Overloaded bar with a 1% weaker element | tears there, and only there |
| Plastic collapse load of a cantilever; permanent set after unloading | ✓ |
| 1.2 t steel frame into a wall at 15 m/s | crushes (8% strain), stops, never passes through, energy only falls |
| 800 kg ball at 12 m/s into five stacks of six crates | energy never rises (it rose by megajoules before the fixes below) |
| Bridge on its piers by contact, 10 t load on top | 10 iterations per step (was 72) |

Convergence study of the crush test (impact substep → error at 5 / 10 / 20 m/s): 1 ms → −7% / −1% / −2%; **0.5 ms → +1% / 0% / −0.1%**; 0.25 and 0.125 ms → within +3% (as the substep shrinks, the end joint's own kinetic energy goes into the crush as well).

## Measured and rejected

- **Damping in parallel with plasticity** (Kelvin–Voigt next to the slider): the dashpot alone carried 1.6 MN in a crash, twice the squash load. The tube crushed 1.6 cm instead of 7 cm.
- **The energy guard reading member energies after the minimisation:** it counted the step's plastic flow as stored elastic energy (13.9 kJ instead of 0.2 kJ) and took the "excess" from the motion. It now uses the minimisation's own stored energy.
- **Backward Euler or a contact dashpot in impact substeps:** each dissipates the work a joint pressed into the wall does on its way in. Measured crush 13–22% short at 5–10 m/s.
- **Newmark in impact substeps:** the end joint chattered against the wall.
- **Restitution impulses on structure joints in substeps:** the light joint was kicked off the wall again and again, and each re-impact threw energy away.
- **Re-stepping groups that end in an unconstrained overlap:** it never triggered once boxes are substepped by their turn, so it was removed.
- **Joint rigid-body matrix without the speculative contacts:** the ball's contacts with the crates were missing from the matrix; the impact still needed 300+ iterations.

## Consequences and limits

- **The penalty contact is soft.** k = 1e8 N/m is 5–10× softer than a Hertz steel-on-concrete contact. A 15 m/s joint sinks up to v·√(m/k) ≈ 7 mm into a wall, and the crash test checks exactly this bound. A stiffer contact needs the contacts in every structure's matrix before every step; that is future work.
- **One beam element per member cannot buckle locally.** Slender members squash at N_p even where Euler buckling (π²EI/L²) would come first. The crash scene uses stocky tubes (80×4 mm) for which squashing does come first. Builders will subdivide slender members (G2). *(Lifted in F5: members split themselves as they are compressed; see the last amendment.)*
- **The material model is idealised.** Steel is perfectly plastic, with no hardening and no strain-rate effect; at crash strain rates mild steel's yield stress rises by up to 1.6× (Cowper–Symonds).
- **Impacts on structures cost substeps.** A 10 t block landing on the 40 m sandbox bridge costs about 19 ms per tick for the few ticks the members yield. At rest under load the bridge costs 1–2 ms per tick.

## Amended in F3 (2026-10-16, ADR-0006)

- **Rolling.** Friction of a sphere's contact acted at the sphere's centre, so it exerted no torque: balls slid or stuck but never rolled (no F2 test rolled a ball). Using the material contact point instead fails too: within one step it lifts off along a cycloid and slips by r·φ³/6, fifteen times the stick distance, so a rolling ball braked as if sliding. Friction of contacts with spheres now acts at the contact point **carried by the body's linearised motion**, p = x + a₀ + φ×a₀ with φ = log(q·q₀⁻¹) (gradient through J⁻ᵀ(φ), exact for the line search); pure rolling slips exactly zero. The normal force still acts through the centre. Between steps such a contact carries its stick offset rather than a material anchor point, so it travels over both surfaces (`world/friction.rs`, `anchor.rs`). Contacts of faces, edges and corners are unchanged bit for bit (all F2 golden hashes hold). *Validated:* a ball rolls down a 15° slope with 5/7·g·sin θ within 1 %.
- **Energy guard slack.** The slack was 10⁻³ of the energy in motion *before* the step plus 1 nJ. A body starting from rest in a field that varies in space (planet gravity) makes a genuine second-order energy error of about 1 nJ on its first step; the guard rejected it and took the "excess" from the motion (a body dropped at the equator fell 0.55 % short in 2 s). The slack now scales with the larger of the energy in motion before and after the step; a step that injects energy E still only gets 10⁻³·E.

## Amended in F5 (2026-10-16): contacts from the materials

The penalty k = 1e8 N/m and its dashpot c = k·h (above, and the first limit in *Consequences*) are replaced. Contacts now follow the materials, up to the stiffness a step can resolve (`world/surface.rs`, `world/contact/`).

### Laws
- **Surfaces.** Every body has a surface (Young's modulus, Poisson's ratio, rounding of its edges): rigid bodies steel, structures their first member's material, fixed geometry and terrain concrete or rock. Two surfaces press with the contact modulus E* = 1/((1 − ν₁²)/E₁ + (1 − ν₂²)/E₂).
- **Points** (balls, joints, rounded edges and corners, crossed edges): Hertz, F = 4/3·E*·√R·δ^{3/2}, R from the features' radii (a corner or edge has its surface's rounding). It continues linearly beyond the depth where its stiffness reaches the limit below (`law.rs`).
- **Faces** (a box on a floor, a wall, another box): a patch over the face's domain (the part that can press, clipped to the other face). The material presses as a flat punch, p = κ·δ with κ = 2·E*/√(π·A), **in series with the limit on each rigid motion of the face**: E(δ) = min over σ of ½·σᵀ·L·σ + ½·κ·∫max(f(δ − B·σ), 0)², B the face's push and two tilts and L their limits (`patch.rs`). A face pressed in all over is exactly as stiff as the limit; a face pressed near a corner or an edge follows the material. *Rejected:* capping κ at limit/A. A crate on terrain twisted by half a millimetre under its face sank 0.19 mm onto two diagonal corners, rocked on the band between them, and fell asleep only after 4–9 s (F4: 1.9 s). With the series it sinks 35–40 µm (a Hertz estimate for its rounded corners: about 36 µm), tips onto a third corner, and sleeps at 2.8 s. The energy is convex (an infimal convolution); its gradient is the material's force at the minimising depths. σ is found by Newton from where the step began, predicted to first order (dσ/dδ = (L + BᵀKB)⁻¹·BᵀK), so E stays a function of the depths within a step.
- **Which faces.** Box on plane: every face turned to the plane, entering smoothly above a cosine of 10⁻³. Box on box: only the most-opposed face of the incident box, clipped to the reference face (all faces facing each other made a pile explode, 27 kJ). Box on ground: the faces turned to the ground, each corner projected once, with one curvature per box (at its deepest corner) as the second-order measure of every corner's gap. A ball on the ground is measured against the ground itself, from its centre to the closest ground point, at every evaluation (`ground.rs`).

### The resolvable stiffness
- Up to the weight of the bodies it joins, a contact is at most m·(Ω/h)² stiff, m their effective mass, **Ω = 15** (`stiffest`); it then sinks the reach d = g·h²/Ω² (12 µm at 60 Hz). Ω = 45 (the old penalty's stiffness for 100 kg crates) was measured as well: no more accurate where it mattered, and slower where structures rest on contacts.
- **Under more load the cap stiffens with it** (`cap.rs`): dF/dδ = F/d up to the most the contact can carry at rest, the weight of its whole group and the loads on it, so every e-fold of load adds d to the depth. Beyond that, the cap is m·(Ω/h)² again: a force no load at rest explains comes from motion (an impact, a pinch) or from an overlap found late, and only a resolvable contact keeps those in bounds (stiffening with any force, a ball smashed into a pile stored 0.3 MJ in one step). For points the law is Hertz's wherever Hertz is the softer at that force (the crossings are in closed form); for faces the layer in series with the material presses per area with the cap's law, so pushing and tilting stiffen together. Measured: a column of thirty crates sinks 1.50 mm, as the law predicts (Σ d·(1 + ln of each contact's load ratio); 10.9 mm with a fixed cap); a 10 t block on a 100 kg crate sinks 0.14 mm (2.45 mm); a frame carrying 100 t stands on its four joints at exactly Hertz's depth, 0.7751 mm against 0.7750 mm (1.21 mm with a fixed cap). Real steel would sink micrometres: what remains is the step's resolution, a reach per e-fold of load.
- The step stops when the most loaded contact's force is right to 1 %: an error of a hundredth of F/k there (its depth for a linear law, the reach for the stiffening one). A hundredth of the depth left a column's forces 4 % off.
- **The limits are set as soon as the step's length is known**, before any energy is measured (`Contacts::set_step`). Measured with the materials' own stiffness, a pile's energy at the start of a step read 2.2 kJ too high, and the energy guard and the impact law's budget allowed a step to gain 100 J against it.

### Damping
- **The dashpot damps the contact's own vibration critically**, c = 2·√(k·m), on the **gap's own rate as the scheme takes it**: its reference is the gap extrapolated along its own history to x̂ (BDF2: (4·g − g₋₁)/3; the history is the positions one step back). Rejected, all measured on a 30 kg ball rolling down a 15° slope over 5 mm bumps:
  - c = k·h (seven times overdamped): the force swung from 108 to 1,125 N for a 284 N weight;
  - the bodies' end-of-step velocity projected on the normal: off by O(h²·x‴) on a path of changing curvature (0.4 mm/s), which the stiff dashpot turned into 12 % of the weight taken from the elastic force friction uses;
  - the gap's mean rate over the step, (g − g₀)/h: a third weaker than the scheme's under BDF2, and 0.3 % off the true force a tick against 0.03 %.
- A contact touching at the start (gap 0) counts as touching: crates set down on each other were missing from the first step's matrix (51 iterations instead of 3).

### Convergence and cost
- The step stops when the deepest contact's force is right to 1 % (a hundredth of its depth), not only at the solver's 0.3 µm.
- The remaining error is estimated more carefully. A rate of convergence measured after a step that did not shrink (ρ ≥ 0.5) is not trusted: a rolling ball moved 0.23 mm, 0.23 mm, then 2 µm, and stopped with its force 4 % off. A step the line search cut short counts as its whole proposed length: one cut to 1/32 stopped 1 % off. A line search that fails at the optimum still refreshes friction's lagged normal force: a box starting on a ramp had no friction in its first step.
- Matrices: a patch couples its corners through its consistent stiffness. That is the Schur complement of the series, projected onto the face's rigid motions; the material's stiffness against twisting, which a rigid face cannot do, would only have given its corners' dashpots a stiffness no motion has. It enters the joint matrix as one 6×6 block per patch, built from rank-one products (Jₖᵀ·nₖ = (n, r × n)) instead of twenty 3×3 products per pair of corners. Every rigid body's block is built in one pass when the step begins (it scanned all contacts for every body). Friction anchors are found through an index by body pair (a scan of all anchors per contact grew as the square of the contacts: 0.15 ms a tick for 400). A patch's last result is remembered for the gradient that follows the line search's energy at the same state.
- **Performance** (interleaved A/B against F4): structures unchanged. Ten stacks of ten crates +31 % (0.586 against 0.447 ms a tick): their contacts are now stiff under load (a contact carrying n times its pair's weight is √n times stiffer), which takes 3.3 iterations per crate and step against 1.5 (24 in the first step from crates set exactly touching, whose matrix is built at no load; then a second, verifying iteration on most steps, the step's stopping noise being a tenth of a micrometre). Before the load-stiffening cap, +7 %. Fifty crates landing on terrain: p50 equal (1.40 against 1.42 ms, with 2.7× the contacts), p99 +68 % (3.2 against 1.9 ms: the landing tick). The bridge on its piers takes 14.5 iterations a step (11.8 with a fixed cap): its joints carry their load at Hertz's stiffness.

### Two faults the piles had hidden
Swept over ball speeds (11.5–14 m/s), the pile scene exploded at 12.5 m/s even before the load-stiffening cap, and at 12 m/s with it; the single 12 m/s test had passed by luck. Both causes are fixed, and the test now runs three speeds:
- **A box spun up within a step.** The world chose its substeps from the spin at the start; a crate struck by the ball during a step turned far enough within it to bury a corner 7.6 cm in its neighbour, 148 kJ of overlap. The step is now checked against the turn it made and redone from the start with the substeps that turn needs when a box turned more than twice 0.05 rad per substep (`world/turn.rs`; a rigid body is saved as a few numbers, `structure/history.rs`: whole clones cost 0.2 ms a tick for 100 crates).
- **A NaN dashpot.** A corner of a clipped face far from where it presses has a negative share of the face's stiffness (pushing the face tilts it and lifts that corner); its dashpot 2·√(k·m) was NaN, the objective NaN, the line search found no decrease and the step stopped at its prediction as if converged. Such a corner now gets no dashpot, and a debug assertion catches a non-finite objective.

### Validation added

| Test | Result |
|---|---|
| Rubber ball rests at its Hertz depth | within 1 % |
| Ball rolling over bumps: its normal force against the analytic m·(g·n + v²·κ), κ of its centre's path | within 3 % of its weight at every tick, 0.5 % on average |
| The ground's curvature (sphere, sine valley) | its shape operator to 1e-6 |
| Crate dropped on terrain | rests on it (no corner deeper than 5 mm), asleep within 6 s |
| Patch: flat punch; in series with the limit pushed and tilted; near a corner it follows the material; forces are the energy's slope; the pressed area's mass is half the integral's Hessian | to 1e-9 – 1e-6 |
| A patch's rank-one blocks against the full 3×3 products | to 1e-12 relative |
| 800 kg ball at 11.5, 12.5 and 14 m/s into five stacks of six crates | the energy never rises (by more than 2e-5 of the start over seven speeds) |
| Bridge on its piers by contact, 10 t load | 14.5 iterations a step (test: < 15) |
| Column of thirty 100 kg crates | sinks Σ d·(1 + ln of each contact's load ratio) within 3 % |
| 10 t block on a 100 kg crate | sinks under 0.2 mm |
| Frame carrying 100 t on four joints | each sinks Hertz's depth within 1 % |
| Point law with every piece (Hertz, limit, stiffening, Hertz, limit) | force the energy's slope, stiffness the force's, depth its inverse, never stiffer than Hertz or the cap; a reach per e-fold |
| A face pressed all over carrying 100× its pair's weight | sinks d·(1 + ln 100) plus the material's compression, to 1e-3 |

### Consequences and limits
- Contacts are no longer softer than the materials, except where the step cannot resolve them; there they sink a reach per e-fold of the load they carry (micrometres to a fraction of a millimetre for anything a builder stacks), where real steel would sink micrometres. In the energy guard's most violent test scene it steps in about as often as with a fixed cap.
- The crash test runs with undamped steel. Steel's material damping (c = 1 ms) over-damps a joint's kilohertz rattle against the wall, which now resolves, and the crush came out 5 % short at 5 m/s; that is a limit of the damping model, not of the contact.
- Golden hashes regenerated: the F2 structure hash once, for the solver's stricter stop (with the old stop the old value came back exactly), and the planet physics hash for each step of this work (documented at the test).

## Amended in F5 (2026-10-16): hardening, strain rate, rigid bodies on structures

### Hardening and strain rate (`structure/flow.rs`, `section.rs`)
The first limit of the material model above is lifted. A plastic increment u of each measure costs the work D(u) = ∫ c dt, its capacity c growing with the increment, so the step stays one convex minimisation; each measure's increment solves a monotone scalar equation (Newton inside a bracket; exact soft thresholding for perfect plasticity, bit for bit as before).
- **Hardening.** The capacities at a step's start come from the flow stress after the member's plastic strain so far; within the step its own increment hardens it at the tangent modulus. S235: the 235 MPa yield plateau (Lüders) to 1.5 % plastic strain, then linearly to the ultimate 360 MPa at 18 % (uniform elongation), flat until it breaks at 26 %. Aluminium 6061-T6 hardens at once to its ultimate 310 MPa at 8 %.
- **Strain rate (Cowper–Symonds).** The flow stress at plastic strain rate ε̇ is the static one times 1 + (ε̇/D)^{1/q} (mild steel D = 40.4 s⁻¹, q = 5; Jones, *Structural Impact*), with ε̇ the increment's equivalent strain over the step. The static strength is taken at the rate of the tensile test that measured it (2.5·10⁻⁴ s⁻¹, EN ISO 6892-1 A1): Cowper–Symonds alone lifts it 8 % already there. Below that rate the flow stress is the static one. Aluminium alloys are taken as rate-insensitive: tests find 6061-T6 nearly so, while the commonly quoted fit (D = 6500 s⁻¹, q = 4) would add a third at crash rates; the materials database is to settle it.
- **Validation.** A bar pulled at 10⁻² and 10 s⁻¹ follows the curve and the rate law within 1 % everywhere it flows (lift 1.099 and 1.666). A bar loaded with 1.2 times its yield load hardens to carry it at 7.7 % plastic strain, within 1 %: approached at about the tensile test's rate, the last percent over 30 s. A 1 t mass on a 200 × 5 mm tube into a wall at 5, 10 and 20 m/s crushes it within 1 % of the same two masses integrated in continuous time (RK4, 1 µs) with the exact rate law: 1.1, 4.6 and 16.4 cm, against 1.6, 7.1 and 28 cm perfectly plastic. Each measure's increment minimises its step potential and its capacity is its work's slope (unit test). The tests of plastic-hinge theory (collapse load, permanent set, tearing at the weakest element, the analytic crush) run on perfectly plastic steel (`Material::perfectly_plastic`).
- **Limits.** Necking is not modelled: past its ultimate strength a member flows on at it until it breaks, where a real bar loses strength at its narrowest point and fails there soon after. Softening would make the step non-convex. Pulled past its ultimate strength, a bar of real steel therefore creeps at the rate the overload lifts it to (with 1.6 times the yield load, 0.3 % a second) and stretches evenly instead of failing at its weakest point. Hardening is isotropic (no Bauschinger effect under reversed loading).

### Rigid bodies on structures, solved jointly (`structure/coupled.rs`)
Each structure has its own factorised matrix and each rigid body its 6×6 one, but nothing coupled a rigid body to the structure it rests on: a bridge on its piers converged in 6.8 iterations a step, with a 10 t block on it in 15.5. They couple only through the translations of the structure's joints in contact, so the joint matrix [A_s + E·D·Eᵀ, E·C; Cᵀ·Eᵀ, A_r] is solved exactly by eliminating the structure: a dense system on the joints in contact and the bodies, with G = Eᵀ·A_s⁻¹·E from three solves with the structure's factorisation per joint, once per step. D is the change of the contacts' stiffness since the structure was factorised (it is refactorised only when that changes by half); without it the system was not positive definite. The joint's rotations stay out of the coupling, as they are out of the structure's contact stiffness. The bridge with the block now takes 1.8 iterations a step (test: < 3). Rigid bodies also touching other rigid bodies stay in the rigid system, and past 48 joints in contact the solves cost more than they save. Structures resting on structures are not coupled yet.

## Amended in F5 (2026-10-16): members that buckle

The second limit above is lifted: slender members buckle, as columns and in frames.

### Bowing (`structure/element.rs`)
The element's stretch measure gains its bowing, L·Σ(2θ_a² − θ_aθ_b + 2θ_b²)/30 per bending plane: the length its cubic shape takes up between its ends. Through it the axial force acts on the bending (the consistent geometric stiffness of a beam column), which the co-rotational form alone gives only for the rotation of the chord. Two elements then buckle within 0.8 % of Euler's load (24 % late without). The F2 structure golden hash was regenerated for it (with the bowing term zeroed in both value and gradient the old hash came back exactly).

### Splitting members as they are compressed (`structure/buckling.rs`)
A member of one element pinned at both ends cannot bend between them. Splitting every member up front would double a frame's elements, most of which never carry much; a member is split at mid-length the first time its compression passes half of min(N_p, P_E), its squash and its Euler load pinned at both ends, unless it is stockier than λ̄ = 0.2 (EN 1993-1-1 §6.3.1.2: it squashes before it buckles). The step that finds it rebuilds the solver after it ends, keeping the history (a new joint takes its member's ends' means); the world gives the new joint a sphere the member's size to collide with.
- **The new joint** starts on the member's cubic (Hermite, from its ends and their tangents), turned to the cubic's tangent there; then it and the member's ends are settled (Gauss–Newton on their degrees of freedom, the members' other ends held) until the forces on each are what its motion needs: on an end, what they were, adjusted for the mass it gave away; on the new joint, its mass times its ends' mean acceleration less its weight (`settle.rs`). Placed on the cubic alone, a joint with a kink under the load the member already carried was out of balance by 2·N·β (4 kN at the foot of the benchmark's 120 m tower), and the member's ends by the moments the kink adds; Newmark's trapezoidal rule does not damp such stiff modes (it is not L-stable), and the tower rang at its new joints at 30 iterations a step. Settled, a split costs one step's rebuild of the solver (36 ms for the tower) and the next steps take 2 iterations. The momentum stays exactly; re-lumped, the member's mass carries less kinetic energy (m/16·|v_a − v_b|² less in the joints' motion), and the energy never rises.
- **Crookedness.** A perfectly straight member would not buckle. The split gives the member the crookedness of EN 1090-2's tolerance, a half sine of L/1000 along its weak axis, as if it had always had it (in its rest shape and its shape alike). Two straight halves carry it as a kink of π²/8·L/1000, which carries the half sine's first mode exactly: the mode the load amplifies, and so what sets the strength. The kink's higher modes (0.23·L/1000 at mid-length) the load hardly amplifies.
- **Halves** carry their member's crookedness and get none of their own; they are split again only to follow its shape, when their compression passes half of their own Euler load (in deep post-buckling, for example).

### Validation (`structure/buckling_tests.rs`)
- An elastic column (3 m of 30 mm round bar) bends as Perry and Robertson say for its kink: within 1 % at 0.6·P_E, 2 % at 0.8·P_E and 10 % at 0.9·P_E (where the two elements' 0.8 % higher critical load shows). Past Euler's load (1.05·P_E) it buckles to more than L/20.
- A column of real S235 at λ̄ = 1 (0.7 m) gives way where its amplified crookedness forms a plastic hinge at mid-length with the members' own interaction of axial force and bending: within 1 % of the kink's prediction and 2 % of the half sine's. That lies above Eurocode 3's curve a, whose imperfections include residual stresses, and below both the squash and Euler's load.
- A member split under 0.6·P_E it already carries is in equilibrium at once: its middle within 0.3 % of the theory for its kink, nothing faster than 0.2 µm/s, one iteration a step.
- A member compressed below half of Euler's load stays whole; a split keeps the momentum and never adds energy, as above.
- `statics::nonlinear` solves again after members split, until none does: a structure in equilibrium only as it was before its splits rang when its motion began.

### Consequences and limits
- Thin walls also buckle locally (a tube's wall folds, a plate between its stiffeners); that is not modelled, so thin-walled tubes are stronger than real in compression. Rolling and welding leave residual stresses, not modelled either: at λ̄ = 1 a column here carries 0.78·N_p, 16–44 % more than EC3's curves a–c (0.67–0.54·N_p), which include them.
- Only compression splits a member: a beam bent about its strong axis is never split, and its lateral-torsional buckling (twisting away sideways) is neither resolved nor tested.
- Members do not collide with each other, nor a member's halves with each other: a column that folds completely passes through itself.
- The crush tests' 200 × 5 mm tube (λ̄ = 0.3) is compared with formulas in which it squashes; they switch splitting off (`Structure::splits`). (A real one, its wall 40 times thinner than it is wide, would fold locally, as above.)
- Cost: the bowing term makes every element's kinematics a little longer: structures at rest under traffic cost 2–6 % more a step than in F4 at the same 2 iterations (interleaved A/B: bridge 1.94 against 1.83 ms, tower 1.83 against 1.73 ms, block 2.64 against 2.62 ms); the bridge still meets its min-spec budget (3.89 ≤ 4 ms).

## Amended in F5 (2026-10-16): convex hulls

Rigid bodies can be any convex shape with flat faces: a rock, a wedge, a cut stone, a casing (`world/hull.rs`). Boxes and balls stay as they were (bit for bit: no golden hash changed).

### The shape (`hull.rs`, `hull_build.rs`)
A hull is built from points by incremental construction: a tetrahedron of extreme points, then every point outside the hull so far replaces the faces it sees by a fan from their horizon. Triangles in one plane (within 10⁻⁹ of the points' extent) are merged into one face, a convex polygon, and corners on a straight edge are dropped: a face presses as one punch of its whole area, and a square split in two triangles would be stiffer than the square (√2 for two halves). The hull is centred on its centroid, where its body's node is; its volume and inertia are exact for a solid of uniform density (fans of tetrahedra, their second moments in closed form). Points in one plane, on a line or not finite make no hull (an error, not a panic).

### Contacts (`hull_narrow.rs`, `hull_sat.rs`, `ground_hull.rs`)
- **Faces as patches.** The patch law (`contact/patch.rs`) blends the depth bilinearly between four corners of a parallelogram. A hull's face has any number of corners, so its patch is the rectangle of least area around the face (one side along a face edge, as the least one always has), and the face, or the part of it that can press, is the rectangle's domain. The domain is now any convex polygon, shared by the patch's four contacts (`shape::Domain`), where it was at most eight corners in each contact.
- **Hull against plane, against the ground:** as a box: every face turned towards it within the margin is a patch (on the ground measured as in the revision below), and points at most 0.5 m apart inside the faces and along the edges (apart, so that no point presses twice) are bumps where the ground rises beyond what the patches see.
- **Hull against hull** (and against a box, as a hull): the separating-axis test of convex polyhedra (Gregorius, GDC 2013): the faces of each, and the cross products of edge pairs whose arcs on the Gauss map cross (only those can touch edge to edge). Across a face, the other hull's most opposed face is clipped against the reference face's sides, a patch; across an edge pair, the closest points of the edges, a point contact of crossed edges. Edge axes are taken only when 0.1 mm better than the faces, as for boxes.
- **Ball against hull:** the hull's closest point to the centre (a face, an edge or a corner), or from inside the nearest face.
- Hulls are turn-checked like boxes (`turn.rs`): flat faces need the substeps a box needs.

### Validation (`world/hull_tests.rs`, `hull_contact_tests.rs`, `hull_motion_tests.rs`, `hull_ground_tests.rs`)
- Construction: a box of points (with points inside, on faces and on edges) gives eight corners and six quadrilaterals, its volume and the box's inertia to 10⁻¹²; 400 points on a ball give a closed convex hull (V − E + F = 2, every point inside, every face convex and counter-clockwise) of 95–100 % of the ball's volume; a regular tetrahedron its volume and isotropic inertia m·a²/20 to 10⁻¹²; an octagonal prism two eight-cornered faces.
- A hull made from a box's corners touches exactly as the box (pressing polygon, area, normal, edge contacts, ball contacts and their features, to 10⁻⁹) in 2,000 random poses against a box, a plane and a ball, and 500 on flat ground; over a 1 cm bump it finds the bump as the box does.
- A triangular face presses as a punch of its area: 2·E*·√(A/π) to 10⁻¹².
- A wedge on a ramp sticks below the friction angle and slides with g·(sin θ − μ·cos θ) within 2 %; a hexagonal prism on a 35° ramp tips over its lower edge, which holds to 0.5 mm along the ramp (pressed in 0.1 mm, it rolls on the ramp as it turns); twelve rocks dropped in a heap, and one on bumpy ground, come to rest without gaining energy.
- The client's rock pile (`--physics rock`, view `rocks`): eighteen rocks, a hexagonal prism and three wedges on a 35° ramp, up to 300 contacts. (Recorded here as 0.4–5.5 ms a step; measured again in F6, its busiest seconds took 35–65 ms a step and its worst 351 ms: see the F6 amendment on Newton's method.)

### Consequences and limits
Three limits of the contact model showed with hulls (boxes behaved identically, measured side by side). Two are fixed in the revision below; one remains:
- **Sliding on part of a face was over-braked** (a 0.4 × 1 m box slid 11 % short at 40°): fixed, the forces of a patch's lifted corners now reach the bodies.
- **Landing tumbles slid** (a cube tumbling down a 50° slope slid on after each landing): fixed, the impact impulses carry Coulomb friction.
- **Sharp corners on flat faces are soft.** A face pressed only near a corner against a plane or another face follows the punch law of its whole face (p = κ·δ, κ = 2·E*/√(π·A_face)), so the force grows as δ³ where a rounded corner follows Hertz (δ^{3/2}): a rock resting on a corner sinks 0.6–1.1 mm where Hertz on a 1 mm rounding gives about 30 µm. Faces on curved ground press as a punch of the area they press with (below), Hertz's law to 6 %; on planes the whole face's law stays (*Rejected* below: the pressed area's own punch there turned a sliding tall box unstable in yaw).

## Amended in F5 (2026-10-17): the contact law revised

Contacts were audited scene by scene against energy, Coulomb's law and coming to rest (the hulls' limits above, a sweep of balls smashed into crates, rocks dropped on bumps, boxes set down in hollows). What was found, and what replaces it:

### Faces (`contact/patch.rs`, `coupling.rs`, `dashpot.rs`)
- **A lifted corner's force reaches the bodies.** A patch's corner carries its share of the face's pressure while lifted itself (the face pressed between the corners); applied only where the corner was pressed in, a face pressed unevenly rested on more than it carried (a tall box stuck on a 20° ramp on 965 N for its 921 N) and slid short. Now its corners' forces add up to the weight to 4·10⁻⁵, and narrow boxes slide by Coulomb's law within 1 %.
- **Face dashpots** damp the points of the face pressed at the step's start (the midpoints of its triangles' edges, a third of each triangle's area), the face's critical damping spread over them per area and scaled by √f for a face pressed over a part f (as stiff as f of the face, critically damped for that). Damped at its corners, a lifted corner's force went undamped (a rock jittered at 1 cm/s for ten seconds); shared by area alone, a crate pressing on part of its face was under-damped; clamped at every evaluation, points switched on and off within the step; damped as stiff as the face (C = τ·K) unclamped, faces lifting off were held and a landing cube's spin turned back.

### Impacts (`impact.rs`, `stepping.rs`)
- **Impulses carry Coulomb friction** (`rub`): each contact's tangential impulse stops its point sliding, up to μ times its normal impulse, before the step and in the rebound. A cube tumbling down a 50° slope (μ = 1.5) keeps turning the way it tumbled through each landing, slower.
- **The rebound's budget is the bodies' energy.** Contacts found pressed into each other at the step's start hold energy that never existed (a ball found inside a crate held 290 kJ), which the rebound was allowed to give back.
- **Impulses see the gaps as measured** (`Contacts::measured_gaps`), the ground's curvature included.

### Normals that turn, and closest points (`contact/turning.rs`, `measure.rs`)
A contact's normal turns with the body whose face or feature it is (n = q·q₀⁻¹·n₀; for crossed edges n = e_A × e_B, each edge turning with its body), the gap's slope gaining the torques of the turning normal. A ball against a box or a hull is measured to the shape's closest point at every evaluation (`Measure::Closest`). Frozen for the step, an edge swept past its plane: a ball smashing into crates ended a step 1.6 mm into a crate's edge with 205 kN, two crates' crossed edges 567 kN into each other, energy no impact had. In a sweep of balls smashed into piles, the worst energy gained in a step fell from kilojoules to 4.5 J.

### Faces on the ground (`ground.rs`, `ground_hull.rs`, `ground_rise.rs`)
- **Each face takes the ground under its centroid** (a point that moves with it smoothly): it presses if turned towards the normal there, and its patch weighs its facing with that same normal. (Taken where the whole body was deepest, a face half a metre away saw the curvature of other ground; gated by another normal, a steep face entered all at once carrying 120 N.)
- **Over curved ground a face presses with the depth measured at the corners of its triangles** (`Domain::Sampled`, `ground_sample.rs`): the face fanned from its centre into triangles split level by level until the depth interpolated across each is within 1 mm of a quadratic's (|eᵀSe|/8 along an edge, the bump threshold; S the ground's curvature across the face, fitted to the normals under its outline; at most 1024 triangles), each corner a material point of the face measured to the ground under it to second order (as `Measure::Curved`), and linearly between. The patch adds, per triangle corner, how far the measured depth lies off its corners' blend; its energy is then a function of the measured depths alone, and its forces act at the measured points (the four corners share out the load for friction and the step's matrix). Within a step each point's depth moves as the ground under it does, so a step ends where the next one finds it. The face's dashpot damps the measured points' own rates.
- **Over curved ground a face presses as a punch of the area it presses with**, κ = 2·E*/√(π·A_p), A_p taken at the step's start (at the cap's reach for a face just touching) and kept for the step, so its energy stays convex: for ground rising as r²/2R, κ·∫δ = √2·E*·√R·δ^{3/2} against Hertz's 4/3. (As the whole face's punch, a 2 m crate on a 15 cm bump sank 11 mm into it.)
- **A face counts if any part of it is within the margin**: its corners less the most the ground can rise under it, then the least measured depth. (By its corners alone a face over a hump came and went with the margin from step to step, a bump of its own pressing in its place, and a rock was kicked every other step.)
- **Corners are measured to the ground under each** (its normal there, so faces sharing a corner see the same ground there and friction acts across the ground under it); on flat ground the plane under the face, as a plane's contact. (Measured across the normal under the face, a box in a hollow pressed a corner with two faces whose grounds' normals were 14° apart, and crept 2 cm in 5 s on a 12° slope with μ = 0.7.)
- **Each point is projected once per body and step** (`ground_sample::Feet`): the face's centroid, outline and triangles' corners, its patch's corners (shared with a box's other faces). Projecting every face's centroid, taking the curvature from normals 2 cm apart and every patch's corners apart, 50 crates landing on the terrain spent 2 ms a step finding their contacts.
- **Curvature below 10⁻⁹ m⁻¹ is flat** (a sphere a million kilometres across): the differences of normals 2 cm apart carry rounding (3·10⁻¹³ m⁻¹ on a plane).
- **A hull's faces keep their rectangles** (`hull::Face::rectangle`, found once in the hull's frame): found again each step in the world's frame, a rock's face whose rectangles tied cycled through three, its corners' friction anchors were lost each time, and it crept.
- **Bumps** (points under a face where the ground rises beyond what its patch sees) press with what they rise beyond it (`RawContact::inset`): counted from 1 mm above the plane, under a face resting on the ground a bump began 1 mm deep and kicked a rock on a hump every few steps.

### Rejected
- **The pressed area's punch on every face** (κ = 2·E*/√(π·A_p) at every evaluation, also on planes): scale-free at corners, but a tall box sliding with its rear edge nearly lifted turned unstable in yaw, 10⁻³ rad/s growing to 0.07, where the whole face's law damps it out. Kept for faces on curved ground, frozen per step.
- **One quadric for the ground under a face** (its curvature where the body was deepest, under its centroid, or where it pressed most, with the quadric's rise across the face): off by millimetres 20 cm from where it was taken on 10 cm bumps 2 m apart, and the error moved with the face: a rock was thrown off a face with 5 kJ every third step, kicked every few steps, and turned on a bump past where its face pressed at a milliradian a second; taken where the face pressed, a slab over a small bump on flat ground saw the bump's curvature under all of it and sank 15 cm.
- **Rises sampled from the ground at detection and moved with such a quadric within the step**: the quadric does not follow the ground, and energy appeared from step to step.

### Validation added
- `friction_tests.rs`: an unevenly pressed face rests on its weight (to 10⁻³; 4·10⁻⁵ measured); narrow boxes slide by Coulomb's law (1 %); a tumbling cube keeps its spin through its landing.
- `hull_ground_tests.rs`: a rock dropped on bumps falls asleep within 6 s (2.3 s measured) without gaining energy; a box at rest stays put to 10 µm on a flat slope, a ridge's flank, dropped on bumps and set down in a hollow; a slab over a bump finds it as a box does.
- `planet_tests.rs`: on flat ground a box has the plane's contacts and forces (4·10⁻⁶, as the steps stop at 1 %) and slides as far (0.1 %); a 2 m crate on a 15 cm bump rests on it, the bump's top within 1 mm of its face.
- `patch_tests.rs`: forces are the energy's slope however the face presses (finite differences), and the material's energy is convex.

### Consequences and limits
- **Faces over curved ground cost points.** A face measures 5–40 points of the ground per step on the reference terrain, each a projection, and its patch integrates its triangles at every evaluation: 50 crates landing on the terrain take about twice as long a step as in F4 (measured below); flat ground costs nothing more.
- **Settling is overdamped.** Contacts are critically damped at their resolvable stiffness, far above a body's rocking frequency: a box dropped on bumps rocks to rest over 2 s instead of a fraction of one.
- **Sharp corners on flat faces are soft** (above).

## Amended in F6 (2026-10-17): the joint solve of bodies on structures

Found while building F6.2's fire scene (a 9 t block on a 4 m steel frame on two piers): after the block landed, the frame and the block hung motionless 2–3 mm above the piers, the piers carrying nothing, every step "converging" in one iteration where it began. Two faults, one hiding the other:

- **The joint solve went uphill** (`structure/coupled.rs`). It corrects the structure's factorised matrix for the contacts' stiffness since it was factorised: D = (stiffness now) − (stiffness factorised). It took the factorised stiffness at this step's β², but the factorisation holds it at the β of the solver's own scheme (Newmark's h/2; contact groups step with BDF2's 2h/3, impacts with backward Euler's h): D took away 78 % (BDF2) to 300 % (backward Euler) more than the matrix had, and the joint matrix was not positive definite (measured: G⁻¹ = 17,300 at a joint whose factorised contact stiffness D assumed was 22,400; the rigid body's Schur complement negative). The factorised stiffness is now taken at the factorisation's β², and turned with the node as `precondition` applies it (R·H·Rᵀ; unturned it was off by ±k·θ). Measured after: not one uphill direction in the bridge, the frame or the test suite (before: ~240 per bridge step).
- **A failed line search read as convergence** (`structure/minimise.rs`). An uphill direction fails every line search; the minimiser then stops as if at the minimum. It now checks that each direction goes downhill (r·d > 0, one dot product) and, should one not, takes each member's own positive definite matrix for it and restarts L-BFGS.

**Consequences.** Bodies resting on structures settle where their forces balance (the frame's piers now carry 95.2 kN of the frame's and the block's 95.3 kN, the block's contacts its 88.2 kN). A structure rings as its damping says: the 40 m bridge under its 10 t block swings at 1.3 Hz, down 4 % a cycle (ζ = 0.6 %), where the false convergences had stopped it within a second; its steps take about 4 iterations while it swings (12 without the joint solve), the test now measuring that (F5's "about two" was the false convergence's). The structures benchmark (supported structures, no joint solve) is unchanged (A/B against F5's commit: bridge p50 2.13 vs 2.12 ms, heat-soaked machine).

## Amended in F6 (2026-10-17): Newton's method for piles of rigid bodies

The sandbox's rock pile and stacks lagged: the physics ran slower than real time while the frame rate stayed at 180. Measured again, F5's rock pile took 35–65 ms a step in its busiest seconds and 351 ms at worst, the stacks 145 ms at worst. Its groups needed 50–500 iterations a step.

### The contacts' stiffness at each iteration's state (`Coupling::relinearise`, `contact/coupling.rs`)
- A group of rigid bodies alone takes its contacts' stiffness at every iteration's state into its matrices and its joint system: Newton's method proper. Groups with structures keep the step's stiffness.
- **Friction:** its exact Hessian at the slip, μλ·[f₁·(P − ŝŝᵀ) + f₂·ŝŝᵀ]. A sliding contact has no stiffness along its slip; the secant μλ/|slip| in every direction made a sliding rock's matrix far too stiff along its motion. Without the exact Hessian the pile took 380,000 iterations in its first four seconds; with it about 80,000.
- **Points:** Hertz's stiffness at the depth now; none apart. The step's anticipating stiffness kept for a contact come apart made a rock's matrix a hundred times too stiff (a third more iterations).
- **Faces:** the patch energy's Hessian at the corner depths now (`Patch::tangent`: the material as pressed there, in series with the limit as loaded there), and the face dashpot's exact form. The dashpot damps for the whole step, lifted or not: left out of a lifted face's matrix, Newton missed most of a rock's stiffness against the ramp.
- **Cost:** the joint system keeps its symbolic analysis while it couples the same blocks, and the matrices are not made again within ten tolerances of the end. A face's motion σ comes from the last evaluation of its law.

### The stop (`structure/minimise.rs`)
- The tolerance is no finer than the positions resolve, 4ε times the farthest coordinate. A barely touching contact asked for 10⁻¹⁹ m, and the step ran to its 500th iteration.
- A shorter line-search trial that is not lower than the start is rounding's. Taken as before (≤), it moved nothing, again and again.

### Energy found anew (`world/left.rs`)
- Contacts are found anew every step, and found anew they may hold more energy than they did: a crate's faces fitted again to the curved ground under them gained a millijoule a step. The exact solve turned it into motion, and the crate rocked on a hillside at 3 mm/s for good. The old inexact stop, 3·10⁻⁷ m short of the solution each step, had damped it.
- The world now keeps the energy each contact held at the end of a step (saved from version 5). The next step's guard takes any surplus of the contacts found anew from the kinetic energy, without redoing the step. Impacts keep their own books (their contacts at the start never counted).

### Drilling friction (`contact/twist.rs`)
- A ball's contact between rigid bodies resists spinning about its normal: Coulomb's torque μ·N·3πa/16 over Hertz's circle of radius a (Lubkin, 1951), smoothed over the twist a spin of 10⁻³ rad/s makes in the step. A point contact's friction has no lever about its normal: a ball or an avatar left spinning about the vertical spun on.

### Tried and rejected
- **The contacts the full step would press, taken into the matrix (an active set).** It cut fewer line searches at first, but took 60 % more iterations once separated contacts left the matrix, and the client's prediction of its avatar parted from the server's more often.
- **Faces and edges** (`sat.rs`, `hull_sat.rs`). An edge pair's axis is taken over a face's only when better by 0.1 mm. A rock rattled every step between an edge contact carrying 800 N and the patch of a steeply inclined face, a sliver carrying 1 N.
  - An edge bias of at most half the face's penetration stopped that, but let crates knocked edge-on switch to their edges where their patches carried, and a ball smashing stacks gained 47 J. A hull and a box of one shape must touch alike, so it could not be for hulls alone.
  - Keeping the last step's choice unless the other was clearly better (hysteresis) held rocks on an edge where faces lay flush, and the pile crept for good.
  - Both reverted (see the limits).
- **L-BFGS** cleared on every new matrix, or pure Newton without its pairs: three to four times slower.
- **Faces evaluated in parallel** within a group: slower. The patches are cheap, and the groups already run in parallel.

### Measured (development machine)
| | before | now |
|---|---|---|
| rock pile, busiest second | 35–93 ms a step | 14–15 ms |
| rock pile, worst step | 351 ms | 44 ms |
| rock pile, settled (20 s) | 2–3 ms, 15–20 of 22 awake | 1–1.2 ms, 9 awake |
| stacks scene, worst step | 145 ms | 10 ms |
| ten stacks of ten crates settling (spikes, A/B) | 0.97–1.02 ms | 1.03–1.04 ms (+3–6 %) |

### Validation added
- `physics_rocks.rs` (client): the pile's first four seconds within 120,000 Newton iterations (about 80,000; 1.1 million before).
- `friction_tests.rs`: a ball spinning on the ground slows at a steady rate (Coulomb's, not a dashpot's), its torque that of a lever of 0.1–5 mm (0.9 mm), then stops and sleeps.
- `creeer_sim`: the avatar sleeps after rolling, a crate on the hills rests through time warp, and the version 5 save fixture. The golden hash is regenerated.

### Limits (`docs/future-improvements.md`)
- Edge contacts and face patches give different forces for the same overlap: nine rocks of the pile stay awake, about 1 ms a step.
- Drilling friction keeps no anchor: under a torque below its limit a ball creeps at up to 10⁻³ rad/s. Only between rigid bodies.
- Contacts found anew still differ from step to step; only their surplus is taken back.

## Amended in F7 (2026-10-18): removing bodies keeps the contacts' energies

`World::retain_bodies`, `remove_body` and the new `take_bodies` (simulation LOD, ADR-0019) renumber the elastic energy the last step left in the contacts (`world::left`), as they already renumbered the friction anchors, instead of clearing it for every body: contacts of the bodies taken out go, the rest follow their bodies' new indices. Before, any removal (a cleared world, a vessel packed) made every resting stack's contacts be found anew without the record the energy guard holds them to.
