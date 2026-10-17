//! The blocks of a logic network: what each computes from its inputs, and the state it keeps
//! from tick to tick.

use crate::signal::Signal;

/// How a comparator compares its first input with its second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    Greater,
    Less,
    AtLeast,
    AtMost,
    Equal,
    NotEqual,
}

/// A PID controller's gains and output range. Inputs: setpoint, measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pid {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub min: f64,
    pub max: f64,
}

/// What a block is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A fixed signal.
    Constant(Signal),
    /// A reading from any domain, set from outside before each tick (`Network::set_sensor`).
    Sensor,
    /// A setting for any domain: its input, read from outside after each tick.
    Actuator,
    Not,
    /// True when all its inputs are (any number of them).
    And,
    Or,
    /// True when an odd number of its inputs are.
    Xor,
    Nand,
    Nor,
    /// Set and reset; reset wins.
    SrLatch,
    /// Data and clock: takes the data on the clock's rising edge.
    DFlipFlop,
    /// True once its input has been true for this long (s).
    OnDelay(f64),
    /// True while its input is, and for this long after (s).
    OffDelay(f64),
    /// True for this long (s) from its input's rising edge.
    Pulse(f64),
    Compare(Compare),
    /// True above `high` until below `low`.
    Schmitt {
        low: f64,
        high: f64,
    },
    /// The sum of its inputs.
    Add,
    /// The first input less the second.
    Sub,
    /// The product of its inputs.
    Mul,
    /// The first input over the second (IEEE: a zero divisor gives ±∞ or NaN).
    Div,
    Min,
    Max,
    /// The second input if the first is true, else the third.
    Select,
    Pid(Pid),
    /// A Lua microcontroller (`Network::add_script`): its index among the scripts.
    Script(usize),
}

/// What a block keeps from tick to tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    /// The latch's, flip-flop's or Schmitt trigger's output; a sensor's reading.
    pub held: Signal,
    /// The last tick's input (edges: a flip-flop's clock, a pulse's trigger).
    pub last: bool,
    /// A timer's time (s); a PID controller's integral.
    pub time: f64,
    /// A PID controller's last measurement.
    pub previous: f64,
}

/// A block that keeps nothing from tick to tick: its output is its inputs' alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pure {
    Actuator,
    Not,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Compare(Compare),
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    Select,
}

impl Pure {
    /// Its output from `inputs`.
    pub(crate) fn evaluate(self, inputs: &[Signal]) -> Signal {
        let at = |k: usize| inputs.get(k).copied().unwrap_or_default();
        let truths = || inputs.iter().map(|s| s.truth());
        let numbers = || inputs.iter().map(|s| s.number());
        match self {
            Self::Actuator => at(0),
            Self::Not => Signal::Bool(!at(0).truth()),
            Self::And => Signal::Bool(truths().all(|t| t)),
            Self::Or => Signal::Bool(truths().any(|t| t)),
            Self::Xor => Signal::Bool(truths().filter(|&t| t).count() % 2 == 1),
            Self::Nand => Signal::Bool(!truths().all(|t| t)),
            Self::Nor => Signal::Bool(!truths().any(|t| t)),
            Self::Compare(op) => {
                let (a, b) = (at(0).number(), at(1).number());
                #[allow(clippy::float_cmp)] // a comparator compares exactly
                let result = match op {
                    Compare::Greater => a > b,
                    Compare::Less => a < b,
                    Compare::AtLeast => a >= b,
                    Compare::AtMost => a <= b,
                    Compare::Equal => a == b,
                    Compare::NotEqual => a != b,
                };
                Signal::Bool(result)
            }
            Self::Add => Signal::Number(numbers().sum()),
            Self::Sub => Signal::Number(at(0).number() - at(1).number()),
            Self::Mul => Signal::Number(numbers().product()),
            Self::Div => Signal::Number(at(0).number() / at(1).number()),
            Self::Min => Signal::Number(numbers().fold(f64::INFINITY, f64::min)),
            Self::Max => Signal::Number(numbers().fold(f64::NEG_INFINITY, f64::max)),
            Self::Select => {
                if at(0).truth() {
                    at(1)
                } else {
                    at(2)
                }
            }
        }
    }
}

