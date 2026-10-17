//! A tick of a network. Compiled, it is split into its parts that no wire joins (each machine's
//! controls), and the parts run side by side: each owns its blocks' values, states and scripts
//! while compiled, so none touches another's, and the results do not depend on which thread
//! ran what. Within a part, blocks run in the plan's order (`order.rs`), a level's scripts side
//! by side. (In one order, a hundred machines' 10,000 gates took 0.15 ms a tick.)

use crate::block::{Kind, Pure, State};
use crate::network::Network;
use crate::order::plan;
use crate::script::Script;
use crate::signal::Signal;
use rayon::prelude::*;
use std::ops::Range;

/// An input with no wire.
const UNWIRED: u32 = u32::MAX;
/// An input that reads the value the tick before left (it closes a loop).
const DELAYED: u32 = 1 << 31;
/// Inputs gathered on the stack (more go to a buffer).
const STACK: usize = 4;

/// How a gate is evaluated: from its inputs alone, not at all (a constant), or with its
/// block's state.
#[derive(Clone, Copy, Debug)]
enum Op {
    Pure(Pure),
    Skip,
    Stateful(Kind),
}

/// A gate: how, its block (in its part), its output and its sources' range.
#[derive(Clone, Debug)]
struct Gate {
    op: Op,
    block: u32,
    out: u32,
    sources: Range<u32>,
}

/// A part of the network no wire joins to another, compiled.
#[derive(Debug, Default)]
pub(crate) struct Part {
    /// Its blocks (global indices), in the order its states are kept.
    blocks: Vec<usize>,
    gates: Vec<Gate>,
    /// Its scripts: the script, its output, its sources' range.
    scripts: Vec<(Script, u32, Range<u32>)>,
    /// Each input's source: an output (`DELAYED` set to read the tick before's), or `UNWIRED`.
    sources: Vec<u32>,
    /// Each level's scripts, and its gates with those of the levels after it that have none.
    levels: Vec<(Range<usize>, Range<usize>)>,
    values: Vec<Signal>,
    before: Vec<Signal>,
    states: Vec<State>,
    /// Whether any input reads the tick before.
    delays: bool,
}

/// The network compiled: its parts, and where each block, output and script is kept.
#[derive(Debug, Default)]
pub(crate) struct Compiled {
    parts: Vec<Part>,
    block_at: Vec<(u32, u32)>,
    value_at: Vec<(u32, u32)>,
    script_at: Vec<(u32, u32)>,
}

impl Compiled {
    pub(crate) fn value(&self, index: usize) -> Signal {
        let (p, k) = self.value_at[index];
        self.parts[p as usize].values[k as usize]
    }

    pub(crate) fn state(&self, block: usize) -> &State {
        let (p, k) = self.block_at[block];
        &self.parts[p as usize].states[k as usize]
    }

    pub(crate) fn state_mut(&mut self, block: usize) -> &mut State {
        let (p, k) = self.block_at[block];
        &mut self.parts[p as usize].states[k as usize]
    }

    pub(crate) fn scripts(&self) -> usize {
        self.script_at.len()
    }

    pub(crate) fn script(&self, script: usize) -> &Script {
        let (p, k) = self.script_at[script];
        &self.parts[p as usize].scripts[k as usize].0
    }
}

/// The parts of `net` no wire joins (union-find), each its blocks ascending, in the order of
/// their lowest block.
fn parts_of(net: &Network) -> Vec<Vec<usize>> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let n = net.blocks.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for (b, block) in net.blocks.iter().enumerate() {
        for &(s, _) in block.inputs.iter().flatten() {
            let (rb, rs) = (root(&mut parent, b), root(&mut parent, s.0));
            parent[rb.max(rs)] = rb.min(rs);
        }
    }
    let mut index = vec![usize::MAX; n];
    let mut parts: Vec<Vec<usize>> = Vec::new();
    for b in 0..n {
        let r = root(&mut parent, b);
        if index[r] == usize::MAX {
            index[r] = parts.len();
            parts.push(Vec::new());
        }
        parts[index[r]].push(b);
    }
    parts
}

fn index(v: usize) -> u32 {
    u32::try_from(v).expect("fewer than 2³¹ outputs")
}

