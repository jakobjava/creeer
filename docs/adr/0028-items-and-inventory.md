# ADR-0028: Items and inventory

**Status:** accepted (2026-10-22, G1.6a done).

**Context:** G1's re-plan (`docs/g1-plan.md`, principles 9–11). A survival player starts in their clothes alone and carries what they find and make. They need a place to keep things, and the world needs to know what a thing is, without a list of item types: a stone, a stick and a crate are one generic kind of thing, as later tools and beams will be.

**Builds on:** ADR-0005 (bodies and shapes), ADR-0008 (server authority, saves, replication), ADR-0018 (data packs), ADR-0027 (reach and carrying: the hands).

**Evidence:** `creeer_sim::items` (the model, `items/kit.rs` for the packs, `items/tests.rs`), `creeer_sim::save` (`ITEM`, version 10, `save/items.rs`), `creeer_server::world::items` (the moves; `items_tests.rs`), `creeer_net::message` (commands 13–18, `ServerMessage::Inventory`, `Own.load`; protocol 14), `creeer_client::inventory_ui`, `data/kit.ron`, the pockets in `data/wearables.ron`.

## Prior work

| Source | What it does | For Creëer |
|---|---|---|
| KSP 1.11 inventory ([wiki: Cargo item](https://wiki.kerbalspaceprogram.com/wiki/Cargo_item)) | Every cargo item has a mass and a packed volume in litres; every inventory has a mass and a volume limit; a kerbal has two small slots | Taken: mass and volume, in litres. Not taken: slots |
| DayZ ([dayz.wiki.gg: Clothing](https://dayz.wiki.gg/wiki/Clothing)) | Clothes are the containers, each with its own cargo; an item larger than the container in either dimension does not fit, even with space left | Taken: the clothes' pockets are the inventory, and an item must fit each pocket's dimensions, not only its volume. Not taken: the grid of cells |
| Project Zomboid, Stalker (weight-based) | A carried weight against a capacity; over it the character slows (a rule) | Not taken as a rule: the pockets' mass is on the body, so it slows and tires as the physics makes it |
| The engineer's model (`art/engineer/detail.py`, `gear.py`) | The outfit's modelled pockets: chest 9.5 × 11.5 cm, thigh cargo pockets 11.5 × 18 cm, the pouch 10 × 11 × 4 cm | Taken: the pockets are the ones drawn. Their depths (3 cm patch, 6 cm bellows) are a garment's usual; no measured pocket volumes were found |

## Decision

1. **One kind of item.** A **piece** is a material (an engineering material's id or a rock's name) and a shape (a block, a ball or a rod); its mass is its density times its volume. A block with a wall is a **container**: its mass is its walls', and it holds what fits inside. A **bulk** is loose matter: mass, composition by mass, temperature, specific heat, grain size, bulk density. Mixing two bulks conserves mass, each species and the heat exactly, and their volumes add.
2. **Stores.** A pocket and a container's inside are both a store: dimensions, and items. A piece fits if each of its box's edges fits the store's (both sorted, DayZ's rule) and its box fits the room left; a bulk takes the store's shape, by its own volume. All the bulk in a store is one mix.
3. **Pockets from the clothes.** Each wearable lists its pockets in `wearables.ron`. The starting outfit has five: 3.6 l in all. No pack or bag until one is made.
4. **The pockets' mass is the body's.** A character's mass is its dry mass, its jetpack's gas and its pockets' load (`character::set_load`); the jets spend only the gas (`Intent.load`). The client predicts with the load the server tells (`Own.load`).
5. **Moves, server-checked and whole.** Put away (E): what the hands hold, or an item in reach, into the first pocket it fits. Take out: into the hands, which must be free. Put in (F): what the hands hold into a container in reach. Take from: out of a container into the hands. An item is always in one place: a body, a pocket or a container. A move that cannot complete gives the item back where it was.
6. **What the player sees** (principle 9): a piece's material, size and mass; a bulk's mass, volume and grains, never its composition.
7. **Dropped matter.** A piece is a body of its own shape (a rod a sixteen-sided prism); a bulk is a heap, a cone at 35°, dry sand's and gravel's angle of repose.
8. **The creative kit** (`kit.ron`): a cobble, a pebble, a stick and a crate, dropped from the inventory window on creative servers.

## Validation

- **Fit:** a 4 cm ball refused by a 3 cm deep pocket that has room for its volume four times over; exactly twelve 3 cm pebbles in a chest pocket.
- **Containers:** the crate's inside and its boards' mass to rounding; a full crate's mass is its boards and contents.
- **Bulk:** mass, species, heat and volume conserved when mixed (10⁻¹⁴); bulk in a store is one mix; more than the room left is refused and given back.
- **Conservation:** random moves between the ground and the three pocket sizes keep the total mass and every species' mass to 10⁻¹² (proptest), and no store overfills.
- **In the world:** a pebble put away weighs on the body to 10⁻¹², the jetpack's gas unchanged; the cobble refused and left; taking out puts it in the hands, the body back to its mass; a crate weighs what is put into it and lets it go again; the whole world's mass unchanged by every move (10⁻⁹); out of reach refused; a save keeps the pockets, the items, and the body's mass.
- **Codec:** items, stores and the inventory message round trip bit for bit.

## Cost

A move is a few map operations and at most one body added or removed: far under a tick. Nothing runs per tick.

## Limits

- A container's inertia is that of a solid block of its mass (its contents and walls as one); a hollow box's is about a third larger.
- Pockets do not stretch or bulge, and what is in them does not show on the body.
- Bulk can only be made by tests and digging (G1.5); the heap is a rigid cone, not a pile that slumps.
- The kit's indices are fixed in the client's buttons by its order in `kit.ron`.

Each is in `docs/future-improvements.md`.
