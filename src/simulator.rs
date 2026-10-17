use crate::{
    arithmetic as alu,
    model::{Circuit, Kind, Node, Port},
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Value(u64),
    Unknown,
    DivideByZero,
    InvalidSr,
}

impl Signal {
    pub fn text(self) -> String {
        match self {
            Self::Value(v) => v.to_string(),
            Self::Unknown => "X".into(),
            Self::DivideByZero => "DIV/0".into(),
            Self::InvalidSr => "SR!".into(),
        }
    }
    pub fn value(self) -> Option<u64> {
        if let Self::Value(v) = self {
            Some(v)
        } else {
            None
        }
    }
    fn invert(self, width: u8) -> Self {
        match self {
            Self::Value(v) => Self::Value(!v & alu::mask(width)),
            other => other,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Memory {
    q: Signal,
    clock: Signal,
}

#[derive(Clone, Debug, Default)]
pub struct Simulator {
    pub values: HashMap<Port, Signal>,
    memory: HashMap<u64, Memory>,
    pub ticks: u64,
}

impl Simulator {
    pub fn reset(&mut self, circuit: &mut Circuit) -> Result<(), String> {
        *self = Self::default();
        for n in &mut circuit.nodes {
            if n.kind == Kind::Clock {
                n.value = 0;
            }
        }
        self.settle(circuit)
    }
    pub fn output(&self, port: Port) -> Signal {
        self.values.get(&port).copied().unwrap_or(Signal::Unknown)
    }
    pub fn input(&self, circuit: &Circuit, port: Port) -> Signal {
        circuit
            .wires
            .iter()
            .find(|w| w.to == port)
            .map(|w| self.output(w.from))
            .unwrap_or(Signal::Unknown)
    }
    pub fn reading(&self, circuit: &Circuit, node: &Node) -> Signal {
        if node.kind == Kind::Output {
            self.input(
                circuit,
                Port {
                    node: node.id,
                    pin: 0,
                },
            )
        } else {
            self.output(Port {
                node: node.id,
                pin: 0,
            })
        }
    }
    /// One step is half a clock period. All clocks change together, then all storage samples together.
    pub fn step(&mut self, circuit: &mut Circuit) -> Result<(), String> {
        for n in &mut circuit.nodes {
            if n.kind == Kind::Clock {
                n.value ^= 1;
            }
        }
        self.propagate(circuit)?;
        self.ticks += 1;
        Ok(())
    }
    /// Re-evaluate manually changed inputs and recognize low-to-high clock transitions.
    pub fn propagate(&mut self, circuit: &Circuit) -> Result<(), String> {
        self.settle(circuit)?;
        let mut updates = vec![];
        for n in circuit.nodes.iter().filter(|n| n.kind.sequential()) {
            let inputs: Vec<_> = (0..n.inputs().len())
                .map(|pin| self.input(circuit, Port { node: n.id, pin }))
                .collect();
            let clk = *inputs.last().unwrap();
            let old = self.memory[&n.id];
            let rising = old.clock == Signal::Value(0) && clk == Signal::Value(1);
            let ambiguous = !matches!(old.clock, Signal::Value(0) | Signal::Value(1))
                && clk == Signal::Value(1);
            let q = if rising {
                sample(n.kind, &inputs, old.q)
            } else if ambiguous {
                Signal::Unknown
            } else {
                old.q
            };
            updates.push((n.id, Memory { q, clock: clk }));
        }
        // Commit only after every flip-flop has sampled the same old circuit state.
        self.memory.extend(updates);
        self.settle(circuit)
    }
    pub fn settle(&mut self, circuit: &Circuit) -> Result<(), String> {
        circuit.validate()?;
        self.values.clear();
        self.memory
            .retain(|id, _| circuit.node(*id).is_some_and(|n| n.kind.sequential()));
        let nodes: HashMap<_, _> = circuit.nodes.iter().map(|n| (n.id, n)).collect();
        let sources: HashMap<_, _> = circuit.wires.iter().map(|w| (w.to, w.from)).collect();
        for id in circuit.order()? {
            let node = nodes[&id];
            let inputs: Vec<_> = (0..node.inputs().len())
                .map(|pin| {
                    sources
                        .get(&Port { node: id, pin })
                        .map(|source| self.output(*source))
                        .unwrap_or(Signal::Unknown)
                })
                .collect();
            let outputs = if node.kind.sequential() {
                let state = self.memory.entry(id).or_insert(Memory {
                    q: Signal::Value(0),
                    clock: Signal::Value(0),
                });
                vec![state.q, state.q.invert(node.width)]
            } else {
                evaluate(node, &inputs)
            };
            for (pin, signal) in outputs.into_iter().enumerate() {
                self.values.insert(Port { node: id, pin }, signal);
            }
        }
        Ok(())
    }
}

fn sample(kind: Kind, x: &[Signal], q: Signal) -> Signal {
    use Signal::*;
    match kind {
        Kind::Dff => x[0],
        Kind::Tff => match x[0] {
            Value(0) => q,
            Value(1) => q.invert(1),
            other => other,
        },
        Kind::Jk => match (x[0], x[1]) {
            (Value(0), Value(0)) => q,
            (Value(1), Value(0)) => Value(1),
            (Value(0), Value(1)) => Value(0),
            (Value(1), Value(1)) => q.invert(1),
            _ => Unknown,
        },
        Kind::Sr => match (x[0], x[1]) {
            (Value(0), Value(0)) => q,
            (Value(1), Value(0)) => Value(1),
            (Value(0), Value(1)) => Value(0),
            (Value(1), Value(1)) => InvalidSr,
            _ => Unknown,
        },
        _ => q,
    }
}

fn evaluate(n: &Node, x: &[Signal]) -> Vec<Signal> {
    use Kind::*;
    use Signal::*;
    if matches!(n.kind, Input | Clock) {
        return vec![Value(n.value & alu::mask(n.width))];
    }
    if n.kind == Output {
        return vec![];
    }
    // An error on an unselected input must not poison a multiplexer.
    if n.kind == Mux4 {
        return vec![match x[4] {
            Value(index) => x[index as usize & 3],
            other => other,
        }];
    }
    if n.kind == Not {
        return vec![x[0].invert(n.width)];
    }
    // Controlling values resolve an unknown gate input (0 AND X = 0; 1 OR X = 1).
    if matches!(n.kind, And | Nand) && x.contains(&Value(0)) {
        return vec![Value(if n.kind == Nand {
            alu::mask(n.width)
        } else {
            0
        })];
    }
    if matches!(n.kind, Or | Nor) && x.contains(&Value(alu::mask(n.width))) {
        return vec![Value(if n.kind == Nor { 0 } else { alu::mask(n.width) })];
    }
    if let Some(error) = x.iter().find(|v| !matches!(v, Value(_))) {
        return vec![*error; n.outputs().len()];
    }
    let a = x[0].value().unwrap();
    let b = x[1].value().unwrap();
    match n.kind {
        And => vec![Value(a & b)],
        Or => vec![Value(a | b)],
        Xor => vec![Value(a ^ b)],
        Nand => vec![Value(!(a & b) & alu::mask(n.width))],
        Nor => vec![Value(!(a | b) & alu::mask(n.width))],
        HalfAdder | FullAdder => {
            let cin = n.kind == FullAdder && x[2] == Value(1);
            let (s, c) = alu::full_adder(a != 0, b != 0, cin);
            vec![Value(s as u64), Value(c as u64)]
        }
        Add | Sub | Mul => {
            let (r, flag) = match n.kind {
                Add => alu::add(a, b, n.width, false),
                Sub => alu::sub(a, b, n.width),
                _ => alu::mul(a, b, n.width),
            };
            vec![Value(r), Value(flag as u64)]
        }
        Div => match alu::div(a, b, n.width) {
            Some((q, r)) => vec![Value(q), Value(r)],
            None => vec![DivideByZero; 2],
        },
        _ => unreachable!("special components handled above"),
    }
}
