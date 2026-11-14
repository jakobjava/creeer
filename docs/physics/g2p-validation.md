# G2P validation report (tick time, G2.11 A6)

What changed in the physics for G2P's work brought forward into G2.11 (`docs/g2p-plan.md`), and what proves it. Every other physics test passes at its present tolerance (`cargo test -p creeer_physics`, 225 tests, the solver's golden hashes among them, bit for bit).

## A body put into the world inside another (ADR-0005, amended in G2.11)

**The physics.** A body put in where it overlaps another holds, in the penalty contact between them, energy that never existed. Resolving the overlap must move the bodies apart without throwing them: the energy may become neither motion nor anything else. The only energy the push-out may cost is what moving the body out against gravity takes, m·g·d for an overlap d, and no body may leave faster than falling through the overlap would make it, √(2·g·d).

**The cases** (`crates/creeer_physics/src/world/born_tests.rs`), a 7.25 kg box (22 cm, the fallen builder's head of *log_bridge*) put into:

| Case | Overlap d | Bound on the energy gained (m·g·d, +10 %) | Gained | Gained before the change | Fastest beyond its start (bound √(2gd)) |
|---|---|---|---|---|---|
| a 200 kg crate at rest on the ground | 13.4 cm | 10.5 J | 10.0 J | 273 J | under 1.6 m/s |
| the same crate landing at 4 m/s (an impact) | 13.4 cm | 10.5 J | within | 217 J | under 1.6 m/s |
| a steel beam's joint as a 50 kg block lands on its other end (impact substeps) | 3 cm | 2.3 J | within | 26,300 J | (the block's blow tips the beam: not bounded) |

The tenth allowed above m·g·d is the limit recorded in ADR-0005: what a new body's contacts still hold after its first step (21 % of a 13.4 cm overlap after one 60 Hz step) is the motion's, here 0.47 J, 5 % of the lift.

**In the play scene** (`creeer_server/src/play/break_probe.rs`): across *log_bridge*'s breaking tick the world's energy rose from 4,299 J to 388,487 J before the change; after it, it only falls (the audit: `docs/g2p-plan.md`).
