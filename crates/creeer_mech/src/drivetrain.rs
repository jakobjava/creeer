//! A drivetrain: shaft nodes (each a rotating part with its inertia, about its axis) joined by
//! elements, held in a housing.
//!
//! Node 0 is the housing itself (at rest in the drivetrain's frame: brakes and bearings act
//! against it). Torques are about each node's axis, positive by its right-hand rule.

use creeer_math::glam::DVec3;
use creeer_network::{Ledger, Sum};

/// A node of a drivetrain; [`HOUSING`] is the housing.
pub type Node = usize;
/// The housing: speed zero, every reaction lands on it.
pub const HOUSING: Node = 0;

/// How a friction element is engaged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grip {
    /// Stuck: no relative motion, torque within its capacity.
    #[default]
    Stuck,
    /// Slipping, the first node faster (+1) or slower (−1) than the second.
    Slipping(i8),
    /// A gear with backlash in mesh on its positive flank (+1: `b` as far ahead of `a` as the
    /// play allows) or its negative one (−1).
    Meshed(i8),
    /// A gear with backlash within its play: no torque.
    Free,
}

/// An element between two nodes.
#[derive(Clone, Debug)]
pub enum Element {
    /// A compliant shaft: torque k·φ + c·φ̇ for its twist φ (stiffness G·J_p/L, N·m/rad).
    Shaft {
        a: Node,
        b: Node,
        stiffness: f64,
        damping: f64,
    },
    /// A rigid transmission (gear mesh, chain): ω_b = ratio·ω_a (negative for an external
    /// mesh, which turns the other way), losing (1 − efficiency) of the power through it in
    /// whichever direction it flows. With `backlash` (rad of play, at `b`) it turns free
    /// within its play and meshes, each flank pushing only, at its ends: a reversing load
    /// clunks across it (the impact's loss booked).
    Gear {
        a: Node,
        b: Node,
        ratio: f64,
        efficiency: f64,
        backlash: f64,
    },
    /// A planetary set: sun, ring and carrier, `teeth` = ring's over sun's (> 1), Willis'
    /// ω_s + teeth·ω_r = (1 + teeth)·ω_c; torques 1 : teeth : −(1 + teeth) on sun, ring and
    /// carrier, losing (1 − efficiency) of the power through its meshes (in the carrier's
    /// frame, the direction it flows there). `teeth` = 1 is an open differential (sun and
    /// ring its two side gears, the carrier its cage).
    Planetary {
        sun: Node,
        ring: Node,
        carrier: Node,
        teeth: f64,
        efficiency: f64,
    },
    /// A belt: a transmission by friction, ω_b = ratio·ω_a while it grips; it carries at
    /// most `capacity` N·m on `a` (the capstan law, see [`belt_capacity`]) and slips beyond.
    Belt {
        a: Node,
        b: Node,
        ratio: f64,
        efficiency: f64,
        capacity: f64,
    },
    /// A clutch (or, to the housing, a brake): Coulomb friction of `capacity` N·m between
    /// two nodes, stuck or slipping. Its capacity is set from outside (pedal, actuator).
    Clutch { a: Node, b: Node, capacity: f64 },
    /// A bearing: viscous friction `viscous` (N·m·s/rad) and Coulomb friction `coulomb`
    /// (N·m) of a node against the housing.
    Bearing { a: Node, viscous: f64, coulomb: f64 },
}

/// The largest torque (N·m) a belt of pretension `tension` (N) carries over a pulley of
/// radius `radius` (m) it wraps by `wrap` rad with friction μ: (T₁ − T₂)·r with
/// T₁/T₂ = e^(μθ) (Euler–Eytelwein) and T₁ + T₂ = 2·T₀.
pub fn belt_capacity(tension: f64, radius: f64, wrap: f64, mu: f64) -> f64 {
    let e = creeer_math::det::exp(mu * wrap);
    2.0 * tension * (e - 1.0) / (e + 1.0) * radius
}

/// A rotating part.
#[derive(Clone, Copy, Debug)]
pub struct Part {
    /// Its inertia about its axis (kg·m²).
    pub inertia: f64,
    /// Its axis in the housing's frame (unit).
    pub axis: DVec3,
    /// Its speed (rad/s); the torque on it from the housing (N·m: a motor's, whose stator
    /// takes the opposite) and from outside (a wheel's from the ground: not on the housing).
    pub speed: f64,
    pub torque: f64,
    pub load: f64,
}

