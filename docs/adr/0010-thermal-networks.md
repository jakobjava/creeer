# ADR-0010: Thermal networks

- **Status:** Accepted (2026-10-17)
- **Context:** F6.2 (`docs/f6-plan.md`). Parts heat and cool: by conduction through each other and their joints, by convection to air and water, by radiation to each other, the sky, deep space and fires; they melt; heat comes from other domains (Joule heating, friction, reactions, plastic work). Heat must change what structures do (a heated member lengthens and weakens). Budget: 20,000 nodes at 20 Hz within 0.5 ms per 60 Hz tick on the minimum-spec CPU (1.5 ms per 20 Hz step, projected as measured / 0.5).

## Decision

### Lumped nodes with enthalpy as their state (`creeer_thermal`)

- A node is a **mass of a material** (or a temperature held from outside: air, ground, sky, a fire). Its state is its **specific enthalpy**, H(T) the exact integral of the material's specific heat (piecewise linear in the data, piecewise quadratic here) plus its heat of fusion spread over its melting range (a pure substance over 1 K around its melting point). Temperatures follow from enthalpies.
- **Each step changes every node's enthalpy by exactly the heat booked into it** (sources, other domains, the links' flows at the step's final temperatures): the ledger closes to rounding however loosely Newton's iteration is stopped (2·10⁻¹⁴ relative after 220 steps of a 21,000-node hall), and melting takes exactly the latent heat.

### Backward Euler, Newton per island (`island.rs`, `island/iterate.rs`, `kernel.rs`)

