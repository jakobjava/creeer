//! The reference sounds (ADR-0037, decision 7): short scripted scenes worked out by the same
//! events the game uses, for the offline renderer (`report.rs`, `creeer_client --sound <dir>`)
//! and the tests. The listener stands at the origin, ears 1.6 m above the ground, facing +z.

use crate::events::{Body, Other, driven, impact, strike};
use crate::happenings::{Underfoot, crack, creak, footstep, rub, wind};
use crate::library::Library;
use crate::mixer::Command;
use crate::output::Recording;
use crate::voice::VoiceSpec;
use creeer_math::glam::{DQuat, DVec3};

/// A scene: its name (the file's), what it should sound like, how long, what is played when, and
/// the sound measured alone (at a metre) for the table.
#[derive(Clone, Debug)]
pub struct Scene {
    pub name: &'static str,
    pub what: &'static str,
    pub seconds: f64,
    pub commands: Recording,
    pub measured: Option<VoiceSpec>,
    /// What beam theory says its lowest mode is (Hz), where it says.
    pub expected: Option<f64>,
}

/// The ground's height below the ears (m).
const GROUND: f64 = -1.6;

fn ahead(metres: f64, height: f64) -> DVec3 {
    DVec3::new(0.0, GROUND + height, metres)
}

fn play(at: f64, spec: VoiceSpec) -> (f64, Command) {
    (at, Command::Play(spec))
}

/// Beam theory's lowest bending mode of a free bar (Hz): (4.730)²/(2πL²)·√(E/ρ)·κ.
fn lowest(lib: &Library, material: &str, length: f64, gyration: f64) -> f64 {
    let a = lib.matter(material).expect("a shipped material").acoustic;
    4.730_040_7f64.powi(2) / (2.0 * std::f64::consts::PI * length * length)
        * a.bar_speed()
        * gyration
}

/// Every reference scene.
///
/// # Panics
/// If the shipped data lacks a material the scenes use (a test keeps it whole).
pub fn all(lib: &Library) -> Vec<Scene> {
    let m = |id: &str| lib.matter(id).expect("a shipped material");
    let soil = lib.ground("soil");
    let rock = lib.ground("granite");
    let gravel = lib.ground("gravel");

    let mut scenes = Vec::new();

    // A green log, 3 m long and 0.2 m across, struck near its end by a stone at 3 m/s.
    let log = Body::member(
        m("spruce_green"),
        (
            ahead(2.0, 0.1) - DVec3::X * 1.5,
            ahead(2.0, 0.1) + DVec3::X * 1.5,
        ),
        0.1,
    );
    let stone = Body::boxed(
        m("granite"),
        DVec3::new(0.04, 0.03, 0.02),
        (ahead(2.0, 0.3), DQuat::IDENTITY),
        None,
    );
    let hit = strike(
        &stone,
        &Other::Body(log),
        ahead(2.0, 0.2) - DVec3::X * 1.2,
        DVec3::new(0.0, -3.0, 0.0),
        1,
    );
    scenes.push(Scene {
        name: "log-struck",
        what: "a green spruce log (3 m, 0.2 m across) struck near its end by a stone: a low wooden knock, dying in a few tenths of a second",
        seconds: 1.5,
        commands: vec![play(0.2, hit)],
        measured: Some(hit),
        expected: Some(lowest(lib, "spruce_green", 3.0, 0.05)),
    });

    // A granite stone dropped half a metre onto rock, and onto soil (3.1 m/s, rebounding a little).
    let drop = DVec3::new(0.0, 1.3 * stone.mass * 3.1, 0.0);
    let on_rock = impact(&stone, &Other::Ground(rock), ahead(1.5, 0.0), drop, 2);
    let on_soil = impact(&stone, &Other::Ground(soil), ahead(1.5, 0.0), drop, 3);
    scenes.push(Scene {
        name: "stone-on-rock",
        what: "a fist-sized granite stone dropped 0.5 m onto rock: a sharp, bright click",
        seconds: 0.8,
        commands: vec![play(0.2, on_rock)],
        measured: Some(on_rock),
        expected: None,
    });
    scenes.push(Scene {
        name: "stone-on-soil",
        what: "the same stone dropped onto soil: a dull, soft thud",
        seconds: 0.8,
        commands: vec![play(0.2, on_soil)],
        measured: Some(on_soil),
        expected: None,
    });

    // A steel bar, 1 m and 25 mm square, struck by a hammer's head at 5 m/s.
    let bar = Body::boxed(
        m("steel_s235"),
        DVec3::new(0.5, 0.0125, 0.0125),
        (ahead(1.5, 0.8), DQuat::IDENTITY),
        None,
    );
    let hammer = Body::boxed(
        m("steel_s235"),
        DVec3::new(0.055, 0.0125, 0.0125),
        (ahead(1.5, 1.0), DQuat::IDENTITY),
        Some(0.45),
    );
    let rung = strike(
        &hammer,
        &Other::Body(bar),
        ahead(1.5, 0.81) + DVec3::X * 0.45,
        DVec3::new(0.0, -5.0, 0.0),
        4,
    );
    scenes.push(Scene {
        name: "steel-bar",
        what: "a 1 m steel bar, 25 mm square, struck by a hammer: a bright ring that lasts seconds",
        seconds: 4.0,
        commands: vec![play(0.2, rung)],
        measured: Some(rung),
        expected: Some(lowest(lib, "steel_s235", 1.0, 0.025 / 12f64.sqrt())),
    });

    // A spruce plank dropped flat onto soil from 0.3 m.
    let plank = Body::boxed(
        m("spruce_c24"),
        DVec3::new(1.0, 0.025, 0.075),
        (ahead(2.0, 0.025), DQuat::IDENTITY),
        None,
    );
    let flat = impact(
        &plank,
        &Other::Ground(soil),
        ahead(2.0, 0.0) + DVec3::X * 0.6,
        DVec3::new(0.0, 1.2 * plank.mass * 2.4, 0.0),
        5,
    );
    scenes.push(Scene {
        name: "plank-dropped",
        what: "a spruce plank (2 m × 150 × 50 mm) dropped flat onto soil: a woody clack with a short ring",
        seconds: 1.0,
        commands: vec![play(0.2, flat)],
        measured: Some(flat),
        expected: Some(lowest(lib, "spruce_c24", 2.0, 0.05 / 12f64.sqrt())),
    });

    scenes.extend(building(lib));
    scenes.extend(walking(lib, (soil, gravel, rock)));

    scenes
}

