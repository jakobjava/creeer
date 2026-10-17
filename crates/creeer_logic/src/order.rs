//! The order a network's blocks are evaluated in each tick.
//!
//! Blocks are taken in topological order, the lowest-numbered first among those ready. Where
//! the remaining blocks all wait on each other (a feedback loop), the lowest-numbered of them
//! goes first, reading what its sources gave the tick before: the loop passes through a
//! one-tick register there, the same one in every run.
//!
//! Blocks are then grouped in levels: a block's level is one past its latest source within
//! the tick. Blocks of a level depend on none of each other, so a level's scripts run side by
//! side (`network.rs`).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The plan of a tick.
#[derive(Clone, Debug, Default)]
pub(crate) struct Plan {
    /// Each level's blocks, in order.
    pub levels: Vec<Vec<usize>>,
    /// For each block, for each input: whether it reads the tick before's value.
    pub delayed: Vec<Vec<bool>>,
}

/// The plan for blocks whose inputs come from `sources[block][input]` (a block, if wired).
pub(crate) fn plan(sources: &[Vec<Option<usize>>]) -> Plan {
    let n = sources.len();
    let mut waiting = vec![0usize; n];
    let mut users: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, inputs) in sources.iter().enumerate() {
        for &s in inputs.iter().flatten() {
            if s != b {
                waiting[b] += 1;
                users[s].push(b);
            }
        }
    }
    let mut done = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut ready: BinaryHeap<Reverse<usize>> =
        (0..n).filter(|&b| waiting[b] == 0).map(Reverse).collect();
    let mut next_unlocked = 0;
    while order.len() < n {
        let b = if let Some(Reverse(b)) = ready.pop() {
            b
        } else {
            // A loop: its lowest-numbered block goes first.
            while done[next_unlocked] {
                next_unlocked += 1;
            }
            next_unlocked
        };
        if done[b] {
            continue;
        }
        done[b] = true;
        order.push(b);
        for &u in &users[b] {
            waiting[u] -= 1;
            if waiting[u] == 0 && !done[u] {
                ready.push(Reverse(u));
            }
        }
    }
    let mut position = vec![0usize; n];
    for (p, &b) in order.iter().enumerate() {
        position[b] = p;
    }
    let delayed: Vec<Vec<bool>> = sources
        .iter()
        .enumerate()
        .map(|(b, inputs)| {
            inputs
                .iter()
                .map(|s| s.is_some_and(|s| position[s] >= position[b]))
                .collect()
        })
        .collect();
    let mut level = vec![0usize; n];
    for &b in &order {
        level[b] = sources[b]
            .iter()
            .zip(&delayed[b])
            .filter(|&(_, &d)| !d)
            .filter_map(|(s, _)| *s)
            .map(|s| level[s] + 1)
            .max()
            .unwrap_or(0);
    }
    let mut levels: Vec<Vec<usize>> = vec![Vec::new(); level.iter().max().map_or(0, |m| m + 1)];
    for &b in &order {
        levels[level[b]].push(b);
    }
    Plan { levels, delayed }
}
