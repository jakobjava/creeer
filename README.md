# Creëer

A realistic-physics building and engineering sandbox: smooth procedural planets, node-beam soft bodies, chemistry-level manufacturing, machines, vehicles and rockets. Its influences are Minecraft/Create, KSP, Astroneer, Dream Car Builder, Factorio and Poly Bridge.

The game runs on a custom engine written in Rust. See [`plan.md`](plan.md) for the full design and roadmap, and [`docs/adr/`](docs/adr) for the decision records.

**Status:** Phase A. F0 (engine skeleton) and F1 (renderer core) complete; next is F2 (physics core).

## Building

Requirements: the Rust toolchain (pinned by `rust-toolchain.toml`, installed automatically by rustup). On Windows you also need the Visual Studio C++ build tools.

```sh
cargo xtask ci          # format check, clippy (warnings = errors), all tests
cargo xtask perf        # spike budgets + regression check against perf/baseline.txt
cargo run -p creeer_client --release   # tech sandbox (window)
cargo run -p creeer_client --release -- --screenshot shot.png --view far --overlay temperature
cargo run -p creeer_client --release -- --bench 600   # headless render benchmark
```

## Layout

- `crates/`: one crate per subsystem (`creeer_math`, `creeer_physics`, `creeer_orbit`, ...). Simulation crates have no rendering dependency.
- `xtask/`: project automation.
- `data/`: game content (materials, reactions, parts, planets) as RON files.
- `docs/adr/`: architecture decision records. `docs/physics/`: derivations and validation reports.
