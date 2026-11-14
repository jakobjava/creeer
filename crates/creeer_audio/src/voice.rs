//! One sound as it plays (audio thread): its modes' resonators, the click of the contact, and
//! what sets them going: one blow, a burst of grains (footsteps on loose ground, a crack), or a
//! continuous drive (rubbing, creaking, wind). Its output is the pressure it makes a metre away
//! (Pa); where it is heard from is the mixer's (`space.rs`).

use crate::filters::{Biquad, OnePole};
use crate::modes::Modes;
use crate::resonator::Bank;
use crate::rng::Rng;
use creeer_math::glam::DVec3;

/// Which volume a sound comes under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Effects,
    Ambience,
}

/// The click of a body stopped short (Koss and Alfredson 1973): Pa at a metre per unit of its
/// drive, how long its contact lasts (s, the click's rise) and the corner below which it radiates
/// less (Hz). In open air a body stopped short radiates as a dipole, falling as f² below the
/// corner; `baffled`, against the ground (a foot, a thing landing), the ground's patch moving with
/// it radiates as a piston in a baffle, falling only as f (Fahy and Gardonio 2007).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Click {
    pub gain: f32,
    pub contact: f32,
    pub corner: f32,
    pub baffled: bool,
}

impl Click {
    /// About the most its click reaches for a drive of 1 (Pa at a metre): a step through its
    /// contact's low-pass (corner 1/(πτ)) and its radiation's high-pass. A contact much longer than
    /// the corner's period passes only its slope (baffled) or its curvature (open air).
    pub fn reach(&self) -> f32 {
        let r = 1.0 / (std::f32::consts::PI * self.contact.max(1e-7) * self.corner.max(1.0));
        let share = if self.baffled {
            r / (r + 2.2)
        } else {
            r * r / (r * r + 2.0)
        };
        self.gain * share
    }
}

/// How a grain burst's rate runs through it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Envelope {
    /// Rising in a tenth of its time, then dying away (a crack).
    Burst,
    /// A footstep: the heel's burst, and the toe's (half as strong) `toe` seconds later.
    Step { toe: f32 },
}

/// What sets a sound going.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Excite {
    /// One blow: an impulse of 1 into the modes and a step of 1 through the click.
    Blow,
    /// Small blows at random (grains under a foot, micro-fractures in a crack): `count` expected
    /// over `seconds`, their energies a power law of `exponent` (together as much as one blow),
    /// their rate following `shape`.
    Grains {
        count: f32,
        seconds: f32,
        exponent: f32,
        shape: Envelope,
    },
    /// Until stopped: a rough contact sliding or rolling, a force of `force` N (its fluctuation's
    /// RMS) of noise low-passed at `corner` Hz into the modes (FoleyAutomatic's rough surface).
    Rub { force: f32, corner: f32 },
    /// Until stopped: stick–slip bursts begun `rate` times a second, each slip `level` N·s.
    Creak { rate: f32, level: f32 },
    /// Until stopped: wind of `speed` m/s at the listener, its pressure `level` Pa at a metre's
    /// reference (ambience, heard from all round).
    Wind { speed: f32, level: f32 },
}

impl Excite {
    /// Whether it plays until stopped.
    pub fn continuous(&self) -> bool {
        matches!(
            self,
            Self::Rub { .. } | Self::Creak { .. } | Self::Wind { .. }
        )
    }
}

/// A sound, ready to play: where (world, m), under which volume, its key (to change or stop a
/// continuous one; 0 none), its modes (Pa at a metre for a unit drive), its click, its drive, the
/// most it is expected to reach at a metre (Pa: for hearing it at all and for giving way to
/// louder ones), and its random seed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceSpec {
    pub at: DVec3,
    pub bus: Bus,
    pub key: u64,
    pub modes: Modes,
    pub click: Click,
    pub excite: Excite,
    pub peak: f32,
    pub seed: u64,
}

/// A grain burst's state.
#[derive(Clone, Copy, Debug, Default)]
struct Grains {
    /// Grains a second per unit of the envelope; each one's amplitude per √energy.
    rate: f32,
    per_energy: f32,
}

/// A creak's state: the burst running (time left, its length, its slip rate now and at its end,
/// how far to the next slip), and the level each slip has.
#[derive(Clone, Copy, Debug, Default)]
struct Creaking {
    left: f32,
    length: f32,
    rate: f32,
    to: f32,
    phase: f32,
}

/// A sound playing.
#[derive(Clone, Debug)]
pub struct Voice {
    pub spec: VoiceSpec,
    bank: Bank,
    contact: Biquad,
    radiate: Biquad,
    noise: Biquad,
    /// What brings the low-passed noise to an RMS of 1.
    noisy: f32,
    gust: (f32, OnePole),
    rng: Rng,
    dt: f32,
    /// Its time since it began (s), and samples still to wait (its sound's travel).
    pub t: f32,
    pub wait: u32,
    grains: Grains,
    creak: Creaking,
    /// A continuous sound's drive as heard now (eased to the asked), and whether it was stopped.
    level: f32,
    pub stopping: bool,
}

