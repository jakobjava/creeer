//! A Lua microcontroller: a script with numbered inputs and outputs, run once per tick within
//! a budget of instructions and memory (`sandbox.rs` for what it may use).
//!
//! The script defines `function tick(dt)`; each tick it reads `input[1..n]` (numbers or
//! booleans) and sets `output[1..m]` (an output it leaves alone keeps its value). The table
//! `memory` is what a save keeps: on loading, the script runs again from its source with
//! `memory` as it was saved (a Lua state's own insides cannot be saved portably).

use crate::persist;
use crate::sandbox;
use crate::signal::Signal;
use mlua::chunk::ChunkMode;
use mlua::{Function, HookTriggers, Lua, LuaOptions, StdLib, Table, Value, VmState};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Instructions between checks of the budget.
const STRIDE: u32 = 100;
/// The budget of a script's first run (its definitions), in ticks' budgets.
const START: u64 = 100;

/// What a script may use each tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Lua instructions a tick (counted in strides of 100).
    pub instructions: u64,
    /// Bytes of memory in all.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            instructions: 10_000,
            memory: 256 * 1024,
        }
    }
}

/// A running (or stopped) microcontroller.
pub struct Script {
    lua: Lua,
    source: String,
    inputs: usize,
    outputs: usize,
    limits: Limits,
    /// Instructions run this tick, and its budget.
    used: Arc<AtomicU64>,
    budget: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
    rng: Arc<Mutex<[u64; 4]>>,
    log: Arc<Mutex<Vec<String>>>,
    tick: Option<Function>,
    input: Table,
    output: Table,
    /// Why it stopped, if it did.
    halted: Option<String>,
}

impl std::fmt::Debug for Script {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Script")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .field("halted", &self.halted)
            .finish_non_exhaustive()
    }
}

impl Script {
    /// A script from `source` with `inputs` and `outputs`, its random numbers seeded by `seed`.
    /// One that does not compile or run is made stopped, with the reason.
    ///
    /// # Panics
    /// If Lua cannot be started at all (out of the process's memory).
    pub fn new(source: &str, inputs: usize, outputs: usize, limits: Limits, seed: u64) -> Self {
        Self::with_memory(
            source,
            (inputs, outputs),
            limits,
            sandbox::seeded(seed),
            &[],
        )
    }

    /// As [`Self::new`], with the random generator's state `rng` and its `memory` as saved
    /// (`persist.rs`).
    pub(crate) fn with_memory(
        source: &str,
        (inputs, outputs): (usize, usize),
        limits: Limits,
        rng: [u64; 4],
        memory: &[u8],
    ) -> Self {
        let libs = StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH;
        let lua = Lua::new_with(libs, LuaOptions::default()).expect("Lua starts");
        let (used, budget) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
        let stopped = Arc::new(AtomicBool::new(false));
        let (rng, log) = (Arc::new(Mutex::new(rng)), Arc::new(Mutex::new(Vec::new())));
        let (input, output) = (
            lua.create_table().expect("a table"),
            lua.create_table().expect("a table"),
        );
        let mut script = Self {
            lua,
            source: source.to_owned(),
            inputs,
            outputs,
            limits,
            used,
            budget,
            stopped,
            rng,
            log,
            tick: None,
            input,
            output,
            halted: None,
        };
        if let Err(e) = script.start(memory) {
            script.halted = Some(e.to_string());
        }
        script
    }

    /// A script as a save held it (`save.rs`): started again from its source with its `memory`,
    /// or stopped as it was.
    pub(crate) fn loaded(
        source: &str,
        pins: (usize, usize),
        limits: Limits,
        rng: [u64; 4],
        memory: &[u8],
        halted: Option<String>,
    ) -> Self {
        let mut script = Self::with_memory(source, pins, limits, rng, memory);
        if halted.is_some() {
            script.halted = halted;
        }
        script
    }

