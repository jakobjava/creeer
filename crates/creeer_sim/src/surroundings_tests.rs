//! The home planet's air and sea on bodies in its world: a crate dropped over the open sea
//! floats at its draft, a ball falling through the air reaches the drag curve's terminal
//! velocity.

use crate::planet_gravity;
use crate::surroundings::Surroundings;
use creeer_materials::species::SpeciesTable;
use creeer_math::glam::DVec3;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_terrain::{Planet, Terrain};

const DT: f64 = 1.0 / 60.0;

fn home() -> (Terrain, Surroundings) {
    let planet = Planet::load(&creeer_terrain::data_path("planets/home.ron")).expect("home.ron");
    let surroundings = Surroundings::of(&planet, &SpeciesTable::reference()).expect("its air");
    (Terrain::new(planet), surroundings)
}

/// Over deep water, a flat crate (1.2 × 0.5 × 1.2 m, 300 kg: GM > 0, it floats level; a cube of
/// that density would not, and floats tilted) dropped from 3 m comes to float with its bottom
/// 0.20 m under (300/(1026·1.44)); a 50 kg box dropped from 3 km falls at its terminal velocity √(2mg/ρC_dA) in the air
/// there. (A smooth steel ball would pass the drag crisis near Re 3·10⁵ and fall at 170 m/s.)
#[test]
fn the_home_planets_air_and_sea_act_on_its_bodies() {
    let (terrain, mut around) = home();
    let planet = terrain.planet();
    let deep = creeer_terrain::elevation::fibonacci_sphere(4000)
        .find(|&d| terrain.surface_height(d) < -200.0)
        .expect("an ocean");
    let level = planet.datum.radius(deep);
    let mut world = World::new(DT);
    world.planet = Some(planet_gravity(planet));
    let mut crate_body = Body::rigid(
        Shape::Box {
            half: DVec3::new(0.6, 0.25, 0.6),
        },
        300.0,
        deep * (level + 3.0),
        DT,
    );
    // Turned to stand on the sea (its y axis up there).
    crate_body.structure.orientation[0] =
        creeer_math::glam::DQuat::from_rotation_arc(DVec3::Y, deep);
    let floating = world.add(crate_body);
    let side = deep.any_orthonormal_vector();
    let ball = world.add(Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.25),
        },
        50.0,
        deep * (level + 3000.0) + side * 50.0,
        DT,
    ));
    let mut falling = (0.0, 0.0);
    for k in 0..60 * 40 {
        around.push(&mut world);
        world.step();
        around.settle(&mut world, DT);
        // At 25 s it is 2 km down, still in the air (at 40 s it is sinking in the sea).
        if k == 60 * 25 {
            let s = &world.bodies[ball].structure;
            let rho = around.sky.as_ref().expect("air").air(s.position[0]).density;
            // C_d 1 on a quarter of its surface (Cauchy's mean projected area).
            falling = (
                s.velocity[0].length(),
                (2.0 * 50.0 * 9.81 / (rho * 0.375)).sqrt(),
            );
        }
    }
    let crate_at = world.bodies[floating].structure.position[0];
    let bottom = crate_at.length() - planet.datum.radius(crate_at) - 0.25;
    let draft = 300.0 / (planet.ocean.expect("home has a sea").density * 1.44);
    assert!(
        (bottom + draft).abs() < 0.01,
        "its bottom {bottom} m, draft {draft} m"
    );
    assert!(
        (falling.0 / falling.1 - 1.0).abs() < 0.02,
        "the box falls at {} m/s, terminal {}",
        falling.0,
        falling.1
    );
}