/// What brings uniform noise (RMS 1/√3) low-passed at `corner` Hz to an RMS of 1: a two-pole
/// Butterworth low-pass keeps its noise bandwidth, 1.11·corner, of the white noise's half rate.
fn unit_noise(corner: f32, rate: f32) -> f32 {
    let kept = (2.22 * corner.min(0.45 * rate) / rate).min(1.0);
    3f32.sqrt() / kept.sqrt()
}

/// The mean of a power law of `exponent` capped at 1000 (`Rng::power_law`).
fn mean_energy(exponent: f32) -> f32 {
    let (t, m) = (exponent, 1000.0f32);
    (t - 1.0) * (m.powf(2.0 - t) - 1.0) / (2.0 - t) + m * m.powf(1.0 - t)
}

impl Voice {
    /// `spec` begun at `rate` samples a second, after `wait` samples.
    pub fn new(spec: VoiceSpec, rate: f32, wait: u32) -> Self {
        let dt = 1.0 / rate;
        let contact = Biquad::lowpass(
            1.0 / (std::f32::consts::PI * spec.click.contact.max(1e-6)),
            0.707,
            rate,
        );
        let mut voice = Self {
            spec,
            bank: Bank::new(&spec.modes, rate),
            contact,
            radiate: if spec.click.baffled {
                Biquad::highpass1(spec.click.corner, rate)
            } else {
                Biquad::highpass(spec.click.corner, 0.707, rate)
            },
            noise: Biquad::lowpass(1000.0, 0.707, rate),
            noisy: 1.0,
            gust: (0.0, OnePole::default()),
            rng: Rng::new(spec.seed),
            dt,
            t: 0.0,
            wait,
            grains: Grains::default(),
            creak: Creaking::default(),
            level: 0.0,
            stopping: false,
        };
        match spec.excite {
            Excite::Grains {
                count,
                seconds,
                exponent,
                shape,
            } => {
                let area = match shape {
                    Envelope::Burst => 0.335 * seconds,
                    Envelope::Step { .. } => 1.5 * (0.0025 + 0.025),
                };
                voice.grains = Grains {
                    rate: count / area,
                    per_energy: 1.0 / (count.max(1.0) * mean_energy(exponent)).sqrt(),
                };
            }
            Excite::Rub { corner, .. } => {
                voice.noise = Biquad::lowpass(corner, 0.707, rate);
                voice.noisy = unit_noise(corner, rate);
            }
            Excite::Wind { speed, .. } => {
                voice.noise = Biquad::lowpass(200.0 + 40.0 * speed, 0.707, rate);
                voice.noisy = unit_noise(200.0 + 40.0 * speed, rate);
                voice.radiate = Biquad::highpass(40.0, 0.707, rate);
                voice.gust.1.set(OnePole::coefficient(0.08, rate));
            }
            Excite::Blow | Excite::Creak { .. } => {}
        }
        voice
    }

    /// A continuous sound's drive changed (its kind kept), or its place.
    pub fn change(&mut self, excite: Excite, at: DVec3, rate: f32) {
        if std::mem::discriminant(&excite) == std::mem::discriminant(&self.spec.excite) {
            if let Excite::Rub { corner, .. } = excite {
                self.noise.retune(&Biquad::lowpass(corner, 0.707, rate));
                self.noisy = unit_noise(corner, rate);
            }
            if let Excite::Wind { speed, .. } = excite {
                self.noise
                    .retune(&Biquad::lowpass(200.0 + 40.0 * speed, 0.707, rate));
            }
            self.spec.excite = excite;
            self.spec.at = at;
        }
    }

    /// The rate of grains now (a second).
    fn grain_rate(&self, shape: Envelope, seconds: f32) -> f32 {
        let burst = |t: f32, attack: f32, decay: f32| {
            if t < 0.0 {
                0.0
            } else if t < attack {
                t / attack
            } else {
                (-(t - attack) / decay).exp()
            }
        };
        let envelope = match shape {
            Envelope::Burst if self.t < seconds => burst(self.t, 0.1 * seconds, 0.3 * seconds),
            Envelope::Burst => 0.0,
            Envelope::Step { toe } => {
                burst(self.t, 0.005, 0.025) + 0.5 * burst(self.t - toe, 0.005, 0.025)
            }
        };
        self.grains.rate * envelope
    }