- **Backward Euler**: unconditionally stable for the stiffest networks (thin parts on thick ones, metal joints), first order in time. (BDF2 was in the plan; at 20 Hz against parts' time constants of minutes, first order is well within the correlations' accuracy, and BE's monotonicity keeps melting fronts and radiators from overshooting.)
- **Conduction and convection coefficients are lagged**, as thermal network codes do (SINDA, TRNSYS): taken at the step's start and re-evaluated only once the temperatures they depend on have moved enough to change them by a fraction of a percent (conduction's mean temperature by 0.5 K: a metal's k moves ~0.1 %/K; convection's film by 1 K or its ΔT by 2 % + 0.05 K: under 0.7 %, against the correlations' own ±10–20 %). The temperatures they multiply stay implicit, so the step stays unconditionally stable, and a network of them alone is linear: one solve a step, one factorisation for good.
- **Radiation is exact**: σF(T₁⁴ − T₂⁴), to a held node with its derivative 4σFT³, between two masses with its secant σF(T₁² + T₂²)(T₁ + T₂), which keeps the matrix symmetric for the Cholesky solve.
- **Melting**: an iterate crossing an end of a melting range lands on it (H(T) has corners there; Newton across a corner jumped back and forth across the range), and the corner's capacity is the larger of its two slopes, so the next step from it is never too long. Ice warming through its melting: at most 5 iterations on the step that crosses a corner, 1–2 otherwise.
- **Newton starts from the last step's trend** carried on (its error is the trend's change), and **stops once the residual over each node's capacity is below 10⁻⁷ K**: that bounds the temperature error (C⁻¹·J has eigenvalues of at least one), where the diagonal-only estimate first used could underestimate a stiff network's smooth modes by the condition number.
- **Factorisations are reused** while the matrix is unchanged; a large island also keeps a stale one while every entry is within 5 % and each iteration still reduces the error tenfold, when refactorising costs more than a solve (`factor_to_solve`, ADR-0009). Small islands refactorise when anything changes (an exact Newton step is cheaper than an extra solve).
- **Islands** step in parallel; an island of 2048 nodes or more also does its per-element work in parallel (links, then nodes, each element's result independent of the others: the same bits either way, tested). Islands sleep when nothing in them moves by more than 10⁻⁶ K a step.

### Physics (`fluid.rs`, `convection.rs`, `environment.rs`, `contact.rs`)

- **Fluids**: air from Incropera's Table A.4 (100–1000 K; Sutherland's laws beyond, ±2 % to 1900 K; JANAF's undissociated air for cp) with ideal-gas density; liquid water from Table A.6 (its negative expansion coefficient below 4 °C included).
- **Convection**: Churchill–Chu (vertical plates: its laminar form below Ra 10⁹, which matches Ostrach's boundary-layer solution to 1 %, blended over a decade into its all-Ra form, which reads 15–30 % high when laminar), Lloyd–Moran and Radziemska–Lewandowski (horizontal plates), Churchill–Chu (cylinders), Churchill (spheres); forced flow by Blasius / mixed plate, Churchill–Bernstein and Whitaker; combined by Churchill's cube sum (a sphere's conduction limit Nu = 2 once). A horizontal plate's face is the side buoyancy lifts fluid from: colder facing up is warmer facing down (Incropera §9.6); water below 4 °C turns that over.
- **Surroundings**: sunlight at any distance from the Sun, through an atmosphere by Kasten–Young air mass and Meinel's attenuation; absorbed by a face by its absorptivity and n·s; the clear sky's temperature by Swinbank (clouds blending to the air's); view factors (coaxial discs, aligned and perpendicular rectangles) and the grey-body exchange factor; the ISO 834 standard fire with EN 1991-1-2's α_c = 25 W/(m²·K) and ε_fire = 1.
- **Joints**: Yovanovich's plastic contact conductance plus gas in the gaps (a bolted steel flange in air at 1 MPa: 1–10 kW/(m²·K), as measured for such joints).
- **Materials**: one source of thermal truth, the engineering materials database (`data/materials/engineering.ron`: EN 1993-1-2's specific heat with its austenite peak, conductivity; ice turning into liquid water's properties above 0 °C, IAPWS-95).

### Heat in structures (`creeer_physics::structure::heat`, `creeer_sim::heat`)

- A member at temperature T lengthens by L·∫α dT from 20 °C (the material's expansion coefficient integrated exactly: EN 1993-1-2's thermal strain to 5·10⁻⁵) and has its material's E, G and strengths at T (EN 1993-1-2's reduction factors). A held member pushes (E·A·ε_th, tested), a heated frame softens (deflection 1/k_E, tested), a member split by buckling keeps its elongation.
- `MemberHeat` makes a structure's members a network: each its mass of its material, members meeting at a joint conducting through their half-lengths, exposed to air (convection by orientation, radiation to the surroundings) or engulfed in a fire (EN 1991-1-2), never both. It sets the members' temperatures after each step, refactorising the structure's solver once a member's stiffness has drifted by half, and follows members the structure splits.
- **Saves**: version 3 (F6) adds a member's thermal elongation and a section's perimeter (versioned decoding: a version-2 save reads as before, its sections' perimeters as round, exact for the tubes and bars earlier sandboxes built).

### Validation (`creeer_thermal` tests, `creeer_sim::heat_tests`, `creeer_physics::structure::heat_tests`)

Lumped cooling (backward Euler's exact recurrence to 10⁻⁹ K; the exponential); a steady rod (linear profile to 10⁻⁶ K); a semi-infinite slab against erfc (0.5 % of ΔT); a plate in sunlight at 1 AU radiating from both faces against (S/2σ)^¼ = 331 K (10⁻⁴ K); ice melting (the plateau holds the latent heat, every joule supplied in the water); the one-phase Stefan problem against Neumann's solution (3 %); the ledger with every link kind, sources, heat received and changing boundaries (10⁻¹⁰); air and water against their tables; the correlations against Ostrach, McAdams, Blasius, Hilpert and Ranz–Marshall within their scatter; view factors against Incropera (and a cube's faces summing to 1); an unprotected steel tube in the ISO 834 fire against EN 1993-1-2 §4.2.5.1's incremental method (within 2 K over an hour); determinism (parallel against one thread, bit for bit); a linear island's one factorisation and one solve a step.

### Measured (`cargo xtask perf`, section `thermal`)

A factory hall of 21,000 nodes awake (500 machines of 20 steel parts on frames on a concrete floor, one island of 11,000 nodes; 1,000 aluminium carts of 10 parts; convection, radiation, sources): **0.63–0.65 ms a step** (p50), projected 1.27–1.30 ms ≤ 1.5 ms. The big island alone 0.61 ms (its Cholesky solve ~0.15 ms, latency-bound along the elimination tree; two evaluations ~0.2 ms; coefficient refreshes ~0.09 ms), the carts alone 0.17 ms.

## Consequences

- **Heat is time-lapsed in the sandbox scenes** (shown on screen): a block cools for an hour, a fire takes a quarter of one; structures' response to heat is slow beside their motion, so the frame sags as it would, only sooner.
- **Two-phase heat transfer** (boiling, condensing) waits for F6.5's equilibria; **radiation between many surfaces** is pairwise (no radiosity network yet); **internal temperature gradients** within a part need it split into nodes (a member is one node).
- **Found and fixed on the way** (ADR-0005 "Amended in F6"): the joint solve of rigid bodies resting on structures went uphill, which froze bodies mid-air and damped a bridge's ringing; the fire scene's frame first hung 3 mm above its piers.
