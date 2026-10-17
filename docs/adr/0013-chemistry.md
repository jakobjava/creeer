# ADR-0013: Materials and chemistry

- **Status:** Accepted (2026-10-17)
- **Context:** F6.5 (`docs/f6-plan.md`). Combustion, smelting, calcining, synthesis and electrolysis need species with real thermodynamics, mixtures, equilibrium with condensed phases appearing and disappearing, kinetics that never overshoot it, reactors with full mass and energy balances, and couplings to heat and electricity. Budget on the minimum-spec CPU: 500 reactors' equilibria at 10 Hz in 0.5 ms a tick.

## Decision

### Data: NASA Glenn's database, imported as published

- **Species** (`data/chemistry/species.ron`, `creeer_materials::species`): 1,798 gases and condensed phases from the CEA program's `thermo.inp` (github.com/nasa/cea, Apache-2.0, licence beside the data): NASA-9 polynomials (McBride, Zehe and Gordon, NASA/TP-2002-211556, updated by NASA GRC to 2021), 200 K to 6,000 K, many gases to 20,000 K. H(298.15 K) is the heat of formation, so enthalpies add across reactions. Each phase of a substance is its own species with its own range (iron's α, γ, δ and liquid).
- **Imported** by `creeer_tools import-nasa9`. It reads the fixed-width records, joins species split over two records (iron's α phase about its Curie point), and leaves out ions, deuterated species, CEA's "inert" copies, and reactants without coefficients. Two corrections to the source are written in the file's header:
  - eleven first intervals that NASA's 2021 revision turned inside out (lower bound 300 K above the upper) start at 200 K again, as before it;
  - wüstite's iron is 0.947, as its name and data have it (its record's two-decimal field says 0.95).
