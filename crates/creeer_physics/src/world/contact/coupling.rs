//! The contacts as a coupling of the group's minimisation: setting up the step, their
//! energy, its gradient, and the lagged normal forces of friction (see `mod.rs`).

use super::{CONVERGED, Carried, Contacts, Law, Memo, STICK_DISTANCE, Side, point};
use crate::structure::group::{Coupling, MemberState, NodeCoupling, StepInfo};
use creeer_math::glam::{DMat3, DVec3};

impl Coupling for Contacts {
    fn begin(&mut self, info: &StepInfo<'_>) {
        self.joints.begin(info);
        self.kept.clear();
        self.beta = info.beta;
        self.smoothing = STICK_DISTANCE;
        self.carried.clear();
        self.set_step(info.h);
        self.memo = Memo::default();
        // Normals turning with the bodies are measured from where they are now (`turning.rs`).
        self.start_turning(info.start);
        let gaps_start = self.gaps(info.start);
        let measured_start = self.measured_depths(info.start);
        for (p, measured) in self.patches.iter_mut().zip(&measured_start) {
            p.prepare_on(std::array::from_fn(|k| -gaps_start[p.first + k]), measured);
        }
        // The gap at the prediction, measured as the gap itself is (a ball rolling through a
        // dip "predicted" 70 µm into a frozen plane would count as pressed in): whether and
        // how deep the contact is expected to touch, for the matrix.
        let gaps_hat = self.gaps(info.hat);
        // The gap at x̂ along its own history, as x̂ = x + τ·ẋ extrapolates the bodies (the
        // rate from the last step's motion): BDF2's (4·g − g₋₁)/3.
        let gaps_previous = self.gaps(info.previous);
        let ahead = info.tau / info.h;
        self.references = gaps_start
            .iter()
            .zip(&gaps_previous)
            .map(|(g, p)| g + ahead * (g - p))
            .collect();
        let measured_previous = self.measured_depths(info.previous);
        self.point_references = measured_start
            .iter()
            .zip(&measured_previous)
            .map(|(now, then)| {
                now.iter()
                    .zip(then)
                    .map(|(d, p)| -d + ahead * (p - d))
                    .collect()
            })
            .collect();
        let (_, forces, _) = self.normal_forces(&gaps_start, &measured_start, false);
        for i in 0..self.list.len() {
            let c = self.list[i];
            let ((pa, arm_a), (_, arm_b)) = (point(c.a, info.start), point(c.b, info.start));
            // Contacts with spheres: friction at A's surface point and at the anchor, both
            // carried by their bodies' linearised motion (`friction.rs`).
            let carried = c.rolls().then(|| {
                let surface = pa - c.normal * c.radius_a;
                let anchor = point(c.anchor, info.start).0;
                [
                    Carried::new(c.a, surface, info.start),
                    Carried::new(c.anchor, anchor, info.start),
                ]
            });
            self.carried.push(carried);
            let c = &mut self.list[i];
            c.arm_a = arm_a;
            c.arm_b = arm_b;
            c.lever = c.normal;
            c.gap_hat = gaps_hat[i];
            c.tangent = None;
            // Touching or pressed in at the start, or closing within the step (an impact, whose
            // stiffness is the coupling that matters most). Not the prediction alone: over a
            // crest it lifts off the curved ground, and the matrix lost a sliding ball's contact
            // (F5). Touching counts: bodies set down on each other (gap 0, no velocity) are
            // pressed together by their weight, and without their contacts the first step
            // of a stack took 51 iterations.
            c.touching = gaps_start[i] <= 0.0 || c.gap_hat + c.approach.min(0.0) * self.beta < 0.0;
            c.lambda = forces[i];
            let k = self.matrix_stiffness(i, gaps_start[i], info.h);
            let c = &mut self.list[i];
            c.stiffness = k;
            // Damps the contact's own vibration critically (module docs).
            c.damping = if c.dashpot {
                2.0 * (k * c.mass.min(1e12)).sqrt()
            } else {
                0.0
            };
        }
        self.start_dashpots(&gaps_start, &measured_start);
        // The contact that carries the most: its force must be right to 1 % when the step
        // stops, an error of a hundredth of F/k, its force over its stiffness (its depth for a
        // linear law; the reach for one stiffening under load, where a hundredth of the depth
        // left a column's forces 4 % off and it never came to rest, F5). With the solver's
        // 0.3 µm alone a rolling ball stopped after two iterations with its force 2.5 % off, a
        // kick its motion then rang with (F5).
        self.tolerance = (0..self.list.len())
            .filter(|&i| forces[i] > 0.0 && self.list[i].stiffness > 0.0)
            .max_by(|&i, &j| forces[i].total_cmp(&forces[j]))
            .map_or(f64::INFINITY, |i| {
                CONVERGED * forces[i] / self.list[i].stiffness
            });
        self.start_twist(info.start, info.h);
        self.set_twist_stiffness(info.start);
        self.pairs = self.patch_pairs();
        self.rigid = self.rigid_blocks();
    }

