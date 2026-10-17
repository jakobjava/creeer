//! A logic network: blocks wired output to input, evaluated once per tick (`order.rs`).

use crate::block::{Kind, State};
use crate::run::Compiled;
use crate::script::Script;
use crate::signal::Signal;

/// A block of a network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub usize);

/// An output of a block: the block and which of its outputs (0 but for scripts).
pub type Pin = (BlockId, usize);

/// One block: what it is, where its inputs come from, its state, where its outputs are kept.
#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub kind: Kind,
    pub inputs: Vec<Option<Pin>>,
    pub state: State,
    /// Its first output's index in `values`.
    pub first: usize,
}

/// A network of logic blocks and microcontrollers.
#[derive(Debug, Default)]
pub struct Network {
    pub(crate) blocks: Vec<Block>,
    pub(crate) scripts: Vec<Script>,
    /// Every output's value, block by block.
    pub(crate) values: Vec<Signal>,
    /// The network compiled (`run.rs`): while it is, its parts hold the values, states and
    /// scripts; any change hands them back first.
    pub(crate) compiled: Option<Compiled>,
}

impl Network {
    /// An empty network.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a block of `kind` (a script with [`Self::add_script`]).
    ///
    /// # Panics
    /// For [`Kind::Script`].
    pub fn add(&mut self, kind: Kind) -> BlockId {
        assert!(!matches!(kind, Kind::Script(_)), "scripts with add_script");
        self.decompile();
        self.push(kind, 1)
    }

    /// Adds a microcontroller running `script`.
    pub fn add_script(&mut self, script: Script) -> BlockId {
        self.decompile();
        let (_, outputs) = script.pins();
        self.scripts.push(script);
        self.push(Kind::Script(self.scripts.len() - 1), outputs)
    }

    fn push(&mut self, kind: Kind, outputs: usize) -> BlockId {
        let first = self.values.len();
        let initial = match kind {
            Kind::Constant(s) => s,
            _ => Signal::default(),
        };
        self.values.extend(std::iter::repeat_n(initial, outputs));
        self.blocks.push(Block {
            kind,
            inputs: Vec::new(),
            state: State::default(),
            first,
        });
        self.compiled = None;
        BlockId(self.blocks.len() - 1)
    }

    /// Wires output `from` to input `input` of block `to` (replacing what was wired there).
    pub fn wire(&mut self, from: Pin, to: BlockId, input: usize) {
        self.decompile();
        let inputs = &mut self.blocks[to.0].inputs;
        if inputs.len() <= input {
            inputs.resize(input + 1, None);
        }
        inputs[input] = Some(from);
        self.compiled = None;
    }

    /// Sets a sensor's reading for the next tick.
    pub fn set_sensor(&mut self, sensor: BlockId, value: impl Into<Signal>) {
        let value = value.into();
        match self.compiled.as_mut() {
            Some(c) => c.state_mut(sensor.0).held = value,
            None => self.blocks[sensor.0].state.held = value,
        }
    }

    /// The value of output `pin` as the last tick left it (an actuator's setting).
    pub fn value(&self, (block, k): Pin) -> Signal {
        self.value_of(self.blocks[block.0].first + k)
    }

    /// The script block `block` runs, if it is one.
    pub fn script(&self, block: BlockId) -> Option<&Script> {
        match self.blocks[block.0].kind {
            Kind::Script(i) => Some(self.script_of(i)),
            _ => None,
        }
    }

    /// Replaces the script of block `block` (same pins).
    ///
    /// # Panics
    /// If the block is no script, or the new one has other pins.
    pub fn replace_script(&mut self, block: BlockId, script: Script) {
        self.decompile();
        let Kind::Script(i) = self.blocks[block.0].kind else {
            panic!("block {} is no script", block.0);
        };
        assert_eq!(script.pins(), self.scripts[i].pins(), "the same pins");
        self.scripts[i] = script;
    }

    /// Each block's first output in `values` again, from the blocks' kinds; `None` where a
    /// wire comes from a missing output or the values do not fit (a damaged save).
    pub(crate) fn renumber_outputs(&mut self) -> Option<()> {
        let mut first = 0;
        let mut outputs = Vec::with_capacity(self.blocks.len());
        for b in &mut self.blocks {
            b.first = first;
            let n = match b.kind {
                Kind::Script(i) => self.scripts.get(i)?.pins().1,
                _ => 1,
            };
            outputs.push(n);
            first += n;
        }
        let wired = self.blocks.iter().flat_map(|b| b.inputs.iter().flatten());
        for &(s, k) in wired {
            if k >= *outputs.get(s.0)? {
                return None;
            }
        }
        (self.values.len() == first).then_some(())
    }

    /// Output `i`'s value, wherever it is kept.
    pub(crate) fn value_of(&self, i: usize) -> Signal {
        self.compiled
            .as_ref()
            .map_or(self.values[i], |c| c.value(i))
    }

    /// Block `b`'s state, wherever it is kept.
    pub(crate) fn state_of(&self, b: usize) -> State {
        self.compiled
            .as_ref()
            .map_or(self.blocks[b].state, |c| *c.state(b))
    }

    /// Script `i`, wherever it is kept.
    pub(crate) fn script_of(&self, i: usize) -> &Script {
        match &self.compiled {
            Some(c) => c.script(i),
            None => &self.scripts[i],
        }
    }

    /// How many scripts it has.
    pub(crate) fn script_count(&self) -> usize {
        self.compiled
            .as_ref()
            .map_or(self.scripts.len(), Compiled::scripts)
    }

    /// How many blocks it has.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether it has no blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}