    fn start(&mut self, memory: &[u8]) -> mlua::Result<()> {
        sandbox::install(
            &self.lua,
            Arc::clone(&self.stopped),
            Arc::clone(&self.log),
            Arc::clone(&self.rng),
        )?;
        let globals = self.lua.globals();
        globals.set("input", &self.input)?;
        globals.set("output", &self.output)?;
        globals.set("memory", persist::read(&self.lua, memory)?)?;
        let (used, budget, stopped) = (
            Arc::clone(&self.used),
            Arc::clone(&self.budget),
            Arc::clone(&self.stopped),
        );
        self.lua.set_global_hook(
            HookTriggers::new().every_nth_instruction(STRIDE),
            move |_, _| {
                let now = used.fetch_add(u64::from(STRIDE), Ordering::Relaxed) + u64::from(STRIDE);
                if now > budget.load(Ordering::Relaxed) {
                    stopped.store(true, Ordering::Relaxed);
                    return Err(mlua::Error::runtime("out of its instruction budget"));
                }
                Ok(VmState::Continue)
            },
        )?;
        self.lua.set_memory_limit(self.limits.memory)?;
        self.charge(self.limits.instructions.saturating_mul(START));
        self.lua
            .load(self.source.as_str())
            .set_name("=controller")
            .set_mode(ChunkMode::Text)
            .exec()?;
        match globals.get::<Value>("tick")? {
            Value::Function(f) => self.tick = Some(f),
            _ => {
                return Err(mlua::Error::runtime(
                    "the script defines no function tick(dt)",
                ));
            }
        }
        Ok(())
    }

    /// A new budget of `instructions`.
    fn charge(&self, instructions: u64) {
        self.used.store(0, Ordering::Relaxed);
        self.budget.store(instructions, Ordering::Relaxed);
    }

    /// Runs one tick of `dt` seconds on `inputs`: each output as the script left it (`None`
    /// where it left it unset). A stopped script sets nothing.
    pub fn run(&mut self, inputs: &[Signal], dt: f64) -> Vec<Option<Signal>> {
        let none = vec![None; self.outputs];
        let Some(tick) = self.tick.clone().filter(|_| self.halted.is_none()) else {
            return none;
        };
        self.charge(self.limits.instructions);
        let result = self.pass_inputs(inputs).and_then(|()| tick.call::<()>(dt));
        if let Err(e) = result {
            self.halted = Some(stop_reason(&e));
            return none;
        }
        (1..=self.outputs)
            .map(|k| match self.output.raw_get::<Value>(k) {
                Ok(Value::Boolean(b)) => Some(Signal::Bool(b)),
                #[allow(clippy::cast_precision_loss)] // Lua's integers as the game's numbers
                Ok(Value::Integer(i)) => Some(Signal::Number(i as f64)),
                Ok(Value::Number(x)) => Some(Signal::Number(x)),
                _ => None,
            })
            .collect()
    }

    fn pass_inputs(&self, inputs: &[Signal]) -> mlua::Result<()> {
        for k in 0..self.inputs {
            let value = match inputs.get(k).copied().unwrap_or_default() {
                Signal::Bool(b) => Value::Boolean(b),
                Signal::Number(x) => Value::Number(x),
            };
            self.input.raw_set(k + 1, value)?;
        }
        Ok(())
    }

    /// Why it stopped, if it did.
    pub fn halted(&self) -> Option<&str> {
        self.halted.as_deref()
    }

    /// What it printed, the last lines.
    pub fn log(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Its source.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Its inputs and outputs.
    pub fn pins(&self) -> (usize, usize) {
        (self.inputs, self.outputs)
    }

    /// Its limits.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The bytes of memory it uses.
    pub fn memory_used(&self) -> usize {
        self.lua.used_memory()
    }

    /// Its `memory` table as a save keeps it, and its random generator's state.
    pub(crate) fn saved(&self) -> (Vec<u8>, [u64; 4]) {
        let memory = self
            .lua
            .globals()
            .get::<Value>("memory")
            .map(|m| persist::write(&m))
            .unwrap_or_default();
        let rng = *self.rng.lock().unwrap_or_else(PoisonError::into_inner);
        (memory, rng)
    }

    /// Starts it again from its source, its `memory` kept (after an edit, or a stop).
    #[must_use]
    pub fn restarted(&self, source: &str) -> Self {
        let (memory, rng) = self.saved();
        Self::with_memory(
            source,
            (self.inputs, self.outputs),
            self.limits,
            rng,
            &memory,
        )
    }
}

/// A stop's reason: Lua's message, without the traceback.
fn stop_reason(e: &mlua::Error) -> String {
    let text = e.to_string();
    text.lines().next().unwrap_or("stopped").to_owned()
}
