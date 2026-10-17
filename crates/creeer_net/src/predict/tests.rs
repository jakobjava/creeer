//! A predicted avatar is where the server will put it, whatever the latency and losses; a
//! correction slides; the server's queue copes with repeats, disorder and a fast client.

use super::*;

const DT: f64 = 1.0 / 60.0;

/// A toy avatar: it pulls its velocity towards the walk asked for and jumps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Toy {
    at: DVec3,
    v: DVec3,
}

fn step(s: &mut Toy, input: &AvatarInput) {
    s.v += (input.walk - s.v) * 0.2;
    if input.jump {
        s.v.y += 4.0;
    }
    s.v.y -= 9.81 * DT;
    s.at += s.v * DT;
    if s.at.y < 0.0 {
        s.at.y = 0.0;
        s.v.y = 0.0;
    }
}

/// A deterministic stream of numbers in [0, 1).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_precision_loss)]
        let u = (self.0 >> 11) as f64 / (1u64 << 53) as f64;
        u
    }
}

/// Messages on their way: what, and the tick it arrives.
struct Pipe<T> {
    flying: Vec<(u32, T)>,
    latency: u32,
    jitter: u32,
    loss: f64,
    rng: Lcg,
}

impl<T> Pipe<T> {
    fn new(latency: u32, jitter: u32, loss: f64, seed: u64) -> Self {
        Self {
            flying: Vec::new(),
            latency,
            jitter,
            loss,
            rng: Lcg(seed),
        }
    }

