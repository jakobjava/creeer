//! What couples the members of a group (`group.rs`): the energy terms between them, as
//! contacts are, and what they see of the members.

use creeer_math::glam::{DMat3, DQuat, DVec3};

/// Positions and orientations of one member, as a coupling sees them.
#[derive(Clone, Copy, Debug)]
pub struct MemberState<'a> {
    /// Node positions.
    pub position: &'a [DVec3],
    /// Node orientations.
    pub orientation: &'a [DQuat],
}

/// What a coupling learns at the start of a step.
#[derive(Clone, Copy, Debug)]
pub struct StepInfo<'a> {
    /// β of the integration scheme (velocity = (x − x̂)/β).
    pub beta: f64,
    /// Time step (s).
    pub h: f64,
    /// Members at the start of the step.
    pub start: &'a [MemberState<'a>],
    /// Members at x̂, q̂ (the velocity reference: damping acts on the change from here).
    pub hat: &'a [MemberState<'a>],
    /// Members one step before the start (the history the schemes extrapolate from).
    pub previous: &'a [MemberState<'a>],
    /// How far ahead of the start x̂ lies (s): 0 for backward Euler, h/3 for BDF2, h/2 for
    /// Newmark.
    pub tau: f64,
}

/// Stiffness coupling two nodes of different members: (member, node) of each side and the
/// 6×6 block over (translation, rotation), rows of the first.
pub type NodeCoupling = ((usize, usize), (usize, usize), [[f64; 6]; 6]);

/// Energy terms coupling the members of a group.
pub trait Coupling: Sync {
    /// Called before every minimisation.
    fn begin(&mut self, info: &StepInfo<'_>);
    /// The largest remaining error of positions (m) the step may stop at, as far as the
    /// coupling's forces need it (the solver's own tolerance applies too).
    fn tolerance(&self) -> f64 {
        f64::INFINITY
    }
    /// Contribution to the incremental potential (elastic, damping, friction).
    fn energy(&self, states: &[MemberState<'_>]) -> f64;
    /// Stored elastic energy only (for the energy guard).
    fn elastic_energy(&self, states: &[MemberState<'_>]) -> f64;
    /// The part of [`Self::elastic_energy`] found anew each step (contacts), against which
    /// [`Self::carried`] is compared: joints' drives are not found anew.
    fn found_energy(&self, states: &[MemberState<'_>]) -> f64 {
        self.elastic_energy(states)
    }
    /// [`Self::elastic_energy`] and its part [`Self::found_energy`] together (one evaluation
    /// where they share it).
    fn elastic_and_found(&self, states: &[MemberState<'_>]) -> (f64, f64) {
        (self.elastic_energy(states), self.found_energy(states))
    }
    /// Adds the gradient of [`Self::energy`] to `gradient[member][block]` (translation
    /// blocks, then rotation blocks).
    fn add_gradient(&self, states: &[MemberState<'_>], gradient: &mut [Vec<DVec3>]);
    /// Gauss-Newton stiffness the coupling adds to node 0 of a rigid-body member, as a 6×6
    /// matrix over (translation, rotation), already including the factor of its damping.
    fn rigid_stiffness(&self, member: usize) -> [[f64; 6]; 6];
    /// Gauss-Newton stiffness coupling two nodes of different members, per contact: the
    /// 6×6 block over (translation, rotation) of `(member a, node a)` (rows) and `(member b,
    /// node b)` (columns), with the same factors as [`Self::rigid_stiffness`].
    fn rigid_coupling(&self) -> Vec<NodeCoupling> {
        Vec::new()
    }
    /// How far along `direction` (each member's blocks from `offsets[member]`) the first of
    /// the coupling's terms now inactive comes into play, if one does within a full step: a
    /// contact now apart that the step would press (its gap followed linearly).
    fn closing(
        &self,
        _states: &[MemberState<'_>],
        _direction: &[DVec3],
        _offsets: &[usize],
    ) -> Option<f64> {
        None
    }
    /// Translation stiffness the coupling adds at nodes of a structure member (3×3 blocks,
    /// possibly several per node), for its factorised matrix.
    fn node_stiffness(&self, _member: usize) -> Vec<(usize, DMat3)> {
        Vec::new()
    }
    /// Updates lagged quantities (friction's normal force) to `states`; returns whether any
    /// changed enough that the minimisation should continue with the new values.
    fn update_lag(&mut self, _states: &[MemberState<'_>]) -> bool {
        false
    }

    /// Takes the coupling's stiffness at `states` into the rigid bodies' matrices (true Newton);
    /// returns whether it did (then [`Self::rigid_stiffness`] and [`Self::rigid_coupling`]
    /// have changed).
    fn relinearise(&mut self, _states: &[MemberState<'_>]) -> bool {
        false
    }

    /// The elastic energy the coupling held at the end of the last step, where it was found
    /// anew for this one (J), if known; given once.
    fn carried(&mut self) -> Option<f64> {
        None
    }

    /// Makes the members' velocities at the end of a substep consistent with the coupling's
    /// constraints (a joint's points moving apart: `world/joint/project.rs`).
    fn project(&mut self, _members: &mut [super::group::Member<'_>]) {}

    /// Whether the coupling's own points are moved (a joint's, by whoever holds it: its work
    /// is then `driven_work`'s).
    fn driven(&self) -> bool {
        false
    }

    /// The work the coupling's moved points did over the last substep (J), which the members'
    /// energy does not count.
    fn driven_work(&self) -> f64 {
        0.0
    }
}

/// No coupling: a single structure on its own.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCoupling;

impl Coupling for NoCoupling {
    fn begin(&mut self, _: &StepInfo<'_>) {}
    fn energy(&self, _: &[MemberState<'_>]) -> f64 {
        0.0
    }
    fn elastic_energy(&self, _: &[MemberState<'_>]) -> f64 {
        0.0
    }
    fn add_gradient(&self, _: &[MemberState<'_>], _: &mut [Vec<DVec3>]) {}
    fn rigid_stiffness(&self, _: usize) -> [[f64; 6]; 6] {
        [[0.0; 6]; 6]
    }
}
