//! The mixer (audio thread; ADR-0037, decision 6): the sounds playing, each placed for the
//! listener, summed on their bus (effects, ambience) under the volumes, then limited. At most
//! `voices` play: a newcomer too quiet to hear where the listener is is never begun, and when all
//! are taken the quietest gives way to a louder newcomer (Raghuvanshi and Lin 2006's audibility
//! culling). It never allocates once made.

use crate::filters::OnePole;
use crate::limiter::Limiter;
use crate::space::{Listener, place, travel};
use crate::voice::{Bus, Excite, Voice, VoiceSpec};
use creeer_math::glam::DVec3;

/// The volumes, each 0 (silent) to 1 (as the physics has it), under the master.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volumes {
    pub master: f32,
    pub effects: f32,
    pub ambience: f32,
}

impl Default for Volumes {
    fn default() -> Self {
        Self {
            master: 1.0,
            effects: 1.0,
            ambience: 1.0,
        }
    }
}

/// What the game asks of the mixer. (A sound is sent whole, not boxed: a box would be freed on
/// the audio thread, which must not touch the heap.)
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Play a sound (or, with a key already playing, change it).
    Play(VoiceSpec),
    /// A continuous sound's drive or place changed.
    Change { key: u64, excite: Excite, at: DVec3 },
    /// Stop a continuous sound (it fades).
    Stop(u64),
    /// The listener moved.
    Listener(Listener),
    /// The volumes changed.
    Volumes(Volumes),
}

/// How the mix is made (`data/sound/hearing.ron`): the most sounds at once; the mix's window
/// (Frostbite's "HDR audio"): its top rides on the loudest sound begun, never below `floor` (Pa at
/// the ears), heard `headroom` (a factor) below full scale, falling back over `release` seconds;
/// sounds quieter than the top by more than `window` (a factor) are not begun, nor any below
/// `threshold` (Pa at the ears).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub voices: usize,
    pub floor: f32,
    pub headroom: f32,
    pub window: f32,
    pub release: f32,
    pub threshold: f32,
}

/// What the mixer did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub playing: usize,
    pub started: u64,
    pub culled: u64,
    pub stolen: u64,
    /// The loudest sample out (of full scale) and the lowest gain the limiter applied.
    pub peak: f32,
    pub least_gain: f32,
    /// The window's top now (Pa at the ears).
    pub top: f32,
}

#[derive(Clone, Debug)]
struct Playing {
    voice: Voice,
    air: OnePole,
    gains: [f32; 2],
    /// Heard from all round (ambience): neither placed nor delayed.
    diffuse: bool,
}

/// The mixer.
#[derive(Debug)]
pub struct Mixer {
    rate: f32,
    settings: Settings,
    playing: Vec<Playing>,
    listener: Listener,
    volumes: Volumes,
    heard: [f32; 3],
    /// The window's top (Pa at the ears) and the scale to full applied now (eased to it).
    top: f32,
    scale: f32,
    limiter: Limiter,
    pub stats: Stats,
}

/// Samples between placements (and the volumes' and gains' steps).
const BLOCK: usize = 64;

impl Mixer {
    /// A mixer at `rate` samples a second.
    pub fn new(rate: f32, settings: Settings) -> Self {
        Self {
            rate,
            settings,
            playing: Vec::with_capacity(settings.voices),
            listener: Listener::default(),
            volumes: Volumes::default(),
            heard: [1.0; 3],
            top: settings.floor,
            scale: 1.0 / (settings.floor * settings.headroom),
            limiter: Limiter::new(rate),
            stats: Stats {
                least_gain: 1.0,
                ..Stats::default()
            },
        }
    }

    /// Its rate (samples a second).
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Does what `command` asks.
    pub fn command(&mut self, command: Command) {
        match command {
            Command::Play(spec) => self.play(spec),
            Command::Change { key, excite, at } => {
                let rate = self.rate;
                if let Some(p) = self.keyed(key) {
                    p.voice.change(excite, at, rate);
                }
            }
            Command::Stop(key) => {
                if let Some(p) = self.keyed(key) {
                    p.voice.stopping = true;
                }
            }
            Command::Listener(l) => self.listener = l,
            Command::Volumes(v) => self.volumes = v,
        }
    }

    fn keyed(&mut self, key: u64) -> Option<&mut Playing> {
        (key != 0)
            .then(|| {
                self.playing
                    .iter_mut()
                    .find(|p| p.voice.spec.key == key && !p.voice.stopping)
            })
            .flatten()
    }

