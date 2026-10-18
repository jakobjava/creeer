//! The character in the home planet's sea: it floats (a person in a sealed suit is lighter
//! than the water it displaces), swims at a swimmer's speed against the water's drag, and
//! rises when asked.

use super::*;
use crate::planet_gravity;
use crate::surroundings::Surroundings;
use creeer_materials::species::SpeciesTable;
use creeer_terrain::{Planet, Terrain};

const DT: f64 = 1.0 / 60.0;

/// A world over deep water on the home planet, a character with its centre at the surface,
/// and the sea's up there.
fn at_sea() -> (World, Surroundings, usize, DVec3) {
    let planet = Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron");
    let around = Surroundings::of(&planet, &SpeciesTable::reference()).expect("its sea");
    let terrain = Terrain::new(planet);
    let deep = creeer_terrain::elevation::fibonacci_sphere(4000)
        .find(|&d| terrain.surface_height(d) < -200.0)
        .expect("an ocean");
    let level = terrain.planet().datum.radius(deep);
    let mut world = World::new(DT);
    world.planet = Some(planet_gravity(terrain.planet()));
    let feet = deep * (level - heights().0);
    let c = world.add(body(feet, deep, DT));
    (world, around, c, deep)
}

/// Steps the character in the sea `n` times asking `act`; returns its depth under the
/// surface (its centre's) and velocity at the end.
fn swim(
    (world, around, c, up): &mut (World, Surroundings, usize, DVec3),
    n: usize,
    act: (DVec3, bool),
) -> (f64, DVec3) {
    let mut reactions = Vec::new();
    for _ in 0..n {
        let at = world.bodies[*c].structure.position[0];
        let water = around.sea.as_ref().map(|sea| sea.depth(at));
        steer(world, *c, act, (*up, water), &mut reactions);
        around.push(world);
        world.step();
        release(world, &mut reactions);
        around.settle(world, DT);
    }
    let s = &world.bodies[*c].structure;
    let depth = around
        .sea
        .as_ref()
        .map_or(0.0, |sea| sea.depth(s.position[0]));
    (depth, s.velocity[0])
}

#[test]
fn it_floats_swims_and_rises() {
    let mut sea = at_sea();
    // Floating: its centre near the surface (the suit's air holds it up), at rest.
    let (depth, v) = swim(&mut sea, 300, (DVec3::ZERO, false));
    assert!(depth.abs() < 0.6, "its centre {depth} m under");
    assert!(v.length() < 0.05, "still moving at {}", v.length());
    // Swimming along the surface: 80 N against a bluff body's drag, about 0.6 m/s.
    let up = sea.3;
    let east = up.any_orthonormal_vector();
    let (_, v) = swim(&mut sea, 300, (east * 2.0, false));
    let speed = v.dot(east);
    assert!((0.3..1.2).contains(&speed), "swims at {speed} m/s");
    // Asked to rise: higher out of the water than floating.
    let (floating, _) = swim(&mut sea, 300, (DVec3::ZERO, false));
    let (rising, _) = swim(&mut sea, 60, (DVec3::ZERO, true));
    assert!(rising < floating - 0.02, "rose to {rising} from {floating}");
    eprintln!("at sea: floats {floating:.2} m under, swims {speed:.2} m/s, rises to {rising:.2}");
}
