//! Unit expressions: the parser and the table of known units and prefixes.

use super::{Dimension, UnitError};

/// SI factor and dimension of a whole unit expression such as `kg/m^3`.
pub(super) fn unit_expression(text: &str) -> Result<(f64, Dimension), UnitError> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        text,
    };
    let result = parser.product()?;
    if parser.at == parser.chars.len() {
        Ok(result)
    } else {
        Err(parser.syntax())
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    at: usize,
    text: &'a str,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn syntax(&self) -> UnitError {
        UnitError::Syntax(self.text.into())
    }

    fn skip_spaces(&mut self) -> bool {
        let start = self.at;
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
        self.at > start
    }

    /// factor (sep factor)*, where everything after a '/' up to the next '/' divides.
    fn product(&mut self) -> Result<(f64, Dimension), UnitError> {
        let (mut value, mut dim) = (1.0, Dimension::NONE);
        let mut dividing = false;
        loop {
            self.skip_spaces();
            let (f, d) = self.factor()?;
            if dividing {
                value /= f;
                dim = dim.div(d);
            } else {
                value *= f;
                dim = dim.mul(d);
            }
            let spaced = self.skip_spaces();
            match self.peek() {
                Some('*' | '·') => self.at += 1,
                Some('/') => {
                    self.at += 1;
                    dividing = true;
                }
                Some(')') | None => return Ok((value, dim)),
                Some(_) if spaced => {}
                Some(_) => return Err(self.syntax()),
            }
        }
    }

    /// atom [power]
    fn factor(&mut self) -> Result<(f64, Dimension), UnitError> {
        let (value, dim) = if self.peek() == Some('(') {
            self.at += 1;
            let inner = self.product()?;
            if self.peek() != Some(')') {
                return Err(self.syntax());
            }
            self.at += 1;
            inner
        } else {
            let start = self.at;
            while self
                .peek()
                .is_some_and(|c| c.is_alphabetic() || matches!(c, 'µ' | 'Ω' | '°' | '%'))
            {
                self.at += 1;
            }
            if self.at == start {
                return Err(self.syntax());
            }
            let symbol: String = self.chars[start..self.at].iter().collect();
            lookup(&symbol).ok_or(UnitError::UnknownUnit(symbol))?
        };
        let power = self.power()?;
        Ok((integer_power(value, power), dim.pow(power)))
    }

    /// `^n`, `**n` or a superscript; 1 if absent.
    fn power(&mut self) -> Result<i8, UnitError> {
        const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
        let mut negative = false;
        let mut digits = String::new();
        if self.peek() == Some('^') || self.chars[self.at..].starts_with(&['*', '*']) {
            self.at += if self.peek() == Some('^') { 1 } else { 2 };
            if self.peek() == Some('-') {
                negative = true;
                self.at += 1;
            }
            while let Some(c) = self.peek().filter(char::is_ascii_digit) {
                digits.push(c);
                self.at += 1;
            }
        } else {
            if self.peek() == Some('⁻') {
                negative = true;
                self.at += 1;
            }
            while let Some(i) = self
                .peek()
                .and_then(|c| SUPERSCRIPTS.iter().position(|&s| s == c))
            {
                digits.push(char::from(b'0' + u8::try_from(i).expect("< 10")));
                self.at += 1;
            }
            if digits.is_empty() {
                return if negative { Err(self.syntax()) } else { Ok(1) };
            }
        }
        let n: i8 = digits.parse().map_err(|_| self.syntax())?;
        Ok(if negative { -n } else { n })
    }
}

/// `value^n` by repeated multiplication (exact for the powers of ten used by prefixes).
fn integer_power(value: f64, n: i8) -> f64 {
    let mut result = 1.0;
    for _ in 0..n.unsigned_abs() {
        result *= value;
    }
    if n < 0 { 1.0 / result } else { result }
}

/// Resolves a unit symbol, with an SI prefix if the unit takes one.
fn lookup(symbol: &str) -> Option<(f64, Dimension)> {
    if let Some(&(_, factor, dim, _)) = UNITS.iter().find(|u| u.0 == symbol) {
        return Some((factor, dim));
    }
    PREFIXES.iter().find_map(|&(prefix, scale)| {
        let unit = symbol.strip_prefix(prefix)?;
        UNITS
            .iter()
            .find(|u| u.0 == unit && u.3)
            .map(|&(_, factor, dim, _)| (scale * factor, dim))
    })
}

/// SI prefixes, two-letter ones first.
const PREFIXES: [(&str, f64); 18] = [
    ("da", 1e1),
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("µ", 1e-6),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
];

type Unit = (&'static str, f64, Dimension, bool);

/// Known units: symbol, SI factor, dimension, takes prefixes.
const UNITS: [Unit; 36] = [
    ("m", 1.0, Dimension::LENGTH, true),
    ("g", 1e-3, Dimension::MASS, true),
    ("s", 1.0, Dimension::TIME, true),
    ("A", 1.0, Dimension::CURRENT, true),
    ("K", 1.0, Dimension::TEMPERATURE, true),
    ("mol", 1.0, Dimension::AMOUNT, true),
    ("cd", 1.0, Dimension::LUMINOSITY, false),
    ("N", 1.0, Dimension::FORCE, true),
    ("Pa", 1.0, Dimension::PRESSURE, true),
    ("J", 1.0, Dimension::ENERGY, true),
    ("W", 1.0, Dimension::POWER, true),
    ("Hz", 1.0, Dimension::FREQUENCY, true),
    ("V", 1.0, Dimension::POWER.div(Dimension::CURRENT), true),
    ("C", 1.0, Dimension::CURRENT.mul(Dimension::TIME), true),
    (
        "Ω",
        1.0,
        Dimension::POWER.div(Dimension::CURRENT.pow(2)),
        true,
    ),
    (
        "ohm",
        1.0,
        Dimension::POWER.div(Dimension::CURRENT.pow(2)),
        true,
    ),
    ("F", 1.0, Dimension::CAPACITANCE, true),
    ("H", 1.0, Dimension::INDUCTANCE, true),
    ("S", 1.0, Dimension::CONDUCTANCE, true),
    ("Wb", 1.0, Dimension::MAGNETIC_FLUX, true),
    ("T", 1.0, Dimension::FLUX_DENSITY, true),
    ("Wh", 3600.0, Dimension::ENERGY, true),
    ("Ah", 3600.0, Dimension::CHARGE, true),
    ("L", 1e-3, Dimension::VOLUME, true),
    ("t", 1e3, Dimension::MASS, true),
    ("eV", 1.602_176_634e-19, Dimension::ENERGY, true),
    ("bar", 1e5, Dimension::PRESSURE, true),
    ("atm", 101_325.0, Dimension::PRESSURE, false),
    ("min", 60.0, Dimension::TIME, false),
    ("h", 3600.0, Dimension::TIME, false),
    ("d", 86_400.0, Dimension::TIME, false),
    ("au", 149_597_870_700.0, Dimension::LENGTH, false),
    ("deg", std::f64::consts::PI / 180.0, Dimension::NONE, false),
    ("°", std::f64::consts::PI / 180.0, Dimension::NONE, false),
    ("rad", 1.0, Dimension::NONE, false),
    ("%", 0.01, Dimension::NONE, false),
];