/// Driving a nail, and a plank loaded until it breaks.
fn building(lib: &Library) -> Vec<Scene> {
    let m = |id: &str| lib.matter(id).expect("a shipped material");
    let mut scenes = Vec::new();
    // A 75 mm nail driven into a log by a claw hammer: its blows over the fastening's two seconds.
    let log = Body::member(
        m("spruce_green"),
        (ahead(1.2, 0.5) - DVec3::X, ahead(1.2, 0.5) + DVec3::X),
        0.1,
    );
    let driver = lib.driver("nail_75").expect("a shipped driver");
    let nail = (m("steel_s235"), 0.0031, 0.075);
    let point = ahead(1.2, 0.6);
    let blows: Vec<VoiceSpec> = (0..driver.blows)
        .map(|k| {
            driven(
                lib,
                driver,
                nail,
                &log,
                (point, DVec3::NEG_Y),
                k,
                10 + u64::from(k),
            )
        })
        .collect();
    let gap = 2.0 / f64::from(driver.blows);
    scenes.push(Scene {
        name: "nail-driven",
        what: "a 75 mm nail driven into a log in five blows: each blow a hammer's ping and the log's knock, the nail ringing higher as it goes in, the last blow duller as the head seats",
        seconds: 2.8,
        commands: blows.iter().enumerate().map(|(k, b)| play(0.2 + gap * k as f64, *b)).collect(),
        measured: blows.first().copied(),
        expected: None,
    });

    // A spruce plank 3 m long loaded from 30 % to 100 % of its strength over five seconds: it
    // creaks from half its strength on, more and more, then breaks near its middle.
    let plank = Body::boxed(
        m("spruce_c24"),
        DVec3::new(1.5, 0.025, 0.1),
        (ahead(2.5, 1.0), DQuat::IDENTITY),
        None,
    );
    let mut commands = Recording::new();
    let key = 0xC4EA;
    let mut creaking = false;
    for k in 0..50 {
        let t = 0.2 + 0.1 * f64::from(k);
        let u = 0.3 + 0.7 * f64::from(k) / 49.0;
        if let Some(spec) = creak(&plank, u, key, 20) {
            if creaking {
                commands.push((
                    t,
                    Command::Change {
                        key,
                        excite: spec.excite,
                        at: spec.at,
                    },
                ));
            } else {
                commands.push(play(t, spec));
                creaking = true;
            }
        }
    }
    commands.push((5.2, Command::Stop(key)));
    let snap = crack(&plank, 0.47, 21);
    commands.push(play(5.2, snap));
    // Its pieces land on soil a third of a second later, falling a metre.
    let soil = lib.ground("soil");
    for (k, side) in [-1.0, 1.0].into_iter().enumerate() {
        let piece = Body::boxed(
            m("spruce_c24"),
            DVec3::new(0.75, 0.025, 0.1),
            (ahead(2.5, 0.0) + DVec3::X * 0.75 * side, DQuat::IDENTITY),
            None,
        );
        let landing = impact(
            &piece,
            &Other::Ground(soil),
            ahead(2.5, 0.0) + DVec3::X * 1.3 * side,
            DVec3::new(0.0, 1.2 * piece.mass * 4.4, 0.0),
            22 + k as u64,
        );
        commands.push(play(5.65 + 0.08 * k as f64, landing));
    }
    scenes.push(Scene {
        name: "plank-creaks-and-breaks",
        what: "a 3 m plank loaded to breaking over five seconds: quiet, then creaks from half its strength, quickening and louder, then a sharp crack and its two halves thudding onto the ground",
        seconds: 7.0,
        commands,
        measured: Some(snap),
        expected: None,
    });

    // A green log, 4 m and 0.25 m across, snapping: green wood tears its fibres for longer.
    let big = Body::member(
        m("spruce_green"),
        (
            ahead(4.0, 1.0) - DVec3::X * 2.0,
            ahead(4.0, 1.0) + DVec3::X * 2.0,
        ),
        0.125,
    );
    let tear = crack(&big, 0.5, 30);
    scenes.push(Scene {
        name: "log-breaks",
        what: "a green log 4 m long, 0.25 m across, snapping: a longer, tearing crack and a deep ring",
        seconds: 2.0,
        commands: vec![play(0.2, tear)],
        measured: Some(tear),
        expected: None,
    });
    scenes
}

