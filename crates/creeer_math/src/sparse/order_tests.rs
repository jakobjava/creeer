//! The minimum-degree ordering keeps its order exactly (F7.6): against the plain algorithm it
//! replaced (a set of every live node's degree, each neighbour's list rebuilt at every
//! elimination), on random patterns and on a hub (where the plain one is quadratic: 73 ms for
//! a hub of 10,000 leaves, against 3.1 ms).

use super::order::minimum_degree;
use std::collections::BTreeSet;

/// The ordering as it was: one (degree, node) entry per live node; every neighbour of an
/// eliminated node takes the clique into its list.
fn plain(n: usize, col_ptr: &[usize], row_idx: &[usize]) -> Vec<usize> {
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for c in 0..n {
        for &r in &row_idx[col_ptr[c]..col_ptr[c + 1]] {
            if r != c {
                adj[c].insert(r);
                adj[r].insert(c);
            }
        }
    }
    let mut queue: BTreeSet<(usize, usize)> = (0..n).map(|v| (adj[v].len(), v)).collect();
    let mut order = Vec::with_capacity(n);
    while let Some((_, v)) = queue.pop_first() {
        order.push(v);
        let neighbours = std::mem::take(&mut adj[v]);
        for &u in &neighbours {
            queue.remove(&(adj[u].len(), u));
            adj[u].remove(&v);
            adj[u].extend(neighbours.iter().filter(|&&w| w != u));
            queue.insert((adj[u].len(), u));
        }
    }
    order
}

/// A symmetric pattern (both triangles and the diagonal) from undirected edges.
fn pattern(n: usize, edges: &[(usize, usize)]) -> (Vec<usize>, Vec<usize>) {
    let mut cols: Vec<BTreeSet<usize>> = (0..n).map(|c| BTreeSet::from([c])).collect();
    for &(a, b) in edges {
        cols[a].insert(b);
        cols[b].insert(a);
    }
    let mut col_ptr = vec![0];
    let mut row_idx = Vec::new();
    for c in cols {
        row_idx.extend(c);
        col_ptr.push(row_idx.len());
    }
    (col_ptr, row_idx)
}

#[test]
fn the_order_is_the_plain_algorithms_on_random_patterns() {
    let mut state = 11u64;
    let mut next = |below: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize % below
    };
    for (n, per_node) in [(10, 1), (60, 2), (300, 3), (1_000, 2), (600, 6)] {
        let edges: Vec<(usize, usize)> = (0..n * per_node)
            .map(|_| (next(n), next(n)))
            .filter(|(a, b)| a != b)
            .collect();
        let (col_ptr, row_idx) = pattern(n, &edges);
        assert_eq!(
            minimum_degree(n, &col_ptr, &row_idx),
            plain(n, &col_ptr, &row_idx),
            "{n} nodes"
        );
    }
}

#[test]
fn a_hub_is_ordered_as_before() {
    // The hub joined to 2,000 leaves, each leaf to a tip of its own.
    let n = 4_001;
    let edges: Vec<(usize, usize)> = (0..2_000)
        .flat_map(|k| [(0, 1 + 2 * k), (1 + 2 * k, 2 + 2 * k)])
        .collect();
    let (col_ptr, row_idx) = pattern(n, &edges);
    assert_eq!(
        minimum_degree(n, &col_ptr, &row_idx),
        plain(n, &col_ptr, &row_idx)
    );
}
