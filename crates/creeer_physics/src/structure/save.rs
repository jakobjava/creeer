//! Structures as a save holds them: every node's state and properties, every member with its
//! permanent deformation, and the solver's history (what the next step continues from).
//!
//! Numbers are written bit for bit. A loaded structure is the saved one; its solver is made
//! anew from it and given the saved history, so only what it caches between steps (its
//! factorised matrix) is rebuilt: the loaded world goes on as the saved one would have,
//! within the solver's tolerance (`world/save.rs`).

use super::Structure;
use super::element::{Element, MEASURES, Stiffness};
use super::section::{Material, RateSensitivity, Section};
use crate::gravity::{PlanetGravity, Tidal};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

use std::sync::Arc;

mod solver;
pub(crate) use solver::{read_solver, write_solver};

/// A number that may be infinite (an unbreakable material's strength) but not NaN.
fn real(r: &mut Reader<'_>) -> Result<f64, DecodeError> {
    let v = r.f64()?;
    if v.is_nan() {
        Err(r.error("not a number"))
    } else {
        Ok(v)
    }
}

impl Encode for Section {
    fn encode(&self, w: &mut Writer) {
        for v in [self.area, self.i_y, self.i_z, self.torsion] {
            w.f64(v);
        }
        for v in [self.shear_factor, self.extent, self.plastic_modulus] {
            w.put(&v);
        }
        w.f64(self.plastic_torsion);
        w.f64(self.perimeter);
    }
}

impl Decode for Section {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let (area, i_y, i_z, torsion) = (r.finite()?, r.finite()?, r.finite()?, r.finite()?);
        let (shear_factor, extent): ([f64; 2], [f64; 2]) = (r.get()?, r.get()?);
        let (plastic_modulus, plastic_torsion) = (r.get()?, r.finite()?);
        // Version 3 (F6) on. Before, as if round (exact for the tubes and bars the earlier
        // sandboxes built).
        let perimeter = if r.version() >= 3 {
            r.finite()?
        } else {
            std::f64::consts::PI * (extent[0] + extent[1])
        };
        Ok(Self {
            area,
            i_y,
            i_z,
            torsion,
            shear_factor,
            extent,
            plastic_modulus,
            plastic_torsion,
            perimeter,
        })
    }
}

impl Encode for Material {
    fn encode(&self, w: &mut Writer) {
        let m = self;
        for v in [
            m.youngs_modulus,
            m.shear_modulus,
            m.density,
            m.yield_strength,
            m.ultimate_strength,
            m.damping,
            m.elongation,
            m.hardening_start,
            m.uniform_elongation,
            m.rate.d,
            m.rate.q,
            m.rate.reference,
        ] {
            w.f64(v);
        }
    }
}

impl Decode for Material {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            youngs_modulus: real(r)?,
            shear_modulus: real(r)?,
            density: real(r)?,
            yield_strength: real(r)?,
            ultimate_strength: real(r)?,
            damping: real(r)?,
            elongation: real(r)?,
            hardening_start: real(r)?,
            uniform_elongation: real(r)?,
            rate: RateSensitivity {
                d: real(r)?,
                q: real(r)?,
                reference: real(r)?,
            },
        })
    }
}

impl Encode for Element {
    fn encode(&self, w: &mut Writer) {
        w.put(&self.a);
        w.put(&self.b);
        w.f64(self.rest_length);
        for v in [self.axis, self.y, self.z] {
            w.put(&v);
        }
        let k = &self.stiffness;
        for v in [
            k.axial,
            k.torsion,
            k.bend[0].0,
            k.bend[0].1,
            k.bend[1].0,
            k.bend[1].1,
        ] {
            w.f64(v);
        }
        w.put(&self.section);
        w.put(&self.material);
        w.put(&self.plastic);
        w.put(&self.flow_rate);
        w.f64(self.plastic_strain);
        w.f64(self.thermal);
        w.put(&self.broken);
        w.put(&self.crooked);
    }
}

impl Decode for Element {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            a: r.get()?,
            b: r.get()?,
            rest_length: r.finite()?,
            axis: r.get()?,
            y: r.get()?,
            z: r.get()?,
            stiffness: Stiffness {
                axial: r.finite()?,
                torsion: r.finite()?,
                bend: [(r.finite()?, r.finite()?), (r.finite()?, r.finite()?)],
            },
            section: r.get()?,
            material: r.get()?,
            plastic: r.get::<[f64; MEASURES]>()?,
            flow_rate: r.get::<[f64; MEASURES]>()?,
            plastic_strain: r.finite()?,
            // Version 3 (F6) on.
            thermal: if r.version() >= 3 { r.finite()? } else { 0.0 },
            broken: r.get()?,
            crooked: r.get()?,
        })
    }
}