- *Rejected:* the 1993 NASA-7 set (TM-4513, as Cantera ships it). It is older: OH's heat of formation is 2 kJ/mol off Ruščić's 2002 value, and two old fits jump at their range joins.
- **Validated** against the tables NASA cites: JANAF, and Gurvich and Ruščić where NASA took theirs. Range joins are continuous but for two genuine jumps (SnS's α→β transition within one species; liquid AlN as Gurvich tabulates it).

### Equilibrium: Gibbs minimisation, the CEA method and what the game needed beyond it

- `creeer_chem::equilibrate` follows Gordon and McBride (NASA RP-1311):
  - Newton on the elements' Lagrange multipliers, the condensed phases' moles, the log of the gas moles and, adiabatic, the log of the temperature;
  - RP-1311's step limits (no species more than e² larger in a step, traces kept from leaping) and its convergence tests;
  - fixed T and P, or fixed H and P (flames).
- **Condensed phases.** After each convergence:
  - a phase out of its range turns into the same substance's phase that covers the temperature (α- into γ-iron), its moles with it;
  - a phase with negative moles leaves;
  - the phase that would lower G most joins.
- **Added for the game's mixtures:**
  - **The phase rule.** A joining phase whose composition lies in those present (iron, wüstite and magnetite are dependent in Fe–O) replaces one by the simplex ratio test, the one that runs out first as it forms. Added, it left a singular system.
  - **The feed's solids start in**, and so does, for each element no gas carries, its most stable phase. A gases-only start is infeasible or singular for calcium or iron.
  - **A phase that keeps going below nothing** is taken out only after 8 iterations in its set (calcite above its decomposition temperature). Taken out at once, Newton's crude first steps cycled the hematite/magnetite sets.
  - **Multipliers that the gases determine only in combination** (CO₂ alone holds carbon and oxygen 1 : 2) are regularised by 10⁻¹² on their diagonal. The moles depend only on the combination.
  - An equilibrium needs some gas, as CEA's does.
- **Warm starts:** a reactor's last equilibrium is the next one's start (1–7 iterations).

### Phase equilibrium, real fluids, transport

- **Vapour pressure** from the same Gibbs data, with the vapour's second virial coefficient and the liquid's Poynting factor: p·φ(p) = P°·e^((g°_l − g°_g)/RT)·e^(V_L·(p − P°)/RT).
  - B(T) by Tsonopoulos, with water's polar term. The alcohols are left ideal rather than guessed.
  - Liquid volumes by Rackett (Yamada–Gunn) anchored at a measured density; water by IAPWS's saturated density.
  - Constants from Poling, Prausnitz and O'Connell.
- **Raoult's law, bubble points and Rachford–Rice flashes** for liquid mixtures (ideal solutions).
- **Transport:**
  - viscosity by Chapman–Enskog (GRI-Mech 3.0 Lennard-Jones parameters, Neufeld's collision integral);
  - conductivity by the modified Eucken correction;
  - mixtures by Wilke's rule;
  - steam by IAPWS's dilute-gas formulas.

### Kinetics and reactors

- **Reactions** are Arrhenius and reversible through K_c from the species' data (detailed balance): a reaction slows to a stop exactly at equilibrium.
- **Reactors** (`creeer_chem::reactor`):
  - batch at constant volume (energy balance on U) or pressure (on H), and CSTRs (feed, outflow n/τ);
  - heat through a conductance, a power in, reactions driven from outside (electrolysis);
  - stepped by BDF2 (backward Euler for the first two steps) with Newton on every species and the temperature together, so stiff kinetics settle and long steps land on equilibrium.
  - The energy ledger closes: reactions keep the energy (it counts heats of formation), so its change is the flows, booked with what the rule made of them.
- **Gas–solid reactions** by the shrinking-core model (Levenspiel): film, product layer and reaction in series, driven by the gas's excess over equilibrium.

### Electrochemistry

- **Chemistry side:** Faraday's law, the reversible voltage ΔG/zF with the Nernst term, the thermoneutral voltage ΔH/zF.
- **Circuit side:** an electrolysis stack is a nonlinear device (`creeer_electric::Electrolyzer`): its reversible voltage set from the chemistry each step, symmetric Butler–Volmer, an ohmic resistance.
  - Newton's step is limited on its overpotential (pnjlim both ways), and the cells start at rest.
  - Below its reversible voltage it runs as a fuel cell.
  - It books the reaction's work as exported, and the rest as heat.
- **The coupler** (`creeer_sim::electrochem`) passes the circuit's booked charge and energy to a reactor exactly: hydrogen to the mole, every joule received.

## Consequences

- **Validated:**

| Quantity | Result | Reference |
|---|---|---|
| CH₄/air adiabatic flame | 2224 K (products CO₂ 8.5 %, H₂O 18.3 %, CO 0.9 %, OH 0.3 %, NO 0.2 %) | CEA 2226 K, same composition |
| H₂/O₂ adiabatic flame | 3074.5 K | 3075–3079 K |
| Calcination at 1 atm CO₂ | 1160 K | ~1170 K |
| Boudouard CO at 1000 K | 71.0 % | 70.9 % (JANAF) |
| CO over iron and wüstite | 61.3 % at 1000 K, 71.1 % at 1273 K | 62.0 and 69.6 % (Darken and Gurry) |
| Ellingham crossover C/CO with Fe/FeO | 994 K | 985 K (Gaskell) |
| NH₃ at 700 K, 1 and 300 bar | 0.30 and 36.7 % | 0.30 and 36.6 % (from JANAF's ΔG) |
| Hematite reduced by CO at 1100 K | the Baur–Glaessner order of phases | Baur–Glaessner |
| Water's vapour pressure | 0.01 % (300 K) to 1.9 % (550 K) off | IAPWS-95; ideal gas was 21 % off at 550 K |
| Water's boiling point at sea level, 3000 m, Everest | within 0.1 K | IAPWS-95 |
| Water's saturated liquid density | to IAPWS's | IAPWS |
| Gas viscosities | within 2.5 % | Lemmon and Jacobsen; REFPROP |
| Gas conductivities | within 1 % (monatomic gases, H₂, steam); 4–8 % high (polyatomic, modified Eucken's bias) | Lemmon and Jacobsen; REFPROP |
| First and second order kinetics, CSTR steady state | to their closed forms | — |
| Reversible kinetics | end at the minimiser's equilibrium at any step length | — |
| Stiff adiabatic burn | ends at the adiabatic equilibrium | — |
| Shrinking core | to its closed forms | Levenspiel |
| Water splitting | 1.229 V reversible, 1.481 V thermoneutral, 1.18 V at 80 °C, Faraday's masses | textbook |

- **Budget met:** 500 furnaces' equilibria (hematite, CO and nitrogen over every iron and carbon phase, warm-started, temperatures sweeping 800–1400 K) take 0.13 ms every 0.1 s on 16 threads: 0.04–0.05 ms a tick projected (≤ 0.5).
- **Sandbox:** the Lab window:
  - equilibria of five mixtures at your temperature and pressure, or adiabatic;
  - N₂O₄ ⇌ 2 NO₂ relaxing, its rate slowed to be watched;
  - the Ellingham diagram from the data;
  - a furnace walking hematite down to iron.
- **Not modelled:**
  - **Solutions:** activity models (slags, brines, aqueous chemistry) and azeotropes. Raoult's ideal solutions only.
  - **Real-fluid gaps:**
    - cross virial coefficients in vapour mixtures;
    - the third virial term near critical points;
    - the alcohols' and ammonia's polar virial terms.
  - **Data gaps:**
    - cryogenic liquids (O₂, H₂, CH₄: NASA has them only as reactants without coefficients; the rockets of G8 need them);
    - distillation-grade vapour pressures of organic liquids. NASA's are TRC fits for combustion, and boil 0.8–5.2 K off; G5's distillation needs better.
  - **Kinetics and phases:**
    - mechanism-level kinetics (one global or elementary step at a time);
    - condensed phases coexisting at a transition temperature in adiabatic problems.
