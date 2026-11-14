//! What came of what the player did, where they are looking (G2.11 A3, ADR-0037): a ring round
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

/// Work under way, from the server's note ("fastening: 8 s"): what is being done, and how far
/// along it is (0 to 1) `elapsed` seconds after the note came. None if the note is not of work,
/// or the work is done.
pub(crate) fn work(note: &str, elapsed: f32) -> Option<(String, f32)> {
    let (doing, rest) = note.split_once(": ")?;
    let seconds: f32 = rest.strip_suffix(" s")?.parse().ok()?;
    let words = doing.chars().all(|c| c.is_ascii_lowercase() || c == ' ');
    let done = elapsed / seconds.max(0.1);
    (words && done < 1.0).then(|| (capital(doing), done))
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
    capital(&why.replace('_', " "))
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// The answer to show now: the later of the last refusal and the last note, if recent, with
/// whether it was a refusal and how faded (1 shown, 0 gone).
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
    let age = when.elapsed().as_secs_f32();
    let shown = ((SHOWN - age) / FADES).clamp(0.0, 1.0);
    (shown > 0.0).then(|| (plain(text), refusal, shown))
}

/// Draws the work ring and the answer.
pub(crate) fn draw(ctx: &egui::Context, c: &creeer_server::Connection) {
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
        None => answer(c.refused.as_ref(), c.noted.as_ref()),
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
