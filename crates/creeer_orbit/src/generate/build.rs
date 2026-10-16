//! Placing the bodies: star, planets, moons, the belt and Trojans, with their start states.

use super::def::{Class, Params};
use super::names;
use super::rng::Rng;
use super::{Generated, Placed};
use crate::analysis::{hill_radius, lagrange_points, roche_limit};
use crate::body::{Body, BodyKind, G, Spin};
use crate::kepler::Elements;
use creeer_math::glam::{DMat3, DQuat, DVec3};

const ROCKY_COLORS: [[u8; 3]; 5] = [
    [176, 120, 86],
    [150, 142, 128],
    [196, 170, 120],
    [120, 140, 110],
    [170, 96, 70],
];
const GIANT_COLORS: [[u8; 3]; 4] = [
    [214, 180, 130],
    [150, 180, 210],
    [200, 150, 110],
    [170, 200, 180],
];

fn pick_color(rng: &mut Rng, list: &[[u8; 3]]) -> [u8; 3] {
    list[usize::try_from(rng.next_u64() % list.len() as u64).expect("small")]
}

/// A body of `class` with radius and gravity drawn from its ranges.
fn draw(rng: &mut Rng, class: &Class, kind: BodyKind, name: String, color: [u8; 3]) -> Body {
    let radius = rng.log_range(class.radius);
    let density = rng.range(class.density);
    let day = class.day.map_or(f64::INFINITY, |d| rng.log_range(d));
    let tilt = rng.range(class.tilt);
    let mut spin = Spin::new(day, tilt, rng.angle());
    // Tilt towards a random direction.
    spin.orientation = DQuat::from_rotation_y(rng.angle()) * spin.orientation;
    Body {
        name,
        kind,
        mu: G * density * 4.0 / 3.0 * std::f64::consts::PI * radius * radius * radius,
        radius,
        spin,
        color,
        seed: rng.next_u64(),
        surface: class.surface.clone(),
    }
}

/// Mutual Hill radius of two bodies orbiting `mu_star` at `a1`, `a2`.
fn mutual_hill(mu1: f64, mu2: f64, a1: f64, a2: f64, mu_star: f64) -> f64 {
    creeer_math::det::cbrt((mu1 + mu2) / (3.0 * mu_star)) * 0.5 * (a1 + a2)
}

/// Random elements about a central body: e and i from ranges, angles uniform.
fn elements(rng: &mut Rng, mu: f64, a: f64, e: (f64, f64), i: (f64, f64)) -> Elements {
    let e = rng.range(e);
    let inclination = rng.range(i);
    let base =
        Elements::from_semi_major_axis(mu, a, e, [inclination, rng.angle(), rng.angle(), 0.0]);
    base.with_mean_anomaly(rng.angle())
}

/// Turns a moon so it always shows its planet the same face: rotation period = orbital
/// period, axis = orbit normal, body +X towards the planet at the start.
fn tidally_lock(body: &mut Body, rel: (DVec3, DVec3), period: f64) {
    let toward = -rel.0.normalize();
    let normal = rel.0.cross(rel.1).normalize();
    let basis = DMat3::from_cols(toward, normal, toward.cross(normal));
    body.spin = Spin {
        orientation: DQuat::from_mat3(&basis).normalize(),
        rate: std::f64::consts::TAU / period,
        epoch: 0.0,
    };
}

