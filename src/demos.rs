use crate::model::{Circuit, Kind, Port, Wire};

fn block(c: &mut Circuit, kind: Kind, width: u8, label: &str, value: u64, x: f32, y: f32) -> u64 {
    let id = c.add(kind, width, [x, y]);
    let node = c.nodes.last_mut().unwrap();
    node.label = label.into();
    node.value = value;
    id
}
fn wire(c: &mut Circuit, a: u64, out: usize, b: u64, input: usize) {
    c.wires.push(Wire {
        from: Port { node: a, pin: out },
        to: Port {
            node: b,
            pin: input,
        },
    });
}

pub fn alu64() -> Circuit {
    let mut c = Circuit::empty();
    c.name = "АЛУ · 64 бита".into();
    let a = block(&mut c, Kind::Input, 64, "A", 42, 40., 80.);
    let b = block(&mut c, Kind::Input, 64, "B", 7, 40., 270.);
    let op = block(&mut c, Kind::Input, 2, "Операция 0…3", 0, 330., 700.);
    let mux = block(&mut c, Kind::Mux4, 64, "Выбор операции", 0, 640., 180.);
    for (i, kind) in [Kind::Add, Kind::Sub, Kind::Mul, Kind::Div]
        .into_iter()
        .enumerate()
    {
        let id = block(
            &mut c,
            kind,
            64,
            kind.title(),
            0,
            330.,
            30. + i as f32 * 165.,
        );
        wire(&mut c, a, 0, id, 0);
        wire(&mut c, b, 0, id, 1);
        wire(&mut c, id, 0, mux, i);
        if kind == Kind::Div {
            let rem = block(&mut c, Kind::Output, 64, "Остаток A % B", 0, 640., 540.);
            wire(&mut c, id, 1, rem, 0);
        }
    }
    wire(&mut c, op, 0, mux, 4);
    let out = block(&mut c, Kind::Output, 64, "Результат", 0, 930., 180.);
    wire(&mut c, mux, 0, out, 0);
    c
}

pub fn flip_flops() -> Circuit {
    let mut c = Circuit::empty();
    c.name = "Триггеры · D / JK / T / SR".into();
    let clock = block(&mut c, Kind::Clock, 1, "Общий CLK", 0, 40., 820.);
    for (i, kind) in [Kind::Dff, Kind::Jk, Kind::Tff, Kind::Sr]
        .into_iter()
        .enumerate()
    {
        let y = 20. + i as f32 * 195.;
        let ff = block(&mut c, kind, 1, kind.title(), 0, 500., y);
        let names = c.node(ff).unwrap().inputs();
        for (pin, name) in names.iter().enumerate() {
            if name.name == "CLK" {
                wire(&mut c, clock, 0, ff, pin);
            } else {
                let input = block(
                    &mut c,
                    Kind::Input,
                    1,
                    name.name,
                    u64::from(pin == 0),
                    40. + pin as f32 * 220.,
                    y,
                );
                wire(&mut c, input, 0, ff, pin);
            }
        }
        let out = block(&mut c, Kind::Output, 1, "Q", 0, 780., y);
        wire(&mut c, ff, 0, out, 0);
    }
    c
}

pub fn full_adder() -> Circuit {
    let mut c = Circuit::empty();
    c.name = "Полный сумматор · из вентилей".into();
    let a = block(&mut c, Kind::Input, 1, "A", 1, 20., 30.);
    let b = block(&mut c, Kind::Input, 1, "B", 1, 20., 210.);
    let cin = block(&mut c, Kind::Input, 1, "Cin", 0, 20., 390.);
    let xor1 = block(&mut c, Kind::Xor, 1, "A XOR B", 0, 290., 30.);
    let xor2 = block(&mut c, Kind::Xor, 1, "Сумма", 0, 560., 30.);
    let and1 = block(&mut c, Kind::And, 1, "A AND B", 0, 290., 210.);
    let and2 = block(&mut c, Kind::And, 1, "Cin AND (A XOR B)", 0, 560., 260.);
    let or = block(&mut c, Kind::Or, 1, "Перенос", 0, 820., 210.);
    let sum = block(&mut c, Kind::Output, 1, "S", 0, 820., 30.);
    let carry = block(&mut c, Kind::Output, 1, "Cout", 0, 1080., 210.);
    for (from, pin, to, input) in [
        (a, 0, xor1, 0),
        (b, 0, xor1, 1),
        (xor1, 0, xor2, 0),
        (cin, 0, xor2, 1),
        (a, 0, and1, 0),
        (b, 0, and1, 1),
        (cin, 0, and2, 0),
        (xor1, 0, and2, 1),
        (and1, 0, or, 0),
        (and2, 0, or, 1),
        (xor2, 0, sum, 0),
        (or, 0, carry, 0),
    ] {
        wire(&mut c, from, pin, to, input);
    }
    c
}

pub fn counter() -> Circuit {
    let mut c = Circuit::empty();
    c.name = "Счётчик · регистр + сумматор".into();
    let one = block(&mut c, Kind::Input, 8, "+1", 1, 20., 220.);
    let clock = block(&mut c, Kind::Clock, 1, "CLK", 0, 310., 250.);
    let add = block(&mut c, Kind::Add, 8, "Инкремент", 0, 310., 20.);
    let reg = block(&mut c, Kind::Dff, 8, "Регистр 8 бит", 0, 590., 20.);
    let out = block(&mut c, Kind::Output, 8, "Счётчик", 0, 870., 20.);
    wire(&mut c, reg, 0, add, 0);
    wire(&mut c, one, 0, add, 1);
    wire(&mut c, add, 0, reg, 0);
    wire(&mut c, clock, 0, reg, 1);
    wire(&mut c, reg, 0, out, 0);
    c
}
