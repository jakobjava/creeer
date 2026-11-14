//! What came of what the player did, where they are looking (G2.11 A3, ADR-0038): a ring round
//! the crosshair filling while work takes its time (fastening, backfilling, taking apart), and
//! the server's answer just above it in plain words, fading after a few seconds: why something
//! was refused, or what came of it. Nothing the player does is silent.

use creeer_ui::egui::{self, pos2};
use creeer_ui::paint::{self, Row};
use creeer_ui::theme::{TEXT, WARN};
use std::time::Instant;

/// How long an answer is shown (s), the last of it fading.
const SHOWN: f32 = 4.0;
const FADES: f32 = 1.0;
/// The work ring's radius (points).
const RING: f32 = 20.0;

/// Work under way, from the server's note ("backfilling: 8 s", "nailing (nail_75, 2 blows):
/// 1.0 s"): what is being done in a word (what is in brackets left out), and how far along it is
/// (0 to 1) `elapsed` seconds after the note came. None if the note is not of work, or the work
/// is done.
pub(crate) fn work(note: &str, elapsed: f32) -> Option<(String, f32)> {
    let (doing, seconds) = work_note(note)?;
    let done = elapsed / seconds.max(0.1);
    (done < 1.0).then(|| (capital(doing), done))
}

/// A note of work: what is being done, and how long it takes (s).
fn work_note(note: &str) -> Option<(&str, f32)> {
    let (doing, rest) = note.rsplit_once(": ")?;
    let seconds: f32 = rest.strip_suffix(" s")?.parse().ok()?;
    let doing = doing.split(" (").next().unwrap_or(doing);
    let words = doing.chars().all(|c| c.is_ascii_lowercase() || c == ' ');
    words.then_some((doing, seconds))
}

/// A fastener's id in words: `nail_75` is "a 75 mm nail", `peg_25` "a 25 mm peg"; another id
/// its words ("bast lashing").
pub(crate) fn fastener_words(id: &str) -> String {
    match id.rsplit_once('_') {
        Some((kind, mm)) if !mm.is_empty() && mm.chars().all(|c| c.is_ascii_digit()) => {
            format!("a {mm} mm {}", kind.replace('_', " "))
        }
        _ => id.replace('_', " "),
    }
}

