//! The game's sounds (G2.11, ADR-0037): what the client sees happen, heard. Each frame it takes
//! in what the connection already has (no message of the protocol is added) and asks
//! `creeer_audio` for each sound: the snapshots' velocities (things landing and struck, rolling
//! and sliding: `sounds_bodies.rs`), the members newly broken, the player's own blows and the
//! fastenings it is told of, each person's feet coming down, and the wind where the ears are.
//! The synthesis runs on the audio thread; this costs the frame a few microseconds.
//!
//! The settings menu sets the volumes with [`Sounds::set_volumes`] and reads them with
//! [`Sounds::volumes`] (`PlanetView::sound`).

use crate::sounds_things::Extra;
use creeer_audio::happenings::{Underfoot, footstep, wind};
use creeer_audio::{Audio, Library, Volumes};
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Kind};
use creeer_server::Connection;
use creeer_terrain::Terrain;
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// What the gathering is told each frame besides the connection.
pub struct Heard<'a> {
    pub connection: &'a Connection,
    pub terrain: &'a Terrain,
    /// The ears: where, and their right.
    pub ears: (DVec3, DVec3),
    /// Each person's feet as drawn (where each stands, whether down), by entity.
    pub feet: &'a dyn Fn(u32) -> Option<[(DVec3, bool); 2]>,
    /// What the crosshair is on (a point, and the body there), asked only when a blow falls.
    pub aimed: &'a dyn Fn() -> Option<(DVec3, Option<EntityId>)>,
    /// The fastener chosen, and the tool in hand (its index in the tools' table).
    pub fastener: Option<&'a str>,
    pub wielding: Option<u8>,
}

/// What gathering cost the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cost {
    pub frames: u64,
    pub total: Duration,
    pub worst: Duration,
    /// Sounds asked for, and of each kind: wind, bodies (struck, rolling), breaks, fastenings,
    /// blows, footsteps (each a command to the mixer: a sound begun, changed or stopped).
    pub sounds: u64,
    pub kinds: [u64; 6],
    /// Loading the sound data and opening the device (the first frame's, once).
    pub opening: Duration,
}

/// A driving blow to come: when (s, the clock's), and blow `k` of what into what.
#[derive(Clone, Debug)]
pub(crate) struct Blow {
    pub(crate) at: f64,
    pub(crate) k: u32,
    pub(crate) fastener: String,
    pub(crate) point: DVec3,
    pub(crate) body: Option<EntityId>,
    pub(crate) axis: DVec3,
}

/// The game's sounds.
#[derive(Default)]
pub struct Sounds {
    pub(crate) out: Option<Audio>,
    pub(crate) lib: Option<Library>,
    pub(crate) extra: Extra,
    volumes: Volumes,
    record: bool,
    /// The last snapshot taken in, each body's velocity then, the bodies rubbing (their key).
    pub(crate) seen: u32,
    pub(crate) velocities: HashMap<EntityId, (DVec3, u32)>,
    pub(crate) rubbing: HashMap<EntityId, (u64, f64)>,
    /// The members known broken, by structure, and each structure's frame as last seen to change.
    broken: HashMap<EntityId, BTreeSet<u32>>,
    frames: HashMap<EntityId, creeer_net::entity::Frame>,
    pub(crate) note: Option<Instant>,
    pub(crate) blows: Vec<Blow>,
    pub(crate) swung: Option<f64>,
    down: HashMap<u32, [bool; 2]>,
    wind: Option<f64>,
    pub(crate) clock: f64,
    pub(crate) seed: u64,
    pub cost: Cost,
}

impl std::fmt::Debug for Sounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sounds")
            .field("out", &self.out)
            .field("cost", &self.cost)
            .finish_non_exhaustive()
    }
}

/// The wind's key, and the most sounds of one kind asked for in a frame (a collapse).
const WIND: u64 = 0x5749_4E44;
pub(crate) const MOST: usize = 12;

impl Sounds {
    /// Sounds recorded with when they were asked for, not played (`--sound-play`).
    pub fn recording() -> Self {
        Self {
            record: true,
            ..Self::default()
        }
    }