impl Kind {
    /// It as a block that keeps nothing, if it is one.
    pub(crate) fn pure(&self) -> Option<Pure> {
        Some(match *self {
            Self::Actuator => Pure::Actuator,
            Self::Not => Pure::Not,
            Self::And => Pure::And,
            Self::Or => Pure::Or,
            Self::Xor => Pure::Xor,
            Self::Nand => Pure::Nand,
            Self::Nor => Pure::Nor,
            Self::Compare(op) => Pure::Compare(op),
            Self::Add => Pure::Add,
            Self::Sub => Pure::Sub,
            Self::Mul => Pure::Mul,
            Self::Div => Pure::Div,
            Self::Min => Pure::Min,
            Self::Max => Pure::Max,
            Self::Select => Pure::Select,
            _ => return None,
        })
    }

    /// Its output from `inputs` over a tick of `dt` seconds, `state` updated.
    pub(crate) fn evaluate(&self, inputs: &[Signal], state: &mut State, dt: f64) -> Signal {
        if let Some(pure) = self.pure() {
            return pure.evaluate(inputs);
        }
        let at = |k: usize| inputs.get(k).copied().unwrap_or_default();
        match *self {
            Self::Constant(s) => s,
            Self::Sensor => state.held,
            Self::SrLatch => {
                if at(1).truth() {
                    state.held = Signal::Bool(false);
                } else if at(0).truth() {
                    state.held = Signal::Bool(true);
                }
                state.held
            }
            Self::DFlipFlop => {
                let clock = at(1).truth();
                if clock && !state.last {
                    state.held = at(0);
                }
                state.last = clock;
                state.held
            }
            Self::OnDelay(delay) => {
                state.time = if at(0).truth() { state.time + dt } else { 0.0 };
                Signal::Bool(at(0).truth() && state.time >= delay)
            }
            Self::OffDelay(delay) => {
                state.time = if at(0).truth() { 0.0 } else { state.time + dt };
                Signal::Bool(at(0).truth() || state.time < delay)
            }
            Self::Pulse(length) => pulse(at(0).truth(), state, length, dt),
            Self::Schmitt { low, high } => {
                let x = at(0).number();
                if x > high {
                    state.held = Signal::Bool(true);
                } else if x < low {
                    state.held = Signal::Bool(false);
                }
                state.held
            }
            Self::Pid(pid) => Signal::Number(pid.step(at(0).number(), at(1).number(), state, dt)),
            // Scripts run apart (`run.rs`), pure blocks above.
            _ => Signal::default(),
        }
    }
}

/// A pulse of `length` seconds from the rising edge of `input`, not started again while on.
fn pulse(input: bool, state: &mut State, length: f64, dt: f64) -> Signal {
    let on = state.held.truth();
    if on {
        state.time += dt;
        if state.time >= length {
            state.held = Signal::Bool(false);
        }
    } else if input && !state.last {
        state.held = Signal::Bool(true);
        state.time = 0.0;
    }
    state.last = input;
    state.held
}

impl Pid {
    /// The controller's output for `setpoint` and `measurement` over `dt`: proportional on the
    /// error, integral of the error (held while the output is driven past its range, so it does
    /// not wind up), derivative of the measurement (a setpoint's jump gives no kick).
    pub fn step(self, setpoint: f64, measurement: f64, state: &mut State, dt: f64) -> f64 {
        let error = setpoint - measurement;
        let rate = if state.last && dt > 0.0 {
            (measurement - state.previous) / dt
        } else {
            0.0
        };
        let (state_last, previous) = (true, measurement);
        let unclamped = self.kp * error + state.time + self.ki * error * dt - self.kd * rate;
        let out = unclamped.clamp(self.min, self.max);
        // Integrate unless that drives the output further past its range.
        #[allow(clippy::float_cmp)] // clamped exactly to its range
        let saturated = (out == self.max && error > 0.0) || (out == self.min && error < 0.0);
        if !saturated {
            state.time += self.ki * error * dt;
        }
        (state.last, state.previous) = (state_last, previous);
        out
    }
}
