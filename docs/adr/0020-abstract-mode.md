# ADR-0020: Abstract mode: rate equations measured from the machines themselves

- **Status:** Accepted (2026-10-18)
- **Context:** F7.3 and F7.4 (`docs/f7-plan.md`). plan.md §1.6: "unloaded factories run as rate equations with buffers, so factories keep producing while you're away". The F6 machines (the powerhouse: water through a turbine, a generator, electrolysis cells, a reactor, heat to the air, a Lua controller) cost 0.023 ms a tick in full; a world of factories nobody is near cannot pay that for each of them. What abstract mode must keep: what a machine makes while nobody watches is what it would have made watched (a stated tolerance, tested against the full simulation), and what nature conserves is conserved through every handover.

## Decision

### The rate solver (`creeer_rates`)

- **Stocks** (an amount between bounds; the surroundings unbounded) and **processes** (flows into and out of stocks at full rate, each linear in the stocks it was measured against, trusted within a box of them).
- **Throttling at bounds:** a stock *held* at a bound (its unthrottled net pushes past it by more than 10⁻¹² of what flows through it) limits the processes pushing past: they share what flows in (or out) in proportion to what they ask, each also held to its other limits: water filling, solved as a fixed point from 1 downwards (the largest throttles that fit). The same rule as Factorio's power networks, over every stock.
- **Segments are exact.** With the throttles fixed, rates linear in the stocks make the stocks a linear system ds/dt = A·s + c. Its *states* (the stocks some rate depends on) and their time integrals evolve by the matrix exponential of [[A, c, 0], [0, 0, 0], [I, 0, 0]], so every flow's amount over a segment is exact: factor·(rate·h + Σ slope·(∫s − at·h)). One state (one tank whose outflow follows its level) has a closed form: s = s₀·eˣ + c·t·φ₁(x), ∫s = s₀·t·φ₁(x) + c·t²·φ₂(x), φ₁ and φ₂ through `expm1` and their series near 0 (`libm`: the same on every platform). A process held at a stock fed by rates that vary runs at φ = −Σ supply/Σ demand, linear in the stocks too: still exact. Trapezoidal segments (with step control on the state stocks' nets) remain only where a held process's own rates vary.
- **Events end segments where they happen:** in closed form for stocks whose flows are steady (straight lines); for one state, by convexity (an amount α + β·t + γ·∫s turns at most once: past at the end or at its turning point, found by interpolation in s and the logarithm in t); otherwise by sampling the exact path (a path of m states turns at most m − 1 times: 4·(m + 1) samples) and regula falsi (Illinois). A held stock's release (φ reaching 1) ends a segment too.
- **Exact books:** each flow's amount is computed once and applied to its stock; every stock keeps what came in and went out in compensated sums. Over a year of a factory the books close to 10⁻¹⁶ of what passed through.
- **Networks are independent** (machines sharing a stock share one) and advanced lazily, in parallel when many are (`advance_all`).

### Machines going abstract (`creeer_sim::abstraction`)