    /// The volumes (master, effects, ambience; each 0 to 1).
    pub fn volumes(&self) -> Volumes {
        self.volumes
    }

    /// Sets the volumes (the settings menu's volume is the master: `app/menu.rs`).
    pub fn set_volumes(&mut self, volumes: Volumes) {
        self.volumes = volumes;
        if let Some(out) = &mut self.out {
            out.set_volumes(volumes);
        }
    }

    /// One frame of the planet (`app/planet.rs`): the ears at the camera, the people's feet as
    /// `people` drew them, blows on what the crosshair is on. The sound device is opened at the
    /// first frame.
    pub fn frame(
        &mut self,
        view: &crate::planet_view::PlanetView,
        camera: &creeer_render::camera::Camera,
        people: Option<&crate::people::People>,
        dt: f64,
    ) {
        let feet = |k: u32| people.and_then(|p| p.feet(k));
        let aimed =
            || crate::planet_aim::target(view, camera).map(|t| (t.aimed.point, t.aimed.body));
        let heard = Heard {
            connection: &view.session.connection,
            terrain: &view.terrain,
            ears: (camera.position, camera.right()),
            feet: &feet,
            aimed: &aimed,
            fastener: view.fastening.name(),
            wielding: view.wielding,
        };
        self.gather(&heard, dt);
    }

    /// The output (once opened).
    pub fn output(&mut self) -> Option<&mut Audio> {
        self.out.as_mut()
    }

    pub(crate) fn next_seed(&mut self) -> u64 {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.seed
    }