impl Network {
    /// Splits the network into its parts and hands each its values, states and scripts.
    pub(crate) fn compile(&mut self) {
        let mut c = Compiled {
            block_at: vec![(0, 0); self.blocks.len()],
            value_at: vec![(0, 0); self.values.len()],
            script_at: vec![(0, 0); self.scripts.len()],
            ..Compiled::default()
        };
        let outputs: Vec<usize> = self.scripts.iter().map(|s| s.pins().1).collect();
        let mut scripts: Vec<Option<Script>> = std::mem::take(&mut self.scripts)
            .into_iter()
            .map(Some)
            .collect();
        for (p, group) in parts_of(self).into_iter().enumerate() {
            let mut part = Part::default();
            for (local, &b) in group.iter().enumerate() {
                let block = &self.blocks[b];
                c.block_at[b] = (index(p), index(local));
                let n = match block.kind {
                    Kind::Script(i) => outputs[i],
                    _ => 1,
                };
                for k in 0..n {
                    c.value_at[block.first + k] = (index(p), index(part.values.len()));
                    part.values.push(self.values[block.first + k]);
                }
                part.states.push(block.state);
            }
            let wiring: Vec<Vec<Option<usize>>> = group
                .iter()
                .map(|&b| {
                    let inputs = &self.blocks[b].inputs;
                    inputs
                        .iter()
                        .map(|pin| pin.map(|(s, _)| c.block_at[s.0].1 as usize))
                        .collect()
                })
                .collect();
            let plan = plan(&wiring);
            for level in &plan.levels {
                let (gates, first_script) = (part.gates.len(), part.scripts.len());
                for &local in level {
                    let block = &self.blocks[group[local]];
                    let start = index(part.sources.len());
                    for (pin, &delayed) in block.inputs.iter().zip(&plan.delayed[local]) {
                        part.sources.push(pin.map_or(UNWIRED, |(s, k)| {
                            part.delays |= delayed;
                            let at = c.value_at[self.blocks[s.0].first + k].1;
                            if delayed { at | DELAYED } else { at }
                        }));
                    }
                    let out = c.value_at[block.first].1;
                    let sources = start..index(part.sources.len());
                    if let Kind::Script(i) = block.kind {
                        c.script_at[i] = (index(p), index(part.scripts.len()));
                        let script = scripts[i].take().expect("one block a script");
                        part.scripts.push((script, out, sources));
                        continue;
                    }
                    let op = match block.kind {
                        Kind::Constant(_) => Op::Skip,
                        kind => kind.pure().map_or(Op::Stateful(kind), Op::Pure),
                    };
                    part.gates.push(Gate {
                        op,
                        block: index(local),
                        out,
                        sources,
                    });
                }
                // Levels without scripts run on with the one before, in one pass.
                match part.levels.last_mut() {
                    Some(last) if first_script == part.scripts.len() => {
                        last.0.end = part.gates.len();
                    }
                    _ => part
                        .levels
                        .push((gates..part.gates.len(), first_script..part.scripts.len())),
                }
            }
            part.blocks = group;
            c.parts.push(part);
        }
        self.compiled = Some(c);
    }

    /// Hands every part's values, states and scripts back (before the network changes).
    pub(crate) fn decompile(&mut self) {
        let Some(c) = self.compiled.take() else {
            return;
        };
        for (i, &(p, k)) in c.value_at.iter().enumerate() {
            self.values[i] = c.parts[p as usize].values[k as usize];
        }
        let mut scripts: Vec<Vec<Option<Script>>> = Vec::with_capacity(c.parts.len());
        for part in c.parts {
            for (local, &b) in part.blocks.iter().enumerate() {
                self.blocks[b].state = part.states[local];
            }
            scripts.push(part.scripts.into_iter().map(|(s, ..)| Some(s)).collect());
        }
        self.scripts = c
            .script_at
            .iter()
            .map(|&(p, k)| {
                scripts[p as usize][k as usize]
                    .take()
                    .expect("every script")
            })
            .collect();
    }

    /// One tick of `dt` seconds: the parts side by side.
    pub fn tick(&mut self, dt: f64) {
        if self.compiled.is_none() {
            self.compile();
        }
        let c = self.compiled.as_mut().expect("compiled");
        c.parts.par_iter_mut().for_each(|part| part.tick(dt));
    }
}

/// The value an input reads.
#[inline(always)]
fn read(source: u32, values: &[Signal], before: &[Signal]) -> Signal {
    if source == UNWIRED {
        Signal::default()
    } else if source & DELAYED != 0 {
        before[(source & !DELAYED) as usize]
    } else {
        values[source as usize]
    }
}

impl Part {
    fn tick(&mut self, dt: f64) {
        if self.delays {
            self.before.clone_from(&self.values);
        }
        for level in 0..self.levels.len() {
            let (gates, scripts) = self.levels[level].clone();
            if !scripts.is_empty() {
                self.run_scripts(scripts, dt);
            }
            for gate in &self.gates[gates] {
                let sources = &self.sources[gate.sources.start as usize..gate.sources.end as usize];
                let mut stack = [Signal::default(); STACK];
                let heap: Vec<Signal>;
                let inputs: &[Signal] = if sources.len() <= STACK {
                    for (slot, &s) in stack.iter_mut().zip(sources) {
                        *slot = read(s, &self.values, &self.before);
                    }
                    &stack[..sources.len()]
                } else {
                    heap = sources
                        .iter()
                        .map(|&s| read(s, &self.values, &self.before))
                        .collect();
                    &heap
                };
                let value = match gate.op {
                    Op::Pure(pure) => pure.evaluate(inputs),
                    Op::Stateful(kind) => {
                        kind.evaluate(inputs, &mut self.states[gate.block as usize], dt)
                    }
                    Op::Skip => continue,
                };
                self.values[gate.out as usize] = value;
            }
        }
    }

    /// Runs the scripts `which` side by side, and keeps what they set.
    fn run_scripts(&mut self, which: Range<usize>, dt: f64) {
        let (values, before, sources) = (&self.values, &self.before, &self.sources);
        let outputs: Vec<(u32, Vec<Option<Signal>>)> = self.scripts[which]
            .par_iter_mut()
            .map(|(script, out, range)| {
                let inputs: Vec<Signal> = sources[range.start as usize..range.end as usize]
                    .iter()
                    .map(|&s| read(s, values, before))
                    .collect();
                (*out, script.run(&inputs, dt))
            })
            .collect();
        for (first, set) in outputs {
            for (k, value) in set.into_iter().enumerate() {
                if let Some(v) = value {
                    self.values[first as usize + k] = v;
                }
            }
        }
    }
}
