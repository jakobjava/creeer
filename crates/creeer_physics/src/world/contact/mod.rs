//! Contacts as energy terms of the implicit step (ADR-0005).
//!
//! A contact joins a point of A and a point of B (or of the fixed world), with a normal from
//! B to A that is frozen for the step. Its gap g is measured (`Measure`) against the plane
//! through B's point, g = n·(p_A − p_B) − offset; on curved ground against that surface to
//! second order; for a round body on the ground from its centre to the closest ground point,
//! found afresh at every evaluation. Its penetration is δ = −g. The energy is
//!
//! - elastic: the contact law's stored energy at δ (`law.rs`): Hertz's for points, the
//!   pressure integrated over the face for a patch's vertices;
//! - damping: c/(2β)·(min(g, 0) − min(g_r, 0))², a dashpot on the penetration rate: the gap's
//!   own rate as the scheme takes it, g_r its value at x̂ along its own history (as
//!   x̂ = x + τ·ẋ extrapolates the bodies; BDF2's (4·g − g₋₁)/3). It damps the contact's
//!   own vibration critically, c = 2·√(k·m), k the law's stiffness for the step
//!   and m the effective mass: a resting body settles, and a moving one keeps its normal
//!   force (c = k·h, seven times overdamped, turned the step's small velocity errors into a
//!   ball's normal force swinging from 190 to 510 N over bumps for its 284 N weight, F5).
//!   Not the bodies' velocity at the end of the step projected on the normal: the scheme's
//!   velocity is off by O(h²·x‴) on a path of changing curvature, 0.4 mm/s for a ball
//!   rolling over bumps at 3 m/s, which the stiff dashpot turned into 12 % of its weight
//!   taken from the elastic force friction uses (F5). Nor the gap's mean rate over the
//!   step, (g − g₀)/h: under BDF2 a third weaker than the scheme's own, and ten times
//!   further off the rolling ball's true force (0.3 % against 0.03 % a tick, F5). Bouncing
//!   follows Newton's impact law with the materials' restitution (`impact.rs`);
//! - friction: μ·λ·f₀(|u|), with u the tangential offset of A's point from its anchor on B
//!   and λ the normal force (lagged, refreshed during the step as in IPC, Li et al. 2020).
//!   u is measured from an anchor point on B, and f₀ is |u| smoothed below 0.1 mm: sticking
//!   is a stiff spring that never creeps, sliding is Coulomb friction μ·λ exactly.

pub mod cap;
mod coupling;
mod dashpot;
mod geometry;
mod integrals;
pub mod law;
mod layer;
mod matrix;
mod measure;
mod normal;
pub mod patch;
#[cfg(test)]
mod patch_tests;
pub mod turning;
mod twist;

use super::friction::Carried;
use super::ground::Ground;
use super::shape::Shape;
use crate::structure::group::MemberState;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use law::Hertz;
use normal::Memo;
use patch::Patch;
use std::sync::Arc;

/// Static friction is a stiff spring between the contact point and an anchor on the other
/// body, up to this offset (m); beyond it the contact slides with force μ·λ and the anchor
/// follows. Measuring from an anchor instead of from the start of each step means a
/// sticking contact holds a constant offset and never creeps.
pub const STICK_DISTANCE: f64 = 1.0e-4;

pub use cap::{cap, reach, stiffest};
pub use measure::Measure;
pub use normal::Sides;

/// How accurate (relative) the deepest contact's force is when a step stops.
const CONVERGED: f64 = 0.01;

/// The least penetration a point contact's stiffness is taken at for the step's matrix (m):
/// Hertz's stiffness vanishes at first touch, but the step must see the contact.
const MATRIX_DEPTH: f64 = 1.0e-5;

/// One end of a contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    /// A point attached to a node (`local` in the node's frame), of member `member` of the
    /// group.
    Node {
        /// Member index in the group.
        member: usize,
        /// Node index in the member.
        node: usize,
        /// Point in the node's frame.
        local: DVec3,
    },
    /// A fixed point of the world.
    Fixed(DVec3),
}

/// How a contact's normal force follows its penetration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Law {
    /// A point contact.
    Hertz(Hertz),
    /// A vertex of a patch (index into the group's patches, and vertex in it).
    Patch {
        /// The patch.
        patch: usize,
        /// The vertex.
        vertex: usize,
    },
}