/// A drivetrain.
#[derive(Clone, Debug)]
pub struct Drivetrain {
    pub(crate) parts: Vec<Part>,
    pub(crate) elements: Vec<Element>,
    /// Each element's state: a shaft's twist (rad), a friction element's grip, the force a
    /// constraint carried last step (N·m), and whether power flowed forward through it.
    pub(crate) twist: Vec<f64>,
    pub(crate) grip: Vec<Grip>,
    pub(crate) carried: Vec<f64>,
    /// The heat each element gave off since it was last taken (J): a brake's, a clutch's.
    pub(crate) heat: Vec<Sum>,
    pub(crate) ledger: Ledger,
    /// The torque on the housing from the parts last step (N·m, in its frame).
    pub(crate) reaction: DVec3,
}

impl Default for Drivetrain {
    fn default() -> Self {
        Self::new()
    }
}

impl Drivetrain {
    /// A drivetrain with only its housing.
    pub fn new() -> Self {
        Self {
            parts: vec![Part {
                inertia: 0.0,
                axis: DVec3::Z,
                speed: 0.0,
                torque: 0.0,
                load: 0.0,
            }],
            elements: Vec::new(),
            twist: Vec::new(),
            grip: Vec::new(),
            carried: Vec::new(),
            heat: Vec::new(),
            ledger: Ledger::new(0.0),
            reaction: DVec3::ZERO,
        }
    }

    /// A rotating part of `inertia` kg·m² about `axis`, at rest.
    ///
    /// # Panics
    /// If its inertia is not positive.
    pub fn part(&mut self, inertia: f64, axis: DVec3) -> Node {
        assert!(inertia > 0.0, "a part has inertia");
        self.parts.push(Part {
            inertia,
            axis: axis.normalize(),
            speed: 0.0,
            torque: 0.0,
            load: 0.0,
        });
        self.parts.len() - 1
    }

    /// Adds an element.
    pub fn add(&mut self, element: Element) -> usize {
        self.twist.push(0.0);
        // A gear with backlash starts in the middle of its play.
        let free = matches!(element, Element::Gear { backlash, .. } if backlash > 0.0);
        self.grip.push(if free { Grip::Free } else { Grip::Stuck });
        self.elements.push(element);
        self.carried.push(0.0);
        self.heat.push(Sum::default());
        self.elements.len() - 1
    }

    /// A part's speed (rad/s).
    pub fn speed(&self, node: Node) -> f64 {
        self.parts[node].speed
    }

    /// Sets a part's speed (rad/s), as it starts.
    pub fn set_speed(&mut self, node: Node, speed: f64) {
        let held = self.stored();
        self.parts[node].speed = speed;
        self.ledger.joined(self.stored() - held);
    }

    /// The torque on a part from the housing (N·m: a motor's; held until changed).
    pub fn set_torque(&mut self, node: Node, torque: f64) {
        self.parts[node].torque = torque;
    }

    /// The torque on a part from outside the drivetrain (N·m: a wheel's from the ground, a
    /// propeller's from the air; held until changed). Its reaction is not the housing's.
    pub fn set_load(&mut self, node: Node, torque: f64) {
        self.parts[node].load = torque;
    }

    /// A clutch's or a brake's capacity (N·m).
    pub fn set_capacity(&mut self, element: usize, newton_metres: f64) {
        if let Element::Clutch { capacity, .. } = &mut self.elements[element] {
            *capacity = newton_metres;
        }
    }

    /// A friction element's grip.
    pub fn grip(&self, element: usize) -> Grip {
        self.grip[element]
    }

    /// The torque an element carried last step (N·m): what its first node drove it with (the
    /// opposite of its torque on that node), for a gear, belt, clutch, brake, bearing or shaft.
    pub fn carried(&self, element: usize) -> f64 {
        self.carried[element]
    }

    /// The torque the parts put on the housing last step (N·m, in its frame): Newton's
    /// third law for every element between a part and the housing, and for the gears'
    /// couples (their axes apart).
    pub fn reaction(&self) -> DVec3 {
        self.reaction
    }

    /// The parts' angular momentum (kg·m²/s, in the housing's frame).
    pub fn momentum(&self) -> DVec3 {
        self.parts
            .iter()
            .map(|p| p.axis * (p.inertia * p.speed))
            .sum()
    }

    /// The energy it holds (J): the parts' kinetic, the shafts' strain.
    pub fn stored(&self) -> f64 {
        let kinetic: f64 = self
            .parts
            .iter()
            .map(|p| 0.5 * p.inertia * p.speed * p.speed)
            .sum();
        let strain: f64 = self
            .elements
            .iter()
            .zip(&self.twist)
            .map(|(e, &phi)| match *e {
                Element::Shaft { stiffness, .. } => 0.5 * stiffness * phi * phi,
                _ => 0.0,
            })
            .sum();
        kinetic + strain
    }

    /// The heat an element gave off since this was last called (J): for the thermal network.
    pub fn take_heat(&mut self, element: usize) -> f64 {
        std::mem::take(&mut self.heat[element]).value()
    }

    /// Where its energy went.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }
}
