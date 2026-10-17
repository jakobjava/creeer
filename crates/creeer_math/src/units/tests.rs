use super::*;

fn si(text: &str) -> (f64, Dimension) {
    let q = parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    (q.value, q.dimension)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs())
}

#[test]
fn plain_numbers_are_dimensionless() {
    assert_eq!(si("42"), (42.0, Dimension::NONE));
    assert_eq!(si(" -3.5e2 "), (-350.0, Dimension::NONE));
    assert_eq!(si("+.5"), (0.5, Dimension::NONE));
}

#[test]
fn base_and_prefixed_units() {
    assert_eq!(si("500 km"), (500_000.0, Dimension::LENGTH));
    assert_eq!(si("3 mm").1, Dimension::LENGTH);
    assert!(close(si("3 mm").0, 0.003));
    assert_eq!(si("2 kg"), (2.0, Dimension::MASS));
    assert_eq!(si("5 t"), (5000.0, Dimension::MASS));
    assert!(close(si("250 µs").0, 250e-6));
    assert!(close(si("250 us").0, 250e-6));
    assert_eq!(si("1 hPa"), (100.0, Dimension::PRESSURE));
    assert_eq!(si("1 dam"), (10.0, Dimension::LENGTH));
    assert_eq!(si("1 MJ"), (1e6, Dimension::ENERGY));
}

#[test]
fn whole_symbols_win_over_prefix_readings() {
    // "min" is minutes, not milli-inches; "mol" is not milli-"ol"; "Pa" is not peta-"a".
    assert_eq!(si("2 min"), (120.0, Dimension::TIME));
    assert_eq!(si("1 mol"), (1.0, Dimension::AMOUNT));
    assert_eq!(si("1 Pa"), (1.0, Dimension::PRESSURE));
    assert_eq!(si("6 h"), (21_600.0, Dimension::TIME));
    assert_eq!(si("1 d"), (86_400.0, Dimension::TIME));
    assert_eq!(si("1 cd"), (1.0, Dimension::LUMINOSITY));
}

#[test]
fn units_without_prefixes_reject_them() {
    for text in ["1 kh", "1 kmin", "1 katm", "1 kdeg"] {
        assert!(
            matches!(parse(text), Err(UnitError::UnknownUnit(_))),
            "{text}"
        );
    }
}

#[test]
fn compound_units() {
    let (v, d) = si("7850 kg/m^3");
    assert_eq!((v, d), (7850.0, Dimension::DENSITY));
    assert_eq!(si("9.81 m/s^2"), (9.81, Dimension::ACCELERATION));
    assert_eq!(si("9.81 m·s^-2"), (9.81, Dimension::ACCELERATION));
    assert_eq!(si("9.81 m s**-2"), (9.81, Dimension::ACCELERATION));
    assert_eq!(si("9.81 m/s²"), (9.81, Dimension::ACCELERATION));
    assert_eq!(si("9.81 m·s⁻²"), (9.81, Dimension::ACCELERATION));
    assert_eq!(si("200 GPa"), (2e11, Dimension::PRESSURE));
    assert_eq!(si("1 N*m"), (1.0, Dimension::ENERGY));
    assert_eq!(si("1 kg*m^2/s^2"), (1.0, Dimension::ENERGY));
}

#[test]
fn everything_after_a_slash_divides_until_the_next_one() {
    let conductivity = Dimension::POWER.div(Dimension::LENGTH.mul(Dimension::TEMPERATURE));
    assert_eq!(si("50 W/m·K"), (50.0, conductivity));
    assert_eq!(si("50 W/(m·K)"), (50.0, conductivity));
    let heat_capacity = Dimension::ENERGY.div(Dimension::MASS.mul(Dimension::TEMPERATURE));
    assert_eq!(si("450 J/kg/K"), (450.0, heat_capacity));
    assert_eq!(si("450 J/(kg K)"), (450.0, heat_capacity));
    assert_eq!(si("450 J/(kg*K)"), (450.0, heat_capacity));
}

