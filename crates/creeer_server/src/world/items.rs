//! Items in the world (G1.6a, ADR-0028): which bodies are items, what each player's pockets
//! hold, and the moves between them. Each move is asked by a player, checked by the server
//! (in reach, it fits, the hands are free) and done whole or not at all: an item is always in
//! exactly one place, a body, a pocket or a container, so mass and matter are conserved. What
//! the pockets hold is carried on the body (`character::set_load`).

use super::*;
use creeer_math::det;
use creeer_net::message::{Inventory, ItemView, StoreView, Target};
use creeer_physics::world::hull::Hull;
use creeer_sim::carry;
use creeer_sim::items::kit::{self, Densities, KitItem, describe};
use creeer_sim::items::{Form, Item, Store};
use std::collections::{BTreeMap, BTreeSet};

/// The angle a heap of loose matter stands at (rad): 35°, dry sand and gravel's angle of repose.
const REPOSE: f64 = 0.61;

/// The items of a world.
#[derive(Debug, Default)]
pub struct Items {
    /// The bodies that are items.
    pub(super) bodies: BTreeMap<EntityId, Item>,
    /// Each avatar's pockets, in its outfit's order.
    pub(super) pockets: BTreeMap<EntityId, Vec<Store>>,
    /// The container each avatar last looked into.
    pub(super) open: BTreeMap<EntityId, EntityId>,
    /// The avatars whose inventory changed since their players were told.
    pub(super) changed: BTreeSet<EntityId>,
    densities: Option<Densities>,
    kit: Vec<KitItem>,
    /// The starting outfit's pockets, empty.
    outfit: Vec<Store>,
}

impl Items {
    /// From the packs (an item a pack lacks is simply not there to make).
    pub(super) fn from_data(data: &Data) -> Self {
        let outfit: Vec<String> = character::bones::STARTING_OUTFIT.map(String::from).into();
        Self {
            densities: Densities::from_data(data).ok(),
            kit: kit::kit(data).unwrap_or_default(),
            outfit: kit::pockets(data, &outfit).unwrap_or_default(),
            ..Self::default()
        }
    }

    /// What avatar `id`'s pockets hold (kg).
    pub fn load(&self, id: EntityId) -> f64 {
        self.pockets
            .get(&id)
            .map_or(0.0, |p| p.iter().map(Store::mass).sum())
    }
}

/// A body for `item` with its centre at `at`, at rest: a piece as its shape, a bulk as a heap
/// (a cone at its angle of repose).
pub(super) fn body_of(item: &Item, at: DVec3) -> Body {
    let (shape, friction) = match item {
        Item::Piece(p, _) => (
            match p.form {
                Form::Block { size } => Shape::Box { half: size * 0.5 },
                Form::Ball { radius } => Shape::Sphere { radius },
                Form::Rod { radius, length } => prism(radius, length),
            },
            0.6,
        ),
        Item::Bulk(b) => (heap(b.volume()), 0.7),
    };
    let mut body = Body::rigid(shape, item.mass(), at, DT);
    body.structure.orientation[0] = upright(at);
    body.friction = friction;
    body.solver.reset_history(&body.structure);
    body
}

/// A rod as a sixteen-sided prism along x.
fn prism(radius: f64, length: f64) -> Shape {
    let points: Vec<DVec3> = (0..32)
        .map(|k| {
            let a = f64::from(k % 16) * std::f64::consts::TAU / 16.0;
            let (s, c) = det::sin_cos(a);
            let x = if k < 16 { -0.5 } else { 0.5 } * length;
            DVec3::new(x, radius * c, radius * s)
        })
        .collect();
    Shape::Hull(Arc::new(Hull::new(&points).expect("a prism is a hull").0))
}

/// A heap of `volume` (m³): a cone at the angle of repose, as a hull.
fn heap(volume: f64) -> Shape {
    // V = π·r²·h/3 with h = r·tan(repose).
    let tan = det::tan(REPOSE);
    let r = det::cbrt(3.0 * volume / (std::f64::consts::PI * tan));
    let mut points: Vec<DVec3> = (0..16)
        .map(|k| {
            let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::TAU / 16.0);
            DVec3::new(r * c, 0.0, r * s)
        })
        .collect();
    points.push(DVec3::new(0.0, r * tan, 0.0));
    Shape::Hull(Arc::new(Hull::new(&points).expect("a cone is a hull").0))
}

fn view(store: &Store) -> StoreView {
    StoreView {
        name: store.name.clone(),
        volume: store.volume(),
        items: (store.items.iter())
            .map(|i| ItemView {
                text: describe(i),
                mass: i.mass(),
                room: i.room(),
            })
            .collect(),
    }
}

impl ServerWorld {
    /// Avatar `id`'s pockets, given the outfit's empty ones the first time.
    fn pockets_of(&mut self, id: EntityId) -> &mut Vec<Store> {
        let outfit = &self.items.outfit;
        self.items
            .pockets
            .entry(id)
            .or_insert_with(|| outfit.clone())
    }

