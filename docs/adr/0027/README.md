# ADR-0027: rejected experiments

The code of approaches measured and rejected in ADR-0027's "Measured and rejected" tables, kept so they can be read or re-run.

| File | What it is | Applies to |
|---|---|---|
| `inertial-unclamped.diff` | G1.4a: Bollo's clamp on an offset's starting velocity dropped, so a velocity away from zero is kept; the probe's worst spikes unchanged to 0.1 rad/s | `crates/creeer_client/src/animate/inertial.rs` at the G1.4a commit |
| `hips-damped-rise.txt` | G1.4a: the hips coming back up critically damped at 12/s: the walking stance knee never straighter than 14–16° (Perry: about 5°); the block as it was, to put back in place of `self.drop = drop.max(0.0)` | `crates/creeer_client/src/animate/stand.rs` at the G1.4a commit, with the rate set to 12 |
