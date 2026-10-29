//! A strut's push in the step when it falls with the speed the strut lengthens at
//! (`world/strut.rs`, `Strut::fastest`; G2.10, ADR-0023): the least of the one-sided Huber
//! spring's push and a force–velocity line falling with the strut's length, so its energy
//! stays convex.

/// A strut's push at length `x` in the step: its one-sided Huber spring's
/// clamp(K·(r − x), 0, most) (`spring` = (K, r)), but no more than its force–velocity line
/// most·(1 − (x − x̂)/(β·v₀)), at least nought (`line` = (x̂, β·v₀): its length at the
/// prediction, and what it lengthens by in the step at v₀). Its energy, the push and its
/// stiffness (minus its slope): the Huber spring's energy and the integral of what the line
/// takes off its push, up to x. Where the line has taken nothing up to x, the Huber spring's
/// own, bit for bit (standing, walking, landing: the line binds only lengthening fast).
pub(super) fn pushing(
    x: f64,
    (k, r): (f64, f64),
    most: f64,
    (hat, reach): (f64, f64),
) -> (f64, f64, f64) {
    let (energy, push, stiffness) = huber(r - x, k, most);
    let spring = |y: f64| (k * (r - y)).clamp(0.0, most);
    let line = |y: f64| (most * (1.0 - (y - hat) / reach)).max(0.0);
    // What the line takes off the spring's push: linear between where either has a kink or
    // they cross, nothing far short of the prediction (the line above the most there).
    let taken = |y: f64| spring(y) - line(y);
    let mut points = vec![hat, hat + reach, r, r - most / k];
    let crossing = (k * r - most * (1.0 + hat / reach)) / (k - most / reach);
    if crossing.is_finite() {
        points.push(crossing);
    }
    points.sort_by(f64::total_cmp);
    let mut off = 0.0;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1].min(x));
        if b <= a {
            continue;
        }
        let (ta, tb) = (taken(a), taken(b));
        off += if ta >= 0.0 && tb >= 0.0 {
            0.5 * (ta + tb) * (b - a)
        } else if ta > 0.0 || tb > 0.0 {
            // Crossing nought within: only the part above it.
            let (high, low) = if ta > 0.0 { (ta, tb) } else { (tb, ta) };
            0.5 * high * (b - a) * high / (high - low)
        } else {
            0.0
        };
    }
    let now = taken(x);
    if off <= 0.0 && now <= 0.0 {
        return (energy, push, stiffness);
    }
    if now > 0.0 {
        let slope = if line(x) > 0.0 { most / reach } else { 0.0 };
        (energy + off, push - now, slope)
    } else {
        (energy + off, push, stiffness)
    }
}

/// A one-sided Huber spring's energy at compression `u` with stiffness `k`, at most `most`;
/// its force and its stiffness there.
pub(super) fn huber(u: f64, k: f64, most: f64) -> (f64, f64, f64) {
    if u <= 0.0 || k <= 0.0 {
        (0.0, 0.0, 0.0)
    } else if k * u <= most {
        (0.5 * k * u * u, k * u, k)
    } else {
        (most * u - 0.5 * most * most / k, most, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{huber, pushing};

    /// A leg's: 30 kN/m and its damper over β (h = 1/60 s, backward Euler).
    const K: f64 = 30_000.0 + 2_130.0 * 60.0;
    const MOST: f64 = 3_600.0;
    /// What it lengthens by in a step at 3.5 m/s.
    const REACH: f64 = 3.5 / 60.0;

    /// The step's spring towards its rest blended with the prediction (`joint/strut.rs`).
    fn rest(spring_rest: f64, hat: f64) -> f64 {
        (30_000.0 * spring_rest + 2_130.0 * 60.0 * hat) / K
    }

    #[test]
    fn standing_landing_and_still_it_is_the_huber_spring_bit_for_bit() {
        // A leg pressed 3.4 cm under the body (standing), or 18 cm by a landing, or still
        // pressed 0.45 m but not moving: wherever it is in the step short of lengthening fast.
        for (pressed, at) in [(0.034, 0.0), (0.18, -0.05), (0.18, 0.0), (0.45, -0.1)] {
            let hat = 1.0 - pressed;
            let r = rest(1.0, hat);
            let x = hat + at;
            assert_eq!(
                pushing(x, (K, r), MOST, (hat, REACH)),
                huber(r - x, K, MOST),
                "pressed {pressed}, {at} on"
            );
        }
    }

    #[test]
    fn its_push_is_its_energys_slope_falls_with_its_length_and_its_speed() {
        // Pressed 0.45 m, lengthening in the step.
        let hat = 0.55;
        let r = rest(1.0, hat);
        let energy = |x: f64| pushing(x, (K, r), MOST, (hat, REACH)).0;
        let mut last = f64::INFINITY;
        for i in 0..400 {
            let x = 0.5 + 0.0015 * f64::from(i);
            let (_, push, stiffness) = pushing(x, (K, r), MOST, (hat, REACH));
            let e = 1e-7;
            let slope = (energy(x - e) - energy(x + e)) / (2.0 * e);
            assert!(
                (slope - push).abs() < 1e-4 * MOST,
                "{x}: {slope} against {push}"
            );
            assert!(
                push <= last + 1e-9 && push >= 0.0,
                "{x}: {push} after {last}"
            );
            let line = MOST * (1.0 - (x - hat) / REACH);
            assert!(
                push <= line.max(0.0) + 1e-9,
                "{x}: {push} over the line {line}"
            );
            assert!(
                push <= huber(r - x, K, MOST).1 + 1e-9,
                "{x}: {push} over the spring"
            );
            assert!(stiffness >= 0.0);
            last = push;
        }
        // Lengthening at 3.5 m/s it pushes no more.
        assert_eq!(pushing(hat + REACH, (K, r), MOST, (hat, REACH)).1, 0.0);
    }
}
