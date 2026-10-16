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
- **One beam element per member cannot buckle locally.** Slender members squash at N_p even where Euler buckling (π²EI/L²) would come first. The crash scene uses stocky tubes (80×4 mm) for which squashing does come first. Builders will subdivide slender members (G2).
- **The material model is idealised.** Steel is perfectly plastic, with no hardening and no strain-rate effect; at crash strain rates mild steel's yield stress rises by up to 1.6× (Cowper–Symonds).
- **Impacts on structures cost substeps.** A 10 t block landing on the 40 m sandbox bridge costs about 19 ms per tick for the few ticks the members yield. At rest under load the bridge costs 1–2 ms per tick.
