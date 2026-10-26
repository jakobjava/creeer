# ADR-0032: Digging, one law for every tool

**Status:** accepted (2026-10-23, G1.5 done).

**Context:** G1's re-plan (`docs/g1-plan.md`, G1.5; principles 3, 9–11). A player who has made a tool digs with it; what they dig is what the geology holds there, mass conserved. There is no table per tool kind: a stone point and a steel pick follow one law from what they are made of. The terrain's finest grid is 1 m, its rounds 2 m, while hand tools break out cubic centimetres to litres a blow. The user chose (2026-10-23) to hand out what is dug at once and cut the terrain when a spot has dug the smallest pit the grid draws, rather than refine the terrain near players (`docs/future-improvements.md`).

**Builds on:** ADR-0006 (terrain, edits, geology and deposits), ADR-0027 (strikes: a swing's energy), ADR-0028 (items: bulk), ADR-0030 (edges, bindings), ADR-0031 (a blow's energy into what does not give).

**Evidence:** `creeer_sim::dig` (`dig_tests.rs`), `creeer_terrain::dug` (`dug_tests.rs`), `RockTable::{composition, hardnesses}`, `Strength`, `data/geology/{minerals,rocks}.ron` (hardness, strength, cohesion, friction, graded), `data/tools.ron` (hardness, tip), `Densities::hardness`, `creeer_server::world::digging` (`digging_tests.rs`), save version 12 (`PITS`), `Edit::apply` (rounds of small brushes).

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| Teale 1965, *The concept of specific energy in rock drilling* (Int. J. Rock Mech.) | Energy per volume broken; its minimum approaches the rock's compressive strength, at a mechanical efficiency of 30–40 % at best | Taken: e_s = UCS for solid rock, η ≤ 0.35 |
| Rabia 1985 (Int. J. Rock Mech.); Pessier & Fear 1992 | Specific energy as the criterion of drilling efficiency, the same relation across bits and rocks | Taken: one law for every head |
| Reece 1964, *The fundamental equation of earthmoving mechanics*; McKyes 1985, *Soil Cutting and Tillage* | A tool's draft in soil from Mohr–Coulomb passive pressure: c·N_c and γ·d·N_γ terms; tillage tools' specific draft 20–100 kPa in loam | Taken: e_s = 2c·√K_p + ½γ·d·K_p for loose ground, d = 0.1 m (a hand tool's bite) |
| Goodman 1989 (Table 3.1); Hoek & Brown 1997 | Intact rocks' uniaxial compressive strengths | Taken as data: typical values per rock |
| Das, *Principles of Geotechnical Engineering*; Carrier et al. 1991 (*Lunar Sourcebook*) | Soils' cohesion and friction angle; lunar regolith's | Taken as data |
| Archard 1953; Rabinowicz 1995 | Wear volume = K·load·sliding distance / hardness; K about 10⁻³–10⁻² in three-body abrasion | Taken: wear = K·(abrasiveness)·E/H, K = 3·10⁻³ calibrated on a steel pick (0.2 mm³ a blow on granite) |
| Kruschov 1957; Richardson 1968 | Abrasive wear drops one to two orders when the abrasive is softer than about 0.8 of the worn surface, full above about 1.2 | Taken: each mineral abrades by its share, a fiftieth below 0.8, fully above 1.2, log-linear between |
| Plinninger 2002; Thuro & Plinninger: rock abrasivity index (RAI = quartz equivalent × UCS) | Tool wear in excavation follows hard-mineral content times rock strength | Taken: wear also as e_s/100 MPa, so a weak quartz sandstone wears a head less than granite |
| Tabor 1951, *The Hardness of Metals* | Indentation hardness about 3× the flow stress | Taken: an engineering material's hardness 3 × its tensile strength |
| Broz, Cook & Whitney 2006 (*Am. Mineral.*); Mindat's VHN | Minerals' Vickers hardness | Taken as data |
| Caterpillar Performance Handbook | Swell of excavated rock (about 1.5) and soil (about 1.25) | Taken: a heap's bulk density |
| Winter 2009, after Dempster 1955 | Body segments' mass, centre and radius of gyration | Taken: bare hands as the forearm and hand swung about the elbow |
| Games: Minecraft (hardness tiers by tool material), Astroneer and Deep Rock Galactic (terrain volume per second), Teardown (voxel holes by tool strength), Valheim (pickaxe tiers) | Digging by tier tables or fixed rates, volume shown at once | Not taken: tables per tool (principle 11); instant volume would hand out more than a blow breaks |

## Decision

1. **Ground has a strength, minerals a hardness** (data). A solid rock: its uniaxial compressive strength. Loose ground: its cohesion and friction angle. Every mineral: its Vickers hardness; a rock's is its minerals' by volume. Ore rocks name the minerals their deposit's grade scales (`graded`).
2. **One law** (`creeer_sim::dig`): V = η·E/e_s. E is the swing's effective mass's energy into ground that does not give. η = 0.35 × 2 mm/(2 mm + tip) × min(1, (H_head/H_rock)²) on solid rock; in loose ground the grains are moved, not broken, and the head's hardness does not count. A forged pick point is 3 mm, an axe's ground edge 10 µm, a stone's face half its thinnest size. Bare hands dig only loose ground (fingertips 8 mm); on rock they are told "too hard for bare hands".
3. **The head wears**: V_wear = 3·10⁻³ × Σ fᵢ·s(Hᵢ/H_head) × e_s/100 MPa × E/H_head. A made tool's head records the share of its volume lost, and an edge blunts as its wedge is cut back (its tip half the flat worn, w² = 4·tan(θ/2)·V/L). The blow also loads a made tool's binding with its whole impulse. The catalogue's tools are not worn.
4. **What is dug is handed out at once**: a heap laid 0.9 m from the spot towards the digger, grown by each blow there: mass ρ·V, the ground's composition at its deposit's grade (the graded minerals scaled, the rest making up the remainder), 15 °C, a bulk density of ρ over its swell.
5. **Each spot keeps a ledger** (0.5 m cells): what was dug and not yet cut. When it holds a pit the finest grid draws, the terrain is cut by a ball (0.5–0.6 m) about where the spot was first struck, sized so the field loses no more than the ledger holds, to a thousandth (`creeer_terrain::dug::Field`: the field sampled every 10 cm, a column's generated density linear in height, each sample's share of rock before and after). What is left over carries to the pit's floor. Ledgers are saved (`PITS`).
6. **Small brushes are rounded over their own radius at most** (`Edit::apply`): a 0.5 m pit is not rounded 2 m wide. Brushes of 2 m and more are rounded as before.
7. **Told as it is seen** (principle 9): "dug 0.84 kg (0.57 l)". The heap shows as loose matter; its minerals are the server's.

## Measured and rejected

| What | Measured | Why not |
|---|---|---|
| Cutting the terrain with the 2 m round at any size | The smallest dig on the 1 m grid takes about 2 m³ (by the rounds' polynomial; a 0.5 m ball's skirt reaches 2.5 m) | At a pick's litre a blow in soil, a dent every few hours |
| A pit's field re-measured whenever the edit log grew | The blow that cut took 5.6 ms (10,000-blow test) | Over the 5 ms tick gate: re-measured now only when an edit came near; pits at most 0.6 m (2.3 ms) |
| Wear by hardness ratio alone (no strength term) | A chert head wore faster on quartz sandstone than on granite | Against Plinninger's index and the plan's check; the strength term reverses it |

## Validation

- Every rock has a strength (specific energy 1 kPa–500 MPa), every mineral a hardness; soil's specific energy within tillage's 20–100 kPa.
- The law: V·e_s/E = η exactly, for granite, sandstone and soil with steel and chert heads. A steel pick: about 1 l a blow in soil, under 1 cm³ on granite; sandstone more than twice granite; chert breaks granite better than steel; in soil the head's hardness does not matter.
- A blunt face breaks under 6 % of what a point does; copper on granite under 5 % of steel, wearing at least 5 times faster.
- A chert head wears faster on granite than on sandstone; a steel pick loses 0.05–0.5 mm³ a blow on granite; soil wears a thousandth of that.
- A porphyry's chalcopyrite at grade 2.5: 2.3–2.5 times its average mass fraction, none at the rim, fractions summing to 1.
- The field: a pit's measured loss equals the field sampled anew after the cut to 10⁻⁹ m³; the ball found never takes more than the ledger, within a thousandth.
- In the world: a pick's blow lays a heap of ρ·V of the ground's composition, a second blow grows it. 10,000 blows at four spots cut 10.3 m³ of pits, the heaps' mass equals ρ × (cut + owed) to 10⁻⁹, and the terrain measured on another grid lost the same within 5 %. Bare hands dig a handful of soil (0.05–1 l) and nothing of granite. A chert edge blunts and wears over 200 blows.

## Cost

A blow: 0.05 ms on average (budget 0.2). The rare blow that cuts a pit: 2.3 ms at worst (field and ball), on top of the tick. The chunk rebuild is off the tick as before.

## Limits

- A pit spanning two grounds (soil over its bedrock) cuts by volume what the blows dug; its mass closes exactly only within one ground.
- (Lifted in G2.9: every blow cuts the terrain, drawn in 12.5 cm cells near the player.)
- A brittle head does not chip or break on rock (only abrasive wear); the heap's grain size is a rough estimate; dug matter does not settle or slide; loose stones lying over a cut pit stay where they lay.
- The fills' rounded skin reads as the ground beneath, not the filled rock (pre-existing: rock is the fill's only within its brush).
- In creative play, walking with bare hands, a click on the ground within reach digs by hand instead of the sandbox's tool.

