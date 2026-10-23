//! The drawn body is the simulated one (G1.3e, ADR-0026 decision 6): a ragdoll's bones are its
//! segments' poses, so each segment's centre at rest is drawn where it is, and the two bones
//! of each joint draw the joint at one place (to what the joint holds).

use super::bones::{fallen_bones, standing_bones};
use super::ragdoll::{LINKS, SEGMENTS, fall};
use super::*;
use creeer_math::glam::DQuat;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;

#[test]
fn a_ragdolls_drawn_bones_are_its_segments() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let mut b = body(DVec3::new(0.0, 2.0, 0.0), DVec3::Y, DT);
    b.structure.orientation[0] = DQuat::from_rotation_x(0.6);
    b.solver.reset_history(&b.structure);
    let c = world.add(b);
    let parts = fall(&mut world, c);
    let mut worst = 0.0f64;
    for tick in 0..180 {
        world.step();
        if tick % 30 != 29 {
            continue;
        }
        let poses: Vec<(DVec3, DQuat)> = (parts.iter())
            .map(|&i| {
                let s = &world.bodies[i].structure;
                (s.position[0], s.orientation[0])
            })
            .collect();
        let bones = fallen_bones(&poses);
        for ((bone, (at, _)), (rest, _, _)) in bones.iter().zip(&poses).zip(&SEGMENTS) {
            assert!((bone.transform_point(*rest) - *at).length() < 1e-12);
        }
        for (child, parent, joint, _, _) in LINKS {
            let apart = bones[child].transform_point(joint) - bones[parent].transform_point(joint);
            worst = worst.max(apart.length());
        }
    }
    println!(
        "a joint drawn by its two bones {:.2} mm apart at worst",
        worst * 1e3
    );
    assert!(worst < 0.005, "{worst} m");
}

#[test]
fn a_standing_character_is_drawn_from_its_feet() {
    let turn = DQuat::from_rotation_y(0.8);
    let centre = DVec3::new(3.0, 10.0, -2.0);
    for bone in standing_bones(centre, turn) {
        let feet = bone.transform_point(DVec3::ZERO);
        assert!((feet - (centre - DVec3::Y * CENTRE)).length() < 1e-12);
        assert!(bone.rotation.abs_diff_eq(turn, 1e-15));
    }
}
