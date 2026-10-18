# ADR-0017's rejected patches

Kept so the variants can be measured again without rebuilding them (each against the tree of
its day, 2026-10-17/18; new files beside their patch as `.txt`):

- `c-friction-coupled.patch` with `c-gmres.rs.txt`, `c-hessian_oracle.rs.txt`, `c-lag.rs.txt`:
  C, friction following its normal force inside Newton (φ·ψ terms, GMRES). Neutral in speed,
  worse worst steps; fixes friction on stale normal forces.
- `a1-controlled-substeps.patch`: A1, impact substeps sized by BDF2's local error. As accurate
  or better, no faster.
- `d-intra-group-parallel.patch` with `d-awake.rs.txt`: parallel loops within one group
  (rayon kept awake). Slower on this machine (wake-ups ~40 µs, SMT).
- `lambda-extrapolation.patch`: impact substeps' multipliers extrapolated. Halved the crash's
  p99 but changed the version 5 save fixture.