/// A contact and its material parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Side A (the normal points towards it).
    pub a: Side,
    /// Side B.
    pub b: Side,
    /// Unit normal from B to A, frozen for the step.
    pub normal: DVec3,
    /// Radii subtracted from the distance (spheres).
    pub offset: f64,
    /// Its law.
    pub law: Law,
    /// Stiffness of the normal force in the step's matrix (N/m): the law's at the penetration
    /// expected in the step (set when the step begins).
    pub stiffness: f64,
    /// Effective mass of the bodies it joins at its points (kg; sets its stiffest).
    pub mass: f64,
    /// Dashpot c (N·s/m), set when the step begins.
    pub damping: f64,
    /// Whether it has a dashpot (impact substeps resolve the vibration and drop it).
    pub dashpot: bool,
    /// Whether it touches at the start of the step or will within it (set when the step
    /// begins): its stiffness then counts in the step's matrix.
    touching: bool,
    /// Coulomb friction coefficient μ.
    pub friction: f64,
    /// Coefficient of restitution for the impact law after the step.
    pub restitution: f64,
    /// Normal velocity of A relative to B at the start of the step (m/s, negative when
    /// approaching).
    pub approach: f64,
    /// Tangential speed of A relative to B at the start of the step (m/s): whether it
    /// slides, for the Newton matrix.
    pub sliding: f64,
    /// Its friction's stiffness at the iteration's slip, once relinearised (`Coupling::
    /// relinearise`); until then the secant of the sliding at the start.
    tangent: Option<DMat3>,
    gap_hat: f64,
    lambda: f64,
    /// The material point of B that A's point sticks to (friction measures slip from it).
    pub anchor: Side,
    /// Radii of the two sides at the contact (spheres). With either above zero, friction
    /// acts at carried points (`friction.rs`) instead of material points.
    pub radius_a: f64,
    /// Radius of side B.
    pub radius_b: f64,
    /// Lever arms (world) of both points from their nodes, and the gap's gradient with respect
    /// to A's point, for the step's matrices: as the step starts, and measured again at each
    /// iterate a group of rigid bodies relinearises at (`Coupling::relinearise`). Kept from the
    /// start alone, a tumbling rock's blocks were off by up to a tenth as its normals turned.
    arm_a: DVec3,
    arm_b: DVec3,
    lever: DVec3,
    /// How its normal turns within the step (`turning.rs`).
    pub turning: turning::Turning,
    /// Contacts with balls between rigid bodies: whether they resist spinning, the orientations
    /// of both sides' nodes at the start of the step, and the stiffness of their drilling
    /// friction for the matrices (`twist.rs`).
    drills: bool,
    spin_start: [DQuat; 2],
    twist_stiffness: f64,
}

impl Contact {
    /// A contact between two sides.
    pub fn new(a: Side, b: Side, normal: DVec3, offset: f64, law: Law, friction: f64) -> Self {
        Self {
            a,
            b,
            normal,
            offset,
            law,
            stiffness: 0.0,
            mass: f64::INFINITY,
            damping: 0.0,
            dashpot: true,
            touching: false,
            friction,
            restitution: 0.0,
            approach: 0.0,
            sliding: 0.0,
            tangent: None,
            gap_hat: 0.0,
            lambda: 0.0,
            anchor: b,
            drills: false,
            spin_start: [DQuat::IDENTITY; 2],
            twist_stiffness: 0.0,
            radius_a: 0.0,
            radius_b: 0.0,
            arm_a: DVec3::ZERO,
            arm_b: DVec3::ZERO,
            lever: DVec3::ZERO,
            turning: turning::Turning::Frozen,
        }
    }

    /// Normal force at the start of the last step (N).
    pub fn normal_force(&self) -> f64 {
        self.lambda
    }

    /// Whether a side is a sphere, whose friction acts at carried points.
    pub fn rolls(&self) -> bool {
        self.radius_a > 0.0 || self.radius_b > 0.0
    }
}

