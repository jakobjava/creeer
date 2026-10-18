//! How fast each process may run when stocks are at their bounds.
//!
//! A stock at its lower bound (empty) can give its consumers only what flows into it: they
//! share it in proportion to what they ask, by one factor φ ≤ 1 for all of them (water
//! filling: a consumer held lower by another limit takes no more than that, and what it
//! leaves goes to the others). A stock at its upper bound (full) takes from its producers
//! only what flows out of it, shared the same way. A process runs at the least of the factors
//! of every stock it is held by. The factors depend on each other (a consumer's supply is
//! another process, held by its own stocks), so they are found by a fixed point: from 1,
//! each stock's factor computed in turn from the others', until none changes. The factors
//! only fall from 1, so the first fixed point found is the largest: the most every process
//! can run.

/// Which bound a stock is held at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// At its lower bound: nothing may flow out beyond what flows in.
    Empty,
    /// At its upper bound: nothing may flow in beyond what flows out.
    Full,
}

/// Rounds of the fixed point at most (each settles at least one stock's factor in a chain).
const ROUNDS: usize = 200;

/// The factor φ ∈ [0, 1] for which Σ min(φ, limit)·amount = `supply` over `asks` (each a
/// process's limit from elsewhere and its amount): 1 if even then they take no more.
pub fn fill(asks: &[(f64, f64)], supply: f64) -> f64 {
    let all: f64 = asks.iter().map(|&(limit, a)| limit * a).sum();
    if all <= supply {
        return 1.0;
    }
    if supply <= 0.0 {
        return 0.0;
    }
    let mut sorted = asks.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    // Below the k-th limit, those from k on take φ each, those before their limits.
    let mut below = 0.0;
    let mut rest: f64 = sorted.iter().map(|s| s.1).sum();
    for &(limit, a) in &sorted {
        if below + limit * rest >= supply {
            return ((supply - below) / rest).clamp(0.0, 1.0);
        }
        below += limit * a;
        rest -= a;
    }
    1.0
}

/// The throttles (each in [0, 1]) of `processes` processes whose flows are `layout`
/// ((process, stock), flat) at full rates `rates`, with stocks `pinned` at a bound.
pub fn throttles(
    processes: usize,
    layout: &[(usize, usize)],
    rates: &[f64],
    pinned: &[(usize, Side)],
) -> Vec<f64> {
    if pinned.is_empty() {
        return vec![1.0; processes];
    }
    WORK.with_borrow_mut(|w| solve(processes, layout, rates, pinned, w))
}

/// Buffers the fixed point reuses (one set a thread: no result depends on them).
#[derive(Default)]
struct Work {
    entries: Vec<(usize, usize, f64, bool)>,
    phi: Vec<f64>,
    asks: Vec<(f64, f64)>,
}

thread_local! {
    static WORK: std::cell::RefCell<Work> = std::cell::RefCell::new(Work::default());
}

fn solve(
    processes: usize,
    layout: &[(usize, usize)],
    rates: &[f64],
    pinned: &[(usize, Side)],
    w: &mut Work,
) -> Vec<f64> {
    // Each flow at a pinned stock: (its stock's place in `pinned`, process, size, whether it
    // pushes the stock past its bound).
    let Work { entries, phi, asks } = w;
    entries.clear();
    for (&(p, stock), &amount) in layout.iter().zip(rates) {
        if amount == 0.0 {
            continue;
        }
        if let Some(k) = pinned.iter().position(|&(s, _)| s == stock) {
            let past = match pinned[k].1 {
                Side::Empty => amount < 0.0,
                Side::Full => amount > 0.0,
            };
            entries.push((k, p, amount.abs(), past));
        }
    }
    phi.clear();
    phi.resize(pinned.len(), 1.0);
    // A process's factor: the least of those of the stocks it is held at (but `except`).
    let throttle = |p: usize, phi: &[f64], except: Option<usize>| -> f64 {
        (entries.iter())
            .filter(|&&(k, q, _, past)| q == p && past && Some(k) != except)
            .map(|&(k, ..)| phi[k])
            .fold(1.0, f64::min)
    };
    let entries = &*entries;
    for _ in 0..ROUNDS {
        let mut changed = false;
        for k in 0..pinned.len() {
            let mut supply = 0.0;
            asks.clear();
            for &(j, p, a, past) in entries {
                if j != k {
                    continue;
                }
                if past {
                    asks.push((throttle(p, phi, Some(k)), a));
                } else {
                    supply += throttle(p, phi, None) * a;
                }
            }
            let next = fill(asks, supply);
            if (next - phi[k]).abs() > 1e-15 {
                phi[k] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..processes).map(|p| throttle(p, phi, None)).collect()
}
