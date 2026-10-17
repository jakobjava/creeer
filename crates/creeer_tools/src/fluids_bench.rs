//! F6.6 fluids: a town's water mains of 5,000 pipes (a 50 × 50 grid of junctions fed by two
//! pumped reservoirs and a tank, every junction drawing its demand, the demand swinging
//! ±50 % over ten minutes) with what the water carries, at the rate it picks from its
//! structure (ADR-0014); and 250 houses' water (5,250 pipes in small networks, each every tick,
//! stepped in parallel as a world's machines are); 1,000 aero panels on 100 bodies tumbling
//! through the air; 200 boxes and hulls floating in a sea.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_fluids::aero::{Section, Shape, Surface};
use creeer_fluids::atmosphere::Atmosphere;
use creeer_fluids::pipes::{LinkKind, Liquid, Network, NodeKind};
use creeer_materials::species::SpeciesTable;
use creeer_math::glam::DVec3;
use creeer_physics::world::shape::Shape as Collider;
use creeer_physics::world::{Body, World, rotation};
use creeer_sim::aero::{Airframe, Datum, Sky};
use creeer_sim::sea::{Floater, Level, Sea};
use creeer_terrain::{AtmosphereData, OceanData};
use rayon::prelude::*;

const DT: f64 = 1.0 / 60.0;
/// Budgets on the minimum-spec CPU (docs/f6-plan.md), ms per 60 Hz tick.
const PIPES_MS: f64 = 1.0;
const PANELS_MS: f64 = 0.2;
const FLOATING_MS: f64 = 0.2;

/// Runs the fluids benchmark.
pub fn run(report: &mut Report) {
    println!("F6.6 fluids (ms per 60 Hz tick, p50 / p99)");
    pipes(report);
    houses(report);
    panels(report);
    floating(report);
    println!();
}

fn mains() -> (Network, Vec<usize>) {
    let mut net = Network::new(Liquid::WATER_20C, 9.806_65);
    let n = 50;
    let junctions: Vec<usize> = (0..n * n)
        .map(|k| {
            net.node(
                ((k % 7) as f64) * 2.0,
                NodeKind::Junction { demand: 2e-4 },
                30.0,
            )
        })
        .collect();
    let pipe = |length: f64, diameter: f64| LinkKind::Pipe {
        length,
        diameter,
        roughness: 1e-4,
        minor: 0.5,
        check: false,
    };
    for i in 0..n {
        for j in 0..n {
            let k = i * n + j;
            if j + 1 < n {
                net.link(junctions[k], junctions[k + 1], pipe(120.0, 0.15));
            }
            if i + 1 < n {
                net.link(junctions[k], junctions[k + n], pipe(120.0, 0.15));
            }
        }
    }
    let pump = LinkKind::Pump {
        shutoff: 60.0,
        resistance: 400.0,
        speed: 1.0,
        best_flow: 0.3,
        best_efficiency: 0.8,
    };
    for corner in [0, n * n - 1] {
        let lake = net.node(0.0, NodeKind::Reservoir, 0.0);
        net.link(lake, junctions[corner], pump);
    }
    let tank = net.node(
        40.0,
        NodeKind::Tank {
            area: 400.0,
            floor: 40.0,
        },
        45.0,
    );
    net.link(tank, junctions[n * n / 2 + n / 2], pipe(200.0, 0.4));
    (net, junctions)
}

