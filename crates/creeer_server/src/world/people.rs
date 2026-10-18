//! Players' characters as people (G1.2d): each one's health, taken by the impacts on it
//! (`creeer_sim::character::injury`) and slowly healed; a character knocked down (toppled,
//! struck, or killed) falls as its ragdoll (`creeer_sim::character::ragdoll`), whose segments
//! become entities of their own, its pelvis the avatar's. A ragdoll lying still for
//! [`RISES_AFTER`] stands up again where it lies; a dead one is taken away and its player
//! stands at the spawn point after [`RESPAWNS_AFTER`], whole.

use super::{DT, ServerWorld, kept};
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_sim::character::injury::{Region, Strikes, pushed_on};
use creeer_sim::character::{self, ragdoll};
use creeer_sim::save::SavedPerson;

/// Ticks a living ragdoll lies still before it stands up (2 s).
pub const RISES_AFTER: u32 = 120;
/// Ticks a dead one lies before its player stands at the spawn point again (5 s).
pub const RESPAWNS_AFTER: u32 = 300;
/// Health healed a second (of a whole one): all of it in ten minutes.
pub const HEALING: f64 = 1.0 / 600.0;
/// A segment moving slower than this (m/s), and turning slower than [`STILL_SPIN`] (rad/s),
/// is still.
const STILL_SPEED: f64 = 0.1;
const STILL_SPIN: f64 = 0.3;

/// A player's character as a person.
#[derive(Clone, Debug)]
pub struct Person {
    /// Its health (a share: 1 whole, 0 dead).
    pub health: f64,
    strikes: Strikes,
    /// Fallen: its segments' entities, the avatar (the pelvis) first; standing: none.
    pub parts: Vec<EntityId>,
    /// Ticks it has lain still, and been dead.
    still: u32,
    dead_for: u32,
    /// Knocked down, its strike not yet over.
    struck: bool,
    /// Its parts' velocities when the step began.
    before: Vec<DVec3>,
}

impl Default for Person {
    fn default() -> Self {
        Self {
            health: 1.0,
            strikes: Strikes::default(),
            parts: Vec::new(),
            still: 0,
            dead_for: 0,
            struck: false,
            before: Vec::new(),
        }
    }
}

impl Person {
    /// Whether it is down (its ragdoll for its body).
    pub fn fallen(&self) -> bool {
        !self.parts.is_empty()
    }
}

impl ServerWorld {
    /// Avatar `id` as a person, if it is one.
    pub fn person(&self, id: EntityId) -> Option<&Person> {
        self.people.get(&id)
    }

    /// Avatar `id`'s jetpack's gas (kg): what it weighs beyond its dry mass, all its segments
    /// together when it is down.
    pub fn gas(&self, id: EntityId) -> f64 {
        let fallen = self.people.get(&id).filter(|p| p.fallen());
        let Some(p) = fallen else {
            return self
                .index(id)
                .map_or(0.0, |i| character::gas(&self.world.bodies[i]));
        };
        let mass: f64 = (p.parts.iter())
            .filter_map(|e| self.index(*e))
            .map(|i| self.world.bodies[i].structure.masses()[0])
            .sum();
        (mass - character::DRY).max(0.0)
    }

    /// Whether entity `id` is part of a fallen person (a ragdoll is not parked: it must
    /// stand up, or be taken away).
    pub(super) fn in_ragdoll(&self, id: EntityId) -> bool {
        self.people.values().any(|p| p.parts.contains(&id))
    }

    /// The entities that became something else since the last call (an avatar fallen, or
    /// up again): whoever has them is told them again.
    pub fn take_changed(&mut self) -> Vec<EntityId> {
        std::mem::take(&mut self.changed)
    }

    /// Before the world's step: each person's parts' velocities.
    pub(super) fn people_before(&mut self) {
        let (world, index) = (&self.world, &self.index);
        for (id, p) in &mut self.people {
            p.before.clear();
            let parts = if p.parts.is_empty() {
                std::slice::from_ref(id)
            } else {
                &p.parts[..]
            };
            for part in parts {
                if let Some(&i) = index.get(part) {
                    p.before.push(world.bodies[i].structure.velocity[0]);
                }
            }
        }
    }

    /// After the world's step: each person's harm, healing, falls, rising and respawning.
    pub(super) fn people_after(&mut self, pushes: &[(usize, usize, DVec3)]) {
        let ids: Vec<EntityId> = self.people.keys().copied().collect();
        for id in ids {
            let Some(mut p) = self.people.remove(&id) else {
                continue;
            };
            if self.index(id).is_some() {
                self.live(id, &mut p, pushes);
            }
            self.people.insert(id, p);
        }
    }