    /// What avatar `id`'s player sees of what they carry.
    pub fn inventory(&self, id: EntityId) -> Inventory {
        let pockets = self.items.pockets.get(&id);
        let pockets = pockets.map_or_else(
            || self.items.outfit.iter().map(view).collect(),
            |p| p.iter().map(view).collect(),
        );
        let open = (self.items.open.get(&id))
            .and_then(|c| Some((*c, self.items.bodies.get(c)?.store()?)))
            .map(|(c, store)| (c, view(&store)));
        Inventory { pockets, open }
    }

    /// The avatars whose inventories changed since the last call.
    pub fn take_inventories(&mut self) -> Vec<EntityId> {
        std::mem::take(&mut self.items.changed)
            .into_iter()
            .collect()
    }

    /// What avatar `id`'s pockets hold (kg).
    pub fn load(&self, id: EntityId) -> f64 {
        self.items.load(id)
    }

    /// Avatar `by`'s pockets changed by `change`: its body carries their mass.
    fn change_pockets<T>(&mut self, by: EntityId, change: impl FnOnce(&mut Vec<Store>) -> T) -> T {
        let before = self.items.load(by);
        let out = change(self.pockets_of(by));
        let after = self.items.load(by);
        if let Some(i) = self.index(by) {
            character::set_load(&mut self.world.bodies[i], before, after);
        }
        self.items.changed.insert(by);
        out
    }

    /// Adds `item` as a body at `at` (lifted clear of what is there if `lift`).
    fn add_item(&mut self, item: Item, at: DVec3, lift: bool) -> EntityId {
        let body = body_of(&item, at);
        let id = if lift {
            self.add(body, None)
        } else {
            self.add_unlifted(body)
        };
        self.items.bodies.insert(id, item);
        id
    }

    /// Takes item body `id` out of the world, handing back its item.
    fn remove_item(&mut self, id: EntityId) -> Option<Item> {
        let i = self.index(id)?;
        let item = self.items.bodies.remove(&id)?;
        let at = self.world.bodies[i].structure.position[0];
        let map = self.world.retain_bodies(|k, _| k != i);
        self.surroundings.retain(&map);
        (self.ids, self.players) = (kept(&map, &self.ids), kept(&map, &self.players));
        self.reindex();
        self.ages.by_entity.remove(&id);
        // What it held up falls.
        self.world.wake_near(at, 1.0);
        Some(item)
    }

    /// Drops creative kit item `index` at `at`.
    pub(super) fn drop_kit(&mut self, index: u8, at: DVec3) -> Result<(), String> {
        let kit = (self.items.kit.get(usize::from(index))).ok_or("no such kit item")?;
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let item = densities.make(kit)?;
        self.add_item(item, at, true);
        Ok(())
    }

    /// The item body `target` names, if avatar `by` reaches it and nobody holds it.
    fn reached_item(&self, by: EntityId, target: &Target) -> Result<EntityId, String> {
        let id = target.body.ok_or("nothing there")?;
        self.reached(by, target.body, target.at, target.seen)?;
        if !self.items.bodies.contains_key(&id) {
            return Err("not an item".into());
        }
        if self
            .carrying
            .iter()
            .any(|(who, c)| c.load == id && *who != by)
        {
            return Err("someone holds it".into());
        }
        Ok(id)
    }

    /// Avatar `by` puts away into its first pocket that fits it what its hands hold (none) or
    /// the item it reaches.
    pub(super) fn stow(&mut self, by: EntityId, target: Option<&Target>) -> Result<(), String> {
        let id = match target {
            Some(t) => self.reached_item(by, t)?,
            None => self.carried(by).ok_or("nothing in hand")?,
        };
        let item = self.items.bodies.get(&id).ok_or("not an item")?;
        let pockets = self.items.pockets.get(&by).unwrap_or(&self.items.outfit);
        let pocket = pockets.iter().position(|p| p.refuses(item).is_none());
        let pocket = pocket.ok_or_else(|| {
            let why = pockets.first().and_then(|p| p.refuses(item));
            format!("fits no pocket ({})", why.unwrap_or_default())
        })?;
        if self.carried(by) == Some(id) {
            self.let_go(by);
        }
        let item = self.remove_item(id).ok_or("gone")?;
        let at = self.hands_point(by);
        self.change_pockets(by, |p| p[pocket].put(item))
            .map_err(|refused| self.give_back(*refused, at))
    }

    /// Avatar `by` takes item `item` of pocket `pocket` into its hands.
    pub(super) fn take_out(&mut self, by: EntityId, pocket: u8, item: u16) -> Result<(), String> {
        self.hands_free(by)?;
        let (p, k) = (usize::from(pocket), usize::from(item));
        let pockets = self.items.pockets.get(&by).ok_or("empty pockets")?;
        pockets
            .get(p)
            .and_then(|s| s.items.get(k))
            .ok_or("no such item")?;
        let taken = self
            .change_pockets(by, |s| s[p].take(k))
            .ok_or("no such item")?;
        self.put_in_hands(by, taken)
    }

