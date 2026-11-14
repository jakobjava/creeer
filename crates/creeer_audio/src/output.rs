//! Where the sound goes (ADR-0037, decisions 7 and 8): the sound device through kira (our whole
//! mixer one kira `Sound`, fed through a lock-free queue), nowhere (the null output: headless runs
//! and machines without a device, the events still counted), or a recording of what was asked,
//! with when, to be rendered offline (`offline.rs`). The game holds an [`Audio`] and asks it.

use crate::library::Library;
use crate::mixer::{Command, Mixer, Settings, Volumes};
use crate::space::Listener;
use crate::voice::{Excite, VoiceSpec};
use creeer_math::glam::DVec3;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Commands the queue to the audio thread holds (a few frames' worth of a busy scene).
const QUEUE: usize = 512;
/// The rate the mixer is made at before the device says its own (samples a second).
const RATE: f32 = 48_000.0;

/// What the audio thread tells the game: sounds playing, begun, not begun, given way; the loudest
/// sample out (f32 bits); and the time it spent mixing (ns) for how many samples.
#[derive(Debug, Default)]
pub struct Shared {
    pub playing: AtomicU32,
    pub started: AtomicU64,
    pub culled: AtomicU64,
    pub stolen: AtomicU64,
    pub peak: AtomicU32,
    pub mixing: AtomicU64,
    pub mixed: AtomicU64,
}

/// What a recording holds: each command with when it was asked (s).
pub type Recording = Vec<(f64, Command)>;

enum Sink {
    Null,
    Device {
        _manager: Box<kira::AudioManager<kira::DefaultBackend>>,
        queue: rtrb::Producer<Command>,
    },
    Record {
        clock: f64,
        commands: Recording,
    },
}

/// The game's side of the sound.
pub struct Audio {
    sink: Sink,
    volumes: Volumes,
    pub shared: Arc<Shared>,
    /// Commands sent, and lost to a full queue.
    pub sent: u64,
    pub lost: u64,
    /// Why there is no sound, if there is none.
    pub silent: Option<String>,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sink = match self.sink {
            Sink::Null => "null",
            Sink::Device { .. } => "device",
            Sink::Record { .. } => "recording",
        };
        f.debug_struct("Audio")
            .field("sink", &sink)
            .field("sent", &self.sent)
            .finish_non_exhaustive()
    }
}

/// Our mixer as kira plays it.
struct Engine {
    mixer: Mixer,
    settings: Settings,
    queue: rtrb::Consumer<Command>,
    scratch: Box<[[f32; 2]; 256]>,
    shared: Arc<Shared>,
}

impl kira::sound::Sound for Engine {
    fn process(&mut self, out: &mut [kira::Frame], dt: f64, _info: &kira::info::Info<'_>) {
        let start = std::time::Instant::now();
        #[allow(clippy::cast_possible_truncation)] // a sample rate
        let rate = (1.0 / dt) as f32;
        if (rate - self.mixer.rate()).abs() > 1.0 {
            // The device's own rate, once: the only allocation the audio thread makes.
            self.mixer = Mixer::new(rate, self.settings);
        }
        while let Ok(command) = self.queue.pop() {
            self.mixer.command(command);
        }
        for chunk in out.chunks_mut(self.scratch.len()) {
            let scratch = &mut self.scratch[..chunk.len()];
            self.mixer.render(scratch);
            for (frame, s) in chunk.iter_mut().zip(scratch.iter()) {
                *frame = kira::Frame {
                    left: s[0],
                    right: s[1],
                };
            }
        }
        let s = &self.mixer.stats;
        let shared = &self.shared;
        #[allow(clippy::cast_possible_truncation)] // a count of voices
        shared.playing.store(s.playing as u32, Ordering::Relaxed);
        shared.started.store(s.started, Ordering::Relaxed);
        shared.culled.store(s.culled, Ordering::Relaxed);
        shared.stolen.store(s.stolen, Ordering::Relaxed);
        shared.peak.store(s.peak.to_bits(), Ordering::Relaxed);
        #[allow(clippy::cast_possible_truncation)] // under 585 years
        shared
            .mixing
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        shared.mixed.fetch_add(out.len() as u64, Ordering::Relaxed);
    }

    fn finished(&self) -> bool {
        false
    }
}

struct Playing(Engine);

impl kira::sound::SoundData for Playing {
    type Error = ();
    type Handle = ();