    /// One frame of `dt` seconds: what happened since the last, heard.
    pub fn gather(&mut self, h: &Heard<'_>, dt: f64) {
        let start = Instant::now();
        if self.lib.is_none() {
            let data = crate::content::data();
            let lib = Library::from_data(&data).unwrap_or_else(|_| Library::base());
            self.extra = Extra::load(&data);
            let mut out = if self.record {
                Audio::recording()
            } else {
                Audio::open(&lib)
            };
            out.set_volumes(self.volumes);
            if let Some(why) = &out.silent {
                eprintln!("sound: {why}");
            }
            self.out = Some(out);
            self.lib = Some(lib);
            // (Loading the data and opening the device, once, is not a frame's gathering.)
            self.cost.opening = start.elapsed();
        }
        let start = Instant::now();
        self.clock += dt;
        let sent = self.out.as_ref().map_or(0, |o| o.sent);
        if let Some(out) = &mut self.out {
            out.advance(dt);
            out.listen(h.ears.0, h.ears.1);
        }
        let steps: [fn(&mut Self, &Heard<'_>); 6] = [
            Self::air,
            Self::bodies,
            Self::breaks,
            Self::fastenings,
            Self::swings,
            Self::footsteps,
        ];
        for (k, step) in steps.into_iter().enumerate() {
            let before = self.out.as_ref().map_or(0, |o| o.sent);
            step(self, h);
            self.cost.kinds[k] += self.out.as_ref().map_or(0, |o| o.sent) - before;
        }
        let elapsed = start.elapsed();
        self.cost.frames += 1;
        self.cost.total += elapsed;
        self.cost.worst = self.cost.worst.max(elapsed);
        self.cost.sounds += self.out.as_ref().map_or(0, |o| o.sent) - sent;
    }

    /// Plays `spec` if it is within hearing range of the ears.
    pub(crate) fn play(&mut self, spec: creeer_audio::VoiceSpec, ears: DVec3) {
        let range = self.lib.as_ref().map_or(300.0, |l| l.hearing.range);
        if spec.at.distance(ears) <= range
            && let Some(out) = &mut self.out
        {
            out.play(spec);
        }
    }

    /// The wind at the ears' height above the ground, set again when it changes.
    fn air(&mut self, h: &Heard<'_>) {
        let (Some(lib), Some(out)) = (&self.lib, &mut self.out) else {
            return;
        };
        // The height above the ground, a few times a second.
        if self.wind.is_some() && (self.clock * 4.0).fract() > 0.25 {
            return;
        }
        let height = h.terrain.density(h.ears.0).clamp(0.0, 500.0);
        let (speed, spec) = wind(lib, height, WIND);
        match self.wind {
            None => out.play(spec),
            Some(was) if (was - speed).abs() > 0.1 => out.change(WIND, spec.excite, spec.at),
            Some(_) => return,
        }
        self.wind = Some(speed);
    }

    /// Members broken since the last frame: each cracks.
    fn breaks(&mut self, h: &Heard<'_>) {
        let c = h.connection;
        let mut cracks = Vec::new();
        for (id, members) in &c.broken {
            let known = self.broken.entry(*id).or_default();
            for &m in members.difference(known) {
                cracks.push((*id, m));
            }
            known.clone_from(members);
        }
        self.broken.retain(|id, _| c.broken.contains_key(id));
        // A structure come apart is told as its members replaced by fewer (its pieces entities
        // of their own), not which broke: the largest member gone cracks.
        let mut apart = Vec::new();
        for (id, kind) in &c.kinds {
            let Kind::Structure(frame) = kind else {
                continue;
            };
            let known = self.frames.get(id).map(|f| f.members.len());
            if known != Some(frame.members.len()) {
                let old = self.frames.insert(*id, frame.clone());
                if let Some(old) = old.filter(|o| o.members.len() > frame.members.len()) {
                    apart.push((*id, old));
                }
            }
        }
        self.frames.retain(|id, _| c.kinds.contains_key(id));
        for (id, member) in cracks.into_iter().take(MOST) {
            let seed = self.next_seed();
            let Some(lib) = &self.lib else { return };
            let frame = match c.kinds.get(&id) {
                Some(Kind::Structure(f)) => f,
                _ => continue,
            };
            if let Some(body) = crate::sounds_things::member(lib, c, (id, frame), member) {
                let spec = creeer_audio::happenings::crack(&body, 0.5, seed);
                self.play(spec, h.ears.0);
            }
        }
        for (id, old) in apart.into_iter().take(MOST) {
            let seed = self.next_seed();
            let Some(lib) = &self.lib else { return };
            let Some(Kind::Structure(now)) = c.kinds.get(&id) else {
                continue;
            };
            let gone = crate::sounds_things::gone(&old, now);
            if let Some(body) =
                gone.and_then(|k| crate::sounds_things::member(lib, c, (id, &old), k))
            {
                let spec = creeer_audio::happenings::crack(&body, 0.5, seed);
                self.play(spec, h.ears.0);
            }
        }
    }

    /// Each person's feet: one that has come down steps.
    fn footsteps(&mut self, h: &Heard<'_>) {
        let c = h.connection;
        let mut steps = Vec::new();
        for (id, kind) in &c.kinds {
            let Kind::Character { fallen: false, .. } = kind else {
                continue;
            };
            let Some(feet) = (h.feet)(id.0) else { continue };
            let was = self.down.insert(id.0, feet.map(|f| f.1));
            for (k, (at, down)) in feet.iter().enumerate() {
                if *down && was.is_some_and(|w| !w[k]) {
                    steps.push((*id, *at));
                }
            }
        }
        for (id, at) in steps.into_iter().take(MOST) {
            let up = at.normalize_or(DVec3::Y);
            let speed = self
                .velocities
                .get(&id)
                .map_or(1.4, |v| v.0.reject_from(up).length());
            // A walk's heel strikes as measured; a run's harder, with the pace.
            let scale = (speed / 1.4).clamp(0.6, 2.0);
            let seed = self.next_seed();
            let Some(lib) = &self.lib else { return };
            let under = crate::sounds_things::underfoot(lib, &self.extra, h, at, up);
            let specs = match &under {
                Some(body) => footstep(lib, &Underfoot::Body(*body), (at, -up), scale, seed),
                None => {
                    let ground = crate::sounds_things::ground(lib, h.terrain, at - up * 0.05);
                    footstep(lib, &Underfoot::Ground(ground), (at, -up), scale, seed)
                }
            };
            for spec in specs.into_iter().flatten() {
                self.play(spec, h.ears.0);
            }
        }
        self.down.retain(|k, _| c.kinds.contains_key(&EntityId(*k)));
    }
}
