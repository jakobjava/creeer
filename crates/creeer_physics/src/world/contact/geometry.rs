//! A state's contact geometry, kept for the state it was measured at.
//!
//! Within a step the contacts' gaps, slips, normals, lever arms and measured depths are a
//! function of the members' positions and orientations alone. A Newton iteration asks for
//! them at the same state five or six times: the line search's energy at the step it takes,
//! the friction's normal forces there, the energy again, the gradient, the matrix's
//! stiffness, how far the next step may go before a contact closes. A rock pile spent a
//! fifth of its step measuring the same geometry again. It is measured once per state.

use super::Contacts;
use crate::structure::group::MemberState;
use creeer_math::glam::DVec3;
use std::sync::{Arc, Mutex, PoisonError};

/// One contact's gap, its dashpot's reference gap, the normal force's direction, its slip,
/// and both points' lever arms (`Contacts::gap_and_slip`).
pub(super) type Geo = (f64, f64, DVec3, DVec3, DVec3, DVec3);

/// The geometry at a state: each contact's, and each patch's measured depths.
#[derive(Clone, Debug)]
pub(super) struct Geometry {
    pub at: Arc<Vec<Geo>>,
    pub measured: Arc<Vec<Vec<f64>>>,
}

impl Geometry {
    /// The contacts' gaps.
    pub fn gaps(&self) -> Vec<f64> {
        self.at.iter().map(|a| a.0).collect()
    }
}

/// The last state measured and its geometry. Valid between a step's start and its end (the
/// contacts' references, carried points and turning normals are set as it starts); a copy
/// starts empty.
#[derive(Debug, Default)]
pub(super) struct Kept(Mutex<Option<(Vec<f64>, Geometry)>>);

impl Clone for Kept {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Kept {
    /// Forgets what it kept.
    pub fn clear(&mut self) {
        *self.0.get_mut().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// The members' positions and orientations, every coordinate: what the geometry is a
/// function of.
fn key(states: &[MemberState<'_>]) -> Vec<f64> {
    let mut key = Vec::new();
    for s in states {
        for p in s.position {
            key.extend([p.x, p.y, p.z]);
        }
        for q in s.orientation {
            key.extend([q.x, q.y, q.z, q.w]);
        }
    }
    key
}

impl Contacts {
    /// The contacts' geometry at `states`, measured once per state.
    pub(super) fn geometry(&self, states: &[MemberState<'_>]) -> Geometry {
        let key = key(states);
        let mut kept = self.kept.0.lock().unwrap_or_else(PoisonError::into_inner);
        #[allow(clippy::float_cmp)] // the very same state
        if let Some((k, geometry)) = kept.as_ref()
            && *k == key
        {
            return geometry.clone();
        }
        let at: Vec<Geo> = (0..self.list.len())
            .map(|i| self.gap_and_slip(i, states))
            .collect();
        let geometry = Geometry {
            at: Arc::new(at),
            measured: Arc::new(self.measured_depths(states)),
        };
        *kept = Some((key, geometry.clone()));
        geometry
    }
}
