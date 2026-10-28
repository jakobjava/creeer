//! The fist holds a handle's own axis (G1.7, ADR-0027 amended; kept with G2.10's grips): after
//! the step, a slender load does not turn in the fist about its own axis beyond what the
//! grip's friction lets (`hands::grip`, as an impulse over the step: Coulomb's, it only takes
//! spin away), relative to how the hands turn it. The grip's turn springs cap at the wrist's
//! 11 N·m held in one hand; about a stick's own axis (its moment a few hundredths of a percent
//! of the others) nothing then held it, and a branch drooping in one hand to the ground while
//! walked was spun about it at 150 rad/s in a step (G2.10), as G1.7's sticks pressed against a
//! trunk or a crate were, to 450–525 rad/s.

use super::hands::{SLENDER, grip};
use super::*;

impl ServerWorld {
    /// After the step: each slender load's twist in the fist taken away, up to the grip's.
    pub(super) fn grip_holds_twist(&mut self) {
        let held: Vec<(usize, usize, DVec3)> = (self.carrying.values())
            .filter_map(|c| Some((self.index(c.load)?, c.node, c.plan.spin)))
            .collect();
        for (b, node, spin) in held {
            let mass = self.mass_of(b);
            let Some(inertia) = (self.world.bodies[b].shapes.first()).map(|(_, s)| s.inertia(mass))
            else {
                continue;
            };
            let moments = [inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z];
            let largest = moments.iter().copied().fold(0.0, f64::max);
            let (k, least) = (moments.iter().copied().enumerate())
                .fold((0, f64::MAX), |m, (k, v)| if v < m.1 { (k, v) } else { m });
            if !(least > 0.0 && least < SLENDER * largest) {
                continue;
            }
            let s = &mut self.world.bodies[b].structure;
            let axis = s.orientation[node] * DVec3::AXES[k];
            let twist = (s.angular_velocity[node] - spin).dot(axis);
            let most = grip() * DT / least;
            s.angular_velocity[node] -= axis * twist.clamp(-most, most);
        }
    }
}