    fn into_sound(self) -> Result<(Box<dyn kira::sound::Sound>, Self::Handle), Self::Error> {
        Ok((Box::new(self.0), ()))
    }
}

impl Audio {
    fn with(sink: Sink, shared: Arc<Shared>, silent: Option<String>) -> Self {
        Self {
            sink,
            volumes: Volumes::default(),
            shared,
            sent: 0,
            lost: 0,
            silent,
        }
    }

    /// Sound on the default device, or the null output (saying why) if there is none.
    pub fn open(lib: &Library) -> Self {
        let shared = Arc::new(Shared::default());
        let settings = lib.settings();
        let (producer, consumer) = rtrb::RingBuffer::new(QUEUE);
        let engine = Engine {
            mixer: Mixer::new(RATE, settings),
            settings,
            queue: consumer,
            scratch: Box::new([[0.0; 2]; 256]),
            shared: Arc::clone(&shared),
        };
        let manager =
            kira::AudioManager::<kira::DefaultBackend>::new(kira::AudioManagerSettings::default());
        match manager {
            Ok(mut manager) => match manager.play(Playing(engine)) {
                Ok(()) => Self::with(
                    Sink::Device {
                        _manager: Box::new(manager),
                        queue: producer,
                    },
                    shared,
                    None,
                ),
                Err(e) => Self::with(Sink::Null, shared, Some(format!("could not play: {e}"))),
            },
            Err(e) => Self::with(Sink::Null, shared, Some(format!("no sound device: {e}"))),
        }
    }

    /// No sound: what is asked is counted and dropped.
    pub fn null() -> Self {
        Self::with(
            Sink::Null,
            Arc::new(Shared::default()),
            Some("null output".into()),
        )
    }

    /// A recording of what is asked, with when (`advance` moves its clock), to render offline.
    pub fn recording() -> Self {
        let sink = Sink::Record {
            clock: 0.0,
            commands: Vec::new(),
        };
        Self::with(sink, Arc::new(Shared::default()), Some("recording".into()))
    }

    /// Whether it plays to a device.
    pub fn is_playing(&self) -> bool {
        matches!(self.sink, Sink::Device { .. })
    }

    /// A recording's clock moves on `dt` seconds.
    pub fn advance(&mut self, dt: f64) {
        if let Sink::Record { clock, .. } = &mut self.sink {
            *clock += dt;
        }
    }

    /// What was recorded so far (taken).
    pub fn take_recording(&mut self) -> Recording {
        match &mut self.sink {
            Sink::Record { commands, .. } => std::mem::take(commands),
            _ => Vec::new(),
        }
    }

    fn send(&mut self, command: Command) {
        self.sent += 1;
        match &mut self.sink {
            Sink::Null => {}
            Sink::Device { queue, .. } => {
                if queue.push(command).is_err() {
                    self.lost += 1;
                }
            }
            Sink::Record { clock, commands } => commands.push((*clock, command)),
        }
    }

    /// Plays `spec` (with a key already playing: changes it).
    pub fn play(&mut self, spec: VoiceSpec) {
        self.send(Command::Play(spec));
    }

    /// A continuous sound's drive or place changed.
    pub fn change(&mut self, key: u64, excite: Excite, at: DVec3) {
        self.send(Command::Change { key, excite, at });
    }

    /// Stops a continuous sound.
    pub fn stop(&mut self, key: u64) {
        self.send(Command::Stop(key));
    }

    /// The ears are at `at`, their right `right`.
    pub fn listen(&mut self, at: DVec3, right: DVec3) {
        self.send(Command::Listener(Listener { at, right }));
    }

    /// The volumes (each 0 to 1): for the settings menu.
    pub fn set_volumes(&mut self, volumes: Volumes) {
        self.volumes = volumes;
        self.send(Command::Volumes(volumes));
    }

    /// The volumes as set.
    pub fn volumes(&self) -> Volumes {
        self.volumes
    }

    /// The audio thread's share of real time spent mixing so far (0 to 1), at `rate` samples a
    /// second (None before it mixed).
    pub fn load(&self) -> Option<f64> {
        let samples = self.shared.mixed.load(Ordering::Relaxed);
        let nanos = self.shared.mixing.load(Ordering::Relaxed);
        #[allow(clippy::cast_precision_loss)] // counts far below 2^52
        (samples > 0).then(|| nanos as f64 * 1e-9 / (samples as f64 / f64::from(RATE)))
    }
}
