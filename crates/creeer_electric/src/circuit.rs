//! A circuit: its nodes and components, how it is built and driven from outside, and what
//! it holds.

use crate::component::{Component, Kind, Node, State};
use crate::devices::{Battery, DcMachine, Diode, SolarModule};
use crate::solve::IslandSolver;
use creeer_network::{Ledger, Sum};

/// A component of a circuit.
pub type ComponentId = usize;

/// A DC or transient circuit (AC grids are [`crate::Grid`]s).
#[derive(Debug)]
pub struct Circuit {
    pub(crate) nodes: usize,
    pub(crate) components: Vec<Component>,
    /// Each node's voltage (V), ground's 0.
    pub(crate) voltage: Vec<f64>,
    pub(crate) ledger: Ledger,
    pub(crate) islands: Option<Vec<IslandSolver>>,
}

impl Default for Circuit {
    fn default() -> Self {
        Self::new()
    }
}

impl Circuit {
    /// A circuit with only its ground node.
    pub fn new() -> Self {
        Self {
            nodes: 1,
            components: Vec::new(),
            voltage: vec![0.0],
            ledger: Ledger::new(0.0),
            islands: None,
        }
    }

    /// A new node.
    pub fn node(&mut self) -> Node {
        self.nodes += 1;
        self.voltage.push(0.0);
        self.islands = None;
        self.nodes - 1
    }

    /// Adds a component of `kind` from `a` to `b`.
    ///
    /// # Panics
    /// If a terminal is no node of the circuit, or both are the same.
    pub fn add(&mut self, kind: Kind, a: Node, b: Node) -> ComponentId {
        assert!(
            a < self.nodes && b < self.nodes && a != b,
            "a component joins two nodes"
        );
        let mut state = State {
            closed: true,
            temperature: 293.15,
            ..State::default()
        };
        if let Kind::Battery(_) = kind {
            // Full, its wells level.
            state.level = 1.0;
            state.extra[2] = 1.0;
        }
        let component = Component { kind, a, b, state };
        self.ledger.joined(stored_in(&component));
        self.components.push(component);
        self.islands = None;
        self.components.len() - 1
    }

    /// A resistor.
    pub fn resistor(&mut self, a: Node, b: Node, ohms: f64) -> ComponentId {
        self.add(Kind::Resistor(ohms), a, b)
    }

    /// A capacitor.
    pub fn capacitor(&mut self, a: Node, b: Node, farads: f64) -> ComponentId {
        self.add(Kind::Capacitor(farads), a, b)
    }

    /// An inductor.
    pub fn inductor(&mut self, a: Node, b: Node, henries: f64) -> ComponentId {
        self.add(Kind::Inductor(henries), a, b)
    }

    /// A voltage source, `plus` at `volts` above `minus`.
    pub fn voltage_source(&mut self, plus: Node, minus: Node, volts: f64) -> ComponentId {
        self.add(Kind::VoltageSource(volts), plus, minus)
    }

    /// A diode or LED.
    pub fn diode(&mut self, anode: Node, cathode: Node, model: Diode) -> ComponentId {
        self.add(Kind::Diode(model), anode, cathode)
    }

    /// A battery, full.
    pub fn battery(&mut self, plus: Node, minus: Node, model: Battery) -> ComponentId {
        self.add(Kind::Battery(model), plus, minus)
    }

    /// A solar module in the dark at 25 °C.
    pub fn solar(&mut self, plus: Node, minus: Node, model: SolarModule) -> ComponentId {
        let id = self.add(Kind::Solar(model), plus, minus);
        self.components[id].state.temperature = 298.15;
        id
    }

    /// A DC machine, its rotor at rest.
    pub fn machine(&mut self, a: Node, b: Node, model: DcMachine) -> ComponentId {
        self.add(Kind::Machine(model), a, b)
    }

    /// Opens or closes a switch (or resets a breaker, or replaces a fuse).
    pub fn set_closed(&mut self, id: ComponentId, closed: bool) {
        let c = &mut self.components[id];
        if c.state.closed != closed {
            c.state.closed = closed;
            c.state.level = 0.0;
            self.event(id);
        }
    }

