//! Pronounceable names for generated bodies.

use super::rng::Rng;

const ONSETS: [&str; 24] = [
    "b", "d", "f", "g", "k", "l", "m", "n", "p", "r", "s", "t", "v", "z", "br", "dr", "kr", "tr",
    "st", "th", "sh", "gl", "vr", "",
];
const VOWELS: [&str; 10] = ["a", "e", "i", "o", "u", "ae", "io", "ou", "ei", "y"];
const CODAS: [&str; 10] = ["", "", "n", "r", "s", "th", "x", "l", "m", "k"];

fn pick<'a>(rng: &mut Rng, list: &[&'a str]) -> &'a str {
    let i = usize::try_from(rng.next_u64() % list.len() as u64).expect("small index");
    list[i]
}

/// A name of two or three syllables, capitalised.
pub fn name(rng: &mut Rng) -> String {
    let syllables = rng.count((2, 3));
    let mut text = String::new();
    for k in 0..syllables {
        text.push_str(pick(rng, &ONSETS));
        text.push_str(pick(rng, &VOWELS));
        if k + 1 == syllables {
            text.push_str(pick(rng, &CODAS));
        }
    }
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_capitalised_and_varied() {
        let mut rng = Rng::new(1, 0);
        let names: Vec<String> = (0..50).map(|_| name(&mut rng)).collect();
        for n in &names {
            assert!(n.len() >= 2, "{n}");
            assert!(n.chars().next().is_some_and(char::is_uppercase), "{n}");
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert!(unique.len() > 40);
    }
}