    fn tolerance(&self) -> f64 {
        self.tolerance
    }

    fn closing(
        &self,
        states: &[MemberState<'_>],
        direction: &[DVec3],
        offsets: &[usize],
    ) -> Option<f64> {
        self.closing_step(states, direction, offsets)
    }

    fn energy(&self, states: &[MemberState<'_>]) -> f64 {
        use crate::profile::{Slot, start};
        let timer = start();
        let geometry = self.geometry(states);
        let (at, measured, gaps) = (&geometry.at, &geometry.measured, geometry.gaps());
        timer.stop(Slot::ContactGeometry);
        let timer = start();
        let (elastic, _, _) = self.normal_forces(&gaps, measured, false);
        timer.stop(Slot::ContactLaws);
        let timer = start();
        let (damping, _, _) = self.dashpots(&gaps, measured);
        let friction: f64 = self
            .list
            .iter()
            .zip(at.iter())
            .map(|(c, &(_, _, _, slip, _, _))| c.friction * c.lambda * self.smooth(slip.length()).0)
            .sum();
        let twist = self.twist_energy(states);
        timer.stop(Slot::ContactRest);
        elastic + damping + friction + twist + self.joints.energy(states)
    }

    fn elastic_energy(&self, states: &[MemberState<'_>]) -> f64 {
        self.found_energy(states) + self.joints.elastic_energy(states)
    }

    fn elastic_and_found(&self, states: &[MemberState<'_>]) -> (f64, f64) {
        let found = self.found_energy(states);
        (found + self.joints.elastic_energy(states), found)
    }

    fn found_energy(&self, states: &[MemberState<'_>]) -> f64 {
        let geometry = self.geometry(states);
        self.normal_forces(&geometry.gaps(), &geometry.measured, false)
            .0
    }

    fn add_gradient(&self, states: &[MemberState<'_>], gradient: &mut [Vec<DVec3>]) {
        self.joints.add_gradient(states, gradient);
        let geometry = self.geometry(states);
        let (at, measured, gaps) = (&geometry.at, &geometry.measured, geometry.gaps());
        let (_, forces, points) = self.normal_forces(&gaps, measured, true);
        let (_, dashpots, point_dashpots) = self.dashpots(&gaps, measured);
        for (i, (c, &(_, _, direction, slip, ra, rb))) in
            self.list.iter().zip(at.iter()).enumerate()
        {
            // The elastic force wherever the law has one: a patch's corner carries its share of
            // the face's pressure while lifted itself (the face pressed in between the corners).
            // Applied only where the corner was pressed in, it never reached the bodies: a
            // face pressed unevenly then rested on less than the forces friction used (a tall
            // box stuck on a 20° ramp on 965 N for its 921 N, sliding 11 % short at 40°, F5).
            // Over curved ground the face's points carry its elastic force (below).
            let measured = match c.law {
                Law::Patch { patch, .. } => self.patches[patch].sampled().is_some(),
                Law::Hertz(_) => false,
            };
            let d_gap = if measured {
                dashpots[i]
            } else {
                dashpots[i] - forces[i]
            };
            let friction = slip * (c.friction * c.lambda * self.smooth(slip.length()).1);
            // The normal force acts at the contact points; friction at the material points
            // too (one force), or for spheres at the carried friction points.
            let at_points = match self.carried(i) {
                None => direction * d_gap + friction,
                Some([fa, fb]) => {
                    fa.add_gradient(states, friction, gradient);
                    fb.add_gradient(states, -friction, gradient);
                    direction * d_gap
                }
            };
            for (side, g, arm) in [(c.a, at_points, ra), (c.b, -at_points, rb)] {
                if let Side::Node { member, node, .. } = side {
                    let half = gradient[member].len() / 2;
                    gradient[member][node] += g;
                    gradient[member][half + node] += arm.cross(g);
                }
            }
            // A normal turning with a body: the gap's slope by that body's turn (`turning.rs`).
            if let Some(turned) = self.turned(i, states) {
                for (member, node, slope) in turned.torques.into_iter().flatten() {
                    let half = gradient[member].len() / 2;
                    gradient[member][half + node] += slope * d_gap;
                }
            }
        }
        self.add_twist_gradient(states, gradient);
        // The measured points of faces over curved ground: each pressed in by the ground under
        // it, its force along the slope of its own gap (`measure.rs`).
        for ((p, forces), damping) in self.patches.iter().zip(&points).zip(&point_dashpots) {
            let Side::Node { member, node, .. } = self.list[p.first].a else {
                continue;
            };
            if forces.iter().chain(damping).all(|&f| f == 0.0) {
                continue;
            }
            let half = gradient[member].len() / 2;
            let at = self.measured_points(p, states);
            for ((&force, &damped), &(_, direction, arm)) in forces.iter().zip(damping).zip(&at) {
                let g = direction * (damped - force);
                gradient[member][node] += g;
                gradient[member][half + node] += arm.cross(g);
            }
        }
    }

    fn update_lag(&mut self, states: &[MemberState<'_>]) -> bool {
        // The joints' multipliers (their augmented Lagrangian) with friction's normal forces.
        let mut changed = self.joints.update_lag(states);
        if changed {
            self.rigid = self.rigid_blocks();
        }
        let geometry = self.geometry(states);
        let (_, forces, _) = self.normal_forces(&geometry.gaps(), &geometry.measured, false);
        for (c, &force) in self.list.iter_mut().zip(&forces) {
            // Only friction uses λ: contacts without friction never need a refresh.
            if c.friction > 0.0 && (force - c.lambda).abs() > 0.05 * force.max(c.lambda) {
                changed = true;
            }
            c.lambda = force;
        }
        changed
    }

    /// Each contact's stiffness at the iteration's state: friction's exact Hessian at its slip
    /// (a sliding contact's Coulomb force is constant along its slip: no stiffness there, only
    /// across it; the secant μλ/slip in every direction made the matrix far too stiff along
    /// the motion and a sliding rock's Newton crept, 50–400 iterations a step, F6), a Hertz
    /// contact's at its depth now, a face's as it presses now. The rigid bodies' matrices are
    /// made again from them.
    fn relinearise(&mut self, states: &[MemberState<'_>]) -> bool {
        self.joints.make_blocks(states);
        let e = self.smoothing;
        let geometry = self.geometry(states);
        let gaps = geometry.gaps();
        for i in 0..self.list.len() {
            let (gap, slip) = (geometry.at[i].0, geometry.at[i].3);
            let at = geometry.at[i];
            let c = &mut self.list[i];
            (c.lever, c.arm_a, c.arm_b) = (at.2, at.4, at.5);
            // Its normal stiffness while pressed, none apart: the step's anticipating stiffness
            // kept for a contact come apart made a sliding rock's matrix a hundred times too stiff
            // (and the pile took a third more iterations, F6).
            self.list[i].touching = gap < 0.0;
            if gap < 0.0 && matches!(self.list[i].law, Law::Hertz(_)) {
                let k = self.matrix_stiffness(i, gap, 0.0);
                self.list[i].stiffness = k;
            }
            let c = &mut self.list[i];
            let n = c.normal;
            let plane = DMat3::IDENTITY - DMat3::from_cols(n * n.x, n * n.y, n * n.z);
            let r = slip.length();
            // f₀ (`smooth`): |u| beyond e, a cubic within; f₁ = f₀'/|u|, f₂ = f₀''.
            let (f1, f2) = if r >= e {
                (1.0 / r, 0.0)
            } else {
                (2.0 / e - r / (e * e), 2.0 / e - 2.0 * r / (e * e))
            };
            let along = if r > 0.0 { slip / r } else { DVec3::ZERO };
            let outer = DMat3::from_cols(along * along.x, along * along.y, along * along.z);
            c.tangent = Some(((plane - outer) * f1 + outer * f2) * (c.friction * c.lambda));
        }
        self.set_twist_stiffness(states);
        // Each pressed face's stiffness as it presses now (`Patch::tangent`).
        let measured = &geometry.measured;
        let faces: Vec<_> = (0..self.patches.len())
            .map(|p| {
                let depths = std::array::from_fn(|k| -gaps[self.patches[p].first + k]);
                self.face_pair(p, depths, &measured[p])
            })
            .collect();
        self.pairs = faces;
        self.rigid = self.rigid_blocks();
        true
    }

    fn rigid_coupling(&self) -> Vec<NodeCoupling> {
        let mut blocks = self.coupling_blocks();
        // The joints' blocks, added to the contacts' between the same nodes.
        for (a, b, m6) in &self.joints.cross {
            match blocks.iter_mut().find(|c| (c.0, c.1) == (*a, *b)) {
                Some(c) => {
                    for (row, add) in c.2.iter_mut().zip(m6) {
                        row.iter_mut().zip(add).for_each(|(x, y)| *x += y);
                    }
                }
                None => blocks.push((*a, *b, *m6)),
            }
        }
        blocks
    }

    fn node_stiffness(&self, member: usize) -> Vec<(usize, DMat3)> {
        let mut blocks = self.node_blocks(member);
        blocks.extend(self.joints.node_blocks(member));
        blocks
    }

    fn rigid_stiffness(&self, member: usize) -> [[f64; 6]; 6] {
        self.rigid.get(member).copied().unwrap_or([[0.0; 6]; 6])
    }

    fn carried(&mut self) -> Option<f64> {
        self.left.take()
    }

    fn project(&mut self, members: &mut [crate::structure::group::Member<'_>]) {
        self.joints.project(members);
    }

    fn driven(&self) -> bool {
        self.joints.driven()
    }

    fn driven_work(&self) -> f64 {
        self.joints.driven_work()
    }
}

impl Contacts {
    /// A face's stiffness between its corners where it presses at `depths` (its points at
    /// `measured` over curved ground), with its dashpot's exact form (on a plane: Σ c·w·wᵀ
    /// over the points pressed at the step's start, which damp for the whole step, lifted or
    /// not: left out of a lifted face's matrix, Newton missed most of a rock's stiffness against
    /// the ramp, F6); `None` for neither.
    fn face_pair(&self, p: usize, depths: [f64; 4], measured: &[f64]) -> Option<[[f64; 4]; 4]> {
        let pressed = depths.iter().chain(measured).any(|d| *d > 0.0);
        let form = self
            .face_dashpots
            .get(p)
            .map(super::dashpot::FaceDashpot::form)
            .filter(|f| f.iter().flatten().any(|x| *x != 0.0));
        let mut pair = match (pressed, form) {
            (true, _) => {
                self.patches[p].tangent(depths, measured, self.sigma_at(p, depths, measured))
            }
            (false, Some(_)) => [[0.0; 4]; 4],
            (false, None) => return None,
        };
        for (row, add) in pair.iter_mut().zip(form.unwrap_or_default()) {
            row.iter_mut()
                .zip(add)
                .for_each(|(a, b)| *a += b / self.beta);
        }
        Some(pair)
    }
}
