//! The powerhouse in full against abstract: what it made, when its tank ran dry, its books;
//! coming back where it was.

use crate::abstraction::{Abstract, Machine, Mode};
use crate::powerhouse::{DT, Powerhouse, SPEED};
use creeer_rates::Event;

/// A tank of 3 m² and 5 m (it runs dry in about 22 minutes, its masses still warming).
fn house() -> Powerhouse {
    Powerhouse::with_tank(3.0, 5.0)
}

fn ticks(seconds: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / DT).round() as u64;
    n
}

/// `make()`'s machine run in full for `span` seconds: its stocks then, when its tank ran dry.
fn full(make: fn() -> Powerhouse, span: f64) -> (Vec<f64>, Option<f64>) {
    let mut h = make();
    let mut dry = None;
    for k in 0..ticks(span) {
        h.tick();
        if dry.is_none() && h.amounts()[0] <= 1e-9 {
            #[allow(clippy::cast_precision_loss)]
            let t = (k + 1) as f64 * DT;
            dry = Some(t);
        }
    }
    (h.amounts(), dry)
}

/// The same machine in full for `warm` seconds, then abstract to `span`.
fn abstracted(
    make: fn() -> Powerhouse,
    span: f64,
    warm: f64,
) -> (Abstract<Powerhouse>, Option<f64>) {
    let mut a = Abstract::new(make());
    for _ in 0..ticks(warm) {
        a.tick();
    }
    a.leave();
    a.advance(span);
    let dry = a.events.iter().find_map(|e| match *e {
        Event::Empty { stock, time } if stock.0 == 0 => Some(time),
        _ => None,
    });
    (a, dry)
}

/// Full against abstract over `span` seconds (abstract after `warm`): each stock within
/// `tolerance` (water to rounding), the tank dry within 1 % of the time; the probes' cost.
fn compare(make: fn() -> Powerhouse, span: f64, warm: f64, tolerance: f64) -> f64 {
    let (whole, dry) = full(make, span);
    let (a, empty) = abstracted(make, span, warm);
    assert_eq!(a.mode(), Mode::Abstract);
    let got = a.amounts();
    let names: Vec<String> = a.machine.stocks().into_iter().map(|s| s.name).collect();
    for (k, name) in names.iter().enumerate() {
        let scale = whole[k].abs().max(if k < 2 { 1.0 } else { 1e-9 });
        let error = (got[k] - whole[k]).abs() / scale;
        eprintln!(
            "{name:24} full {:13.6e}  abstract {:13.6e}  error {error:.2e}",
            whole[k], got[k]
        );
        let allowed = if k < 2 { 1e-9 } else { tolerance };
        assert!(error <= allowed, "{name}: {} against {}", got[k], whole[k]);
        // The abstract books close (each stock its start, plus in, less out).
        let (into, out, off) = a.books(k);
        assert!(
            off.abs() <= 1e-12 * (into + out + got[k].abs()).max(1.0),
            "{name}: books {off}"
        );
    }
    let (dry, empty) = (
        dry.expect("it runs dry"),
        empty.expect("it runs dry abstract"),
    );
    assert!(
        (empty - dry).abs() <= 0.01 * dry,
        "dry at {empty} s against {dry} s"
    );
    #[allow(clippy::cast_precision_loss)]
    let cost = a.probe_ticks as f64 / ticks(span) as f64;
    eprintln!(
        "dry at {empty:.1} s against {dry:.1} s; {} probes, {:.1} % of the full ticks",
        a.probes,
        cost * 100.0
    );
    cost
}

/// A small tank run dry while its masses warm (the hardest case: every rate changes all the
/// time): what it made within 1 % (most within 0.5 %), dry within 1 % of the time.
#[test]
fn abstract_the_powerhouse_makes_what_it_makes_in_full() {
    let cost = compare(house, 2400.0, 120.0, 0.01);
    assert!(cost < 0.35, "probes cost {cost}");
}

/// The full-size tank over four and a half hours (dry after 3.7 h): within 1 %, the probes
/// about 15 % of the full ticks (its masses warm for hours: a probe every few minutes).
#[test]
#[ignore = "four and a half hours of the powerhouse in full (30 s in release)"]
fn abstract_the_full_size_powerhouse_makes_what_it_makes_in_full() {
    let cost = compare(Powerhouse::new, 4.5 * 3600.0, 600.0, 0.01);
    assert!(cost < 0.2, "probes cost {cost}");
}

