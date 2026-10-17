//! The elastic energy the last step left in its contacts, for the next step's energy guard
//! (`structure/group.rs`). Contacts are found anew every step, and found anew they may hold
//! more than they did (a crate's faces fitted again to the curved ground under them): that
//! surplus is no one's, and may not become motion.

use super::World;
use super::contact::{Contacts, Sides};
use crate::structure::NodeId;
use crate::structure::group::MemberState;

/// Each point contact's and face's elastic energy at the end of the last step (J) by the
/// bodies it joined, and the bodies that step moved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Left {
    pub(super) energies: Vec<(Sides, f64)>,
    pub(super) stepped: Vec<usize>,
}

impl World {
    /// Keeps what a group's step left in its contacts (`group`: its bodies' indices).
    pub(super) fn leave(&mut self, group: &[usize], contacts: &Contacts) {
        let states: Vec<MemberState<'_>> = group
            .iter()
            .map(|&b| MemberState {
                position: &self.bodies[b].structure.position,
                orientation: &self.bodies[b].structure.orientation,
            })
            .collect();
        let global = |side: Option<(usize, NodeId)>| side.map(|(m, n)| (group[m], n));
        for ((a, b), energy) in contacts.energies(&states) {
            self.left.energies.push(((global(a), global(b)), energy));
        }
        self.left.stepped.extend_from_slice(group);
    }

    /// What the last step left in the contacts within each of `groups` (`group_of`: each
    /// body's group, `usize::MAX` for none), where it moved all of the group's bodies.
    pub(super) fn left_by_group(
        &self,
        groups: &[Vec<usize>],
        group_of: &[usize],
    ) -> Vec<Option<f64>> {
        let mut stepped = vec![false; self.bodies.len()];
        for &b in &self.left.stepped {
            if let Some(s) = stepped.get_mut(b) {
                *s = true;
            }
        }
        let mut left: Vec<Option<f64>> = groups
            .iter()
            .map(|g| g.iter().all(|&b| stepped[b]).then_some(0.0))
            .collect();
        let group = |side: Option<(usize, NodeId)>| side.map(|(b, _)| group_of[b]);
        for &((a, b), energy) in &self.left.energies {
            let g = match (group(a), group(b)) {
                (Some(x), Some(y)) if x == y => x,
                (Some(x), None) | (None, Some(x)) => x,
                _ => continue,
            };
            if let Some(Some(sum)) = left.get_mut(g) {
                *sum += energy;
            }
        }
        left
    }
}

impl Left {
    /// What is left for the bodies a world kept (`map`: each old body's new index, if
    /// kept): contacts that joined a body taken out go (no step will find them again), the
    /// rest follow their bodies' new indices. Kept, the energy a resting stack's contacts
    /// hold is not found anew as a surplus when bodies elsewhere come and go.
    pub(super) fn renumbered(&self, map: &[Option<usize>]) -> Self {
        let side = |s: Option<(usize, NodeId)>| -> Option<Option<(usize, NodeId)>> {
            match s {
                Some((b, n)) => Some(Some((map.get(b).copied().flatten()?, n))),
                None => Some(None),
            }
        };
        Self {
            energies: (self.energies.iter())
                .filter_map(|&((a, b), e)| Some(((side(a)?, side(b)?), e)))
                .collect(),
            stepped: (self.stepped.iter())
                .filter_map(|&b| map.get(b).copied().flatten())
                .collect(),
        }
    }
}