/// Builds one candidate system from `rng` (the stability check comes after).
pub fn build(p: &Params, rng: &mut Rng) -> Generated {
    let mut majors: Vec<Placed> = Vec::new();
    let mut parents: Vec<Option<usize>> = Vec::new();
    let star_mu = p.star_gravity * p.star_radius * p.star_radius;
    majors.push(Placed {
        body: Body {
            name: p.star_name.clone(),
            kind: BodyKind::Star,
            mu: star_mu,
            radius: p.star_radius,
            spin: Spin::new(p.star_day, 0.0, 0.0),
            color: p.star_color,
            seed: rng.next_u64(),
            surface: None,
        },
        x: DVec3::ZERO,
        v: DVec3::ZERO,
    });
    parents.push(None);

    // Semi-major axes: outwards and inwards from home, by spacing ratios, around the belt.
    let near_belt = |a: f64| a > p.belt.0 / 1.2 && a < p.belt.1 * 1.2;
    let mut axes = vec![p.home_a];
    let mut a = p.home_a;
    loop {
        a *= rng.range(p.spacing);
        if near_belt(a) {
            a = p.belt.1 * 1.2 * rng.range(p.spacing).sqrt();
        }
        if a > p.outer {
            break;
        }
        axes.push(a);
    }
    a = p.home_a;
    loop {
        a /= rng.range(p.spacing);
        if near_belt(a) {
            a = p.belt.0 / 1.2 / rng.range(p.spacing).sqrt();
        }
        if a < p.inner {
            break;
        }
        axes.push(a);
    }
    axes.sort_by(f64::total_cmp);

    // Planets.
    let mut planets: Vec<(Body, f64, bool)> = Vec::new(); // body, a, is home
    let mut first_beyond = true;
    for &a in &axes {
        #[allow(clippy::float_cmp)] // the very value put in the list above
        if a == p.home_a {
            let h = &p.home;
            let mut spin = Spin::new(h.day, h.tilt, 0.0);
            spin.orientation = spin.orientation.normalize();
            planets.push((
                Body {
                    name: h.name.clone(),
                    kind: BodyKind::Planet,
                    mu: h.surface_gravity * h.radius * h.radius,
                    radius: h.radius,
                    spin,
                    color: [90, 130, 190],
                    seed: h.seed,
                    surface: Some(p.home_file.clone()),
                },
                a,
                true,
            ));
            continue;
        }
        // The first planet beyond the snow line is always a giant (like Jupiter).
        let beyond = a > p.giant_beyond;
        let giant = beyond && (first_beyond || rng.chance(p.giant_chance));
        first_beyond &= !beyond;
        let body = if giant {
            let color = pick_color(rng, &GIANT_COLORS);
            let name = names::name(rng);
            draw(rng, &p.giant, BodyKind::GasGiant, name, color)
        } else {
            let color = pick_color(rng, &ROCKY_COLORS);
            let name = names::name(rng);
            draw(rng, &p.rocky, BodyKind::Planet, name, color)
        };
        planets.push((body, a, false));
    }
    // Neighbours too close in mutual Hill radii: drop the lighter one (never home).
    let mut k = 0;
    while k + 1 < planets.len() {
        let (b1, a1, h1) = (&planets[k].0, planets[k].1, planets[k].2);
        let (b2, a2, h2) = (&planets[k + 1].0, planets[k + 1].1, planets[k + 1].2);
        let separation = (a2 - a1) / mutual_hill(b1.mu, b2.mu, a1, a2, star_mu);
        if separation < p.planet_hill_separation {
            let drop_first = !h1 && (h2 || b1.mu < b2.mu);
            planets.remove(if drop_first { k } else { k + 1 });
        } else {
            k += 1;
        }
    }

    for (body, a, home) in planets {
        let el = if home {
            Elements::from_semi_major_axis(star_mu, a, p.home_e, [0.0, 0.0, 0.0, 0.0])
                .with_mean_anomaly(rng.angle())
        } else {
            elements(rng, star_mu, a, p.planet_e, p.planet_i)
        };
        let (x, v) = el.to_state();
        let index = majors.len();
        let moons = if home {
            p.home_moons
        } else if body.kind == BodyKind::GasGiant {
            p.giant.moons
        } else {
            p.rocky.moons
        };
        let hill = hill_radius(a, el.e, body.mu, star_mu);
        majors.push(Placed { body, x, v });
        parents.push(Some(0));
        add_moons(p, rng, &mut majors, &mut parents, index, moons, hill);
    }

    // Belt asteroids massive enough to matter: majors.
    for _ in 0..p.belt_majors {
        let placed = asteroid(p, rng, star_mu, p.major_radius);
        majors.push(placed);
        parents.push(Some(0));
    }

    let mut minors = Vec::new();
    for _ in 0..p.belt_minors {
        minors.push(asteroid(p, rng, star_mu, p.minor_radius));
    }
    add_trojans(p, rng, &majors, &mut minors);
    let home = majors
        .iter()
        .position(|m| m.body.surface.as_deref() == Some(p.home_file.as_str()))
        .expect("home is placed");
    to_barycentre(&mut majors, &mut minors);
    Generated {
        name: p.name.clone(),
        majors,
        minors,
        parents,
        home,
        attempts: 0,
    }
}

/// Moons of the planet at `index`, from outside its Roche limit to `hill_fraction` of its
/// Hill radius, spaced in mutual Hill radii.
fn add_moons(
    p: &Params,
    rng: &mut Rng,
    majors: &mut Vec<Placed>,
    parents: &mut Vec<Option<usize>>,
    index: usize,
    count: (u32, u32),
    hill: f64,
) {
    let count = rng.count(count);
    let (planet, px, pv) = (majors[index].body.clone(), majors[index].x, majors[index].v);
    let outer = p.hill_fraction * hill;
    let mut a = 0.0f64;
    let mut previous_mu = 0.0;
    // Moons stay well below their planet (Charon is half of Pluto; most are far smaller).
    let mut class = p.moon.clone();
    class.radius.1 = class.radius.1.min(0.4 * planet.radius);
    if class.radius.0 > class.radius.1 {
        return;
    }
    for _ in 0..count {
        let name = names::name(rng);
        let mut moon = draw(rng, &class, BodyKind::Moon, name, [150, 145, 140]);
        let shade = 110 + u8::try_from(rng.next_u64() % 80).expect("< 80");
        moon.color = [shade, shade - 5, shade.saturating_sub(12)];
        let roche = roche_limit(planet.radius, planet.density(), moon.density());
        let inner = (p.roche_factor * roche).max(3.0 * planet.radius);
        a = if a == 0.0 {
            inner * rng.range((1.0, 3.0))
        } else {
            let mut next = a * rng.range(p.moon_spacing);
            let separation = |b: f64| (b - a) / mutual_hill(previous_mu, moon.mu, a, b, planet.mu);
            while separation(next) < p.moon_hill_separation {
                next *= 1.05;
            }
            next
        };
        if a > outer {
            break;
        }
        let el = elements(rng, planet.mu + moon.mu, a, p.moon_e, p.moon_i);
        let rel = el.to_state();
        tidally_lock(&mut moon, rel, el.period());
        previous_mu = moon.mu;
        majors.push(Placed {
            body: moon,
            x: px + rel.0,
            v: pv + rel.1,
        });
        parents.push(Some(index));
    }
}