#[test]
fn gravitational_parameter_and_rates() {
    let (v, d) = si("3.986e14 m^3/s^2");
    assert_eq!(d, Dimension::VOLUME.div(Dimension::TIME.pow(2)));
    assert!(close(v, 3.986e14));
    assert_eq!(si("50 Hz"), (50.0, Dimension::FREQUENCY));
    assert_eq!(si("3 s^-1").1, Dimension::FREQUENCY);
    assert!(close(si("72 km/h").0, 20.0));
}

#[test]
fn angles_temperatures_and_percentages() {
    assert!(close(si("180 deg").0, std::f64::consts::PI));
    assert!(close(si("90 °").0, std::f64::consts::FRAC_PI_2));
    assert_eq!(si("23.5 °").1, Dimension::NONE);
    assert_eq!(si("25 %"), (0.25, Dimension::NONE));
    assert_eq!(si("300 K"), (300.0, Dimension::TEMPERATURE));
    let (t, d) = si("25 °C");
    assert_eq!(d, Dimension::TEMPERATURE);
    assert!(close(t, 298.15));
    assert!(close(si("-273.15 degC").0, 0.0));
}

#[test]
fn electrical_units_are_consistent() {
    // V = W/A, Ω = V/A, C = A·s.
    assert_eq!(si("1 V").1, Dimension::POWER.div(Dimension::CURRENT));
    assert_eq!(si("1 Ω").1, si("1 V/A").1);
    assert_eq!(si("1 ohm").1, si("1 V/A").1);
    assert_eq!(si("1 C").1, si("1 A·s").1);
    assert!(close(si("1 eV").0, 1.602_176_634e-19));
    assert!(close(si("1 keV").0, 1.602_176_634e-16));
    // F = C/V, H = V·s/A, S = 1/Ω, T = Wb/m²; Wh and Ah for batteries.
    assert_eq!(si("1 F").1, si("1 C/V").1);
    assert_eq!(si("1 H").1, si("1 V·s/A").1);
    assert_eq!(si("1 S").1, si("1 A/V").1);
    assert_eq!(si("1 T").1, si("1 Wb/m^2").1);
    assert!(close(si("470 µF").0, 4.7e-4) && close(si("10 mH").0, 0.01));
    assert!(close(si("2.5 Ah").0, 9000.0) && close(si("3200 mAh").0, 11_520.0));
    assert!(close(si("1 kWh").0, 3.6e6) && si("1 kWh").1 == Dimension::ENERGY);
    // Whole symbols win: T is tesla, TW terawatt, ms a millisecond, mS a millisiemens.
    assert!(close(si("1 TW").0, 1e12) && si("1 TW").1 == Dimension::POWER);
    assert!(close(si("2 ms").0, 2e-3) && si("2 mS").1 == Dimension::CONDUCTANCE);
}

#[test]
fn parse_as_checks_the_dimension() {
    assert_eq!(parse_as("6 h", Dimension::TIME), Ok(21_600.0));
    let err = parse_as("5 kg", Dimension::LENGTH).unwrap_err();
    assert!(matches!(err, UnitError::WrongDimension { .. }));
    let message = err.to_string();
    assert!(
        message.contains("kg") && message.contains("expected m"),
        "{message}"
    );
}

#[test]
fn malformed_input_is_rejected() {
    assert!(matches!(parse("km"), Err(UnitError::BadNumber(_))));
    assert!(matches!(parse(""), Err(UnitError::BadNumber(_))));
    assert!(matches!(parse("5 furlong"), Err(UnitError::UnknownUnit(_))));
    for text in [
        "5 m/", "5 (m", "5 m)", "5 m^", "5 m^x", "5 m//s", "5 *m", "5 m⁻",
    ] {
        assert!(matches!(parse(text), Err(UnitError::Syntax(_))), "{text}");
    }
}

#[test]
fn dimension_display() {
    assert_eq!(Dimension::DENSITY.to_string(), "m^-3·kg");
    assert_eq!(Dimension::LENGTH.to_string(), "m");
    assert_eq!(Dimension::NONE.to_string(), "1 (dimensionless)");
}