    /// Avatar `by` puts what its hands hold into the container it reaches.
    pub(super) fn put_in(&mut self, by: EntityId, target: &Target) -> Result<(), String> {
        let container = self.reached_item(by, target)?;
        let held = self.carried(by).ok_or("nothing in hand")?;
        if held == container {
            return Err("not into itself".into());
        }
        let item = self.items.bodies.get(&held).ok_or("not an item")?;
        let store = (self.items.bodies.get(&container))
            .and_then(Item::store)
            .ok_or("not a container")?;
        if let Some(why) = store.refuses(item) {
            return Err(why);
        }
        self.let_go(by);
        let item = self.remove_item(held).ok_or("gone")?;
        let at = self.hands_point(by);
        self.change_container(container, |inside| inside.put(item))
            .map_err(|refused| self.give_back(*refused, at))?;
        self.items.open.insert(by, container);
        self.items.changed.insert(by);
        Ok(())
    }

    /// Avatar `by` takes item `item` out of the container it reaches, into its hands.
    pub(super) fn take_from(
        &mut self,
        by: EntityId,
        target: &Target,
        item: u16,
    ) -> Result<(), String> {
        let container = self.reached_item(by, target)?;
        self.hands_free(by)?;
        let taken = self.change_container(container, |inside| inside.take(usize::from(item)));
        let taken = taken.ok_or("no such item")?;
        self.items.open.insert(by, container);
        self.items.changed.insert(by);
        self.put_in_hands(by, taken)
    }

    /// Avatar `by` looks into the container it reaches.
    pub(super) fn open(&mut self, by: EntityId, target: &Target) -> Result<(), String> {
        let container = self.reached_item(by, target)?;
        (self.items.bodies.get(&container))
            .and_then(Item::store)
            .ok_or("not a container")?;
        self.items.open.insert(by, container);
        self.items.changed.insert(by);
        Ok(())
    }

    fn hands_free(&self, by: EntityId) -> Result<(), String> {
        if self.carrying.contains_key(&by) {
            return Err("hands full".into());
        }
        if self.people.get(&by).is_some_and(people::Person::fallen) {
            return Err("down".into());
        }
        Ok(())
    }

    /// Container body `id`'s store changed by `change`: its mass follows, and whoever looks
    /// into it is told.
    fn change_container<T>(&mut self, id: EntityId, change: impl FnOnce(&mut Store) -> T) -> T {
        let Some(Item::Piece(piece, inside)) = self.items.bodies.get_mut(&id) else {
            unreachable!("a container is a piece");
        };
        let mut store = Item::Piece(piece.clone(), std::mem::take(inside))
            .store()
            .expect("a container");
        let out = change(&mut store);
        *inside = store.items;
        let mass = self.items.bodies[&id].mass();
        if let Some(i) = self.index(id) {
            let body = &mut self.world.bodies[i];
            body.solver.set_mass(&mut body.structure, 0, mass);
        }
        let lookers = (self.items.open.iter())
            .filter(|(_, c)| **c == id)
            .map(|(a, _)| *a);
        self.items.changed.extend(lookers.collect::<Vec<_>>());
        out
    }

    /// `item` into avatar `by`'s hands: a body where the hands hold a load, held.
    fn put_in_hands(&mut self, by: EntityId, item: Item) -> Result<(), String> {
        self.index(by).ok_or("no such avatar")?;
        let at = self.hands_point(by);
        let id = self.add_item(item, at, false);
        let load = self.index(id).expect("just added");
        // Gripped on its top, as a hand takes it (not at its centre: no lever to turn it by).
        let body = &self.world.bodies[load];
        let up = character::plumb(&self.world, at);
        let top = (body.shapes.iter())
            .map(|(_, s)| s.extent(body.structure.orientation[0], up))
            .fold(0.0, f64::max);
        self.take_hold(by, (load, 0), at + up * top)
    }
}

impl ServerWorld {
    /// Where avatar `by`'s hands hold a load (its centre where it has none).
    fn hands_point(&self, by: EntityId) -> DVec3 {
        let Some(i) = self.index(by) else {
            return DVec3::ZERO;
        };
        let s = &self.world.bodies[i].structure;
        let (centre, turn) = (s.position[0], s.orientation[0]);
        let up = character::plumb(&self.world, centre);
        let facing = (turn * DVec3::Z)
            .reject_from(up)
            .normalize_or(up.any_orthonormal_vector());
        carry::held_out(centre, up, facing, carry::forearm())
    }

    /// `item`, refused where it was to go, back in the world at `at` (nothing is lost).
    fn give_back(&mut self, (item, why): (Item, String), at: DVec3) -> String {
        self.add_item(item, at, true);
        why
    }
}