/// Footsteps on each ground, a log rolling and a crate sliding, and the wind.
fn walking(
    lib: &Library,
    (soil, gravel, rock): (
        &crate::library::Ground,
        &crate::library::Ground,
        &crate::library::Ground,
    ),
) -> Vec<Scene> {
    let m = |id: &str| lib.matter(id).expect("a shipped material");
    let mut scenes = Vec::new();
    let log = Body::member(
        m("spruce_green"),
        (
            ahead(0.0, -0.1) - DVec3::Z * 2.0,
            ahead(0.0, -0.1) + DVec3::Z * 2.0,
        ),
        0.15,
    );
    let grounds: [(&'static str, &'static str, Underfoot<'_>); 4] = [
        (
            "steps-soil",
            "six steps on soil: soft, low thuds with a faint crumble",
            Underfoot::Ground(soil),
        ),
        (
            "steps-gravel",
            "six steps on gravel: a crunch at each heel and toe",
            Underfoot::Ground(gravel),
        ),
        (
            "steps-rock",
            "six steps on bare rock: firmer taps, a little grit",
            Underfoot::Ground(rock),
        ),
        (
            "steps-log-bridge",
            "six steps along a green log (0.3 m across): each a hollow wooden knock",
            Underfoot::Body(log),
        ),
    ];
    for (k, (name, what, under)) in grounds.into_iter().enumerate() {
        let mut commands = Recording::new();
        let mut first = None;
        for step in 0u32..6 {
            let side = if step % 2 == 0 { -0.12 } else { 0.12 };
            let at = ahead(0.3 + 0.7 * f64::from(step), 0.0) + DVec3::X * side;
            for spec in footstep(
                lib,
                &under,
                (at, DVec3::NEG_Y),
                1.0,
                40 + k as u64 * 10 + u64::from(step),
            )
            .into_iter()
            .flatten()
            {
                // The first step's louder sound is the one measured.
                if step == 0 && first.is_none_or(|f: VoiceSpec| f.peak < spec.peak) {
                    first = Some(spec);
                }
                commands.push(play(0.2 + 0.55 * f64::from(step), spec));
            }
        }
        scenes.push(Scene {
            name,
            what,
            seconds: 3.8,
            commands,
            measured: first,
            expected: None,
        });
    }
    // A log rolling and a crate sliding across the ground at 1.5 m/s for two seconds.
    let crate_box = Body::boxed(
        m("spruce_c24"),
        DVec3::splat(0.25),
        (ahead(2.0, 0.25), DQuat::IDENTITY),
        Some(20.0),
    );
    for (name, what, body, rolling) in [
        (
            "log-rolling",
            "a log rolling along the ground: a low rumble",
            log,
            true,
        ),
        (
            "crate-sliding",
            "a wooden crate pushed along the ground: a rough scrape",
            crate_box,
            false,
        ),
    ] {
        let spec = rub(&body, ahead(2.0, 0.0), (1.5, rolling), 0x5C, 50);
        scenes.push(Scene {
            name,
            what,
            seconds: 2.6,
            commands: vec![play(0.2, spec), (2.2, Command::Stop(0x5C))],
            measured: None,
            expected: None,
        });
    }
    let (_, air) = wind(lib, 1.6, 0x71D);
    scenes.push(Scene {
        name: "wind",
        what: "the wind at head height in open country (3.4 m/s): a soft, gusting rush",
        seconds: 8.0,
        commands: vec![play(0.0, air)],
        measured: None,
        expected: None,
    });
    scenes
}