/// Back after a stretch abstract (the tank still half full): its stocks are the abstract
/// ones, its shaft at its speed within 0.5 % at once, and a minute later it is where the
/// machine run in full all along is.
#[test]
fn coming_back_the_powerhouse_runs_on_where_it_would_be() {
    let (mut a, _) = abstracted(house, 900.0, 120.0);
    let before = a.amounts();
    a.come_back();
    assert_eq!(a.mode(), Mode::Active);
    // (A tank's volume through its head and back, to rounding.)
    for (x, y) in a.amounts().iter().zip(&before) {
        assert!((x - y).abs() <= 1e-12 * y.abs().max(1.0), "{x} against {y}");
    }
    let mut worst = 0.0f64;
    for _ in 0..ticks(60.0) {
        a.tick();
        worst = worst.max((a.machine.readings().speed - SPEED).abs() / SPEED);
    }
    assert!(worst < 0.005, "the shaft {worst} off its speed");
    let (whole, _) = full(house, 960.0);
    let now = a.amounts();
    for k in 0..now.len() {
        let error = (now[k] - whole[k]).abs() / whole[k].abs().max(1e-9);
        assert!(error < 0.01, "stock {k}: {} against {}", now[k], whole[k]);
    }
}

/// Writing a machine's own amounts back into it changes nothing it does, bit for bit.
#[test]
fn writing_its_own_amounts_back_changes_nothing() {
    let (mut plain, mut written) = (house(), house());
    for k in 0..6000 {
        plain.tick();
        written.tick();
        if k % 600 == 0 {
            let a = written.amounts();
            written.set_amounts(&a);
        }
    }
    let (p, w) = (plain.amounts(), written.amounts());
    for k in 0..p.len() {
        assert!(
            p[k].to_bits() == w[k].to_bits(),
            "{k}: {} against {}",
            w[k],
            p[k]
        );
    }
}

/// Leaving from its own last windows run in full (as the world leaves a machine nobody is
/// near): no probe then, and what it makes is still what it makes in full.
#[test]
fn leaving_from_its_own_windows_needs_no_probe() {
    let mut a = Abstract::new(house());
    let mut waited = 0;
    loop {
        a.tick();
        waited += 1;
        if waited >= ticks(120.0) && a.try_leave() {
            break;
        }
        assert!(waited < ticks(1200.0), "never steady");
    }
    assert_eq!((a.mode(), a.probes), (Mode::Abstract, 0));
    let left = a.time();
    a.advance(2400.0);
    let (whole, _) = full(house, 2400.0);
    let got = a.amounts();
    for k in 0..got.len() {
        let error = (got[k] - whole[k]).abs() / whole[k].abs().max(if k < 2 { 1.0 } else { 1e-9 });
        assert!(
            error < 0.01,
            "stock {k}: {} against {} (left at {left} s)",
            got[k],
            whole[k]
        );
    }
}

/// Saved and restored (built anew, its stocks written in), an abstract machine goes on as it
/// was: the same stocks to its stretch's end, and within 1 % of the full run after it.
#[test]
fn an_abstract_machine_saved_and_restored_goes_on() {
    let (mut a, _) = abstracted(house, 600.0, 120.0);
    let snapshot = a.snapshot();
    let mut back = Abstract::restore(house(), &snapshot);
    assert_eq!(back.snapshot(), snapshot);
    let end = snapshot.stretch.as_ref().expect("a stretch").ends;
    a.advance(end - 1.0);
    back.advance(end - 1.0);
    assert_eq!(a.amounts(), back.amounts());
    back.advance(1500.0);
    a.advance(1500.0);
    let (whole, _) = full(house, 1500.0);
    let got = back.amounts();
    for k in 0..got.len() {
        let error = (got[k] - whole[k]).abs() / whole[k].abs().max(if k < 2 { 1.0 } else { 1e-9 });
        assert!(error < 0.01, "stock {k}: {} against {}", got[k], whole[k]);
    }
}

/// The abstract machine against the full one every 150 s (a diagnostic: `--ignored`).
#[test]
#[ignore = "diagnostic: prints the errors along the way"]
fn errors_along_the_way() {
    let mut full = house();
    let mut a = Abstract::new(house());
    for _ in 0..ticks(120.0) {
        full.tick();
        a.tick();
    }
    a.leave();
    let mut t = 120.0;
    for k in 1..=15 {
        let until = 120.0 + 150.0 * f64::from(k);
        while t < until - 1e-9 {
            full.tick();
            t += DT;
        }
        a.advance(until);
        let (f, b) = (full.amounts(), a.amounts());
        let e: Vec<String> = (2..10)
            .map(|i| format!("{:+.1e}", b[i] / f[i] - 1.0))
            .collect();
        eprintln!("t {until:.0}: {}", e.join(" "));
    }
}

#[test]
#[ignore = "diagnostic: when a powerhouse from rest is first steady"]
fn when_is_it_steady() {
    let mut a = Abstract::new(Powerhouse::new());
    for k in 0..ticks(1200.0) {
        a.tick();
        if k % 600 == 0 && a.try_leave() {
            eprintln!("steady after {} s", a.time());
            return;
        }
    }
    eprintln!("not steady in 1200 s");
}