    fn send(&mut self, now: u32, message: T) {
        if self.rng.next() < self.loss {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let late = (self.rng.next() * f64::from(self.jitter + 1)) as u32;
        self.flying.push((now + self.latency + late, message));
    }

    fn arrived(&mut self, now: u32) -> Vec<T> {
        let (here, flying) = std::mem::take(&mut self.flying)
            .into_iter()
            .partition(|m| m.0 <= now);
        self.flying = flying;
        here.into_iter().map(|m| m.1).collect()
    }
}

/// Plays a client against a server for `ticks`: the fraction of the client's predictions
/// (after it first heard from the server) that are exactly the server's state after the
/// same inputs, and the largest correction after the first second (m).
fn play(up: Pipe<Vec<AvatarInput>>, down: Pipe<(u32, Toy, u32)>, ticks: u32) -> (f64, f64) {
    let (mut up, mut down) = (up, down);
    let (mut log, mut queue) = (InputLog::default(), InputQueue::default());
    let (mut predicted, mut server) = (Toy::default(), Toy::default());
    // The server's state just after it applied each input; the client's after each.
    let mut after = std::collections::HashMap::new();
    let mut predictions = Vec::new();
    let (mut heard, mut worst) = (None, 0.0f64);
    let mut rng = Lcg(99);
    for now in 0..ticks {
        // The client: a new input, applied at once and sent with the last few.
        let walk = DVec3::new(
            6.0 * creeer_math::det::sin(f64::from(now) * 0.05),
            0.0,
            3.0 * (rng.next() - 0.5),
        );
        let input = log.push(walk, now % 50 == 7);
        step(&mut predicted, &input);
        up.send(now, log.to_send());
        // The server: what came, one input applied, a snapshot.
        for inputs in up.arrived(now) {
            queue.receive(&inputs);
        }
        let input = queue.take();
        step(&mut server, &input);
        after.entry(queue.last()).or_insert(server);
        down.send(now, (now, server, queue.last()));
        // The client: the newest snapshot, if newer than any before, reconciled.
        let newest = down.arrived(now).into_iter().max_by_key(|s| s.0);
        if let Some((tick, state, last)) = newest.filter(|s| heard.is_none_or(|h| s.0 > h)) {
            heard = Some(tick);
            let before = predicted;
            predicted = reconcile(state, last, &mut log, step);
            if now > 60 {
                worst = worst.max((before.at - predicted.at).length());
            }
        }
        if heard.is_some() {
            predictions.push((now + 1, predicted)); // one input a tick, from 1
        }
    }
    let checked: Vec<bool> = predictions
        .iter()
        .filter_map(|(seq, p)| after.get(seq).map(|truth| truth == p))
        .collect();
    assert!(
        checked.len() > ticks as usize / 2,
        "{} checked",
        checked.len()
    );
    #[allow(clippy::cast_precision_loss)]
    let exact = checked.iter().filter(|&&e| e).count() as f64 / checked.len() as f64;
    (exact, worst)
}

#[test]
fn on_a_steady_link_the_prediction_is_exactly_the_server() {
    // 100 ms each way.
    let (exact, worst) = play(Pipe::new(6, 0, 0.0, 1), Pipe::new(6, 0, 0.0, 2), 3000);
    assert!((exact - 1.0).abs() < f64::EPSILON, "{exact} exact");
    assert!(worst < 1e-12, "corrected by {worst} m");
}

#[test]
fn on_a_lossy_jittery_link_the_prediction_is_rarely_corrected() {
    // 80–130 ms each way, one packet in ten lost.
    let (exact, worst) = play(Pipe::new(5, 3, 0.1, 3), Pipe::new(5, 3, 0.1, 4), 6000);
    assert!(exact > 0.98, "{exact} exact");
    assert!(worst < 0.1, "corrected by {worst} m");
}

#[test]
fn the_queue_ignores_repeats_sorts_late_ones_and_holds_back_a_fast_client() {
    let mut log = InputLog::default();
    let inputs: Vec<AvatarInput> = (0..30)
        .map(|i| log.push(DVec3::X * f64::from(i), false))
        .collect();
    let mut queue = InputQueue::default();
    queue.receive(&[inputs[1]]);
    queue.receive(&[inputs[0], inputs[1]]);
    // Buffered: two had, the first applied.
    assert_eq!(queue.take().seq, 1);
    assert_eq!(queue.take().seq, 2);
    // None came: the last again, held, until the buffer (now three) fills again.
    let held = queue.take();
    assert_eq!((held.seq, held.walk), (2, inputs[1].walk));
    assert_eq!(queue.target(), BUFFER + 1);
    queue.receive(&inputs[2..4]);
    assert_eq!(queue.take().seq, 2);
    queue.receive(&inputs[4..5]);
    assert_eq!(queue.take().seq, 3);
    assert_eq!(queue.last(), 3);
    // Far ahead: only the newest few kept.
    queue.receive(&inputs[5..30]);
    let first_kept = 30 - u32::try_from(QUEUED).unwrap() + 1;
    assert_eq!(queue.take().seq, first_kept);
    // Old ones again: ignored.
    queue.receive(&inputs[0..25]);
    assert_eq!(queue.take().seq, first_kept + 1);
}

#[test]
fn the_buffer_shrinks_again_on_a_link_that_got_better() {
    let mut log = InputLog::default();
    let mut queue = InputQueue::default();
    // A burst of lateness: the buffer grows to its most.
    for _ in 0..MOST_BUFFER {
        let late: Vec<AvatarInput> = (0..8).map(|_| log.push(DVec3::X, false)).collect();
        queue.receive(&late);
        for _ in 0..12 {
            queue.take();
        }
    }
    assert_eq!(queue.target(), MOST_BUFFER);
    // Then a steady link, one input a tick: it settles back.
    for _ in 0..u32::try_from(MOST_BUFFER).unwrap() * SETTLE + 10 {
        queue.receive(&[log.push(DVec3::X, false)]);
        queue.take();
    }
    assert!(queue.target() <= BUFFER + 1, "{}", queue.target());
}

#[test]
fn a_correction_slides_away_and_a_teleport_snaps() {
    let mut smoothing = Smoothing::default();
    smoothing.corrected(DVec3::new(0.3, 0.0, 0.0), DVec3::ZERO);
    let mut offset = DVec3::ZERO;
    for _ in 0..60 {
        offset = smoothing.advance(DT);
    }
    // e^(−10) after a second.
    assert!(
        (offset.x - 0.3 * creeer_math::det::exp(-10.0)).abs() < 1e-9,
        "{offset}"
    );
    smoothing.corrected(DVec3::new(100.0, 0.0, 0.0), DVec3::ZERO);
    assert_eq!(smoothing.advance(DT), DVec3::ZERO);
}

#[test]
fn an_input_is_predicted_with_what_the_server_is_told() {
    let input = AvatarInput {
        seq: 3,
        walk: DVec3::new(1.234_567_89, 0.0, -2.0 / 3.0),
        jump: true,
    };
    let sent: AvatarInput =
        crate::message::from_bytes(&crate::message::to_bytes(&input.quantised())).unwrap();
    assert_eq!(sent, input.quantised());
    assert_eq!(sent.quantised(), sent);
}
