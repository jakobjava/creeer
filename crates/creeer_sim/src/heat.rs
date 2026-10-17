//! Heat in structures (F6.2): each member of a structure a mass of a thermal network, its
//! temperature fed back into the structure, which lengthens and weakens with it.
//!
//! - A member's heat capacity is its mass of its material's enthalpy.
//! - Members meeting at a joint conduct heat through their half-lengths, k·A/(½L₁ + ½L₂),
//!   A the smaller section.
//! - A member exposed to air (and not in a fire) convects from its outline (the correlation of a horizontal
//!   cylinder, or of a vertical plate as tall as it is when it stands upright) and radiates
//!   to the surroundings, its emissivity that of its material's usual surface.
//! - A member engulfed in a fire takes heat from it as EN 1991-1-2 has it: by convection at
//!   α_c = 25 W/(m²·K) and by radiation at ε_member·ε_fire (the fire's gas temperature its
//!   radiation temperature).
//!
//! After each thermal step, every member is set to its temperature
//! (`Structure::set_temperature`), and the structure's solver is refactorised once a member's
//! stiffness has drifted by half from what the solver was factorised with. A member that
//! buckles is split in two by the structure; the network follows, each half as hot as the
//! member was, exposed as it was, every joule still counted.

use creeer_materials::Material;
use creeer_math::glam::DVec3;
use creeer_physics::structure::{BUILT_AT, ImplicitSolver, Structure};
use creeer_thermal::environment::{FIRE_CONVECTION, FIRE_EMISSIVITY};
use creeer_thermal::{Enthalpy, Link, Medium, Network, NodeId, Shape, StepReport};
use std::sync::Arc;

/// A member's stiffness may drift this far (relative) from the solver's factorisation.
const DRIFT: f64 = 0.5;

/// A fire: the members it engulfs, its gas temperature (K), its node.
#[derive(Clone, Debug)]
struct Fire {
    engulfs: Vec<bool>,
    temperature: f64,
    node: NodeId,
}

/// A structure's members as a thermal network.
#[derive(Debug)]
pub struct MemberHeat {
    network: Network,
    material: Material,
    gravity: DVec3,
    /// Each member's outline area (m²), length (m), and whether it stands upright.
    surface: Vec<(f64, f64, bool)>,
    /// Each member's stiffness over what the solver was factorised with.
    drift: Vec<f64>,
    /// Still air around every member: its temperature (K), pressure (Pa), and the
    /// surroundings' temperature (K).
    air: Option<(f64, f64, f64)>,
    fires: Vec<Fire>,
}

impl MemberHeat {
    /// The members of `structure`, all of `material`, at 20 °C (as built), under gravity
    /// `gravity` (for natural convection); nothing around them yet.
    pub fn new(structure: &Structure, material: &Material, gravity: DVec3) -> Self {
        let members = structure.element_count();
        let mut heat = Self {
            network: Network::new(gravity.length()),
            material: material.clone(),
            gravity,
            surface: Vec::new(),
            drift: vec![1.0; members],
            air: None,
            fires: Vec::new(),
        };
        heat.build(structure, &vec![BUILT_AT; members]);
        heat
    }

    /// Builds the network for `structure`'s members at `temperatures`, with the exposures
    /// so far (their nodes after the members').
    fn build(&mut self, structure: &Structure, temperatures: &[f64]) {
        let material = &self.material;
        let mut network = Network::new(self.gravity.length());
        let enthalpy = Arc::new(Enthalpy::new(&material.specific_heat, material.melting));
        let conductivity = Arc::new(material.conductivity.clone());
        let members = structure.member_geometry();
        let up = if self.gravity.length_squared() > 0.0 {
            -self.gravity.normalize()
        } else {
            DVec3::Y
        };
        self.surface.clear();
        let mut at_joint: Vec<Vec<usize>> = vec![Vec::new(); structure.node_count()];
        for (k, &(a, b, mass, length, section)) in members.iter().enumerate() {
            network.add_mass(mass, enthalpy.clone(), temperatures[k]);
            let axis = (structure.position[b] - structure.position[a]).normalize_or_zero();
            self.surface
                .push((section.perimeter * length, length, axis.dot(up).abs() > 0.7));
            at_joint[a].push(k);
            at_joint[b].push(k);
        }
        for joined in &at_joint {
            for (i, &p) in joined.iter().enumerate() {
                for &q in &joined[i + 1..] {
                    let ((.., lp, sp), (.., lq, sq)) = (members[p], members[q]);
                    let (area, length) = (sp.area.min(sq.area), 0.5 * (lp + lq));
                    network.link(Link::Conduction {
                        nodes: (p, q),
                        conductivity: conductivity.clone(),
                        area,
                        length,
                    });
                }
            }
        }
        if let Some((air, pressure, around)) = self.air {
            let (air, around) = (network.add_fixed(air), network.add_fixed(around));
            // A member in a fire sees flames all round, not the room.
            let engulfed = |k: usize| self.fires.iter().any(|f| f.engulfs[k]);
            for (k, &(area, length, upright)) in self
                .surface
                .iter()
                .enumerate()
                .filter(|(k, _)| !engulfed(*k))
            {
                let diameter = area / length / std::f64::consts::PI;
                let shape = if upright {
                    Shape::VerticalPlate(length)
                } else {
                    Shape::Cylinder(diameter)
                };
                network.link(Link::Convection {
                    nodes: (k, air),
                    medium: Medium::Air,
                    area,
                    shape,
                    speed: 0.0,
                    pressure,
                });
                network.link(Link::Radiation {
                    nodes: (k, around),
                    factor: material.emissivity * area,
                });
            }
        }
        for fire in &mut self.fires {
            fire.node = network.add_fixed(fire.temperature);
            for (k, _) in fire.engulfs.iter().enumerate().filter(|(_, e)| **e) {
                let area = self.surface[k].0;
                network.link(Link::Conductance {
                    nodes: (k, fire.node),
                    conductance: FIRE_CONVECTION * area,
                });
                let factor = material.emissivity * FIRE_EMISSIVITY * area;
                network.link(Link::Radiation {
                    nodes: (k, fire.node),
                    factor,
                });
            }
        }
        network.continue_from(&self.network);
        self.network = network;
    }