    /// How loud a sound of `peak` Pa at a metre is at the ears from `distance` m.
    fn heard(peak: f32, distance: f64) -> f32 {
        #[allow(clippy::cast_possible_truncation)] // a gain
        let near = 1.0 / distance.max(0.5) as f32;
        peak * near
    }

    fn play(&mut self, spec: VoiceSpec) {
        if spec.key != 0 && self.keyed(spec.key).is_some() {
            self.command(Command::Change {
                key: spec.key,
                excite: spec.excite,
                at: spec.at,
            });
            return;
        }
        let diffuse = matches!(spec.excite, Excite::Wind { .. });
        let placed = place(&self.listener, spec.at);
        let loudness = if diffuse {
            spec.peak
        } else {
            Self::heard(spec.peak, placed.distance)
        };
        if loudness < self.settings.threshold || loudness * self.settings.window < self.top {
            self.stats.culled += 1;
            return;
        }
        self.top = self.top.max(loudness);
        let wait = if diffuse {
            0
        } else {
            travel(placed.distance, self.rate)
        };
        let mut air = OnePole::default();
        air.set(OnePole::coefficient(placed.air, self.rate));
        let entry = Playing {
            voice: Voice::new(spec, self.rate, wait),
            air,
            gains: if diffuse { [1.0; 2] } else { placed.gains },
            diffuse,
        };
        if self.playing.len() < self.settings.voices {
            self.playing.push(entry);
        } else {
            let listener = self.listener;
            let quietest = (0..self.playing.len()).min_by(|&a, &b| {
                let level = |p: &Playing| {
                    let d = place(&listener, p.voice.spec.at).distance;
                    Self::heard(p.voice.loudness(), if p.diffuse { 1.0 } else { d })
                };
                level(&self.playing[a]).total_cmp(&level(&self.playing[b]))
            });
            let Some(q) = quietest else { return };
            let d = place(&listener, self.playing[q].voice.spec.at).distance;
            if Self::heard(self.playing[q].voice.loudness(), d) >= loudness {
                self.stats.culled += 1;
                return;
            }
            self.playing[q] = entry;
            self.stats.stolen += 1;
        }
        self.stats.started += 1;
    }

    /// Fills `out` with the next stereo samples (each within ±1).
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        let floor = self.settings.floor;
        for block in out.chunks_mut(BLOCK) {
            let n = block.len();
            // The window's top falls back towards the floor.
            #[allow(clippy::cast_precision_loss)] // 64
            let fall = (-(n as f32) / (self.rate * self.settings.release)).exp();
            self.top = floor + (self.top - floor) * fall;
            let full = 1.0 / (self.top * self.settings.headroom);
            let mut buses = [[[0.0f32; 2]; BLOCK]; 2];
            for p in &mut self.playing {
                let target = if p.diffuse {
                    [1.0, 1.0]
                } else {
                    let placed = place(&self.listener, p.voice.spec.at);
                    p.air.set(OnePole::coefficient(placed.air, self.rate));
                    placed.gains
                };
                let bus = &mut buses[usize::from(p.voice.spec.bus == Bus::Ambience)];
                #[allow(clippy::cast_precision_loss)] // 64
                let step = [0, 1].map(|e| (target[e] - p.gains[e]) / n as f32);
                for frame in bus.iter_mut().take(n) {
                    let s = p.air.run(p.voice.sample());
                    p.gains[0] += step[0];
                    p.gains[1] += step[1];
                    frame[0] += s * p.gains[0];
                    frame[1] += s * p.gains[1];
                }
            }
            self.playing.retain(|p| !p.voice.done());
            let want = [
                self.volumes.master,
                self.volumes.effects,
                self.volumes.ambience,
            ];
            for (k, frame) in block.iter_mut().enumerate() {
                for (h, w) in self.heard.iter_mut().zip(want) {
                    *h += 0.002 * (w.clamp(0.0, 1.0) - *h);
                }
                // Down at once when a louder sound comes (the limiter looks ahead), up gently.
                self.scale = if full < self.scale {
                    full
                } else {
                    self.scale + 0.0005 * (full - self.scale)
                };
                let [master, effects, ambience] = self.heard;
                let scale = self.scale;
                let mixed = [0, 1].map(|e| {
                    (buses[0][k][e] * effects + buses[1][k][e] * ambience) * master * scale
                });
                *frame = self.limiter.run(mixed);
                self.stats.peak = self.stats.peak.max(frame[0].abs()).max(frame[1].abs());
            }
        }
        self.stats.playing = self.playing.len();
        self.stats.least_gain = self.limiter.least;
        self.stats.top = self.top;
    }
}

#[cfg(test)]
#[path = "mixer_tests.rs"]
mod tests;
