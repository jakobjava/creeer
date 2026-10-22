//! Things made of pieces (G1.6d, ADR-0030): a tool is parts placed in its own frame (its grip at
//! the origin, its haft along +x, the head's edge towards +y), held together by bindings. What
//! it is to swing (mass, inertia about the grip, balance, reach) follows from the parts alone,
//! so a knapped stone on a stick and a forged steel axe are one kind of thing, and a single
//! piece held in the hand is swung by the same rule.

use super::{Form, Item, Piece};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// A part of an assembly: a piece where it sits and how it is turned (the assembly's frame).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub piece: Piece,
    pub at: DVec3,
    pub turn: DQuat,
}

/// Pieces made into one thing.
#[derive(Clone, Debug, PartialEq)]
pub struct Assembly {
    pub parts: Vec<Part>,
    /// What binds them (bark strips wound about the joint), by their own pieces.
    pub bindings: Vec<Piece>,
    /// The most force the joint holds before the head works loose (N).
    pub hold: f64,
}

/// A thing as it is swung about its grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swinging {
    /// Its mass (kg), its radius of gyration about the grip (m), how far out its centre of mass
    /// is (m), and how far out it strikes (m).
    pub mass: f64,
    pub gyration: f64,
    pub carry: f64,
    pub strike: f64,
    /// Which part strikes (the farthest out).
    pub head: usize,
}

/// A form's moments of inertia about its centre along its own axes (kg·m², for `mass`).
fn moments(form: Form, mass: f64) -> DVec3 {
    let box_ = |s: DVec3, k: f64| {
        DVec3::new(
            s.y * s.y + s.z * s.z,
            s.x * s.x + s.z * s.z,
            s.x * s.x + s.y * s.y,
        ) * (mass / k)
    };
    match form {
        Form::Block { size } => box_(size, 12.0),
        // An ellipsoid of semi-axes size/2: m(b² + c²)/5, so (size)²/20.
        Form::Lump { size } => box_(size, 20.0),
        Form::Ball { radius } => DVec3::splat(0.4 * mass * radius * radius),
        Form::Rod { radius, length } => {
            let across = mass * (3.0 * radius * radius + length * length) / 12.0;
            DVec3::new(0.5 * mass * radius * radius, across, across)
        }
    }
}

impl Part {
    /// Its box's half extents along the assembly's axes.
    fn half(&self) -> DVec3 {
        let e = match self.piece.form {
            Form::Block { size } | Form::Lump { size } => size * 0.5,
            Form::Ball { radius } => DVec3::splat(radius),
            Form::Rod { radius, length } => DVec3::new(length * 0.5, radius, radius),
        };
        let m = DMat3::from_quat(self.turn);
        DVec3::new(
            m.row(0).abs().dot(e),
            m.row(1).abs().dot(e),
            m.row(2).abs().dot(e),
        )
    }
}

impl Assembly {
    /// Its mass, bindings included (kg).
    pub fn mass(&self) -> f64 {
        let parts: f64 = self.parts.iter().map(|p| p.piece.mass()).sum();
        parts + self.bindings.iter().map(Piece::mass).sum::<f64>()
    }

    /// Its box's edges, longest first (m).
    pub fn extents(&self) -> [f64; 3] {
        let (lo, hi) = (self.parts.iter()).fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| {
            (lo.min(p.at - p.half()), hi.max(p.at + p.half()))
        });
        let mut e = (hi - lo).to_array();
        e.sort_by(|a, b| b.total_cmp(a));
        e
    }

    /// As swung about its grip (the origin), about the axis across the haft and the head (z).
    pub fn swinging(&self) -> Swinging {
        let mass = self.mass();
        let (mut inertia, mut moment) = (0.0, DVec3::ZERO);
        let (mut strike, mut head) = (0.0, 0);
        for (k, p) in self.parts.iter().enumerate() {
            let m = p.piece.mass();
            let own = DMat3::from_quat(p.turn)
                * DMat3::from_diagonal(moments(p.piece.form, m))
                * DMat3::from_quat(p.turn).transpose();
            inertia += own.z_axis.z + m * (p.at.x * p.at.x + p.at.y * p.at.y);
            moment += p.at * m;
            let reach = (p.at.truncate() + p.half().truncate()).length();
            if reach > strike {
                (strike, head) = (reach, k);
            }
        }
        Swinging {
            mass,
            gyration: (inertia / mass).sqrt(),
            carry: (moment / mass).truncate().length(),
            strike,
            head,
        }
    }
}

impl Item {
    /// As swung in the hand: a made thing about its grip; a single piece about its end (held by
    /// it along its longest axis, as a stick is, or a hammerstone in the fist). None for a bulk.
    pub fn swinging(&self) -> Option<Swinging> {
        match self {
            Self::Made(a) => Some(a.swinging()),
            Self::Piece(p, _) => Some(Assembly::held(p.clone()).swinging()),
            Self::Bulk(_) => None,
        }
    }

    /// The piece that strikes when it is swung.
    pub fn head(&self) -> Option<&Piece> {
        match self {
            Self::Made(a) => a.parts.get(a.swinging().head).map(|p| &p.piece),
            Self::Piece(p, _) => Some(p),
            Self::Bulk(_) => None,
        }
    }
}

impl Assembly {
    /// One piece as held by the end of its longest axis.
    pub fn held(piece: Piece) -> Self {
        let long = piece.form.extents()[0];
        let turn = match piece.form {
            // A block's or lump's longest axis is its x (its sizes in order) once sorted; turned so.
            Form::Block { size } | Form::Lump { size } if size.y > size.x => {
                DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2)
            }
            _ => DQuat::IDENTITY,
        };
        Self {
            parts: vec![Part {
                piece,
                at: DVec3::X * (0.5 * long),
                turn,
            }],
            bindings: Vec::new(),
            hold: f64::INFINITY,
        }
    }
}
