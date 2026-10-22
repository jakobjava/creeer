//! Players' looks (G1.3e): a colour of their own, the starting outfit, their chosen skin tone
//! and hair colour kept and told again.

use super::looks::Drawn;
use super::tests::world;
use creeer_net::message::Command;
use creeer_sim::character::bones::STARTING_OUTFIT;

#[test]
fn each_player_gets_a_colour_of_their_own_in_the_starting_outfit() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    let bob = w.avatar_of("bob");
    let (look_a, look_b) = (w.look_of("ada"), w.look_of("bob"));
    assert_eq!((look_a.colour, look_b.colour), (0, 1));
    assert_eq!(look_a.outfit, STARTING_OUTFIT);
    // The same name, the same look; skin and hair from the name until chosen.
    assert_eq!(w.look_of("ada"), look_a);
    assert!(look_a.skin < 10 && look_a.hair < 9);
    let Drawn::Person { look, fallen, .. } = w.drawn(w.index(bob).unwrap()) else {
        panic!("bob is drawn as a person");
    };
    assert_eq!((look, fallen), (look_b, false));
    // Ada chooses: kept, her colour with it, and whoever has her told again.
    w.take_changed();
    w.apply(&Command::Look { skin: 7, hair: 2 }, Some(ada))
        .expect("a player chooses their look");
    let chosen = w.look_of("ada");
    assert_eq!((chosen.colour, chosen.skin, chosen.hair), (0, 7, 2));
    assert!(w.take_changed().contains(&ada));
    // Only a player has one.
    assert!(w.apply(&Command::Look { skin: 1, hair: 1 }, None).is_err());
}
