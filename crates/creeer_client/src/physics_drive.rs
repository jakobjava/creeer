//! F6.4 drivetrain scene: a flywheel station. A 400 kg frame floats in space, tumbling
//! slowly; inside, a motor spins a 60 kg steel flywheel up, lets it coast, and brakes it
//! back. The frame turns the other way as the wheel spins up (angular momentum conserved:
//! the drivetrain's housing reaction on the frame, `creeer_sim::mount`), and its tumble
//! wobbles as the wheel's momentum is carried round with it.

use creeer_math::glam::{DQuat, DVec3};
use creeer_mech::{Drivetrain, Element, Node};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_sim::mount::Mount;

/// The frame's half extents and mass, the wheel's radius and mass, the motor's torque.
const FRAME: DVec3 = DVec3::new(1.0, 0.6, 1.4);
const FRAME_MASS: f64 = 400.0;
const WHEEL_RADIUS: f64 = 0.35;
const WHEEL_MASS: f64 = 60.0;
const MOTOR: f64 = 25.0;
/// The motor's program (s): spin up, coast, brake back to rest, rest; then again.
const PROGRAM: [f64; 4] = [8.0, 4.0, 8.0, 4.0];

/// The scene's drivetrain on its frame.
#[derive(Debug)]
pub struct SceneDrive {
    drivetrain: Drivetrain,
    wheel: Node,
    mount: Mount,
    /// The wheel's angle in the frame (rad: drawing it), the scene's time (s).
    angle: f64,
    time: f64,
}

/// Builds the station in `world` (in space: no ground, no gravity).
pub fn station(world: &mut World) -> SceneDrive {
    world.statics.clear();
    let dt = world.dt();
    let mut frame = Body::rigid(
        Shape::Box { half: FRAME },
        FRAME_MASS,
        DVec3::new(0.0, 3.0, 0.0),
        dt,
    );
    frame.structure.gravity = DVec3::ZERO;
    frame.structure.angular_velocity[0] = DVec3::new(0.08, 0.0, 0.03);
    frame.structure.orientation[0] = DQuat::from_rotation_x(0.2);
    let body = world.add(frame);
    let mut drivetrain = Drivetrain::new();
    // A solid steel disc on the frame's own y axis, in ball bearings.
    let inertia = 0.5 * WHEEL_MASS * WHEEL_RADIUS * WHEEL_RADIUS;
    let wheel = drivetrain.part(inertia, DVec3::Y);
    drivetrain.add(Element::Bearing {
        a: wheel,
        viscous: 0.002,
        coulomb: 0.05,
    });
    SceneDrive {
        drivetrain,
        wheel,
        mount: Mount::new(body, 0),
        angle: 0.0,
        time: 0.0,
    }
}

impl SceneDrive {
    /// The motor's torque at time `t` (N·m): its program.
    fn torque(&self, t: f64) -> f64 {
        let cycle: f64 = PROGRAM.iter().sum();
        let mut phase = t % cycle;
        for (k, length) in PROGRAM.iter().enumerate() {
            if phase < *length {
                return match k {
                    0 => MOTOR,
                    // Braking regeneratively to rest (no further: it does not reverse).
                    2 if self.drivetrain.speed(self.wheel) > 0.5 => -MOTOR,
                    _ => 0.0,
                };
            }
            phase -= length;
        }
        0.0
    }

    /// Before the world's step of `dt`: the drivetrain steps in the frame's turning frame,
    /// its reaction put on the frame.
    pub fn before(&mut self, world: &mut World, dt: f64) {
        let torque = self.torque(self.time);
        self.drivetrain.set_torque(self.wheel, torque);
        let spin = self.mount.spin(world, &self.drivetrain);
        self.drivetrain.step(dt, spin);
        self.mount.push(world, &self.drivetrain);
        self.angle = (self.angle + self.drivetrain.speed(self.wheel) * dt) % std::f64::consts::TAU;
        self.time += dt;
    }

    /// After the world's step of `dt`: the mount's books.
    pub fn after(&mut self, world: &mut World, dt: f64) {
        self.mount.settle(world, &self.drivetrain, dt);
    }

    /// The wheel to draw: centre, orientation, half extents (a disc as a flat box, on the
    /// frame's top: its mass is counted in the frame's, its reaction a couple, the same
    /// wherever on a rigid frame it acts).
    pub fn wheel(&self, world: &World) -> (DVec3, DQuat, DVec3) {
        let s = &world.bodies[self.mount.body].structure;
        let q = s.orientation[0] * DQuat::from_rotation_y(self.angle);
        let r = WHEEL_RADIUS * std::f64::consts::FRAC_1_SQRT_2;
        let at = s.position[0] + s.orientation[0] * DVec3::new(0.0, FRAME.y + 0.08, 0.0);
        (at, q, DVec3::new(r, 0.05, r))
    }

    /// What the scene shows: the wheel's speed, the frame's turning, the momentum held.
    pub fn label(&self, world: &World) -> String {
        let s = &world.bodies[self.mount.body].structure;
        let axis = s.orientation[0] * DVec3::Y;
        let turning = s.angular_velocity[0].dot(axis);
        let rpm = self.drivetrain.speed(self.wheel) * 60.0 / std::f64::consts::TAU;
        format!(
            "flywheel {rpm:.0} rpm ({:.0} kg·m²/s) · frame turning about its axis at {:.1} °/s · motor {:+.0} N·m",
            self.drivetrain.momentum().length(),
            turning.to_degrees(),
            self.drivetrain.torque(self.wheel)
        )
    }
}