    fn live(&mut self, id: EntityId, p: &mut Person, pushes: &[(usize, usize, DVec3)]) {
        let parts: Vec<usize> = if p.fallen() {
            p.parts.iter().filter_map(|e| self.index(*e)).collect()
        } else {
            self.index(id).into_iter().collect()
        };
        if parts.len() != p.before.len() {
            // Just fallen, risen or back: nothing measured over this step.
            return;
        }
        let mut harm = 0.0;
        if p.fallen() {
            for (k, (&b, &v)) in parts.iter().zip(&p.before).enumerate() {
                let motion = (v, pushed_on(pushes, b));
                harm += (p.strikes).after_step(&self.world, (k + 1, b), motion, ragdoll::region(k));
            }
        } else {
            let (b, v) = (parts[0], p.before[0]);
            let motion = (v, pushed_on(pushes, b));
            harm += (p.strikes).after_step(&self.world, (0, b), motion, Region::Torso);
        }
        let alive = p.health > 0.0;
        p.health = (p.health - harm).max(0.0);
        if p.health > 0.0 {
            p.health = (p.health + HEALING * DT).min(1.0);
        }
        if !p.fallen() {
            let i = parts[0];
            let up = self.world.bodies[i].structure.position[0].normalize();
            let motion = (p.before[0], pushed_on(pushes, i));
            p.struck |= ragdoll::knocked(&self.world, i, up, motion);
            // Struck, it falls once the strike is over: its body takes the whole strike (falling
            // in it, its segments were given its speed into the ground at once, and struck it
            // again). Killed, at once.
            if p.health <= 0.0 || (p.struck && !p.strikes.striking(0)) {
                // (An impact it is in so far is the body's; its segments take the rest.)
                let harm = (p.strikes).end(&self.world, (0, i), Region::Torso);
                p.health = (p.health - harm).max(0.0);
                self.fall(id, p);
            }
            return;
        }
        let still = parts.iter().all(|&b| {
            let s = &self.world.bodies[b].structure;
            s.velocity[0].length() < STILL_SPEED && s.angular_velocity[0].length() < STILL_SPIN
        });
        p.still = if still { p.still + 1 } else { 0 };
        if p.health <= 0.0 {
            p.dead_for = if alive { 0 } else { p.dead_for + 1 };
            if p.dead_for >= RESPAWNS_AFTER {
                let feet = self.spawn;
                self.rise(id, p, Some(feet));
                p.health = 1.0;
                if let Some(i) = self.index(id) {
                    character::refill(&mut self.world.bodies[i]);
                }
            }
        } else if p.still >= RISES_AFTER {
            self.rise(id, p, None);
        }
    }

    /// Avatar `id` (person `p`) falls: its body becomes its ragdoll.
    fn fall(&mut self, id: EntityId, p: &mut Person) {
        p.struck = false;
        let Some(i) = self.index(id) else { return };
        let segments = ragdoll::fall(&mut self.world, i);
        self.surroundings.renew(i, &self.world);
        p.parts = vec![id];
        for &b in &segments[1..] {
            let part = EntityId(self.next_id);
            self.next_id += 1;
            debug_assert_eq!(b, self.ids.len());
            self.index.insert(part, b);
            self.ids.push(part);
            self.players.push(None);
            p.parts.push(part);
        }
        (p.still, p.dead_for) = (0, 0);
        self.changed.push(id);
    }

    /// Fallen avatar `id` (person `p`) stands up: where its pelvis lies, or with its feet at
    /// `at`; its other segments go.
    fn rise(&mut self, id: EntityId, p: &mut Person, at: Option<DVec3>) {
        let parts: Vec<usize> = p.parts.iter().filter_map(|e| self.index(*e)).collect();
        let Some(&pelvis) = parts.first() else { return };
        let lying = self.world.bodies[pelvis].structure.position[0];
        let feet = at.unwrap_or_else(|| self.ground_under(lying));
        let up = feet.normalize();
        let body = ragdoll::standing(&self.world, &parts, feet + up * 0.05, up);
        self.world.bodies[pelvis] = body;
        let map = self.world.retain_bodies(|i, _| !parts[1..].contains(&i));
        self.surroundings.retain(&map);
        (self.ids, self.players) = (kept(&map, &self.ids), kept(&map, &self.players));
        self.reindex();
        if let Some(i) = self.index(id) {
            self.surroundings.renew(i, &self.world);
            self.lift_clear(i);
        }
        p.parts.clear();
        (p.still, p.dead_for) = (0, 0);
        p.strikes = Strikes::default();
        self.changed.push(id);
    }

    /// Every fallen person stands up where it lies (before the world is cleared).
    pub(super) fn stand_all(&mut self) {
        let ids: Vec<EntityId> = (self.people.iter())
            .filter(|(_, p)| p.fallen())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let Some(mut p) = self.people.remove(&id) else {
                continue;
            };
            self.rise(id, &mut p, None);
            self.people.insert(id, p);
        }
    }

    /// The people as a save holds them.
    pub(super) fn saved_people(&self) -> Vec<SavedPerson> {
        (self.people.iter())
            .map(|(id, p)| SavedPerson {
                id: id.0,
                health: p.health,
                parts: p.parts.iter().map(|e| e.0).collect(),
                still: p.still,
                dead_for: p.dead_for,
            })
            .collect()
    }

    /// The people of a save (and a whole one for each avatar it does not name).
    pub(super) fn restore_people(&mut self, people: &[SavedPerson]) {
        for s in people {
            let person = Person {
                health: s.health.clamp(0.0, 1.0),
                parts: s.parts.iter().map(|&e| EntityId(e)).collect(),
                still: s.still,
                dead_for: s.dead_for,
                ..Person::default()
            };
            self.people.insert(EntityId(s.id), person);
        }
        for id in self.avatars() {
            self.people.entry(id).or_default();
        }
    }
}
