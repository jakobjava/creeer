//! F7.2 simulation LOD (ADR-0019): a planet with 100,000 resting crates parked far from its
//! 8 players, who walk among 400 bodies of their own. Measured: the LOD pass (every
//! `lod::PERIOD` ticks), the tick with and without the parked crates, and a player arriving
//! at a parked yard of 1,000 crates.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_core::TimingStats;
use creeer_data::Data;
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_net::message::{AvatarInput, Command};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_server::ServerWorld;
use creeer_server::world::{DT, load_terrain};
use creeer_sim::lod;
use creeer_sim::save::ParkedBody;
use std::time::{Duration, Instant};

/// The LOD pass on min-spec (ms): its share of a tick.
const PASS_BUDGET_MS: f64 = 0.1;
/// The most a region's waking may add to a tick on min-spec (ms).
const WAKE_BUDGET_MS: f64 = 2.0;

/// A point `east` and `north` metres from the spawn along the ground, `height` above it.
pub(crate) fn place(w: &ServerWorld, east: f64, north: f64, height: f64) -> DVec3 {
    let up = w.spawn().normalize();
    let (e, n) = creeer_terrain::geology::deposits::tangent_frame(up);
    let at = w.ground_under(w.spawn() + e * east + n * north);
    at + at.normalize() * height
}

fn crate_body(at: DVec3) -> Body {
    let mut body = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        at,
        DT,
    );
    body.friction = 0.7;
    body
}

/// The world: 8 players 300 m apart, 50 crates in a yard by each, settled.
pub(crate) fn world() -> (ServerWorld, Vec<EntityId>) {
    let data = Data::base();
    let terrain = load_terrain(&data, "planets/home.ron").expect("home.ron");
    let mut w = ServerWorld::new(data, terrain, "planets/home.ron");
    let mut players = Vec::new();
    for k in 0..8 {
        let east = 300.0 * f64::from(k);
        let at = place(&w, east, 0.0, creeer_sim::avatar::RADIUS + 0.02);
        players.push(w.add_body(creeer_sim::avatar::body(at, DT), Some(format!("p{k}"))));
        for c in 0..50 {
            let (x, y) = (f64::from(c % 10) * 1.5, f64::from(c / 10) * 1.5);
            let at = place(&w, east + 5.0 + x, 5.0 + y, 0.6);
            w.apply(&Command::Crate(at), None).expect("a crate");
        }
    }
    (w, players)
}

/// Parked yards: `yards` of `per` crates, each 10 m from the next, spread round the planet
/// beyond the players' reach (their ground found once a yard: parked, they are not stepped).
pub(crate) fn yards(w: &ServerWorld, yards: u32, per: u32, first_id: u32) -> Vec<Vec<ParkedBody>> {
    let mut template = World::new(DT);
    template.planet = w.world.planet.clone();
    let mut id = first_id;
    let mut islands = Vec::new();
    for y in 0..yards {
        let (east, north) = (
            20_000.0 + 40.0 * f64::from(y % 500),
            40.0 * f64::from(y / 500),
        );
        let base = place(w, east, north, 0.5);
        let up = base.normalize();
        let (e, n) = creeer_terrain::geology::deposits::tangent_frame(up);
        for c in 0..per {
            let at = base + e * (2.0 * f64::from(c % 10)) + n * (2.0 * f64::from(c / 10));
            template.add(crate_body(at));
        }
        let (bodies, _) = template.take_bodies(|_, _| true);
        for mut body in bodies {
            body.sleep.asleep = true;
            islands.push(vec![ParkedBody {
                id,
                player: None,
                body,
                water: Vec::new(),
            }]);
            id += 1;
        }
    }
    islands
}

pub(crate) fn walk(w: &mut ServerWorld, players: &[EntityId], tick: u64) {
    for (k, &id) in players.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let angle = tick as f64 * DT * 0.3 + k as f64;
        let input = AvatarInput {
            walk: DVec3::new(
                creeer_math::det::cos(angle),
                0.0,
                creeer_math::det::sin(angle),
            ) * 2.0,
            ..AvatarInput::default()
        };
        w.steer(id, &input);
    }
}