    /// The members' temperatures (K).
    fn temperatures(&self) -> Vec<f64> {
        (0..self.members())
            .map(|k| self.network.temperature(k))
            .collect()
    }

    /// Every member in still air at `air` K and `pressure` Pa, radiating to surroundings at
    /// `surroundings` K.
    pub fn in_air(&mut self, structure: &Structure, air: f64, pressure: f64, surroundings: f64) {
        self.air = Some((air, pressure, surroundings));
        self.build(structure, &self.temperatures());
    }

    /// A fire, its gas at `temperature` K, engulfing `members`. Returns its index.
    pub fn fire(&mut self, structure: &Structure, members: &[usize], temperature: f64) -> usize {
        let mut engulfs = vec![false; self.members()];
        for &k in members {
            engulfs[k] = true;
        }
        self.fires.push(Fire {
            engulfs,
            temperature,
            node: 0,
        });
        self.build(structure, &self.temperatures());
        self.fires.len() - 1
    }

    /// Fire `index`'s gas is at `temperature` K (as it grows or dies down).
    pub fn set_fire(&mut self, index: usize, temperature: f64) {
        let fire = &mut self.fires[index];
        fire.temperature = temperature;
        self.network.set_fixed(fire.node, temperature);
    }

    /// How many members it follows.
    pub fn members(&self) -> usize {
        self.surface.len()
    }

    /// Member `k`'s temperature (K).
    pub fn temperature(&self, k: usize) -> f64 {
        self.network.temperature(k)
    }

    /// The network.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Follows members the structure split (buckling): each half as hot as the member was,
    /// exposed as it was. A split appends the second half; the first takes the member's place,
    /// and the joint between them is new, shared by those two alone.
    fn follow_splits(&mut self, structure: &Structure) {
        let (before, now) = (self.members(), structure.element_count());
        if now == before {
            return;
        }
        let members = structure.member_geometry();
        let mut temperatures = self.temperatures();
        for j in before..now {
            let joint = members[j].0;
            let parent = (0..now)
                .find(|&e| e != j && (members[e].0 == joint || members[e].1 == joint))
                .expect("a split's first half shares its new joint");
            temperatures.push(temperatures[parent]);
            for fire in &mut self.fires {
                let engulfed = fire.engulfs[parent];
                fire.engulfs.push(engulfed);
            }
        }
        self.drift.resize(now, 1.0);
        self.build(structure, &temperatures);
    }

    /// Steps the heat by `dt` seconds and sets the members of `structure` (the one it was
    /// made from) to their new temperatures, refactorising `solver` once they have drifted
    /// far from it. Returns the thermal step's report and whether it refactorised.
    pub fn step(
        &mut self,
        dt: f64,
        structure: &mut Structure,
        solver: Option<&mut ImplicitSolver>,
    ) -> (StepReport, bool) {
        self.follow_splits(structure);
        let report = self.network.step(dt);
        let mut far = false;
        for (k, drift) in self.drift.iter_mut().enumerate() {
            *drift *= structure.set_temperature(k, self.network.temperature(k), &self.material);
            far |= (*drift - 1.0).abs() > DRIFT;
        }
        let refactor = far && solver.is_some();
        if let Some(solver) = solver.filter(|_| far) {
            solver.refactor(structure);
            self.drift.fill(1.0);
        }
        (report, refactor)
    }
}