Each is in `docs/future-improvements.md`.

## Amended in G2.9 (2026-10-26): every blow cuts the ground

The user's play: "there is no visible hole". A post hole is 30–50 l; the smallest pit the 1 m grid drew held about 90–260 l, so a post hole never showed, and the ground stayed solid where it had been dug. With 12.5 cm cells near the player (ADR-0006, amended), every blow cuts.

**Decision.**
- Each spot has **one brush that grows**, in its place in the edit log (`Edits::replace`).
  - It grows as a dent (a ball up to 15 cm, the hole's radius: a spade's width, a post hole 25–30 cm across), then a shaft down to 1 m (what an arm and a tool reach below the ground one stands on), then wider, up to 0.6 m.
  - Each blow it is sized so the field, measured without it, loses exactly what was dug there, to a thousandth (`Field::grown_for`: regula falsi on the shape's parameter; each shape contains the ones before).
  - The field is sampled in a box about the hole's axis, on a grid of a fifth of its radius (1.5–4 cm). Sampled in a ball about the whole hole, a blow took 70 ms.
- A blow within 10 cm of a hole digs that hole (its floor and walls), wherever its cell.
- If any later edit comes near a hole (a hole backfilled, then dug again), a new brush begins: grown in place, the hole would lie under the fill.
- A hole as wide as it grows passes what it could not take to a new spot at its floor. Otherwise every blow there measured it again, to cut nothing.
- Saves keep what a spot still owes (its hole is in the edits). Dug again, a new brush takes it.
- Every hole is grown in place in the edit log; only a later *fill* nearby starts a new brush. Digs combine the same in either order, and treated as any edit, a hole a metre from another began anew every blow: 3,000 edits in 3,000 blows, each blow slower than the last.

**Measured and rejected.**

| Tried | Measured | Why not |
|---|---|---|
| The field sampled in a ball about the whole hole | 70 ms a blow | Most samples far from the hole |
| The whole hole measured each blow, bracketed by its whole volume × 1.33 | 2.5 ms a blow at 0.9 m deep | A deep shaft's every blow reached into widening it, its whole wall measured |
| A later edit of any kind stops a hole growing in place | 3,000 edits for 3,000 blows at four spots a metre apart; blows slowed to 70 ms | Digs after it take the same ground in either order; only fills matter |
| A 4 cm grid for wide holes | 7 ms a blow at 0.6 m wide | The old pits were measured on 10 cm |

**Cost.** A blow measures only where its hole grows (`Field::beyond_without`): its floor for a shaft, its wall as it widens. The grid is a fifth of the hole's radius, 1.5–10 cm.
- A post hole dug at one spot: 0.3–0.9 ms a blow, to 73 l and about 0.9 m deep in 60 blows.
- 10,000 blows at four spots a metre apart: 1.4 ms a blow on average, 5.3 ms at worst (the old ledger: 0.05 ms a blow, 2.3 ms for the rare blow that cut a pit). The heaps' mass equals ρ × (cut + owed) to 10⁻⁹, and the terrain measured anew on another grid lost the same within 5 %.
- Four holes are four edits.
