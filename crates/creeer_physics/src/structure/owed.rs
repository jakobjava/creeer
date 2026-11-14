//! Energy that never existed, taken back out of a substep's motion (`group.rs`): what the
//! contacts of a body new to the world held when it was put in pressed into another
//! (`Coupling::owed`). The step resolves the overlap as it would any; the energy guard then takes
//! the velocity the overlap gave back out of what the substep gained, so the bodies are pushed
//! apart without being thrown apart (as engines resolve a penetration by positions alone:
//! Bullet's split impulse, PhysX's depenetration velocity).

use super::group::Member;
use creeer_math::glam::{DMat3, DVec3};

/// Each member's node velocities and spins at a substep's start.
pub(super) type Velocities = Vec<(Vec<DVec3>, Vec<DVec3>)>;

/// The members' velocities and spins now.
pub(super) fn velocities(members: &[Member<'_>]) -> Velocities {
    (members.iter())
        .map(|(s, _)| (s.velocity.clone(), s.angular_velocity.clone()))
        .collect()
}

/// Takes `excess` (J) from the members' kinetic energy out of what their velocities gained in
/// the substep from `starts`: each velocity v₀ + α·(v − v₀), with the α in [0, 1] that leaves
/// the kinetic energy `excess` less (it is a parabola in α). Where even α = 0 leaves more, the
/// rest is taken by scaling every velocity down (`remove_kinetic`).
pub(super) fn remove_gained(members: &mut [Member<'_>], starts: &Velocities, excess: f64) {
    // K(α) = a·α² + b·α + c.
    let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
    for ((s, _), (v0, w0)) in members.iter().zip(starts) {
        for i in 0..s.node_count() {
            let r = DMat3::from_quat(s.orientation[i]);
            let inertia = r * s.inertia[i] * r.transpose();
            let (m, dv, dw) = (
                s.mass[i],
                s.velocity[i] - v0[i],
                s.angular_velocity[i] - w0[i],
            );
            a += 0.5 * (m * dv.length_squared() + dw.dot(inertia * dw));
            b += m * v0[i].dot(dv) + w0[i].dot(inertia * dw);
            c += 0.5 * (m * v0[i].length_squared() + w0[i].dot(inertia * w0[i]));
        }
    }
    let target = a + b + c - excess;
    let alpha = if c >= target || a <= 0.0 {
        0.0
    } else {
        let root = (b * b - 4.0 * a * (c - target)).max(0.0).sqrt();
        ((root - b) / (2.0 * a)).clamp(0.0, 1.0)
    };
    for ((s, _), (v0, w0)) in members.iter_mut().zip(starts) {
        for i in 0..s.node_count() {
            s.velocity[i] = v0[i] + (s.velocity[i] - v0[i]) * alpha;
            s.angular_velocity[i] = w0[i] + (s.angular_velocity[i] - w0[i]) * alpha;
        }
    }
    if c > target {
        super::group::remove_kinetic(members, c - target);
    }
}
