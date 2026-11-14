//! Sound rendered without a sound device (ADR-0037, decision 7), for listening at the check-in
//! and for proving the game's hooks without ears:
//!
//! - `creeer_client --sound <dir>`: the reference sounds (`creeer_audio::scenes`), each a WAV in
//!   `<dir>`, and `<dir>/sounds.md`, a table of what each should sound like beside its measured
//!   peaks, decay and level.
//! - `creeer_client --sound-play <dir> [--seconds <s>]`: the *log bridge* play scene (two
//!   builders nail a plank across two sills, crates fall on it until it breaks) as its first
//!   builder hears it, through the game's own hooks (`sounds.rs`): `<dir>/log-bridge-played.wav`,
//!   with what the gathering cost a frame.

use crate::sounds::{Heard, Sounds};
use creeer_audio::Library;
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Kind};
use creeer_net::message::Command;
use std::path::PathBuf;

/// Runs `--sound` or `--sound-play`.
///
/// # Errors
/// If a file cannot be written or the scene cannot be played.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let dir_after = |flag: &str| {
        let i = args.iter().position(|a| a == flag)?;
        args.get(i + 1).map(PathBuf::from)
    };
    let lib = Library::from_data(&crate::content::data())?;
    if let Some(dir) = dir_after("--sound") {
        let scenes = creeer_audio::scenes::all(&lib);
        let table = creeer_audio::report::write_all(&lib, &scenes, &dir)?;
        let text = format!(
            "# The reference sounds (ADR-0037)\n\nRendered by `creeer_client --sound <dir>`: \
             each scene through the game's mixer at 48 kHz, the listener's ears 1.6 m above the \
             ground. The measured columns are of the scene's main sound alone, a metre away \
             (its four strongest spectral peaks, its T60 by Schroeder's integration, its peak \
             level); the file's peak is after the mix's window and limiter.\n\n{table}"
        );
        std::fs::write(dir.join("sounds.md"), &text)?;
        println!("{text}");
        return Ok(());
    }
    let dir = dir_after("--sound-play").ok_or("--sound-play needs a directory")?;
    let seconds: f64 = (args.iter().position(|a| a == "--seconds"))
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(30.0);
    played(&lib, &dir, seconds)
}

/// The log bridge as its first builder hears it.
fn played(
    lib: &Library,
    dir: &std::path::Path,
    seconds: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    use creeer_server::play::{Play, scenes};
    let scene = scenes::scene("log_bridge").ok_or("no log_bridge scene")?;
    let terrain = scenes::terrain();
    let world = (scene.build)(std::sync::Arc::clone(&terrain));
    let mut play = Play::new(world, scene.name, scene.players, &terrain);
    let mut sounds = Sounds::recording();
    let mut animator = crate::animate::Animator::default();
    let ground = crate::people_ground::PlanetGround { terrain: &terrain };
    let dt = 1.0 / f64::from(crate::TICK_RATE_HZ);
    let mut aimed: Option<(DVec3, Option<EntityId>)> = None;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few thousand ticks
    let ticks = (seconds / dt) as u32;
    for tick in 0..ticks {
        let acts = (scene.script)(tick, &play);
        // What the first builder fastens is what its crosshair was on.
        for command in acts
            .first()
            .map(|a| a.commands.as_slice())
            .unwrap_or_default()
        {
            if let Command::Fasten { target, .. } = command {
                aimed = Some((target.at, target.body));
            }
        }
        play.tick(&acts);
        let c = play.client(0);
        // The people walked as the window's animation walks them (their feet for footsteps).
        let poses = c.poses();
        for (id, kind) in &c.kinds {
            if let (Kind::Character { fallen: false, .. }, Some(pose)) = (kind, poses.get(id)) {
                let body = crate::animate::Body::Standing(pose.position, pose.orientation);
                animator.bones(id.0, &body, dt, &ground);
            }
        }
        let Some(me) = c.avatar() else { continue };
        let up = me.position.normalize_or(DVec3::Y);
        let feet = |k: u32| animator.feet(k);
        let aim = || aimed;
        let heard = Heard {
            connection: c,
            terrain: &terrain,
            ears: (me.position + up * 0.7, me.orientation * DVec3::X),
            feet: &feet,
            aimed: &aim,
            fastener: Some("nail_75"),
            wielding: None,
        };
        sounds.gather(&heard, dt);
    }
    let recording = sounds
        .output()
        .map(creeer_audio::Audio::take_recording)
        .unwrap_or_default();
    #[allow(clippy::cast_precision_loss)] // the rate
    let (frames, stats) = creeer_audio::offline::render(
        lib.settings(),
        creeer_audio::report::RATE as f32,
        seconds,
        &recording,
    );
    std::fs::create_dir_all(dir)?;
    let path = dir.join("log-bridge-played.wav");
    creeer_audio::wav::write(&path, &frames, creeer_audio::report::RATE)?;
    let cost = sounds.cost;
    #[allow(clippy::cast_precision_loss)] // frames
    let mean = cost.total.as_secs_f64() / cost.frames.max(1) as f64;
    println!(
        "{}: {:.0} s, {} sounds asked for ({} begun, {} not, {} given way), the mix's peak {:.1} dBFS; \
         gathering a frame: mean {:.1} µs, worst {:.1} µs over {} frames",
        path.display(),
        seconds,
        cost.sounds,
        stats.started,
        stats.culled,
        stats.stolen,
        20.0 * creeer_math::det::log10(f64::from(stats.peak.max(1e-9))),
        mean * 1e6,
        cost.worst.as_secs_f64() * 1e6,
        cost.frames,
    );
    Ok(())
}
