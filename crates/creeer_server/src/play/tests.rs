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
/// bodies), a tenth above what it measured (2026-10-18: crowd 446, its players characters),
/// brought down as the
/// solver needs fewer.
const WORK: [(&str, u64); 1] = [("crowd", 490)];

#[test]
#[ignore = "measures the work ceilings"]
fn measure_the_work_ceilings() {
    for s in &SCENES {
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
        let report = run(s, s.ticks.1);
        let (tick, most) = report.most_work();
        let ceiling = WORK.iter().find(|w| w.0 == s.name).map(|w| w.1);
        assert!(
            ceiling.is_some_and(|c| most <= c),
            "{}: tick {tick} took {most} Newton iterations (ceiling {ceiling:?})",
            s.name
        );
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