fn pipes(report: &mut Report) {
    let (mut net, junctions) = mains();
    let pipes = net
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pipe { .. }))
        .count();
    let mut tracer = net.tracer(&[0.0, 288.0]);
    // Its own rate, from its structure (ADR-0014).
    let every = net.ticks_per_step();
    for _ in 0..30 * every {
        if let Some((_, h)) = net.tick(DT) {
            net.carry(&mut tracer, h);
        }
    }
    let mut tick = 0u32;
    let mut iterations = 0;
    // Each sample: one step's interval of ticks.
    let t = time_ticks(
        || {
            for _ in 0..every {
                // The demand swings ±50 % over ten minutes (a day's swing 144 times as fast).
                let swing = 1.0
                    + 0.5
                        * creeer_math::det::sin(
                            f64::from(tick) * DT * std::f64::consts::TAU / 600.0,
                        );
                for &j in &junctions {
                    net.nodes[j].kind = NodeKind::Junction {
                        demand: 2e-4 * swing,
                    };
                }
                if let Some((r, h)) = net.tick(DT) {
                    iterations = iterations.max(r.iterations);
                    net.carry(&mut tracer, h);
                }
                tick += 1;
            }
        },
        200,
    );
    let per_tick = p(&t, 50.0) / f64::from(every);
    println!(
        "  {pipes} pipes and their content: every {every} ticks ({} Hz), {:.3} / {:.3} ms a step, {per_tick:.3} ms a tick | Newton <= {iterations} iterations",
        60 / every,
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("fluids_5000_pipes_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = per_tick / MIN_SPEC_CORE_FACTOR;
    report.budget(projected <= PIPES_MS, format!("{pipes} pipes at their rate projected to min-spec: {projected:.2} ms a tick <= {PIPES_MS} ms"));
}

fn standard_sky() -> Sky {
    let data = AtmosphereData {
        composition: vec![
            ("N2".into(), 0.78084),
            ("O2".into(), 0.20946),
            ("Ar".into(), 0.00934),
            ("CO2".into(), 0.00036),
        ],
        surface_pressure: 101_325.0,
        surface_temperature: 288.15,
        layers: vec![(11_000.0, -6.5e-3), (20_000.0, 0.0)],
    };
    let r0 = 6_356_766.0;
    let atmosphere = Atmosphere::new(&data, r0, 9.806_65 * r0 * r0, &SpeciesTable::reference())
        .expect("standard air");
    Sky {
        atmosphere,
        datum: Datum::Flat,
        wind: DVec3::new(5.0, 0.0, 0.0),
    }
}

fn panels(report: &mut Report) {
    let sky = standard_sky();
    let mut world = World::new(DT);
    let mut frames = Vec::new();
    for b in 0..100 {
        let at = DVec3::new(
            f64::from(b % 10) * 20.0,
            1000.0 + f64::from(b / 10) * 20.0,
            0.0,
        );
        let body = world.add(Body::rigid(Collider::Sphere { radius: 0.5 }, 20.0, at, DT));
        let s = &mut world.bodies[body].structure;
        s.velocity[0] = DVec3::new(40.0, -5.0, 3.0);
        s.angular_velocity[0] = DVec3::new(0.3, 1.0, -0.5);
        for k in 0..10 {
            let normal = rotation(DVec3::Z, f64::from(k) * 0.6) * DVec3::Y;
            let chord = normal.cross(DVec3::Z).normalize();
            let shape = Shape::Panel {
                area: 0.5,
                aspect: 5.0,
                efficiency: 0.9,
                section: Section::NACA_2412,
                normal,
                chord,
            };
            frames.push(Airframe::new(
                body,
                0,
                Surface {
                    shape,
                    offset: normal * 0.8,
                },
            ));
        }
    }
    let t = time_ticks(
        || {
            creeer_sim::aero::push(&sky, None, &mut world, &mut frames);
            creeer_sim::aero::settle(&mut world, &mut frames);
        },
        200,
    );
    println!(
        "  {} aero panels on 100 bodies: {:.3} / {:.3} ms",
        frames.len(),
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("fluids_1000_panels_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= PANELS_MS,
        format!("1000 panels projected to min-spec: {projected:.3} ms <= {PANELS_MS} ms"),
    );
}

fn floating(report: &mut Report) {
    let water = OceanData {
        density: 1025.97,
        viscosity: 1.22e-3,
        sound: 1506.7,
        temperature: 288.15,
    };
    let sea = Sea {
        level: Level::Flat,
        water,
        surface_pressure: 101_325.0,
    };
    let mut world = World::new(DT);
    let mut floaters = Vec::new();
    let hull = std::sync::Arc::new(creeer_physics::world::hull::Hull::cuboid(DVec3::new(
        1.5, 0.4, 0.6,
    )));
    for b in 0..200 {
        let at = DVec3::new(f64::from(b % 20) * 5.0, -0.1, f64::from(b / 20) * 5.0);
        let shape = if b % 2 == 0 {
            Collider::Box {
                half: DVec3::new(0.5, 0.3, 0.5),
            }
        } else {
            Collider::Hull(hull.clone())
        };
        let body = world.add(Body::rigid(shape.clone(), 300.0, at, DT));
        world.bodies[body].structure.orientation[0] =
            rotation(DVec3::new(1.0, 0.0, 1.0).normalize(), 0.2);
        world.bodies[body].structure.velocity[0] = DVec3::new(1.0, -0.3, 0.0);
        floaters.push(Floater::new(body, 0, shape, 1.0));
    }
    let t = time_ticks(
        || {
            creeer_sim::sea::push(&sea, &mut world, &mut floaters);
            creeer_sim::sea::settle(&mut world, &mut floaters, DT);
        },
        200,
    );
    println!(
        "  200 floating bodies: {:.3} / {:.3} ms",
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("fluids_200_floating_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= FLOATING_MS,
        format!("200 floating bodies projected to min-spec: {projected:.3} ms <= {FLOATING_MS} ms"),
    );
}

/// A house's water: the mains, a pump up to a roof tank, twenty taps on a branching line.
fn house() -> Network {
    let mut net = Network::new(Liquid::WATER_20C, 9.806_65);
    let mains = net.node(0.0, NodeKind::Reservoir, 30.0);
    let tank = net.node(
        12.0,
        NodeKind::Tank {
            area: 1.0,
            floor: 12.0,
        },
        12.5,
    );
    let pump = LinkKind::Pump {
        shutoff: 25.0,
        resistance: 4e4,
        speed: 1.0,
        best_flow: 0.002,
        best_efficiency: 0.6,
    };
    net.link(mains, tank, pump);
    let mut last = tank;
    for k in 0..20 {
        let tap = net.node(
            10.0 - f64::from(k % 5),
            NodeKind::Junction { demand: 5e-5 },
            0.0,
        );
        let line = LinkKind::Pipe {
            length: 3.0,
            diameter: 0.02,
            roughness: 1e-4,
            minor: 0.5,
            check: false,
        };
        net.link(if k % 4 == 0 { tank } else { last }, tap, line);
        last = tap;
    }
    net
}

/// 250 houses (5,250 pipes in small networks): each picks every tick, and they are stepped in
/// parallel, as a world's separate machines are.
fn houses(report: &mut Report) {
    let mut town: Vec<(Network, creeer_fluids::pipes::content::Tracer)> = (0..250)
        .map(|_| {
            let net = house();
            let tracer = net.tracer(&[0.0, 288.0]);
            (net, tracer)
        })
        .collect();
    let every = town[0].0.ticks_per_step();
    let pipes: usize = town.iter().map(|(n, _)| n.links.len()).sum();
    let mut tick = 0u32;
    let t = time_ticks(
        || {
            let swing = 1.0
                + 0.5 * creeer_math::det::sin(f64::from(tick) * DT * std::f64::consts::TAU / 600.0);
            town.par_iter_mut().for_each(|(net, tracer)| {
                for node in &mut net.nodes {
                    if let NodeKind::Junction { demand } = &mut node.kind {
                        *demand = 5e-5 * swing;
                    }
                }
                if let Some((_, h)) = net.tick(DT) {
                    net.carry(tracer, h);
                }
            });
            tick += 1;
        },
        200,
    );
    println!(
        "  250 houses, {pipes} pipes: every {every} tick ({} Hz), {:.3} / {:.3} ms a tick",
        60 / every,
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("fluids_250_houses_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(projected <= PIPES_MS, format!("{pipes} pipes in 250 houses projected to min-spec: {projected:.2} ms a tick <= {PIPES_MS} ms"));
}