/// An asteroid of the belt with radius in `radius`.
fn asteroid(p: &Params, rng: &mut Rng, star_mu: f64, radius: (f64, f64)) -> Placed {
    let r = rng.log_range(radius);
    let mass = p.belt_density * 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
    let a = rng.range(p.belt);
    let (x, v) = elements(rng, star_mu, a, p.belt_e, p.belt_i).to_state();
    let shade = 70 + u8::try_from(rng.next_u64() % 50).expect("< 50");
    Placed {
        body: Body {
            name: names::name(rng),
            kind: BodyKind::Asteroid,
            mu: G * mass,
            radius: r,
            spin: Spin::new(
                spin_period(rng, (2.0 * 3600.0, 40.0 * 3600.0), r, G * mass),
                rng.range((0.0, 3.1)),
                rng.angle(),
            ),
            color: [shade, shade, shade - 6],
            seed: rng.next_u64(),
            surface: p.asteroid_surface.clone(),
        },
        x,
        v,
    }
}

/// Trojans of every gas giant, scattered about its L4 and L5 points.
fn add_trojans(p: &Params, rng: &mut Rng, majors: &[Placed], minors: &mut Vec<Placed>) {
    let star = &majors[0];
    for giant in majors.iter().filter(|m| m.body.kind == BodyKind::GasGiant) {
        let points = lagrange_points(
            (star.x, star.v, star.body.mu),
            (giant.x, giant.v, giant.body.mu),
        );
        let d = giant.x - star.x;
        let h = d.cross(giant.v - star.v);
        let omega = h / d.length_squared();
        for k in 0..rng.count(p.trojans) {
            let l = points[3 + usize::try_from(k % 2).expect("0 or 1")];
            // Libration: a few degrees along the orbit, a percent in radius.
            let rel = l - star.x;
            let turned = DQuat::from_axis_angle(omega.normalize(), rng.range((-0.1, 0.1))) * rel;
            let pos = star.x + turned * rng.range((0.99, 1.01));
            let r = rng.log_range(p.trojan_radius);
            let mass = p.trojan_density * 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
            let shade = 80 + u8::try_from(rng.next_u64() % 40).expect("< 40");
            minors.push(Placed {
                body: Body {
                    name: names::name(rng),
                    kind: BodyKind::Asteroid,
                    mu: G * mass,
                    radius: r,
                    spin: Spin::new(
                        spin_period(rng, (3.0 * 3600.0, 30.0 * 3600.0), r, G * mass),
                        0.5,
                        rng.angle(),
                    ),
                    color: [shade, shade - 8, shade - 14],
                    seed: rng.next_u64(),
                    surface: p.asteroid_surface.clone(),
                },
                x: pos,
                v: star.v + omega.cross(pos - star.x),
            });
        }
    }
}

/// Moves everything into the frame of the majors' barycentre (at rest at the origin).
fn to_barycentre(majors: &mut [Placed], minors: &mut [Placed]) {
    let total: f64 = majors.iter().map(|m| m.body.mu).sum();
    let centre = majors.iter().fold(DVec3::ZERO, |s, m| s + m.x * m.body.mu) / total;
    let drift = majors.iter().fold(DVec3::ZERO, |s, m| s + m.v * m.body.mu) / total;
    for m in majors.iter_mut().chain(minors.iter_mut()) {
        m.x -= centre;
        m.v -= drift;
    }
}

/// A rotation period from `range`, but never faster than 1.5 times the period of an orbit
/// skimming the surface: spun faster, loose rock would leave the equator (the "spin barrier"
/// of rubble-pile asteroids near 2.2 h), and sea level would have no solution.
fn spin_period(rng: &mut Rng, (lo, hi): (f64, f64), radius: f64, mu: f64) -> f64 {
    let skimming = std::f64::consts::TAU * (radius * radius * radius / mu).sqrt();
    let lo = lo.max(1.5 * skimming);
    rng.log_range((lo, hi.max(lo)))
}
