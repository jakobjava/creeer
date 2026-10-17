//! A drivetrain in a free-floating body: its housing's reaction turns the body, and the
//! angular momentum of the body and its flywheel together stays what it was (F6.4).

use crate::mount::Mount;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_mech::{Drivetrain, Element};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};

const DT: f64 = 1.0 / 60.0;

/// The body's and the drivetrain's angular momentum in the world's frame.
fn momentum(world: &World, inertia: DMat3, d: &Drivetrain) -> DVec3 {
    let s = &world.bodies[0].structure;
    let turn = DMat3::from_quat(s.orientation[0]);
    turn * inertia * turn.transpose() * s.angular_velocity[0] + turn * d.momentum()
}

/// A 100 kg box, 1 × 1.5 × 2 m, tumbling slowly in free space, a 0.5 kg·m² flywheel on its
/// own z axis spun up by a motor: the box turns the other way, wobbles as the flywheel's
/// momentum is carried round by the tumble, and the total stays put (`mount`: exactly, but for
/// the one step's correction it still owes).
#[test]
fn a_flywheel_spinning_up_turns_a_free_station_the_other_way() {
    let half = DVec3::new(0.5, 0.75, 1.0);
    let mass = 100.0;
    let shape = Shape::Box { half };
    let inertia = shape.inertia(mass);
    let mut world = World::new(DT);
    let mut body = Body::rigid(shape, mass, DVec3::ZERO, DT);
    body.structure.gravity = DVec3::ZERO;
    body.structure.angular_velocity[0] = DVec3::new(0.05, 0.0, 0.0);
    body.structure.orientation[0] = DQuat::from_rotation_y(0.3);
    world.add(body);
    let mut d = Drivetrain::new();
    let wheel = d.part(0.5, DVec3::Z);
    d.add(Element::Bearing {
        a: wheel,
        viscous: 0.002,
        coulomb: 0.0,
    });
    let mut mount = Mount::new(0, 0);
    let start = momentum(&world, inertia, &d);
    let mut worst = 0.0f64;
    for tick in 0..600 {
        // The motor for five seconds, then coasting on its bearing.
        d.set_torque(wheel, if tick < 300 { 2.0 } else { 0.0 });
        let spin = mount.spin(&world, &d);
        d.step(DT, spin);
        mount.push(&mut world, &d);
        world.step();
        mount.settle(&mut world, &d, DT);
        // The body's Newmark step carries half its last torque's impulse into the next.
        let held = momentum(&world, inertia, &d) + mount.owed(DT) + mount.given() * (0.5 * DT);
        worst = worst.max((held - start).length());
    }
    // The flywheel took 10 kg·m²/s about z less what its bearing gave back to the box; the
    // box's own momentum is the opposite.
    let flywheel = d.momentum().length();
    assert!(flywheel > 9.0, "the flywheel's momentum {flywheel}");
    let body = world.bodies[0].structure.angular_velocity[0];
    let z = DMat3::from_quat(world.bodies[0].structure.orientation[0]) * DVec3::Z;
    assert!(body.dot(z) < -0.1, "the box turns the other way: {body}");
    // Against 10 kg·m²/s exchanged: to the body's solver's tolerance (its Newton stop and
    // its rotation's fixed point, 1e-9 a step; measured 2e-8 of it over the ten seconds), and
    // never a step the energy guard redid (the torques' work credited as Newmark did it).
    assert!(
        worst < 1e-7 * flywheel,
        "momentum drifted by {worst} kg·m²/s"
    );
    assert_eq!(world.bodies[0].solver.fallbacks, 0, "steps the guard redid");
}