/// A server's reason or note in plain words for the screen: the terse ones written out, every
/// one begun with a capital, ids' underscores as spaces.
pub(crate) fn plain(why: &str) -> String {
    const WORDS: &[(&str, &str)] = &[
        ("down", "You are down"),
        ("hands full", "Your hands are full"),
        ("taken", "Someone took it first"),
        ("not to be carried", "That cannot be carried"),
        ("someone holds it", "Someone else holds it"),
        ("too many commands", "Too fast: a moment"),
        ("a point that is not finite", "Nothing there"),
        ("not an item", "That cannot be put away"),
        ("nothing in hand", "Nothing in your hands"),
        ("no such kit item", "No such part in the kit"),
    ];
    if let Some((_, plain)) = WORDS.iter().find(|(w, _)| *w == why) {
        return (*plain).to_owned();
    }
    if why.starts_with("seen ") {
        return "It moved: aim again".to_owned();
    }
    if why.starts_with("beyond reach") || why.ends_with(": out of reach") {
        return "Too far".to_owned();
    }
    if let Some(kg) = why.strip_suffix(": too heavy to move") {
        return format!("Too heavy to move ({kg})");
    }
    // Ids in words: a fastener's (`nail_75`: "a 75 mm nail"), the rest's underscores as spaces.
    let words: Vec<String> = (why.split(' '))
        .map(|w| {
            let open = w.len() - w.trim_start_matches('(').len();
            let bare = w[open..].trim_end_matches([':', ',', ')', '.']);
            if bare.contains('_') {
                let rest = &w[open + bare.len()..];
                format!("{}{}{rest}", &w[..open], fastener_words(bare))
            } else {
                w.to_owned()
            }
        })
        .collect();
    capital(&words.join(" "))
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// The answer to show now: the later of the last refusal and the last note, if recent, with
/// whether it was a refusal and how faded (1 shown, 0 gone). A note of work is shown by its ring
/// while it lasts, and not as an answer after (its outcome is told when it is done).
pub(crate) fn answer(
    refused: Option<&(String, Instant)>,
    noted: Option<&(String, Instant)>,
) -> Option<(String, bool, f32)> {
    let last = match (refused, noted) {
        (Some(r), Some(n)) if n.1 > r.1 => (n, false),
        (Some(r), _) => (r, true),
        (None, Some(n)) => (n, false),
        (None, None) => return None,
    };
    let ((text, when), refusal) = last;
    if !refusal && work_note(text).is_some() {
        return None;
    }
    let age = when.elapsed().as_secs_f32();
    let shown = ((SHOWN - age) / FADES).clamp(0.0, 1.0);
    (shown > 0.0).then(|| (plain(text), refusal, shown))
}

/// Draws the work ring and the answer; a note of what failed that is told at the top already
/// (`failed`, a creative server's) not again (G2.11 Q: it was told three times at once).
pub(crate) fn draw(ctx: &egui::Context, c: &creeer_server::Connection, failed: &[String]) {
    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("feedback"),
    ));
    let centre = ctx.content_rect().center();
    let working = (c.noted.as_ref())
        .and_then(|(note, when)| work(note, when.elapsed().as_secs_f32()))
        .filter(|_| {
            c.refused
                .as_ref()
                .is_none_or(|r| Some(r.1) < c.noted.as_ref().map(|n| n.1))
        });
    let shown = match &working {
        Some((doing, done)) => {
            paint::ring(&painter, centre, RING, *done);
            Some((format!("{doing}…"), false, 1.0))
        }
        None => answer(c.refused.as_ref(), c.noted.as_ref())
            .filter(|(text, _, _)| !failed.iter().any(|f| plain(f) == *text)),
    };
    if let Some((text, refusal, opacity)) = shown {
        painter.multiply_opacity(opacity);
        let row = Row {
            key: None,
            text: &text,
            colour: if refusal { WARN } else { TEXT },
            size: 16.0,
        };
        paint::strip(&painter, pos2(centre.x, centre.y - RING - 14.0), &[row]);
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn work_notes_fill_the_ring_and_others_do_not() {
        let (doing, done) = work("fastening: 8 s", 2.0).expect("work");
        assert_eq!(doing, "Fastening");
        assert!((done - 0.25).abs() < 1e-6);
        let (doing, _) = work("finishing the lashing: 6 s", 1.0).expect("work");
        assert_eq!(doing, "Finishing the lashing");
        assert!(work("fastening: 8 s", 8.5).is_none(), "done");
        assert!(work("dug 0.20 kg (0.21 l)", 0.0).is_none());
        assert!(work("73 % through", 0.0).is_none());
        assert!(work("not fastened: it slipped", 0.0).is_none());
        // The server's note of a nail (G2.11 B: its time is its blows).
        let (doing, done) = work("nailing (nail_75, 2 blows): 1.0 s", 0.5).expect("work");
        assert_eq!(doing, "Nailing");
        assert!((done - 0.5).abs() < 1e-6);
        assert!(work("fastening (peg_25): 10.0 s", 1.0).is_some());
    }

    /// A note of work is the ring's: once done it is not shown in words (its outcome is), and
    /// an older refusal does not come back after it.
    #[test]
    fn a_work_note_is_not_shown_as_an_answer() {
        let now = Instant::now();
        let before = now.checked_sub(Duration::from_secs(1)).expect("before");
        let refused = ("hands full".to_owned(), before);
        let nailing = ("nailing (nail_75, 2 blows): 1.0 s".to_owned(), now);
        assert!(answer(Some(&refused), Some(&nailing)).is_none());
        let fastened = ("fastened with nail_75".to_owned(), now);
        let (text, refusal, _) = answer(Some(&refused), Some(&fastened)).expect("shown");
        assert_eq!(
            (text.as_str(), refusal),
            ("Fastened with a 75 mm nail", false)
        );
    }

    #[test]
    fn terse_reasons_are_written_out_and_the_rest_begin_with_a_capital() {
        assert_eq!(plain("down"), "You are down");
        assert_eq!(plain("seen 12 ticks ago"), "It moved: aim again");
        assert_eq!(plain("1.8 m away: out of reach"), "Too far");
        assert_eq!(plain("beyond reach (500 m)"), "Too far");
        assert_eq!(
            plain("200 kg: too heavy to move"),
            "Too heavy to move (200 kg)"
        );
        assert_eq!(
            plain("fastened with bast_lashing"),
            "Fastened with bast lashing"
        );
        assert_eq!(
            plain("fastened with nail_100"),
            "Fastened with a 100 mm nail"
        );
        assert_eq!(
            plain("tacked with bast_lashing: let go"),
            "Tacked with bast lashing: let go"
        );
        assert_eq!(plain(""), "");
    }

    #[test]
    fn the_later_answer_is_shown_and_fades_after_a_few_seconds() {
        let now = Instant::now();
        let old = now
            .checked_sub(Duration::from_secs(10))
            .expect("a while ago");
        let refused = ("hands full".to_owned(), now);
        let noted = ("dug 0.2 kg".to_owned(), old);
        let (text, refusal, shown) = answer(Some(&refused), Some(&noted)).expect("shown");
        assert_eq!((text.as_str(), refusal), ("Your hands are full", true));
        assert!(shown > 0.99);
        let gone = ("hands full".to_owned(), old);
        assert!(answer(Some(&gone), None).is_none());
        let then = now.checked_sub(Duration::from_millis(3500)).expect("then");
        let fading = ("cut through".to_owned(), then);
        let (_, _, shown) = answer(None, Some(&fading)).expect("still shown");
        assert!((0.3..0.7).contains(&shown), "{shown}");
    }
}
