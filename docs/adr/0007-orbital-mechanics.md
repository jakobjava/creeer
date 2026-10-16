# ADR-0007: Orbital mechanics and the solar system

- **Status:** Accepted (2026-10-16)
- **Context:** F4 (orbital mechanics & solar system). The plan asked for Kepler rails for the bodies, restricted n-body propagation for ships, frame switching, a floating origin, time warp, a map view, a generated solar system and an orbit validation suite; the sandbox: put a test mass in orbit and fly to the moon. Before starting, the user asked for more: real n-body physics for the bodies themselves, so that one day huge machines can move asteroids, moons and planets, and Lagrange points that matter ("very realistic, hard to master, infinite possibilities, really performant").

## Decision

### Everything pulls on everything that matters (`creeer_orbit`)

- **Major bodies** (star, planets, moons, the few asteroids heavy enough to swing a small moon in a close pass) are integrated together under their mutual gravity. There are no Kepler rails: orbits precess, resonate and can be changed. A body is a point mass; its spin turns about a fixed axis.
- **Minor bodies** (belt asteroids, Trojans) and **ships** are test particles on rails: they feel every major body and do not pull on them (the restricted problem; a 1e12 kg asteroid moves a moon by less than its own rounding). Particles that must pull on each other — a ship towing an asteroid by gravity — form a **group** integrated together (Newton's third law inside it).
- **Pushes**: a constant force on a body over an interval — an engine, a mass driver, the reaction of what stands on it. Integration steps end exactly at push edges.

### One integrator for every trajectory (`radau`)

- **IAS15** (Everhart 1985; Rein & Spiegel 2015): 15th order, Gauss–Radau substeps, predictor–corrector to convergence. Its errors stay below f64 rounding at ε = 10⁻⁹, so energy only random-walks (Brouwer's law), and close encounters are resolved by shrinking steps — the "cosmic billiards" a symplectic Wisdom–Holman map cannot do.
- **Step control** after Pham, Rein & Spiegel (2024): τ² = 2|a|²/(|ȧ|² + |a||ä|), step (ε·7!)^{1/7}·τ ≈ T/36 on a circular orbit. Where a particle's acceleration passes through zero (the middle body of the figure-eight) this asked for ever shorter steps forever; each particle's time is now at least |ȧ|/|ä|, which is the same 1/ω on a circle.
- **Kahan compensation** of positions and velocities, kept through every stop and restart: a 1 GN engine on a 3.7·10²² kg planet adds 0.003 of the velocity's last bit per tick, and plain sums would lose all of it (tested).
- Steps cut short to land on game ticks or push edges carry their fit over unstretched; a fit stretched more than 4× is restarted from zero (the coefficients of a very short step are rounding noise, and stretching multiplies bₘ by q^{m+1}: the tick-cut test was 0.9 mm off before).

### The ephemeris: truth up to now, forecast after (`ephemeris`)

- The majors' integrator runs ahead of the present; every step is stored with **its own fitted acceleration polynomial** (dense output exact to integrator precision). Quintic Hermite interpolation between step ends was tried first: with steps of T/36 it was 27 m off on the Moon's orbit; the stored polynomial is under a millimetre. ~290 bytes per body per step: 22 bodies, a year ahead, ~6 MB.
- A new push drops the forecast after its start and restarts from the stored state before it (with its compensation), recording a pair of samples at the edge (the acceleration jumps). The past is never rewritten.

### Analysis and the map (`kepler`, `analysis`)

- Universal-variable Kepler propagation (one code path for ellipses, parabola and hyperbolas; Newton's method safeguarded by bisection — plain Newton crawled on near-radial hyperbolas), orbital elements with defined fallbacks for equatorial and circular orbits, mean anomaly ↔ true anomaly.
- Sphere-of-influence hierarchy (for display: which body something orbits), Hill radius, Roche limit, Hohmann transfer, escape speed, the five Lagrange points of any pair, and a **co-rotating frame** (Y-up: y along the pair's angular momentum) in which they stand still.
- Predicted paths are computed, not drawn from elements, and drawn in a chosen plotting frame: centred on a body, or turning with a pair (as in the Principia mod).

### Procedural systems (`generate`, `data/systems/default.ron`)

- Data-driven: a star, the hand-made home planet on its given orbit, planets spaced outwards and inwards by period-ratio draws and at least 12 mutual Hill radii apart, the first planet beyond the snow line a gas giant, moons between twice the Roche limit and 30 % of the Hill radius (8 mutual Hill radii apart, at most 40 % of their planet's radius, tidally locked), a belt with three massive asteroids and 150 small ones, Trojans scattered about every giant's L4 and L5. KSP scale means small worlds with ordinary gravity, so planets and moons are drawn with ~10× real densities (home: 70,000 kg/m³); asteroids keep real ones and spin no faster than 1.5× their surface orbital period (the rubble-pile spin barrier).
- **Stability checks:** every candidate is integrated — the planets (each carrying its moons' mass) for 1,000 orbits of the innermost, every moon system with the star's tides for 200 orbits of its outermost moon, in parallel — and replaced by the next seed's candidate if any semi-major axis drifts more than 2 % (moons 5 %).
- Terrain for generated bodies comes from templates (`rocky.ron`, `moon.ron`, `asteroid.ron`) whose relief and noise wavelengths scale with the radius; airless worlds get their own geology: regolith over anorthosite highlands and basalt plains, with high-titanium (ilmenite) basalt as an ore.

### The physics bubble and its frames (`creeer_sim::universe`)

- **Surface frame** (within 15 % of a radius, at least 10 km, of a body's surface; leaving takes 20 % more): turning with the body; terrain is static ground. **Orbit frame** otherwise: free-falling with a point on rails next to the focus (a floating origin, re-based when the focus is 1 km or 20 m/s away). The world is moved between frames by an exact change of coordinates (positions, velocities, orientations, spins).
- **Gravity in the frame** (`creeer_physics::gravity`): the anchor's own field, the centrifugal and Coriolis terms, a uniform field opposite to the anchor's push, and the **tides of every other major body** (their pull here minus at the origin, potential differences without cancellation). In free fall gravity is purely tidal.
- Three precision fixes the frame tests found:
  - **Coriolis** from the velocity extrapolated to the end of the step (Newmark pairs start and end forces): a ship at 3 km/s in the rotating frame drifted 3.8 mm in 10 s against the inertial computation, now 2.4 µm.
  - **Spinning bodies feel the frame's rotation**: τ = −Ω×I(ω+Ω) − ω×(IΩ) + I(ω×Ω) beyond what the solver integrates; without it a free gyroscope's axis turned with the planet (1.7·10⁻³ rad/s off after 10 s; now 6·10⁻⁶).
  - **The field at the end of each step** and set at once after frame changes and spawns: a ball 800 m from a free-falling origin drifted 7 mm from its rails in 2 minutes, halving with the step (first order); now 1 µm, the rounding of inertial coordinates.
- **Newton's third law through the ground:** in a surface frame, what the bubble's contents gain in momentum beyond the known forces came from the ground and the anchor's gravity; the opposite goes into the anchor as a push (collected until it changes the anchor's velocity by 10⁻⁹ m/s, at least daily — delayed, never lost). An engine pushing down on the ground moves the planet; a hovering ship pulls it.
- **Vessels** leave the bubble beyond 3 km of the focus (or for warp): resting ones are stored in the body's frame (landed), the others go on rails at their centre of mass with their shape and spin kept about it; within 2.5 km they come back.
- **Time warp**: up to 4× the physics runs several steps per tick; beyond, everything is packed and only orbits advance (up to 10⁷×). A vessel on rails that hits a body comes to rest there and stops the warp. The sandbox's **warp governor** lowers the warp when a tick would take longer than 12 ms (100 low orbits at 10⁷× are 360,000 integration steps per tick, which nothing may skip) and raises it again when there is room.

### Client (`creeer_client::system_*`)

- **Flight view** in bubble coordinates: the terrain of the nearest body (within 30 radii) streamed in that body's moving, turning frame (the terrain pass draws chunks with a per-body rotation and position, 32-byte instances), every other body as a sphere, the star emissive, the sun where the star really is, home's sky. The camera is carried through frame changes by the same change. Dig with the left mouse button on the body you stand on.
- **Map** (M) at true scale: osculating orbits of the bodies, the focus ship's predicted path (red when it ends on a surface), labels and markers, the five Lagrange points, a co-rotating option.
- Ships in orbit with an engine (prograde/retrograde/normal/radial, B), pushing any body (log force, direction, duration), warp keys `,` and `.`; headless `--system <home|orbit|moon|map|system|lagrange|transfer> file.png`.

## Validation

| Test | Result |
|---|---|
| Vallado example 2-4 (universal Kepler) | within the book's printed digits |
| Kepler propagation (proptests): energy, momentum, reversibility, composition, elements round trip | 1e-9 relative |
| Hohmann LEO→GEO; Earth's Hill radius and SOI; the Moon's Roche limit | 2,426 + 1,467 m/s, 5.275 h; 1.497 Gm, 0.925 Gm; 9,492 km |
| IAS15: 1,000 orbits at e = 0, 0.5, 0.9, 0.99 against Kepler | position 2.6e-11 … 2.1e-9 of a; energy drift ≤ 8.5e-14 (35–159 steps per orbit) |
| IAS15: degree-7 force exact in one step; figure-eight returns; grazing hyperbolic flyby | 1e-13; 1e-6 (published digits); < 1 mm |
| Compensated sums: 3,600 pushes of 0.003 ulp each, every one restarting the integration | sum exact to 1e-3 |
| Ephemeris vs Kepler, 3 Moon orbits at off-grid times | < 5 mm, 1e-7 m/s |
| A push added after the forecast passed it ≡ known from the start | < 1 mm |
| Sun–planet–moon for a planet year | energy drift < 1e-13, momentum and angular momentum to 1e-13 |
| Ship on rails vs everything integrated together, one day (54 orbits) | 0.26 mm |
| Radial fall from 100 km stops at the surface | time within 1 ms of the analytic fall; on the surface to 1 µm |
| Gravity tractor (20 t ship, 1e10 kg asteroid, 100 m) | pull μ/r²·t within 0.3 %, momentum balanced |
| Trojans (Routh's criterion): mass ratio 0.001 / 0.1 | librates within 2 % of a / leaves (250 a) |
| Lagrange points of any mass ratio are equilibria of the rotating frame | 1e-12 |
| Rotating vs inertial frame, 10 s at 3 km/s with a spinning box | 2.4 µm, 2e-7 m/s, 6e-6 rad/s |
| Tides: difference of pulls; potential's gradient | exact to rounding |
| Bubble vs rails, 2 minutes in free fall (ball at the origin / 870 m away) | 1e-10 m / 1e-6 m |
| Surface → orbit frame change | no jump (within one tick of flight) |
| Warp 1,000× for 1,000 s and back | 5 cm from the rails |
| Engine (1 MN, 5 s) on a crate on the ground / ship hovering 10 s | planet gains the impulse within 4e-4 / is pulled by m·μ/r²·t within 2e-3 |
| Crate through a 10,000 s warp (landed) | back in place within 1 cm |
| Generated system: home where the file says, moons within Roche–Hill bounds and locked, barycentric, Trojans, terrain on every surface | ✓ |
| Prediction: closed ellipse around home after one period; a fall ends on the surface; L-points still in the co-rotating frame | 200 m (sun and moon perturbations); 1 m; 1e-9 |
| GPU: terrain in a turned and moved body frame looks the same | largest channel difference ≤ 6 |
| Golden hashes (Windows, Linux, macOS in CI) | generated system, 30 days of the bodies with a push, a universe through frame change and warps |

## Measured (dev machine: 16 threads)

- Generating the default system with its stability checks: **0.32 s** (19 majors, 153 minors).
- One home year of the 19 bodies: **0.49 s** (21,790 steps; budget 2 s).
- A tick at 1× (a ship in the bubble, 153 small bodies on rails): **0.10 ms** (budget 2 ms of the 10 ms tick).
- A tick at 10⁵× with 100 ships on rails: **1.1 ms** (budget 8 ms); at 10⁷×: 108 ms — the warp governor holds it near 12 ms.
- A predicted path of 2 hours in 600 points: 3.5 ms (computed twice a second).
- The sandbox at 1080p (RTX 4070 SUPER, frames paced at 60 Hz, `--system-bench`): following a ship in low orbit with home's terrain streaming below **p50 2.1 ms, p99 10.8 ms**, the map 4.1 / 10.2 ms, the map at 10⁵× warp 4.3 / 11.3 ms; no frame over 15 ms after the first (setup, 139 ms). Simulation ticks meanwhile: p50 0.18 ms, p99 3.5 ms (the prediction).

- **Perf gate:** every earlier metric within noise except the F0 meshing spike's noise sampling (`terrain_generate_ms_per_chunk`, 3.15 → 3.46 ms, +10 % in three interleaved A/B pairs against F3). Its code did not change, and built without the orbit benchmark linked the same binary measures 3.13 ms again: a code-layout effect of linking the orbit crate into the benchmark tool (both call the same deterministic math functions). The production terrain pipeline (chunk builds, chunks per second, streaming) is unchanged. The baseline was saved again with the orbit metrics.
- F3 renders are unchanged (orbit, high, tunnel and night byte-identical; low and high below the visibility threshold; the surface scene differs only where its crates fell differently under the new Coriolis model).

## Consequences and limits

- **Restricted problem for small things:** minor bodies and ships do not pull the major bodies (except, through the bubble, the body a ship stands on or hovers over); minor bodies do not pull each other; bodies in the bubble do not attract each other.
- **Point masses:** no oblateness (J2: no sun-synchronous orbits yet), no irregular gravity of lumpy asteroids. Spins never change (pushes do not torque bodies; no tidal evolution).
- **Collisions of major bodies are detected, not resolved** (the warp stops and the log says so); a minor body that hits a major one stops there.
- On rails a vessel turns at constant angular velocity (a tumbling brick's precession is not followed).
- Every push on a major body rewrites its forecast: a machine pushing a moon continuously costs one recomputation per applied push.
- The map draws the bodies' osculating ellipses (the true paths wander from them slowly) and the focus ship's predicted path; there are no maneuver nodes yet (G8).
- Only the nearest body's terrain is streamed; others are spheres. Only home has an atmosphere (F6), and other bodies cast no shadows (no eclipses).
