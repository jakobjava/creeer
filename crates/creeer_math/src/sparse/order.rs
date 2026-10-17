//! Fill-reducing ordering: minimum degree on the symmetric pattern of A + Aᵀ.
//!
//! Eliminating a node joins its remaining neighbours into a clique (the fill it makes);
//! eliminating the node of fewest neighbours first keeps that fill small. Ties go to the
//! lowest node number, so the order is the same on every run and platform.

use std::collections::BTreeSet;

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
    let mut queue: BTreeSet<(usize, usize)> = (0..n).map(|v| (adj[v].len(), v)).collect();
    let mut order = Vec::with_capacity(n);
    let mut merged = Vec::new();
    while let Some((_, v)) = queue.pop_first() {
        order.push(v);
        alive[v] = false;
        let neighbours = std::mem::take(&mut adj[v]);
        for &u in &neighbours {
            queue.remove(&(adj[u].len(), u));
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
            queue.insert((adj[u].len(), u));
        }
    }
    order
}
