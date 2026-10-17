# ADR-0015: Logic and Lua microcontrollers

- **Status:** Accepted (2026-10-17)
- **Context:** F6.7 (`docs/f6-plan.md`). Machines need controls: signals on wires, gates and blocks, sensors that read any domain, actuators that set them, and programmable microcontrollers. Everything must run the same on every machine and in every run (servers and clients, saves). Budget on the minimum-spec CPU: 10,000 gates and 100 scripts at 1,000 instructions each in 0.5 ms a tick.

## Decision

### Signals and blocks (`creeer_logic`)
- **Signals:** a boolean or a number (`Signal`). As a condition a number is true when not zero (NaN false); as a number a boolean is 0 or 1. An unconnected input reads false.
- **Blocks:**
  - constants, sensors (set from outside) and actuators (read from outside);
  - NOT, AND, OR, XOR, NAND, NOR (any number of inputs);
  - an SR latch (reset wins) and a D flip-flop (rising clock edge);
  - on-delay, off-delay and pulse timers;
  - comparators and a Schmitt trigger;
  - sum, difference, product, quotient (IEEE: a zero divisor gives ±∞ or NaN), minimum, maximum, select;
  - a PID block: integral held while the output is saturated and the error drives it further, derivative on the measurement (a setpoint's jump gives no kick);
  - Lua microcontrollers.
- **Order** (`order.rs`): once per tick, topological, the lowest-numbered ready block first. A feedback loop passes through a one-tick register at its lowest-numbered block: that block reads what its sources gave the tick before, the same one in every run. An odd ring of inverters toggles once a tick.
- **Parallelism** (`run.rs`): compiled, a network is split into its parts that no wire joins (each machine's controls). The parts run side by side, each owning its blocks' values, states and scripts while compiled, so none touches another's and the result cannot depend on the threads. Within a part, blocks run in order, and a level's scripts side by side.
- **Speed:** gates that keep nothing are compiled to a compact operation on flat source lists; reading every block's wiring and state cost 27 ns a gate, and the blocks' memory alone 6.5 ns. Constants are skipped.

### Lua 5.4 microcontrollers (`script.rs`, `sandbox.rs`)
- **Standard PUC-Rio Lua 5.4.9** (lua.org's, the language Factorio scripts in) through `mlua` 0.12 (features `lua54`, `vendored`, `send`). It is not Luau, and has nothing of Roblox.
- **Built by us:** `vendor/lua-src` is our copy of the `lua-src` crate, patched in for `mlua` in the root `Cargo.toml`, with only the Lua 5.4 sources. Its build script compiles without contraction into fused multiply-adds (`-ffp-contract=off`), and `luaconf.h`'s local section sets two things:
  - the **string hash seed** is fixed (stock Lua mixes in the time and addresses, so `next` visited string keys in a different order every run);
  - **`x^y`** is our own pow (`lcreeer.c`), a C port of the Rust `libm` crate's (FreeBSD's `e_pow.c`), so `^` and the game's `creeer_math::det::pow` agree bit for bit (tested over 2,000 values).
- **The sandbox:**
  - **Left out:** `io`, `os`, `debug`, `package`, `require`, `load` (and so bytecode), `loadfile`, `dofile`, `collectgarbage`, `string.dump`, and `next`.
  - **`pairs`** visits a table's keys in its own order, the same everywhere once the seed is fixed, for keys that are numbers, strings or booleans. A table keyed by tables or functions would be visited in the order of their addresses: iterating one is an error.
  - **`tostring`, `print`, `string.format`** name tables and functions by their type, never their address (`%p` is refused). `print` goes to the script's log (its last 64 lines).
  - **`math`:** Lua's own for what IEEE arithmetic fixes (floor, ceil, fmod, sqrt, …). The game's `libm` gives sin, cos, tan, asin, acos, atan, exp and log. `math.random` is the script's own xoshiro256** generator (Lua 5.4's), seeded by the game and saved; `randomseed` reseeds it.
- **Budgets:**
  - **Instructions:** a count hook every 100 instructions, on every coroutine. A script past its tick's budget, or its start's (100 ticks' worth), stops.
  - **Memory:** `mlua`'s allocator limit (256 kB by default).
  - **Stops are final:** `pcall`, `xpcall` and `coroutine.resume` raise a budget or memory stop again instead of returning it. A stopped script sets nothing; its outputs keep their values.
- **Pins:** `input[1..n]` and `output[1..m]` tables, `function tick(dt)`. An output a script leaves unset keeps its value.

### Saves
- A network saves its blocks, wiring, states and every output's value (`save.rs`).
- A Lua state's own insides cannot be saved portably. A script saves its source, its limits, its generator's state and its **`memory` table**: data (booleans, numbers, strings and tables of them, keyed by numbers, strings or booleans), in `pairs` order, 32 deep at most. On loading, it starts again from its source with `memory` restored, before its top level runs (`memory.x = memory.x or …` keeps what was saved). A stopped script stays stopped. A network saved and loaded goes on bit for bit (tested).
- Uploading an edited script (the control panel) is the same restart, `memory` kept.

### On machines (`creeer_sim::controls`)
- `Controls` wires a network to a `Plant`: sensors read pipe heads and flows, drivetrain speeds, circuit voltages and currents; actuators set pump speeds, valve openings and sources (clamped to their range; a NaN setting is ignored).
- Each tick: readings in, the network's tick, settings out, for the domains' steps to come. A controller acts on what it measured a tick before, as a real one does.

### Sandbox
- Control panel window (client): a tank whose level a Lua PI script holds with its pump while the taps open and close, and a motor whose speed one holds with its supply's voltage under a load. The script is edited live and uploaded with its memory kept, with a plot, its state, its memory use and its last printed line.

### Validation
- `creeer_logic` tests:
  - truth tables; the latch's and flip-flop's sequences; timers switching on time (at a tick of 1/8 s, exact in binary);
  - comparators and the Schmitt trigger's hysteresis; feedback through the one-tick register;
  - a PID block bringing a first-order plant to its setpoint (within 10⁻³, overshoot under 10 %);
  - a saved network going on bit for bit.
- Scripts:
  - a runaway script stops by its budget, however it tries to catch that (`pcall`, `xpcall`, coroutines);
  - endless recursion stops by its memory, as do three memory hogs;
  - the sandbox has no way out; `^` and `math` are the game's, bit for bit;
  - a run of gates and scripts (string keys through `pairs`, `^`, `math`, random numbers, formatted numbers, a growing table) hashes to one value in debug and release (`0x65d3_0974_3d57_9ef9`).
- `creeer_sim::controls_tests`:
  - a Lua PI script holds a 2 m² tank's level within a centimetre, before and after the taps open from 30 % to 70 % (8 mm of overshoot);
  - a Lua PI script holds a scooter motor at 150 rad/s within 0.5 %, before and two seconds after a 10 N·m load comes on its wheel.
- Client: both rigs hold their setpoints, and an uploaded runaway script stops while the next upload runs again with the memory kept.

### Budget
- Bench `logic`: a hundred machines' controls of a hundred blocks each (sensors, gates, latches, timers, comparators, PID blocks, a feedback loop) and 100 scripts of about 1,000 instructions. It runs in 0.07 ms a tick on the development machine, 0.14 ms projected to the minimum-spec CPU (budget 0.5 ms).
- Run as one part (no parallelism) it took 0.25 ms.

## Future improvements
- **Tables keyed by tables in `pairs`:** they are an error now. Visiting them in creation order would need a creation stamp on every table inside Lua.
- **Number formatting:** `tostring` and `string.format` turn numbers into text with the C library's `snprintf` (and back with `strtod`). glibc, MSVC since 2015 and macOS all round correctly, so they agree, but the conversion is the platform's, not ours.
- **Saving a script's functions and coroutines** (only `memory`'s data survives a save).
- **Messages between controllers** other than wires (radio, buses).

## Consequences
- Lua comes from our own copy of its build. Updating Lua means updating `vendor/lua-src` and keeping its `luaconf.h` section and `lcreeer.c`.
- A machine's controls cost little per gate, and parallelise across machines.