/// World position of a side, and the lever arm from its node.
fn point(side: Side, states: &[MemberState<'_>]) -> (DVec3, DVec3) {
    match side {
        Side::Node {
            member,
            node,
            local,
        } => {
            let s = &states[member];
            let arm = s.orientation[node] * local;
            (s.position[node] + arm, arm)
        }
        Side::Fixed(p) => (p, DVec3::ZERO),
    }
}

/// The contacts of one group.
#[derive(Clone, Debug, Default)]
pub struct Contacts {
    /// The contacts.
    pub list: Vec<Contact>,
    /// The patches their [`Law::Patch`] vertices belong to.
    pub patches: Vec<Patch>,
    /// For each contact of the step, its carried friction points (contacts with spheres;
    /// `friction.rs`). Kept beside the contacts: most contacts have none, and a larger
    /// `Contact` made every step slower.
    carried: Vec<Option<[Carried; 2]>>,
    beta: f64,
    /// How each contact's gap is measured (empty: all against their frozen plane).
    measures: Vec<Measure>,
    /// Each contact's gap extrapolated along its own history to x̂, as the scheme extrapolates
    /// the bodies: its dashpot damps the gap's rate from there.
    references: Vec<f64>,
    /// The reference gaps of the patches' measured points over curved ground, as `references`.
    point_references: Vec<Vec<f64>>,
    /// Each patch's dashpot between its corners for the step (N·s/m, `dashpot.rs`).
    face_dashpots: Vec<dashpot::FaceDashpot>,
    /// The ground, for contacts measured against it.
    ground: Option<Arc<dyn Ground>>,
    /// The shapes balls are measured against ([`Measure::Closest`]).
    pub shapes: Vec<Shape>,
    /// The remaining error of positions the step may stop at (m; set when it begins).
    tolerance: f64,
    /// The patches' stiffness between corners for the step's matrices (`matrix.rs`).
    pairs: Vec<Option<[[f64; 4]; 4]>>,
    /// The rigid bodies' 6×6 blocks for the step's matrices, by member (`matrix.rs`).
    rigid: Vec<[[f64; 6]; 6]>,
    smoothing: f64,
    /// The last result of each patch's law (`normal.rs`).
    memo: Memo,
    /// The geometry at the last state measured (`geometry.rs`).
    kept: geometry::Kept,
    /// The most any contact carries at rest (N; `Self::set_weight`).
    weight: f64,
    /// The elastic energy the last step left in these contacts, if known (`Self::set_left`).
    left: Option<f64>,
    /// The twist (rad) within which drilling friction sticks in this step (`twist.rs`).
    twist_smoothing: f64,
}

impl Contacts {
    /// Contacts with the given list, patches, how each gap is measured (`measures`; empty:
    /// all against their frozen planes) and the ground they may be measured against.
    pub fn new(
        list: Vec<Contact>,
        patches: Vec<Patch>,
        measures: Vec<Measure>,
        (ground, shapes): (Option<Arc<dyn Ground>>, Vec<Shape>),
    ) -> Self {
        Self {
            list,
            patches,
            carried: Vec::new(),
            measures,
            references: Vec::new(),
            point_references: Vec::new(),
            face_dashpots: Vec::new(),
            ground,
            shapes,
            tolerance: f64::INFINITY,
            pairs: Vec::new(),
            rigid: Vec::new(),
            beta: 1.0,
            smoothing: 1.0,
            memo: Memo::default(),
            kept: geometry::Kept::default(),
            weight: 0.0,
            left: None,
            twist_smoothing: 1.0,
        }
    }

    /// Sets every contact's stiffness limit for steps of `h` seconds (`stiffest`). Their
    /// energy means nothing before: measured with the materials' own stiffness, it put
    /// 2,200 J into a pile's energy at the start of a step that the energy guard and the
    /// impact law then allowed it (F5).
    pub fn set_step(&mut self, h: f64) {
        let weight = self.weight;
        for c in &mut self.list {
            if let Law::Hertz(law) = &mut c.law {
                law.set_cap(cap(c.mass, h, weight));
            }
        }
        for p in &mut self.patches {
            p.set_cap(cap(self.list[p.first].mass, h, weight));
        }
    }

    /// Sets the most any contact of the group carries at rest (N): the weight of all its
    /// bodies and the loads on them (`cap.rs`).
    pub fn set_weight(&mut self, weight: f64) {
        self.weight = weight;
    }

    /// The carried friction points of contact `i`, if it has them.
    fn carried(&self, i: usize) -> Option<[Carried; 2]> {
        self.carried.get(i).copied().flatten()
    }

    /// Where the anchor of contact `i` must move after the step so that its offset is at
    /// most [`STICK_DISTANCE`] (world point), or `None` if it stuck.
    pub fn moved_anchor(&self, i: usize, states: &[MemberState<'_>]) -> Option<DVec3> {
        let slip = self.geometry(states).at[i].3;
        let length = slip.length();
        let anchor = self.list[i].anchor;
        (length > STICK_DISTANCE)
            .then(|| point(anchor, states).0 + slip * (1.0 - STICK_DISTANCE / length))
    }

    /// The stick offset of contact `i` after the step, at most [`STICK_DISTANCE`] long: what
    /// a rolling contact carries into the next step (`anchor.rs`).
    pub fn end_slip(&self, i: usize, states: &[MemberState<'_>]) -> DVec3 {
        let slip = self.geometry(states).at[i].3;
        let length = slip.length();
        if length > STICK_DISTANCE {
            slip * (STICK_DISTANCE / length)
        } else {
            slip
        }
    }

    /// f₀ and f₁(x)/x of the friction smoothing.
    fn smooth(&self, x: f64) -> (f64, f64) {
        let e = self.smoothing;
        if x >= e {
            (x, 1.0 / x)
        } else {
            (
                -x * x * x / (3.0 * e * e) + x * x / e + e / 3.0,
                2.0 / e - x / (e * e),
            )
        }
    }
}
