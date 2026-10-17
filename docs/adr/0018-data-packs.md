# ADR-0018: Data packs, the validator and hot-reload

- **Status:** Accepted (2026-10-18)
- **Context:** F7.1 (`docs/f7-plan.md`). Until F6 every loader read its own file by its own path: materials and species were compiled in (`include_str!`), planets, geologies and systems were read from paths built from where the source tree was at compile time (`creeer_terrain::data_path`). That made mods impossible and a shipped binary unable to find its data on any other machine. plan.md asks for data packs that override or extend `data/`, a validator, and hot-reload.

## Decision

### Packs (`creeer_data`)

- **A pack is a directory with `pack.ron`:** name (lower-case letters, digits, `_`, `-`), version, description, the packs it requires, and the entries of earlier packs it removes (by table and name). The base pack is `data/` and must be named `base`.
- **Load order:** the base, then the mods, each after the packs it requires, ties by name. The order never depends on how a directory lists. A missing requirement, a circle of requirements, two packs of one name and a mod named `base` are refused with the pack named.
- **Read once.** Opening the packs reads every `.ron` file into memory: a `Data` is a consistent snapshot (a hot-reload opens a new one, so nothing is read half-written). Line ends are made `\n`, so a pack edited on Windows reads and hashes as on Linux.
- **Files and tables.**
  - A *file* (a planet, a geology, a solar system) comes from the last pack that has it (`Data::file`). Paths inside files are paths in the packs, relative to the file (a mod's planet may use the base pack's `home_geology.ron`).
  - A *table* of named entries merges by name (`Data::table`, `Table::merge`): `materials` (`materials/*.ron`, by id), `species` (`chemistry/*.ron`), `minerals` and `rocks` (`geology/minerals.ron`, `geology/rocks.ron`). A later pack's entry replaces the earlier one *in its place*, a new name is added after the earlier entries, a removal takes a name out. A name defined twice within one pack, or a removal of a name no earlier pack has, is an error. Each entry keeps where it came from (`merge_traced`), so an error found when it is built names the pack and file.
  - Keeping a replaced entry's place matters for rocks: a rock's index is its terrain material id, and adding rocks after the base's keeps the base's ids (tested).
- **One way in.** `MaterialTable::from_data`, `SpeciesTable::from_data`, `RockTable::from_data`, `Planet::load(data, path)`, `Geology::load(data, path, …)`, `Params::load(data, path)`, `generate_file(data, path)`, `Universe::new(…, Arc<Data>, …)`, `load_terrain(data, planet)`, `ServerWorld::new(data, …)`. `creeer_terrain::data_path` is gone. The compiled-in `reference()` tables stay for tests and tools; a test keeps them equal to the base pack's.
- **Content hash.** FNV-1a over every pack's name, removals, file paths and texts in load order. A client sends it in its hello (protocol version 2); a server refuses a client whose hash differs and tells it the server's packs and versions. (FNV-1a identifies content, it does not protect it: the server is authoritative anyway, ADR-0008.)

### Finding the data at run time (`creeer_data::locate`)

The base pack: `--data <dir>`, else `CREEER_DATA`, else a `data/` beside the executable, else the source tree's (development builds). Mods: every `--mods <dir>` and the directories of `CREEER_MODS`, else a `mods/` beside the base pack's directory if there is one. The client and the dedicated server both take `--data` and `--mods`; the server prints its packs at start.

### The validator (`creeer_sim::content`, `creeer_tools validate`)

It loads everything the game would load from the merged packs and reports errors (the data would not load) and warnings (it loads, but looks wrong), each named by pack, file and entry:

- every table, each entry built and checked as the solvers need it (materials' positive properties and ordered strengths, species' ranges, rocks' mineral fractions);
- every planet of `planets/` (its geology, its air's gases among the species); a file of `planets/` that is neither a planet nor any planet's geology is a warning;
- every system of `systems/`, and that each planet template it names is a planet;
- data integrity (plan.md §6) over the merged data: every species' heat capacity positive over its range (sampled every 10 K), its enthalpy continuous where its ranges join (a jump over 2·10⁻³ RT is a warning: NASA's own fits join within 1.3·10⁻³ RT, and two jumps are kept as NASA gives them, ADR-0013: SnS(cr) and ALN(L)), and the sign of ΔG at 298 K of four reactions whose direction is beyond doubt (water from its elements, limestone not calcining, iron rusting, methane burning), so a mod's species cannot make water fall apart by itself.

When a table fails, the planets and systems are not checked further (each would repeat the table's error).

### Hot-reload

- **The client polls** its packs' files (sizes and modification times) twice a second (`creeer_data::Watcher`): no platform API, the same on every OS, well under a millisecond for a few hundred files.
- **A change is validated first.** Broken packs keep the old data and the Sandbox window shows the errors; valid ones replace the data (`content::data()`), and the open sandboxes start again on it: the planet sandbox's singleplayer world (a new world on the new planet data), the solar system, the physics scene (its heat scenes read the packs' materials), the lab (its species).
- **A server's data is fixed for its run.** A game on a dedicated server keeps the server's data: its world and its clients must agree (the hash), and changing a running world's planet under its players' edits has no right answer. A server applies changed packs when it is started again.

### An example mod (`tests/packs/example`)

It replaces the base pack's granite, adds basalt, a mineral (chromite), a rock (chromitite) and a planet on the base pack's geology. It is the tests' fixture and a template for modders.

## Measured

- Loading and validating the base pack (9 materials, 1,798 species, 38 minerals, 25 rocks, 4 planets, a system): **30 ms** in a release build (budget 1 s). The example mod adds 2 ms.
- A hot-reload's validation is the same 30 ms; a sandbox's restart (the planet's terrain) is the sandbox's own cost.

## Tests

- `creeer_data`: merge order with requirements, replacement in place, removal, every refusal (missing and circular requirements, a removal of nothing, a name twice in a pack, a broken entry, a bad pack name), the hash ignoring line ends and seeing removals, the watcher seeing changes and new files.
- `creeer_sim::content_tests`: the base pack validates with only NASA's two steps as warnings; the compiled-in tables equal the base pack's; the example mod replaces, adds, keeps the base's rock ids and validates; the validator catches a unit of the wrong dimension, a rock of an unknown mineral, a missing geology file, a species that makes water unstable, a negative heat capacity and a system naming a missing template, each once and with its origin.
- `creeer_server`: a client with other data is refused and told the server's packs.
- `creeer_client::content`: a copied base pack edited on disk reloads (the watcher sees it, the new value is in the data); broken, it is refused with the entry named and the data before it kept.

## Not done (in `docs/future-improvements.md`)

- **Physics sandbox members** use the solver's built-in S235 section constants (`creeer_physics::structure::section::STEEL_S235`), not the packs' steel: changing the packs' steel changes the heat scenes but not the bridge. The part catalog of G2 builds members from the packs' materials.
- **A server reloading its packs while running** (see above).
- **A Lua scripting API for mods** (plan.md: mods are data packs; scripting comes later).
- **Signed or checksummed packs** for public servers (the hash identifies content; it does not authenticate it).
