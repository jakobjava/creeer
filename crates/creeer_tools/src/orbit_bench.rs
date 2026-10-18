//! F4 benchmarks (ADR-0007): generating the default solar system, integrating its bodies,
//! and ticks of the universe at 1× (physics bubble) and at high warp (everything on rails).

use crate::spikes::{Better, Report, p, time_ticks};
use creeer_math::glam::DVec3;
use creeer_orbit::generate::{Params, generate};
use creeer_physics::world::Body;
use creeer_physics::world::shape::Shape;
use creeer_sim::Universe;
use creeer_sim::universe::Focus;
use creeer_sim::universe::predict::PlotFrame;
use creeer_sim::universe::terrains::BodyRef;
use std::time::Instant;

const DT: f64 = 1.0 / 60.0;

/// Balls in circular orbits `altitudes` above home (bubble coordinates of home's surface
/// frame), the first one focused.
fn orbiters(u: &mut Universe, altitudes: impl Iterator<Item = f64>) {
    let home = u.bodies.body(BodyRef::Major(u.home)).clone();
    let pose = u.bubble_pose();
    let axis = pose.rotation.conjugate() * home.spin.axis();
    let spin = pose.rotation.conjugate() * pose.spin;
    let mut first = None;
    for (k, altitude) in altitudes.enumerate() {
        let r = home.radius + altitude;
        #[allow(clippy::cast_precision_loss)]
        let angle = k as f64 * 0.37;
        let x =
            (DVec3::X * creeer_math::det::cos(angle) + DVec3::Z * creeer_math::det::sin(angle)) * r;
        let v = axis.cross(x).normalize() * (home.mu / r).sqrt() - spin.cross(x);
        let mut ball = Body::rigid(Shape::Sphere { radius: 1.0 }, 1000.0, x, DT);
        ball.structure.velocity[0] = v;
        let id = u.spawn("ball", ball);
        first.get_or_insert(id);
    }
    if let Some(id) = first {
        u.focus = Focus::Vessel(id);
    }
}

pub fn run(report: &mut Report) {
    println!("F4 orbits: the default solar system");
    let data = creeer_data::Data::base();
    let params = Params::load(&data, "systems/default.ron").expect("default.ron");
    let start = Instant::now();
    let system = generate(&params).expect("stable");
    let generation_ms = start.elapsed().as_secs_f64() * 1e3;
    println!(
        "  generation with stability checks | {generation_ms:.0} ms ({} majors, {} minors, {} rejected)",
        system.majors.len(),
        system.minors.len(),
        system.attempts
    );
    report.metric("orbit_generate_ms", generation_ms, Better::Info);

    // A year of the bodies (the home planet's).
    let mut eph = system.ephemeris();
    let year = 9.2e6;
    let start = Instant::now();
    eph.extend_to(year);
    let year_ms = start.elapsed().as_secs_f64() * 1e3;
    println!(
        "  bodies over one home year        | {year_ms:.1} ms ({} steps, {} force evaluations)",
        eph.sample_count(),
        eph.evaluations()
    );
    report.metric("orbit_majors_year_ms", year_ms, Better::Lower);
    report.budget(
        year_ms < 2_000.0,
        format!("a game year of the bodies in {year_ms:.0} ms (< 2 s)"),
    );

    // 1×: a ship in the bubble, 160 small bodies on rails.
    let mut u = Universe::new(system.clone(), data.clone(), DT);
    orbiters(&mut u, std::iter::once(150_000.0));
    for _ in 0..30 {
        u.step();
    }
    let normal = time_ticks(|| u.step(), 300);
    println!(
        "  tick at 1× (bubble + rails)      | {:.3} / {:.3} ms",
        p(&normal, 50.0),
        p(&normal, 99.0)
    );
    report.metric("orbit_tick_1x_p50_ms", p(&normal, 50.0), Better::Lower);
    report.budget(
        p(&normal, 50.0) < 2.0,
        format!(
            "tick at 1× in {:.2} ms (< 2 ms of the 10 ms tick)",
            p(&normal, 50.0)
        ),
    );
    let start = Instant::now();
    let (x, v) = u.focus_state();
    let frame = PlotFrame::Body(BodyRef::Major(u.home));
    let prediction = u.predict((x, v), 2.0 * 3600.0, 600, frame);
    let predict_ms = start.elapsed().as_secs_f64() * 1e3;
    println!(
        "  prediction, 2 h in 600 points    | {predict_ms:.2} ms ({} points)",
        prediction.points.len()
    );
    report.metric("orbit_predict_ms", predict_ms, Better::Lower);

    // Warp: 100 ships on rails plus the small bodies.
    let mut u = Universe::new(system, data, DT);
    #[allow(clippy::cast_precision_loss)]
    orbiters(&mut u, (0..100).map(|k| 100_000.0 + 20_000.0 * k as f64));
    u.step();
    for (warp, gated) in [(1e5, true), (1e7, false)] {
        u.set_warp(warp).expect("all in orbit");
        for _ in 0..10 {
            u.step();
        }
        let ticks = time_ticks(|| u.step(), 120);
        println!(
            "  tick at {warp:.0e}×, 100 ships on rails | {:.3} / {:.3} ms",
            p(&ticks, 50.0),
            p(&ticks, 99.0)
        );
        if gated {
            report.metric("orbit_tick_1e5x_p50_ms", p(&ticks, 50.0), Better::Lower);
            report.budget(
                p(&ticks, 50.0) < 8.0,
                format!("tick at 10⁵× in {:.2} ms (< 8 ms)", p(&ticks, 50.0)),
            );
        } else {
            report.metric("orbit_tick_1e7x_p50_ms", p(&ticks, 50.0), Better::Info);
        }
    }
    println!();
}