impl Encode for PlanetGravity {
    fn encode(&self, w: &mut Writer) {
        w.f64(self.mu);
        w.put(&self.spin);
        w.put(&self.uniform);
        w.varint(self.tidal.len() as u64);
        for t in self.tidal.iter() {
            w.f64(t.mu);
            w.put(&t.at);
        }
    }
}

impl Decode for PlanetGravity {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let (mu, spin, uniform) = (r.finite()?, r.get()?, r.get()?);
        let tidal: Vec<Tidal> = (0..r.count(32)?)
            .map(|_| {
                Ok(Tidal {
                    mu: r.finite()?,
                    at: r.get()?,
                })
            })
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self {
            mu,
            spin,
            uniform,
            tidal: Arc::from(tidal),
        })
    }
}

impl Encode for Structure {
    fn encode(&self, w: &mut Writer) {
        w.put(&self.position);
        w.put(&self.orientation);
        w.put(&self.velocity);
        w.put(&self.angular_velocity);
        w.put(&self.load);
        w.put(&self.torque);
        w.put(&self.drag);
        w.put(&self.gravity);
        w.put(&self.planet);
        w.put(&self.rest);
        w.put(&self.mass);
        w.put(&self.inertia);
        w.put(&self.free_translation);
        w.put(&self.free_rotation);
        w.put(&self.elements);
        w.put(&self.splits);
    }
}

impl Decode for Structure {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let s = Self {
            position: r.get()?,
            orientation: r.get()?,
            velocity: r.get()?,
            angular_velocity: r.get()?,
            load: r.get()?,
            torque: r.get()?,
            drag: r.get()?,
            dashpots: Vec::new(),
            added_mass: Vec::new(),
            gravity: r.get()?,
            planet: r.get()?,
            rest: r.get()?,
            mass: r.get()?,
            inertia: r.get()?,
            free_translation: r.get()?,
            free_rotation: r.get()?,
            elements: r.get()?,
            splits: r.get()?,
        };
        let n = s.position.len();
        let per_node = [
            s.orientation.len(),
            s.velocity.len(),
            s.angular_velocity.len(),
            s.load.len(),
            s.torque.len(),
            s.drag.len(),
            s.rest.len(),
            s.mass.len(),
            s.inertia.len(),
            s.free_translation.len(),
            s.free_rotation.len(),
        ];
        if n == 0 || per_node.iter().any(|&m| m != n) {
            return Err(r.error("a structure's nodes disagree in number"));
        }
        if s.elements
            .iter()
            .any(|e| e.a >= n || e.b >= n || e.a == e.b)
        {
            return Err(r.error("a member joins nodes the structure lacks"));
        }
        if s.mass.iter().any(|&m| m <= 0.0) {
            return Err(r.error("a node without mass"));
        }
        if !bounded(&s) {
            return Err(r.error("a structure with numbers no world has"));
        }
        Ok(s)
    }
}

/// The largest magnitude a saved structure's numbers may have: beyond any world (10¹⁸ m,
/// kg, N), and small enough that what the solver makes of them stays finite.
pub(super) const LARGEST: f64 = 1e18;

/// Whether every number of `s` is within [`LARGEST`] and its orientations are rotations.
fn bounded(s: &Structure) -> bool {
    let vectors = [
        &s.position,
        &s.velocity,
        &s.angular_velocity,
        &s.load,
        &s.torque,
        &s.rest,
    ];
    let vectors_ok = vectors
        .iter()
        .all(|v| v.iter().all(|x| x.abs().max_element() <= LARGEST))
        && [&s.free_translation, &s.free_rotation].iter().all(|v| {
            v.iter()
                .all(|m| m.to_array().iter().all(|a| [0.0, 1.0].contains(a)))
        });
    let scalars_ok = s.mass.iter().chain(&s.drag).all(|m| m.abs() <= LARGEST)
        && s.inertia
            .iter()
            .all(|i| i.to_cols_array().iter().all(|x| x.abs() <= LARGEST));
    let rotations_ok = s
        .orientation
        .iter()
        .all(|q| (q.length() - 1.0).abs() < 1e-6);
    let elements_ok = s.elements.iter().all(|e| {
        let k = &e.stiffness;
        [
            k.axial,
            k.torsion,
            k.bend[0].0,
            k.bend[0].1,
            k.bend[1].0,
            k.bend[1].1,
            e.rest_length,
        ]
        .iter()
        .all(|x| x.abs() <= LARGEST)
            && e.rest_length > 0.0
    });
    vectors_ok
        && scalars_ok
        && rotations_ok
        && elements_ok
        && s.gravity.abs().max_element() <= LARGEST
}