- **A `Machine`** names its stocks (what moves slowly: water in tanks, a reactor's species and its temperature, heat in large masses, heat given to the air), which of them *relax* towards a balance (a mass's heat, a temperature), and can be ticked, read and written (`amounts`, `set_amounts`), and its fast states saved (`fast`, `set_fast`).
- **The rates come from the machine's own full simulation:** no recipe, no second model that could disagree with the physics.
- **Leaving** costs nothing: while active the machine's stocks are recorded every 10 s; it leaves once its last two windows' rates agree within 5 % (a machine never steady stays active, as a body never at rest does). From rest the powerhouse is steady after about 200 s.
- **A probe** runs the full machine from the abstract stocks: it settles 5 s, then two 10 s windows whose rates agree within 5 % (a transient, the runner spinning down after its tank ran dry, is waited out, up to 600 s). The probe's seconds are real time for the machine.
- **A predictor-corrector across probes** (between probes the rates are an integrator's evaluations):
  - each stretch's rates are predicted to change linearly in time from the last probe to this one (a long baseline: none of one window's noise), or, for a relaxing stock, linearly in its own amount (Newton cooling: exponential, exact in the rate solver);
  - when the next probe measures the rates, the prediction's miss m corrects the stretch's amounts by ½·m·Δ (the trapezoid of a miss growing from nothing), scaled as a whole so no stock passes a bound (a stock already at its bound keeps its own correction out: a dry tank's 10⁻¹³ m³/s of rounding must not stop every other correction);
  - step control: the miss is second order in the stretch's length, so the next stretch is Δ·√(2 %/miss), growing at most twofold, between 60 s and 2 h.
- **A new regime** (a stock at a bound, between probes or inside one: a tank running dry) carries neither correction nor trend over.
- **Never past the time asked for:** a probe due with less time left than one takes runs the machine in full to the end instead, and the next stretch starts with a probe. A machine caught up for a player stands exactly at the world's time.
- **Coming back:** the abstract stocks are written into the machine, whose fast states are its last probe's.

### Found on the way

- **Tanks drained below their floor** (`creeer_fluids`, F6.6): a tank had no notion of empty, and its head fell past its floor: water from nowhere (−0.045 m at the end of the powerhouse's drain). Now as EPANET does: a dry tank's links carry water only into it (a check valve towards it, opening again when the head beyond would fill it), and the step in which a tank runs dry is cut where it does (the fraction of the step found by the secant method, then the rest stepped dry). Torricelli's emptying time to the tick; a dry tank refills by Torricelli again.
- **Written states and integrator histories:** the reactor's BDF2 took a state written from outside for a rate (hydrogen seven times too fast in a probe's first seconds); a write now restarts it (backward Euler next), and a write that changes nothing changes nothing, bit for bit (the thermal network compares the stored heat, the powerhouse the reactor's moles and temperature).
- `creeer_thermal`: `Network::set_stored`, `stored_in`; `Enthalpy::temperature` (Newton on H(T)).

### In the server's world (`creeer_server::world::machines`)

- **Active** within 3 km of a player (ticked with the world, side by side when there are several); **leaving** beyond 3 km of every player once steady; **abstract**: nothing per tick, not even a parallel loop's waking; **catching up** when a player comes within 3 km: on a thread of its own to the time 60 ticks on, taken in on that tick (waited for if not done: what it finds never depends on the thread's speed), then active if a player is within 2.5 km.
- **A command** builds a powerhouse (`Command::Powerhouse`, protocol 3); every client is sent the machines' list once a second (`ServerMessage::Machines`: mode, place, water, hydrogen, time): the planet sandbox's window lists them, its tool *powerhouse* builds one.
- **Saves** (version 7, `MACH`): each machine as its abstract self keeps it (mode, time, stocks, the stretch under way, the last measurement, the rates' scales) and its fast states (the powerhouse's shaft speeds, its valve, its controller's memory). Restored, the machine is built anew with them; a saved and restored abstract machine goes on exactly as it was to its stretch's end.

## Measured

| What | Measured | Budget |
|---|---|---|
| 10,000 machines (50,000 stocks) through a day | 2.6 segments a machine, 4.8–4.9 ms (9.6–9.7 projected) | 10 ms |
| A year of a factory's books | 3 segments, closed to 10⁻¹⁶ of throughput | — |
| 1,000 abstract powerhouses, nothing due | +0.003 ms a tick (0.007 projected) | 0.05 ms |
| The powerhouse abstract against full, small tank (22 min to dry, its masses warming throughout) | every stock at the end within 0.5 %, along the way within 0.7 % (but the first minutes of warm-up, where the heat stocks are small: 6 %); dry within 0.1 % of the time | 1 % |
| The same, full-size tank (4.5 h, dry after 3.7 h) | every stock within 0.8 % (most within 0.05 %), dry within 0.01 % | 1 % |
| What probing costs against running in full | 27 % (small tank), 15 % (4.5 h, warming throughout) | the plan's 2 % target: **not met** while the machine's slow thermal states (time constants of 30 min to 4.6 h) change; a steady machine probes every 2 h (0.3 %) |

## Tests

- `creeer_rates`: a tank draining to its exact empty time; consumers sharing a limited inflow; a full bin holding back its feed; water filling; a consumer starved upstream; a closed empty loop; exponential decay exact in one segment; a level-dependent tank filling to its cap at the analytic time; a held outlet handing over to free decay at the analytic time; a stock peaking between a segment's ends caught at its crossing; stale rates; advancing in steps or at once; networks side by side; a year's books; the matrix exponential against closed forms, the one-state closed form against it.
- `creeer_sim::abstraction_tests`: the powerhouse abstract against full (small tank in CI; full size `--ignored`); coming back where it would be (the shaft within 0.5 % of its speed at once); leaving from its own windows (no probe); saved and restored, going on; writing its own amounts back changes nothing, bit for bit.
- `creeer_fluids::pipes::dry_tests`: a tank runs dry at its floor when Torricelli says and stays dry; a dry tank fills again.
- `creeer_server::world::machine_tests`: a machine goes abstract far away and comes back where it would be (within 1 %, at the world's time exactly); an abstract machine is saved and loaded byte for byte.
- `creeer_sim::save::machine_tests`: the version 7 fixture keeps its machines.

## Not done (in `docs/future-improvements.md`)

- **Probing costs 15–27 % of running in full while a machine's slow states change** (the plan aimed at 2 %). A model-based abstract self (each domain's own slow linear dynamics, the thermal network's conductances, taken as the rates' dependence) would follow coupled thermal states without probes every few minutes.
- **Machines sharing stocks** (a tank two machines draw from): the rate solver handles shared stocks; the world's machines do not share any yet (the building system of G2 connects them).
- **The first minutes of a warm-up** are followed less closely (6 % on heat stocks still small).
- **A machine's insides beyond its fast states** (the platform's motion, the circuit's and the pipes' internal states) are rebuilt at rest when a save is loaded: they settle in seconds.