    /// The drive this sample: (into the modes, into the click).
    fn drive(&mut self) -> (f32, f32) {
        match self.spec.excite {
            Excite::Blow => (if self.t < 0.5 * self.dt { 1.0 } else { 0.0 }, 1.0),
            Excite::Grains {
                seconds,
                exponent,
                shape,
                ..
            } => {
                if self.rng.unit() >= self.grain_rate(shape, seconds) * self.dt {
                    return (0.0, 0.0);
                }
                let a = self.rng.power_law(exponent, 1000.0).sqrt() * self.grains.per_energy;
                let a = if self.rng.unit() < 0.5 { -a } else { a };
                // A grain's click: a pulse of its contact's length, as an impulse of that area.
                (a, a * self.spec.click.contact / self.dt)
            }
            Excite::Rub { force, .. } => {
                self.level += 0.002 * (if self.stopping { 0.0 } else { force } - self.level);
                // The force's impulse this sample (N·s), its noise of unit RMS; the contact patch's
                // own shaking through the click (its gain per N of the force).
                let noise = self.noise.run(self.rng.signed()) * self.noisy;
                (noise * self.level * self.dt, noise * self.level)
            }
            Excite::Creak { rate, level } => {
                self.level += 0.002 * (if self.stopping { 0.0 } else { level } - self.level);
                (self.slip(rate), 0.0)
            }
            Excite::Wind { .. } => (0.0, 0.0),
        }
    }

    /// A creak's slip this sample, if one comes.
    fn slip(&mut self, rate: f32) -> f32 {
        let c = &mut self.creak;
        if c.left <= 0.0 {
            if self.stopping || self.rng.unit() >= rate * self.dt {
                return 0.0;
            }
            // A burst of stick–slip: a third to three quarters of a second, its slips speeding up
            // from 12–27 a second to one and a half to two and a half times that.
            c.length = 0.25 + 0.5 * self.rng.unit();
            c.left = c.length;
            c.rate = 12.0 + 15.0 * self.rng.unit();
            c.to = c.rate * (1.5 + self.rng.unit());
            c.phase = 0.0;
        }
        let c = &mut self.creak;
        let through = 1.0 - c.left / c.length;
        c.left -= self.dt;
        c.phase += (c.rate + (c.to - c.rate) * through) * self.dt;
        if c.phase < 1.0 {
            return 0.0;
        }
        c.phase -= 1.0 + 0.2 * (self.rng.unit() - 0.5);
        let swell = (std::f32::consts::PI * through).sin();
        self.level * swell * (0.7 + 0.6 * self.rng.unit())
    }

    /// The next sample of its pressure a metre away (Pa).
    pub fn sample(&mut self) -> f32 {
        if self.wait > 0 {
            self.wait -= 1;
            return 0.0;
        }
        let out = if let Excite::Wind { level, .. } = self.spec.excite {
            self.wind(level)
        } else {
            let (modes, click) = self.drive();
            let clicked = self.radiate.run(self.contact.run(click)) * self.spec.click.gain;
            self.bank.tick(modes) + clicked
        };
        self.t += self.dt;
        out
    }

    /// Wind: turbulent noise, its gusts slow (a time constant of two seconds), its pressure as
    /// the speed cubed (a dipole's power grows as U⁶: Curle 1955).
    fn wind(&mut self, level: f32) -> f32 {
        let target = if self.stopping { 0.0 } else { level };
        self.level += 0.0005 * (target - self.level);
        let g = &mut self.gust;
        g.0 = g.1.run(self.rng.signed() * 12.0);
        let gusting = (1.0 + 0.3 * g.0.clamp(-2.5, 2.5)).max(0.1);
        let noise = self.radiate.run(self.noise.run(self.rng.signed())) * self.noisy;
        noise * self.level * gusting * gusting * gusting
    }

    /// The loudest it can still be at a metre (Pa).
    pub fn loudness(&self) -> f32 {
        let drive = match self.spec.excite {
            Excite::Grains { seconds, .. } if self.t < seconds => self.spec.peak,
            e if e.continuous() => self
                .level
                .max(if self.stopping { 0.0 } else { self.spec.peak }),
            _ => 0.0,
        };
        self.bank.reach() + drive + if self.t < 0.05 { self.spec.peak } else { 0.0 }
    }

    /// Whether it has died away (a continuous one only once stopped).
    pub fn done(&self) -> bool {
        if self.wait > 0 {
            return false;
        }
        let quiet = self.bank.reach() < 1e-4 * self.spec.peak.max(1e-6) + 1e-7;
        match self.spec.excite {
            Excite::Blow => self.t > 0.05 && quiet,
            Excite::Grains { seconds, shape, .. } => {
                let end = match shape {
                    Envelope::Burst => seconds,
                    Envelope::Step { toe } => toe + 0.2,
                };
                self.t > end && quiet
            }
            _ => self.stopping && self.level < 1e-3 * self.spec.peak.max(1e-6) && quiet,
        }
    }
}

#[cfg(test)]
#[path = "voice_tests.rs"]
mod tests;
