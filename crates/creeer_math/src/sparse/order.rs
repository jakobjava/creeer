//! Fill-reducing ordering: minimum degree on the symmetric pattern of A + Aᵀ.
//!
//! Eliminating a node joins its remaining neighbours into a clique (the fill it makes);
//! eliminating the node of fewest neighbours first keeps that fill small. Ties go to the
//! lowest node number, so the order is the same on every run and platform.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The elimination order (position → node) for the matrix pattern `col_ptr`/`row_idx`
/// (n columns), symmetrised.
pub fn minimum_degree(n: usize, col_ptr: &[usize], row_idx: &[usize]) -> Vec<usize> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for c in 0..n {
        for &r in &row_idx[col_ptr[c]..col_ptr[c + 1]] {
            if r != c {
                adj[c].push(r);
                adj[r].push(c);
            }
        }
    }
    for a in &mut adj {
        a.sort_unstable();
        a.dedup();
    }
    let mut alive = vec![true; n];
    // Each node's live neighbours: its list may still hold eliminated ones (dropped at its next
    // merge), so a node eliminated beside a hub of thousands costs the hub one count, not a
    // rebuilt list (an 11,000-node thermal island's ordering took 7.3 ms so).
    let mut degree: Vec<usize> = adj.iter().map(Vec::len).collect();
    // (degree, node), least first; an entry whose node was eliminated or whose degree changed
    // since is stale and skipped: the least live entry is what a set of the current ones
    // would give (ties to the lowest node).
    let mut queue: BinaryHeap<Reverse<(usize, usize)>> =
        (0..n).map(|v| Reverse((degree[v], v))).collect();
    let mut order = Vec::with_capacity(n);
    let mut merged = Vec::new();
    while let Some(Reverse((d, v))) = queue.pop() {
        if !alive[v] || d != degree[v] {
            continue;
        }
        order.push(v);
        alive[v] = false;
        let mut neighbours = std::mem::take(&mut adj[v]);
        neighbours.retain(|&u| alive[u]);
        for &u in &neighbours {
            if neighbours.len() == 1 {
                // No clique to join: u only loses v.
                degree[u] -= 1;
            } else {
                // u's neighbours, less v, plus v's other neighbours: the clique.
                merged.clear();
                let (a, b) = (&adj[u], &neighbours);
                let (mut i, mut j) = (0, 0);
                while i < a.len() || j < b.len() {
                    let next = match (a.get(i), b.get(j)) {
                        (Some(&x), Some(&y)) if x == y => {
                            i += 1;
                            j += 1;
                            x
                        }
                        (Some(&x), Some(&y)) if x < y => {
                            i += 1;
                            x
                        }
                        (Some(_), Some(&y)) | (None, Some(&y)) => {
                            j += 1;
                            y
                        }
                        (Some(&x), None) => {
                            i += 1;
                            x
                        }
                        (None, None) => unreachable!(),
                    };
                    if next != u && alive[next] {
                        merged.push(next);
                    }
                }
                std::mem::swap(&mut adj[u], &mut merged);
                degree[u] = adj[u].len();
            }
            queue.push(Reverse((degree[u], u)));
        }
    }
    order
}
