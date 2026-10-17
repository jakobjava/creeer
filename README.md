# Creëer

A realistic-physics building and engineering sandbox: smooth procedural planets, node-beam soft bodies, chemistry-level manufacturing, machines, vehicles and rockets. Its influences are Minecraft/Create, KSP, Astroneer, Dream Car Builder, Factorio and Poly Bridge.

The game runs on a custom engine written in Rust. See [`plan.md`](plan.md) for the full design and roadmap, and [`docs/adr/`](docs/adr) for the decision records.

**Status:** Phase A. F0 (engine skeleton), F1 (renderer core), F2 (physics core), F3 (planets & terrain), F4 (orbital mechanics & solar system: n-body orbits, a generated solar system, time warp, the map) and F5 (networking & persistence: an authoritative server, in process for singleplayer or dedicated over UDP; versioned saves) complete; next is F6 (simulation domains).

## Building

Requirements: the Rust toolchain (pinned by `rust-toolchain.toml`, installed automatically by rustup). On Windows you also need the Visual Studio C++ build tools.

```sh
cargo xtask ci          # format check, clippy (warnings = errors), all tests
cargo xtask perf        # spike budgets + regression check against perf/baseline.txt
cargo run -p creeer_client --release   # tech sandbox (window)
cargo run -p creeer_client --release -- --screenshot shot.png --view far --overlay temperature
cargo run -p creeer_client --release -- --bench 600   # headless render benchmark
cargo run -p creeer_client --release -- --planet surface shot.png   # planet views: orbit/high/low/surface/tunnel/night
cargo run -p creeer_client --release -- --planet-bench 40   # fly from orbit to the ground: frame times
cargo run -p creeer_client --release -- --system moon shot.png   # solar system: home/orbit/moon/map/system/lagrange/transfer
cargo run -p creeer_client --release -- --system-bench   # solar-system sandbox: frame times
cargo run -p creeer_server --release -- --save world.creeer   # dedicated server (UDP 27015; console: save, players, quit)
cargo run -p creeer_client --release -- --connect 127.0.0.1:27015 --name ada   # join it (V walks)
```

## Layout

- `crates/`: one crate per subsystem (`creeer_math`, `creeer_physics`, `creeer_orbit`, ...). Simulation crates have no rendering dependency.
- `xtask/`: project automation.
- `data/`: game content (materials, reactions, parts, planets, solar systems) as RON files.
- `docs/adr/`: architecture decision records. `docs/physics/`: derivations and validation reports.