pub(crate) fn centres(w: &ServerWorld, players: &[EntityId]) -> Vec<DVec3> {
    (players.iter())
        .filter_map(|&id| w.index(id))
        .map(|i| w.world.bodies[i].structure.position[0])
        .collect()
}

pub fn run(report: &mut Report) {
    println!("F7.2 simulation LOD: 8 players, 400 bodies near them, 100,000 parked (ms)");
    let (mut w, players) = world();
    for tick in 0..300 {
        walk(&mut w, &players, tick);
        w.step();
    }
    let mut save = w.to_save();
    let first = save.next_id;
    save.parked = yards(&w, 1000, 100, first);
    save.next_id = first + 100_000;
    let terrain = w.terrain.clone();
    let mut parked = ServerWorld::from_save(Data::base(), save.clone(), terrain);
    assert_eq!(parked.parked().1, 100_000);

    // The pass alone, on the ticks it runs (the players walk).
    let mut passes = TimingStats::new(60);
    let mut ticks = TimingStats::new(600);
    let mut alone = w;
    let mut ticks_alone = TimingStats::new(600);
    for tick in (parked.tick..).take(600) {
        for (k, world) in [&mut parked, &mut alone].into_iter().enumerate() {
            walk(world, &players, tick);
            let start = Instant::now();
            let c = centres(world, &players);
            let pass = Instant::now();
            world.regions(&c);
            if k == 0 && world.tick.is_multiple_of(lod::PERIOD) {
                passes.push(pass.elapsed());
            }
            world.step();
            [&mut ticks, &mut ticks_alone][k].push(start.elapsed());
        }
    }
    println!(
        "  LOD pass: {:.3} p50 / {:.3} worst | tick {:.3} p50 / {:.3} p99 with 100,000 parked, {:.3} / {:.3} without",
        p(&passes, 50.0),
        p(&passes, 100.0),
        p(&ticks, 50.0),
        p(&ticks, 99.0),
        p(&ticks_alone, 50.0),
        p(&ticks_alone, 99.0)
    );
    report.metric("lod_pass_worst_ms", p(&passes, 100.0), Better::Lower);
    report.metric(
        "lod_tick_100k_parked_p50_ms",
        p(&ticks, 50.0),
        Better::Lower,
    );
    let projected = p(&passes, 100.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= PASS_BUDGET_MS,
        format!("LOD pass (worst) projected to min-spec: {projected:.3} ms <= {PASS_BUDGET_MS} ms"),
    );
    let extra = p(&ticks, 50.0) / p(&ticks_alone, 50.0) - 1.0;
    report.budget(
        extra <= 0.05,
        format!(
            "tick with 100,000 parked against none: {:+.1} % <= +5 %",
            extra * 100.0
        ),
    );
    wake(report, &save);
    machines(report, &save);
}

