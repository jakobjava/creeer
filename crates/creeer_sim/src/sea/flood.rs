//! Flooding: water running into a hull through an opening, Torricelli's Q = C_d·a·√(2g·Δh)
//! with Δh the sea's head over the opening less the water's inside (both ways: it drains
//! out of a hull lifted above it); the water inside weighs on the hull and moves with it
//! (its inertia as added mass, the water coming in from the still sea). It lies on the floor,
//! its weight at its middle, over the opening by half its depth (held at the node, a hull
//! filling with water capsized: its centre of gravity stayed high); its free surface's
//! sloshing is a later phase's.

use super::Sea;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::{AddedMass, NodeId};
use creeer_physics::world::World;

/// A sharp-edged hole's discharge coefficient (the vena contracta's 0.61).
pub const DISCHARGE: f64 = 0.61;

/// A hull's opening and the water it let in.
#[derive(Clone, Debug, PartialEq)]
pub struct Flood {
    pub body: usize,
    pub node: NodeId,
    /// The opening: where (node frame), its area (m²); the hull's inside: its floor's area
    /// (m², the water rising over the opening by volume/floor) and what it holds (m³).
    pub opening: DVec3,
    pub area: f64,
    pub floor: f64,
    pub capacity: f64,
    /// The water in it (m³), and the load it put on the node.
    pub water: f64,
    load: DVec3,
    torque: DVec3,
}

impl Flood {
    /// A hull on node `node` of body `body`, open at `opening` through `area`.
    pub fn new(
        body: usize,
        node: NodeId,
        opening: DVec3,
        area: f64,
        floor: f64,
        capacity: f64,
    ) -> Self {
        Self {
            body,
            node,
            opening,
            area,
            floor,
            capacity,
            water: 0.0,
            load: DVec3::ZERO,
            torque: DVec3::ZERO,
        }
    }

    /// Before the world's step of `h`: the water that runs in or out over it, its weight on
    /// the node and its inertia.
    pub fn push(&mut self, sea: &Sea, world: &mut World, h: f64) {
        let g = super::effective_gravity(
            world,
            self.body,
            world.bodies[self.body].structure.position[self.node],
        );
        let s = &mut world.bodies[self.body].structure;
        let (x, v) = (s.position[self.node], s.velocity[self.node]);
        let hole = x + DMat3::from_quat(s.orientation[self.node]) * self.opening;
        let outside = sea.depth(hole).max(0.0);
        let inside = self.water / self.floor;
        let head = outside - inside;
        let flow = DISCHARGE * self.area * (2.0 * g.length() * head.abs()).sqrt() * head.signum();
        let before = self.water;
        self.water = (self.water + flow * h).clamp(0.0, self.capacity);
        let rho = sea.water.density;
        self.load = g * (rho * self.water);
        let middle = hole - g.normalize_or_zero() * (0.5 * self.water / self.floor);
        self.torque = (middle - x).cross(self.load);
        s.load[self.node] += self.load;
        s.torque[self.node] += self.torque;
        // The water that stayed moves with the hull; what came in was still.
        let momentum = v * (rho * before.min(self.water));
        s.added_mass.push(AddedMass {
            node: self.node,
            matrix: DMat3::IDENTITY * (rho * self.water),
            momentum,
        });
    }

    /// After the world's step: its load and inertia off again.
    pub fn settle(&mut self, world: &mut World) {
        world.bodies[self.body].take_load(self.node, self.load);
        let s = &mut world.bodies[self.body].structure;
        s.torque[self.node] -= self.torque;
        if let Some(k) = s.added_mass.iter().rposition(|a| a.node == self.node) {
            s.added_mass.remove(k);
        }
    }
}
