# ADR-0009: The network kit shared by the simulation domains

- **Status:** Accepted (2026-10-17)
- **Context:** F6 (simulation domains, `docs/f6-plan.md`). Heat, current, torque, chemistry and flow are all solved as networks: nodes with a state, edges that carry something between them, a sparse system each step. Built separately, each domain would repeat the solvers, the bookkeeping and the bugs. F6.1 builds what they share, with the realism rules of the plan: every joule, coulomb and kilogram accounted for, deterministic across platforms, within the per-tick budgets on the minimum-spec CPU.

## Decision

### Sparse solvers (`creeer_math::sparse`)

- **Assembly** (`matrix.rs`): a domain stamps its elements' contributions into fixed slots of a CSC pattern made once, summed in stamp order. The same stamps give the same bits on every platform, whatever the thread count.
- **LU** (`lu.rs`, Gilbert–Peierls left-looking) for the unsymmetric systems (circuits by modified nodal analysis, flow networks): threshold partial pivoting at 0.1, preferring the diagonal, so a circuit's ordering is kept unless it would be unstable. **Refactorisation reuses the pivots** of the first factorisation and gives the same bits as a fresh one (its updates run in ascending order); where a reused pivot would fall below the threshold it factorises afresh by itself, and says which it did.
- **Cholesky** (`chol.rs`, up-looking, CSparse-style) for the symmetric positive definite systems (heat, mechanical networks): minimum-degree ordering, then the elimination tree in postorder, so every subtree is a contiguous range of rows. Each row's reach and the slot of each of its entries are found once (the pattern's), so a refactorisation is only arithmetic. **Independent subtrees are factorised and solved in parallel** (jobs of whole subtrees, the rows above them after, in order): the same bits as one thread, for networks of 4096 rows and more.
- **Complex scalars** (`scalar.rs`, Smith's division) for AC phasors on the same LU.
- `factor_to_solve()` gives a factorisation's cost against a solve's from the pattern (multiply-adds), so a domain decides between refactorising and reusing a stale factorisation by numbers that do not depend on timing: deterministic.

### Network bookkeeping (`creeer_network`)

- **Islands**: a network splits into the parts nothing joins, each solved on its own, in parallel, each able to sleep (`Sleep`: still for 30 steps by its domain's threshold; anything acting on it changing wakes it).
- **Ledger** (`Ledger`, `Sum`): what a network was given (sources), dissipated or lost to its surroundings, passed to or received from other domains, and holds, summed by Neumaier's compensated summation (a long run adds millions of small amounts). A ledger that does not close to rounding is a bug: every domain's tests check it. A network rebuilt (a part split) continues its predecessor's accounts.
- **Exchange** between domains stepping at different rates (heat at 20 Hz, current at 60 Hz): energy accumulated and handed over at the receiver's step, never a rate sampled, so nothing is lost or made between rates. **Schedule** spreads the slow domains over the ticks.

### Measured (`cargo xtask perf`, section `networks`; minimum-spec projection = measured / 0.5)

| network | unknowns | in the factor | step (p50) | budget |
|---|---|---|---|---|
| factory wiring (LU, refactor + solve) | 10,014 | 30,067 | 0.20 ms | 1 ms projected |
| machines' parts (Cholesky, refactor + solve) | 22,000 | 83,497 | 0.68 ms | 1.5 ms projected |
| wiring mesh 100 × 100 (LU) | 10,100 | 394,012 | 14.4 ms | informational |
| conduction sheet 100 × 100 (Cholesky) | 10,000 | 221,032 | 5.2 ms | informational |

Real networks are tree-like (machines on floors, cables in feeders): their factors barely fill. A 2D mesh is a direct solver's worst case (fill grows as n·log n, work as n^1.5); no domain builds one at that size today.

### Rejected or deferred

- **A row-ordered copy of L for the forward solve, a reciprocal diagonal**: measured no faster on these networks (the solve is latency-bound along the elimination tree's chains, about 14 ns a row, not bandwidth-bound) and the copy slowed every refactorisation; reverted.
- **Iterative solvers (PCG)** for large meshed islands: deferred until a domain meshes (no island in F6 needs one; the thermal network lumps parts).
- **Supernodal factorisation**: the factors here are too thin for it to pay.

## Consequences

- Every F6 domain builds on these: thermal (ADR-0010) on the Cholesky, the electrical network on the LU (real and complex), mechanical and flow networks on either.
- Determinism holds by construction (fixed stamp order, fixed elimination order, parallel jobs whose results do not depend on scheduling), and is tested (`sparse/tests.rs`: parallel against sequential bits, refactor against fresh bits).
- A domain's per-step cost is dominated by its own per-element work (evaluating coefficients, residuals) as soon as its factorisation is reused: the kit makes reuse cheap and safe, the domains decide when.
