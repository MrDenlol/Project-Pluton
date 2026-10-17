use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Input,
    Clock,
    Output,
    And,
    Or,
    Xor,
    Not,
    Nand,
    Nor,
    HalfAdder,
    FullAdder,
    Add,
    Sub,
    Mul,
    Div,
    Mux4,
    Dff,
    Jk,
    Tff,
    Sr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    pub label: String,
    pub kind: Kind,
    pub width: u8,
    pub value: u64,
    pub position: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Port {
    pub node: u64,
    pub pin: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wire {
    pub from: Port,
    pub to: Port,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Circuit {
    pub version: u32,
    pub name: String,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
}

#[derive(Clone, Debug)]
pub struct Pin {
    pub name: &'static str,
    pub width: u8,
}

impl Kind {
    pub const ALL: [Kind; 20] = [
        Self::Input,
        Self::Clock,
        Self::Output,
        Self::And,
        Self::Or,
        Self::Xor,
        Self::Not,
        Self::Nand,
        Self::Nor,
        Self::HalfAdder,
        Self::FullAdder,
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Mux4,
        Self::Dff,
        Self::Jk,
        Self::Tff,
        Self::Sr,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Input => "Вход",
            Self::Clock => "Генератор",
            Self::Output => "Индикатор",
            Self::And => "AND",
            Self::Or => "OR",
            Self::Xor => "XOR",
            Self::Not => "NOT",
            Self::Nand => "NAND",
            Self::Nor => "NOR",
            Self::HalfAdder => "Полусумматор",
            Self::FullAdder => "Полный сумматор",
            Self::Add => "ADD +",
            Self::Sub => "SUB −",
            Self::Mul => "MUL ×",
            Self::Div => "DIV ÷",
            Self::Mux4 => "MUX 4:1",
            Self::Dff => "D-триггер",
            Self::Jk => "JK-триггер",
            Self::Tff => "T-триггер",
            Self::Sr => "SR-триггер",
        }
    }
    pub fn sequential(self) -> bool {
        matches!(self, Self::Dff | Self::Jk | Self::Tff | Self::Sr)
    }
    pub fn single_bit(self) -> bool {
        matches!(
            self,
            Self::Clock | Self::HalfAdder | Self::FullAdder | Self::Jk | Self::Tff | Self::Sr
        )
    }
}

impl Node {
    pub fn inputs(&self) -> Vec<Pin> {
        use Kind::*;
        let w = self.width;
        let pins: Vec<(&str, u8)> = match self.kind {
            Input | Clock => vec![],
            Output | Not => vec![("IN", w)],
            And | Or | Xor | Nand | Nor | Add | Sub | Mul | Div => vec![("A", w), ("B", w)],
            HalfAdder => vec![("A", 1), ("B", 1)],
            FullAdder => vec![("A", 1), ("B", 1), ("Cin", 1)],
            Mux4 => vec![
                ("D0 +", w),
                ("D1 −", w),
                ("D2 ×", w),
                ("D3 ÷", w),
                ("SEL", 2),
            ],
            Dff => vec![("D", w), ("CLK", 1)],
            Jk => vec![("J", 1), ("K", 1), ("CLK", 1)],
            Tff => vec![("T", 1), ("CLK", 1)],
            Sr => vec![("S", 1), ("R", 1), ("CLK", 1)],
        };
        pins.into_iter()
            .map(|(name, width)| Pin { name, width })
            .collect()
    }
    pub fn outputs(&self) -> Vec<Pin> {
        use Kind::*;
        let w = self.width;
        let pins: Vec<(&str, u8)> = match self.kind {
            Output => vec![],
            HalfAdder | FullAdder => vec![("S", 1), ("Cout", 1)],
            Add => vec![("R", w), ("Carry", 1)],
            Sub => vec![("R", w), ("Borrow", 1)],
            Mul => vec![("R", w), ("Overflow", 1)],
            Div => vec![("R", w), ("Rem", w)],
            Dff | Jk | Tff | Sr => vec![("Q", w), ("!Q", w)],
            _ => vec![("OUT", w)],
        };
        pins.into_iter()
            .map(|(name, width)| Pin { name, width })
            .collect()
    }
}

impl Circuit {
    pub fn empty() -> Self {
        Self {
            version: 1,
            name: "Новая схема".into(),
            nodes: vec![],
            wires: vec![],
        }
    }
    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn add(&mut self, kind: Kind, width: u8, position: [f32; 2]) -> u64 {
        let used: HashSet<_> = self.nodes.iter().map(|n| n.id).collect();
        let id = (1..).find(|id| !used.contains(id)).expect("finite circuit");
        self.nodes.push(Node {
            id,
            label: kind.title().into(),
            kind,
            width: if kind.single_bit() { 1 } else { width },
            value: 0,
            position,
        });
        id
    }
    pub fn remove(&mut self, id: u64) {
        self.nodes.retain(|n| n.id != id);
        self.wires.retain(|w| w.from.node != id && w.to.node != id);
    }
    pub fn connect(&mut self, from: Port, to: Port) -> Result<(), String> {
        let old = self.wires.clone();
        self.wires.retain(|w| w.to != to);
        self.wires.push(Wire { from, to });
        if let Err(e) = self.validate() {
            self.wires = old;
            return Err(e);
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("Неизвестная версия файла: {}", self.version));
        }
        if self.nodes.len() > 4096 || self.wires.len() > 16384 {
            return Err("Схема слишком большая (максимум 4096 блоков / 16384 связей)".into());
        }
        let mut ids = HashSet::new();
        for n in &self.nodes {
            if n.id == u64::MAX || !ids.insert(n.id) {
                return Err("Некорректный или повторяющийся ID блока".into());
            }
            if !(1..=64).contains(&n.width) || (n.kind.single_bit() && n.width != 1) {
                return Err(format!("Недопустимая разрядность: {}", n.label));
            }
            if n.position
                .iter()
                .any(|x| !x.is_finite() || x.abs() > 1_000_000.0)
            {
                return Err("Некорректные координаты".into());
            }
            if n.value & !crate::arithmetic::mask(n.width) != 0 {
                return Err(format!(
                    "Значение не помещается в {} бит: {}",
                    n.width, n.label
                ));
            }
        }
        let nodes: HashMap<_, _> = self.nodes.iter().map(|n| (n.id, n)).collect();
        let mut destinations = HashSet::new();
        for w in &self.wires {
            let a = nodes
                .get(&w.from.node)
                .ok_or("Связь ссылается на отсутствующий блок")?;
            let b = nodes
                .get(&w.to.node)
                .ok_or("Связь ссылается на отсутствующий блок")?;
            let outputs = a.outputs();
            let inputs = b.inputs();
            let out = outputs.get(w.from.pin).ok_or("Отсутствует выходной порт")?;
            let input = inputs.get(w.to.pin).ok_or("Отсутствует входной порт")?;
            if out.width != input.width {
                return Err(format!(
                    "Разрядность портов не совпадает: {} → {}",
                    out.width, input.width
                ));
            }
            if !destinations.insert(w.to) {
                return Err("У входа может быть только один источник".into());
            }
        }
        self.order().map(|_| ())
    }
    /// Kahn sort: sequential outputs are graph roots; feedback through storage is legal.
    pub fn order(&self) -> Result<Vec<u64>, String> {
        let mut indegree: HashMap<u64, usize> = self.nodes.iter().map(|n| (n.id, 0)).collect();
        let nodes: HashMap<_, _> = self.nodes.iter().map(|n| (n.id, n)).collect();
        let mut edges: HashMap<u64, Vec<u64>> = HashMap::new();
        for w in &self.wires {
            let target = nodes.get(&w.to.node).ok_or("Отсутствующий блок")?;
            if !target.kind.sequential() {
                *indegree.get_mut(&w.to.node).ok_or("Отсутствующий блок")? += 1;
                edges.entry(w.from.node).or_default().push(w.to.node);
            }
        }
        let mut queue: std::collections::VecDeque<_> = self
            .nodes
            .iter()
            .filter(|n| indegree[&n.id] == 0)
            .map(|n| n.id)
            .collect();
        let mut order = vec![];
        while let Some(id) = queue.pop_front() {
            order.push(id);
            for target in edges.get(&id).into_iter().flatten() {
                let degree = indegree.get_mut(target).ok_or("Отсутствующий блок")?;
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(*target);
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err("Комбинационная петля: добавьте триггер в обратную связь".into());
        }
        Ok(order)
    }
}