    /// Sets a source's value (V or A).
    ///
    /// # Panics
    /// If the component is no source.
    pub fn set_source(&mut self, id: ComponentId, value: f64) {
        match &mut self.components[id].kind {
            Kind::VoltageSource(v) | Kind::CurrentSource(v) => *v = value,
            _ => panic!("not a source"),
        }
        self.wake(id);
    }

    /// A component's temperature (K), from the thermal network: a wire's resistance, a
    /// solar module's cells follow it.
    pub fn set_temperature(&mut self, id: ComponentId, kelvin: f64) {
        self.components[id].state.temperature = kelvin;
        self.wake(id);
    }

    /// The irradiance on a solar module (W/m²).
    pub fn set_irradiance(&mut self, id: ComponentId, watts: f64) {
        self.components[id].state.extra[0] = watts;
        self.wake(id);
    }

    /// The torque braking a machine's rotor (N·m; negative drives it: a generator).
    pub fn set_load(&mut self, id: ComponentId, torque: f64) {
        self.components[id].state.load = torque;
        self.wake(id);
    }

    /// A node's voltage (V).
    pub fn voltage(&self, node: Node) -> f64 {
        self.voltage[node]
    }

    /// A component's current from its first terminal through it to its second (A).
    pub fn current(&self, id: ComponentId) -> f64 {
        self.components[id].state.i[0]
    }

    /// A component's voltage, first terminal against second (V).
    pub fn across(&self, id: ComponentId) -> f64 {
        let c = &self.components[id];
        self.voltage[c.a] - self.voltage[c.b]
    }

    /// Whether a switch is closed, a fuse whole, a breaker on, a battery connected.
    pub fn closed(&self, id: ComponentId) -> bool {
        self.components[id].state.closed
    }

    /// A machine's rotor speed (rad/s), a battery's state of charge (0–1).
    pub fn level(&self, id: ComponentId) -> f64 {
        let c = &self.components[id];
        match &c.kind {
            Kind::Battery(b) => {
                b.available * c.state.level + (1.0 - b.available) * c.state.extra[2]
            }
            _ => c.state.level,
        }
    }

    /// The irradiance on a solar module (W/m²).
    pub fn irradiance(&self, id: ComponentId) -> f64 {
        self.components[id].state.extra[0]
    }

    /// A machine's torque on its shaft (N·m).
    pub fn torque(&self, id: ComponentId) -> f64 {
        match &self.components[id].kind {
            Kind::Machine(m) => m.constant * self.components[id].state.i[0],
            _ => 0.0,
        }
    }

    /// The heat a component gave off since this was last called (J): for the thermal network.
    pub fn take_heat(&mut self, id: ComponentId) -> f64 {
        std::mem::take(&mut self.components[id].state.heat).value()
    }

    /// The energy it holds (J): capacitors', inductors', machines' rotors and armatures',
    /// batteries' chemical energy and their RC pairs'.
    pub fn stored(&self) -> f64 {
        let mut total = Sum::default();
        for c in &self.components {
            total.add(stored_in(c));
        }
        total.value()
    }

    /// Where its energy went.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Something about component `id` changed that its island's matrix depends on.
    fn wake(&mut self, id: ComponentId) {
        if let Some(islands) = &mut self.islands {
            for s in islands.iter_mut().filter(|s| s.has(id)) {
                s.wake();
            }
        }
    }

    /// Component `id` switched: its island restarts its integration (backward Euler) and
    /// refactorises.
    pub(crate) fn event(&mut self, id: ComponentId) {
        if let Some(islands) = &mut self.islands {
            for s in islands.iter_mut().filter(|s| s.has(id)) {
                s.restart();
            }
        }
    }
}

/// The energy component `c` holds (J).
pub(crate) fn stored_in(c: &Component) -> f64 {
    let s = &c.state;
    match &c.kind {
        Kind::Capacitor(f) => 0.5 * f * s.v[0] * s.v[0],
        Kind::Inductor(h) => 0.5 * h * s.i[0] * s.i[0],
        Kind::Machine(m) => {
            0.5 * m.inertia * s.level * s.level + 0.5 * m.inductance * s.i[0] * s.i[0]
        }
        Kind::Battery(b) => {
            b.energy(b.available, s.level)
                + b.energy(1.0 - b.available, s.extra[2])
                + 0.5 * b.c1 * s.extra[0] * s.extra[0]
        }
        _ => 0.0,
    }
}
