//! The profiler: what it records, and when not.

use super::profile::{self, Frame, Span};

fn busy(micros: u64) {
    let until = std::time::Instant::now() + std::time::Duration::from_micros(micros);
    while std::time::Instant::now() < until {}
}

/// One test for the whole process-wide profiler (tests run in parallel; its state is
/// global).
#[test]
fn records_scopes_of_every_thread_into_frames_and_nothing_when_off() {
    let mut clock = profile::clock(1).expect("the clock is free");
    assert!(profile::clock(1).is_none(), "one holder at a time");
    let mut seen = 0;
    let _ = profile::frames_since(&mut seen);
    // Off: nothing recorded.
    {
        let _s = profile::scope("off");
        busy(50);
    }
    clock.end_tick(1);
    assert!(profile::frames().iter().all(|f| f.tick != 1));

    let want = profile::want();
    {
        let _outer = profile::scope("outer");
        busy(200);
        {
            let _inner = profile::scope("inner");
            busy(100);
        }
        std::thread::Builder::new()
            .name("helper".into())
            .spawn(|| {
                let _s = profile::scope("on a thread");
                busy(100);
            })
            .expect("spawn")
            .join()
            .expect("join");
    }
    clock.end_tick(2);
    let frame = profile::frames_since(&mut seen).pop().expect("a frame");
    assert_eq!(frame.tick, 2);
    let named = |n: &str| {
        let i = frame.names.iter().position(|x| x == n).expect(n);
        frame
            .spans
            .iter()
            .find(|s| usize::from(s.name) == i)
            .copied()
            .expect(n)
    };
    let (outer, inner, other) = (named("outer"), named("inner"), named("on a thread"));
    assert_eq!((outer.depth, inner.depth), (0, 1));
    assert!(outer.start <= inner.start && inner.end <= outer.end);
    assert!(inner.end - inner.start >= 100_000);
    assert_eq!(frame.threads[usize::from(other.thread)], "helper");
    assert_ne!(other.thread, outer.thread);
    assert!(frame.duration >= outer.end - outer.start);

    drop(want);
    {
        let _s = profile::scope("after");
    }
    clock.end_tick(3);
    assert!(profile::frames_since(&mut seen).is_empty(), "off again");

    // A higher priority takes the clock: the lower one cuts no more frames.
    let want = profile::want();
    let mut server = profile::clock(2).expect("a server takes it");
    assert!(!clock.holds() && server.holds());
    clock.end_tick(4);
    server.end_tick(5);
    let ticks: Vec<u64> = profile::frames_since(&mut seen)
        .iter()
        .map(|f| f.tick)
        .collect();
    assert_eq!(ticks, [5]);
    drop(clock);
    assert!(server.holds(), "the lower one's drop leaves it");
    drop((server, want));
    assert!(profile::clock(1).is_some(), "free again");
}

#[test]
fn totals_count_a_name_inside_itself_once() {
    let span = |name, depth, start, end| Span {
        name,
        thread: 0,
        depth,
        start,
        end,
    };
    let frame = Frame {
        tick: 1,
        duration: 100,
        names: vec!["step".into(), "solve".into()],
        threads: vec!["main".into()],
        spans: vec![
            span(1, 2, 20, 30),
            span(0, 1, 10, 40),
            span(0, 0, 0, 90),
            span(1, 1, 50, 60),
        ],
    };
    assert_eq!(frame.totals(), vec![("step", 90), ("solve", 20)]);
}

#[test]
fn a_frames_longest_spans_keep_their_order_names_and_threads() {
    let span = |name, thread, start, end| Span {
        name,
        thread,
        depth: 0,
        start,
        end,
    };
    let frame = Frame {
        tick: 3,
        duration: 100,
        names: vec!["a".into(), "b".into(), "c".into()],
        threads: vec!["main".into(), "worker".into()],
        spans: vec![
            span(0, 0, 0, 5),
            span(2, 1, 0, 60),
            span(1, 0, 10, 20),
            span(2, 0, 30, 80),
        ],
    };
    let short = frame.longest(2);
    assert_eq!((short.tick, short.duration), (3, 100));
    assert_eq!(short.names, ["c"]);
    assert_eq!(short.threads, ["worker", "main"]);
    assert_eq!(short.spans, [span(0, 0, 0, 60), span(0, 1, 30, 80)]);
    assert_eq!(frame.longest(10).totals(), frame.totals());
}
