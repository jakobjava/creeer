//! Play scenes play the same every time, a recording replays to the same state, and every
//! scene keeps within its work and its bandwidth (the gates CI can hold: the same on every
//! machine; their times are `creeer_tools play --gate`'s).

use super::record::{Act, Recording};
use super::scenes::{SCENES, run, scene, terrain};
use super::{replay, scenes};
use creeer_math::glam::DVec3;
use creeer_net::channel::BYTES_PER_SECOND;
use creeer_net::message::Command;
use creeer_terrain::edits::{Brush, Edit, Edits};

#[test]
fn a_recording_reads_back_as_written() {
    let mut edits = Edits::default();
    edits.push(Edit::Dig(Brush::Sphere {
        center: DVec3::new(1.0, 2.0, 3.0),
        radius: 0.5,
    }));
    let recording = Recording {
        scene: "crowd".into(),
        players: vec!["ada".into(), "ben".into()],
        ticks: vec![
            vec![
                Act {
                    walk: DVec3::new(0.1, -2.0, 1.0 / 3.0),
                    jump: true,
                    thrust: DVec3::new(0.0, 0.7, 0.2),
                    commands: vec![Command::Crate(DVec3::X), Command::Edit(edits.to_bytes())],
                },
                Act::default(),
            ],
            vec![Act::walk(DVec3::Y), Act::walk(DVec3::NEG_Z * 7.0)],
        ],
    };
    let bytes = recording.to_bytes();
    assert_eq!(Recording::from_bytes(&bytes).unwrap(), recording);
    // Cut short, or of another kind: refused.
    assert!(Recording::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    assert!(Recording::from_bytes(b"CREEDITS").is_err());
}

#[test]
fn a_scene_plays_the_same_every_time() {
    let crowd = scene("crowd").unwrap();
    let (a, b) = (run(crowd, 120), run(crowd, 120));
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.recording, b.recording);
    let work = |r: &super::Report| r.ticks.iter().map(|t| t.work).collect::<Vec<_>>();
    assert_eq!(work(&a), work(&b));
}

#[test]
fn what_the_clients_recorded_replays_to_the_same_state() {
    let crowd = scene("crowd").unwrap();
    let mut play = super::Play::new(
        (crowd.build)(terrain()),
        crowd.name,
        crowd.players,
        &terrain(),
    );
    for t in 0..150 {
        let acts = (crowd.script)(t, &play);
        play.tick(&acts);
    }
    let hash = play.hash();
    // Each client's own track, put together: what the script asked, act for act.
    let recorded = Recording::from_tracks(crowd.name, play.tracks());
    assert_eq!(&recorded, play.recording());
    assert!(
        recorded
            .ticks
            .iter()
            .flatten()
            .any(|a| !a.commands.is_empty())
    );
    // Written to a file and read back, it plays the same world again.
    let read = Recording::from_bytes(&recorded.to_bytes()).unwrap();
    let again = replay((crowd.build)(terrain()), &read, &terrain());
    assert_eq!(again.hash, hash);
}

/// Each scene's most work in a tick at CI's length (Newton iterations summed over its
/// bodies), a tenth above what it measured, brought down as the solver needs fewer.
/// *crowd*: 446 with its players standing (G1.2a); since G1.2d its knocked-down players lie
/// under the crates it drops, 4,730 at tick 102, and the user accepted that measurement as
/// the gate after P1's first pass (2026-10-18, ADR-0024 "Outcome of the first pass": no lever
/// measured cuts it by the factor 490 asked). With the ragdoll moved to a real person's
/// proportions (G1.3f), 6,651 at tick 82, a crate landing on a fallen player; the old ragdoll
/// reached 6,347 at full length, and the scene's times fell (p99 15.1 → 10.6 ms, worst 35 →
/// 28 ms at full length). The user accepted it (2026-10-21). G1.8 brings it back down.
///
/// Since G1.3i the work is gated over an ensemble (as Baker et al. 2015 test a chaotic model):
/// each scene played turned about the spawn to [`HEADINGS`] headings, its median member and its
/// worst (a change of up by 0.0004 rad had moved the one sample across its ceiling). At the
/// spawn moved to level ground at a temperate latitude (2026-10-22): 127-248 (median 166), and
/// 4,994 where a crate lands on a fallen player at tick 299. The median a tenth above; the worst
/// keeps the ceiling above.
const WORK: [(&str, u64, u64); 9] = [
    ("crowd", 183, 7_300),
    ("work", 216, 216),
    ("lumberjack", 50, 50),
    ("quarry", 185, 185),
    ("toolmaking", 135, 135),
    // G2.8, measured 2026-10-23 (a tenth above): the towers' Newton works little, their
    // factorisations much; the builders' loose poles never rest (ADR-0024, G2.0's open hull
    // contacts).
    ("tower", 6, 6),
    ("collapse", 6, 6),
    ("drop", 427, 427),
    ("builders", 4_792, 4_792),
];

/// The headings each scene is played at for its ensemble (`Play::heading`).
const HEADINGS: u32 = 8;

#[test]
#[ignore = "measures the work ceilings"]
fn measure_the_work_ceilings() {
    for s in &SCENES {
        for k in 0..HEADINGS {
            let heading = f64::from(k) * std::f64::consts::TAU / f64::from(HEADINGS);
            let report = super::scenes::run_turned(s, s.ticks.1, heading, |_, _| {});
            let (tick, most) = report.most_work();
            println!(
                "{} turned {heading:.2}: {most} at tick {tick}; worst {:.2} ms",
                s.name,
                report.percentile_ms(100.0)
            );
        }
        let report = run(s, s.ticks.1);
        let (tick, most) = report.most_work();
        let (down, up) = report.kbits();
        println!(
            "{}: {most} at tick {tick}; {down:.0} kbit/s down, {up:.1} up; worst {:.2} ms",
            s.name,
            report.percentile_ms(100.0)
        );
    }
}

#[test]
fn every_scene_keeps_within_its_work_and_its_bandwidth() {
    for s in &SCENES {
        let (_, median, ceiling) = *WORK.iter().find(|w| w.0 == s.name).expect("a gate");
        let mut works = Vec::new();
        let headings = if s.turned { HEADINGS } else { 1 };
        for k in 0..headings {
            let heading = f64::from(k) * std::f64::consts::TAU / f64::from(HEADINGS);
            let report = super::scenes::run_turned(s, s.ticks.1, heading, |_, _| {});
            let (tick, most) = report.most_work();
            assert!(
                most <= ceiling,
                "{} turned {heading:.2}: tick {tick} took {most} Newton iterations",
                s.name
            );
            works.push(most);
        }
        works.sort_unstable();
        let n = works.len();
        let middle = (works[(n - 1) / 2] + works[n / 2]) / 2;
        assert!(
            middle <= median,
            "{}: its median member {middle} ({works:?})",
            s.name
        );
        let report = run(s, s.ticks.1);
        // plan.md §5: 256 kbit/s to a client, on the wire.
        let (down, up) = report.kbits();
        assert!(
            down <= BYTES_PER_SECOND * 8e-3 * 1.01,
            "{}: {down:.0} kbit/s to a client",
            s.name
        );
        assert!(up <= 64.0, "{}: {up:.0} kbit/s from a client", s.name);
    }
    assert_eq!(
        scenes::SCENES.len(),
        WORK.len(),
        "a scene without a ceiling"
    );
}