/// A player walks up to a parked yard of 1,000 crates (resting on the ground, in ten
/// stacks of a hundred side by side): the worst tick of the next two seconds against the
/// same player's ticks before.
fn wake(report: &mut Report, save: &creeer_sim::save::Save) {
    let data = Data::base();
    let terrain = load_terrain(&data, "planets/home.ron").expect("home.ron");
    let mut w = ServerWorld::from_save(data, save.clone(), terrain);
    let player = w.avatars()[0];
    // The yard 3.2 km east of the player, built and settled first, then parked.
    let east = 3_200.0;
    let yard: Vec<EntityId> = (0..1000)
        .map(|c| {
            let (x, y) = (f64::from(c % 32) * 1.2, f64::from(c / 32) * 1.2);
            w.apply(&Command::Crate(place(&w, east + x, y, 0.6)), None)
                .expect("crate");
            *w.ids().last().unwrap()
        })
        .collect();
    let centre = |w: &ServerWorld| centres(w, &[player]);
    let mut settle = 0;
    while !yard.iter().all(|&id| w.is_parked(id)) {
        let c = centre(&w);
        w.regions(&c);
        w.step();
        settle += 1;
        assert!(settle < 20_000, "the yard never rested");
    }
    let tick = |w: &mut ServerWorld, at: Option<DVec3>| -> Duration {
        // Every avatar asked for nothing, as the server asks those of absent players.
        for id in w.avatars() {
            w.steer(id, &AvatarInput::default());
        }
        let start = Instant::now();
        let c = at.map_or_else(|| centre(w), |a| vec![a]);
        w.regions(&c);
        w.step();
        start.elapsed()
    };
    let before = time_ticks(
        || {
            let _ = tick(&mut w, None);
        },
        120,
    );
    // The player is suddenly within reach of it (a vehicle, a teleport).
    let near = place(&w, east - 2_000.0, 0.0, 1.0);
    let mut after = TimingStats::new(120);
    for _ in 0..120 {
        after.push(tick(&mut w, Some(near)));
    }
    assert!(yard.iter().all(|&id| !w.is_parked(id)), "the yard woke");
    let extra = p(&after, 100.0) - p(&before, 100.0);
    println!(
        "  waking 1,000 parked crates: worst tick {:.3} (before {:.3}, extra {extra:.3}); p50 {:.3} (before {:.3})",
        p(&after, 100.0),
        p(&before, 100.0),
        p(&after, 50.0),
        p(&before, 50.0)
    );
    report.metric("lod_wake_1000_extra_ms", extra.max(0.0), Better::Info);
    let projected = extra / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= WAKE_BUDGET_MS,
        format!("waking 1,000 parked crates projected to min-spec: +{projected:.2} ms <= {WAKE_BUDGET_MS} ms"),
    );
}

/// 1,000 abstract powerhouses far from the players (restored from one's snapshot): what they
/// add to a tick when none is due.
fn machines(report: &mut Report, save: &creeer_sim::save::Save) {
    use creeer_sim::abstraction::Abstract;
    use creeer_sim::powerhouse::Powerhouse;
    use creeer_sim::save::SavedMachine;
    let mut one = Abstract::new(Powerhouse::new());
    while !one.try_leave() {
        for _ in 0..600 {
            one.tick();
        }
    }
    let snapshot = one.snapshot();
    let with = |n: u32| {
        let mut s = save.clone();
        s.parked.clear();
        s.machines = (0..n)
            .map(|k| SavedMachine {
                id: k + 1,
                kind: "powerhouse".into(),
                at: DVec3::new(0.0, 0.0, 600_000.0 + f64::from(k)),
                born: s.tick,
                snapshot: snapshot.clone(),
            })
            .collect();
        let data = Data::base();
        let terrain = load_terrain(&data, "planets/home.ron").expect("home.ron");
        ServerWorld::from_save(data, s, terrain)
    };
    let (mut none, mut many) = (with(0), with(1000));
    let players: Vec<EntityId> = none.avatars();
    let mut stats = [TimingStats::new(600), TimingStats::new(600)];
    for tick in 0..600u64 {
        for (k, w) in [&mut none, &mut many].into_iter().enumerate() {
            walk(w, &players, tick);
            let start = Instant::now();
            let c = centres(w, &players);
            w.regions(&c);
            w.step();
            stats[k].push(start.elapsed());
        }
    }
    let extra = p(&stats[1], 50.0) - p(&stats[0], 50.0);
    println!(
        "  1,000 abstract powerhouses: tick {:.3} p50 against {:.3} without (extra {extra:.3})",
        p(&stats[1], 50.0),
        p(&stats[0], 50.0)
    );
    report.metric(
        "lod_1000_abstract_machines_extra_ms",
        extra.max(0.0),
        Better::Info,
    );
    let projected = extra / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= 0.05,
        format!("1,000 abstract machines projected to min-spec: +{projected:.3} ms <= 0.05 ms"),
    );
}
