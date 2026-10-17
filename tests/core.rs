use circuit_lab::{
    arithmetic as alu, demos,
    model::{Circuit, Kind, Port, Wire},
    simulator::{Signal, Simulator},
    storage,
};
use std::path::PathBuf;

fn p(node: u64, pin: usize) -> Port {
    Port { node, pin }
}
fn connect(c: &mut Circuit, a: u64, out: usize, b: u64, input: usize) {
    c.connect(p(a, out), p(b, input)).unwrap();
}
fn set(c: &mut Circuit, id: u64, value: u64) {
    c.nodes.iter_mut().find(|n| n.id == id).unwrap().value = value;
}
fn input(c: &mut Circuit, value: u64, width: u8) -> u64 {
    let id = c.add(Kind::Input, width, [0., 0.]);
    set(c, id, value);
    id
}
fn ff(kind: Kind) -> (Circuit, Simulator, u64, Vec<u64>, u64) {
    let mut c = Circuit::empty();
    let id = c.add(kind, 1, [0., 0.]);
    let count = c.node(id).unwrap().inputs().len() - 1;
    let inputs = (0..count)
        .map(|pin| {
            let n = input(&mut c, 0, 1);
            connect(&mut c, n, 0, id, pin);
            n
        })
        .collect();
    let clock = c.add(Kind::Clock, 1, [0., 0.]);
    connect(&mut c, clock, 0, id, count);
    let mut s = Simulator::default();
    s.reset(&mut c).unwrap();
    (c, s, id, inputs, clock)
}

#[test]
fn full_adder_all_eight_rows() {
    for a in 0..=1 {
        for b in 0..=1 {
            for cin in 0..=1 {
                let (sum, carry) = alu::full_adder(a != 0, b != 0, cin != 0);
                assert_eq!(u8::from(sum) + 2 * u8::from(carry), a + b + cin);
            }
        }
    }
}
#[test]
fn exhaustive_eight_bit_arithmetic() {
    for a in 0u64..256 {
        for b in 0u64..256 {
            assert_eq!(alu::add(a, b, 8, false), ((a + b) & 255, a + b > 255));
            assert_eq!(alu::sub(a, b, 8), (a.wrapping_sub(b) & 255, a < b));
            assert_eq!(alu::mul(a, b, 8), ((a * b) & 255, a * b > 255));
            assert_eq!(
                alu::div(a, b, 8),
                a.checked_div(b).map(|quotient| (quotient, a % b))
            );
        }
    }
}
#[test]
fn boundaries_and_random_64_bit_arithmetic() {
    let values = [0, 1, 2, 3, 63, 64, 1 << 32, 1 << 63, u64::MAX - 1, u64::MAX];
    let check = |a: u64, b: u64| {
        assert_eq!(alu::add(a, b, 64, false), a.overflowing_add(b));
        assert_eq!(alu::sub(a, b, 64), a.overflowing_sub(b));
        assert_eq!(alu::mul(a, b, 64), a.overflowing_mul(b));
        assert_eq!(
            alu::div(a, b, 64),
            a.checked_div(b).map(|quotient| (quotient, a % b))
        );
    };
    for a in values {
        for b in values {
            check(a, b);
        }
    }
    let mut seed = 0x1234_5678_9ABC_DEF0u64;
    for _ in 0..10_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let a = seed;
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        check(a, seed);
    }
}
#[test]
fn all_bus_widths() {
    for w in 1..=64 {
        let m = alu::mask(w);
        assert_eq!(alu::add(m, 1, w, false), (0, true));
        assert_eq!(alu::sub(0, 1, w), (m, true));
        assert_eq!(alu::mul(m, m, w), (1, w > 1));
        assert_eq!(alu::div(m, 1, w), Some((m, 0)));
    }
}
#[test]
fn gate_truth_tables() {
    for kind in [Kind::And, Kind::Or, Kind::Xor, Kind::Nand, Kind::Nor] {
        for a in 0..=1 {
            for b in 0..=1 {
                let mut c = Circuit::empty();
                let x = input(&mut c, a, 1);
                let y = input(&mut c, b, 1);
                let g = c.add(kind, 1, [0., 0.]);
                connect(&mut c, x, 0, g, 0);
                connect(&mut c, y, 0, g, 1);
                let mut s = Simulator::default();
                s.settle(&c).unwrap();
                let expected = match kind {
                    Kind::And => a & b,
                    Kind::Or => a | b,
                    Kind::Xor => a ^ b,
                    Kind::Nand => !(a & b) & 1,
                    _ => !(a | b) & 1,
                };
                assert_eq!(s.output(p(g, 0)), Signal::Value(expected));
            }
        }
    }
}
#[test]
fn half_and_full_adder_components() {
    for kind in [Kind::HalfAdder, Kind::FullAdder] {
        for a in 0..=1 {
            for b in 0..=1 {
                for cin in 0..=1 {
                    let mut c = Circuit::empty();
                    let g = c.add(kind, 1, [0., 0.]);
                    for (pin, value) in [a, b, cin]
                        .into_iter()
                        .take(if kind == Kind::FullAdder { 3 } else { 2 })
                        .enumerate()
                    {
                        let n = input(&mut c, value, 1);
                        connect(&mut c, n, 0, g, pin);
                    }
                    let mut s = Simulator::default();
                    s.settle(&c).unwrap();
                    let total = a + b + if kind == Kind::FullAdder { cin } else { 0 };
                    assert_eq!(s.output(p(g, 0)), Signal::Value(total & 1));
                    assert_eq!(s.output(p(g, 1)), Signal::Value(total >> 1));
                }
            }
        }
    }
}
#[test]
fn gate_level_demo_truth_table() {
    let mut c = demos::full_adder();
    let ids: Vec<_> = c
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Input)
        .map(|n| n.id)
        .collect();
    for row in 0..8 {
        for (bit, id) in ids.iter().enumerate() {
            set(&mut c, *id, (row >> bit) & 1);
        }
        let mut s = Simulator::default();
        s.settle(&c).unwrap();
        let output = |label: &str| {
            s.reading(&c, c.nodes.iter().find(|n| n.label == label).unwrap())
                .value()
                .unwrap()
        };
        assert_eq!(
            output("S") + 2 * output("Cout"),
            u64::from(row.count_ones())
        );
    }
}
#[test]
fn dff_only_samples_rising_edge() {
    let (mut c, mut s, id, inputs, _) = ff(Kind::Dff);
    set(&mut c, inputs[0], 1);
    s.propagate(&c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(0));
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(1));
    set(&mut c, inputs[0], 0);
    s.propagate(&c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(1));
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(1));
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(0));
}
#[test]
fn jk_set_hold_reset_toggle() {
    let (mut c, mut s, id, inputs, _) = ff(Kind::Jk);
    for (j, k, expected) in [(1, 0, 1), (0, 0, 1), (0, 1, 0), (1, 1, 1), (1, 1, 0)] {
        set(&mut c, inputs[0], j);
        set(&mut c, inputs[1], k);
        s.step(&mut c).unwrap();
        assert_eq!(s.output(p(id, 0)), Signal::Value(expected));
        s.step(&mut c).unwrap();
    }
}
#[test]
fn t_flip_flop_toggle_and_hold() {
    let (mut c, mut s, id, inputs, _) = ff(Kind::Tff);
    set(&mut c, inputs[0], 1);
    for expected in [1, 0, 1, 0] {
        s.step(&mut c).unwrap();
        assert_eq!(s.output(p(id, 0)), Signal::Value(expected));
        s.step(&mut c).unwrap();
    }
    set(&mut c, inputs[0], 0);
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(0));
}
#[test]
fn sr_invalid_state_and_recovery() {
    let (mut c, mut s, id, inputs, _) = ff(Kind::Sr);
    set(&mut c, inputs[0], 1);
    set(&mut c, inputs[1], 1);
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::InvalidSr);
    s.step(&mut c).unwrap();
    set(&mut c, inputs[0], 0);
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(id, 0)), Signal::Value(0));
}
#[test]
fn simultaneous_registers_do_not_race() {
    let (mut c, mut s, first, inputs, clock) = ff(Kind::Dff);
    let second = c.add(Kind::Dff, 1, [0., 0.]);
    connect(&mut c, first, 0, second, 0);
    connect(&mut c, clock, 0, second, 1);
    set(&mut c, inputs[0], 1);
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(first, 0)), Signal::Value(1));
    assert_eq!(s.output(p(second, 0)), Signal::Value(0));
    s.step(&mut c).unwrap();
    s.step(&mut c).unwrap();
    assert_eq!(s.output(p(second, 0)), Signal::Value(1));
}
#[test]
fn feedback_counter_wraps_after_256_clocks() {
    let mut c = demos::counter();
    let mut s = Simulator::default();
    s.reset(&mut c).unwrap();
    let id = c.nodes.iter().find(|n| n.kind == Kind::Dff).unwrap().id;
    for count in 1..=260 {
        s.step(&mut c).unwrap();
        assert_eq!(s.output(p(id, 0)), Signal::Value(count & 255));
        s.step(&mut c).unwrap();
    }
}
#[test]
fn alu_demo_all_operations_and_fault_isolation() {
    let mut c = demos::alu64();
    let mut s = Simulator::default();
    let opcode = c.nodes.iter().find(|n| n.width == 2).unwrap().id;
    for (op, result) in [(0, 49), (1, 35), (2, 294), (3, 6)] {
        set(&mut c, opcode, op);
        s.settle(&c).unwrap();
        let out = c.nodes.iter().find(|n| n.label == "Результат").unwrap();
        assert_eq!(s.reading(&c, out), Signal::Value(result));
    }
    let b = c.nodes.iter().find(|n| n.label == "B").unwrap().id;
    set(&mut c, b, 0);
    set(&mut c, opcode, 0);
    s.settle(&c).unwrap();
    assert_eq!(
        s.reading(&c, c.nodes.iter().find(|n| n.label == "Результат").unwrap()),
        Signal::Value(42)
    );
    set(&mut c, opcode, 3);
    s.settle(&c).unwrap();
    assert_eq!(
        s.reading(&c, c.nodes.iter().find(|n| n.label == "Результат").unwrap()),
        Signal::DivideByZero
    );
}
#[test]
fn validation_rejects_widths_cycles_duplicates_and_missing_ports() {
    let mut c = Circuit::empty();
    let a = input(&mut c, 0, 8);
    let b = c.add(Kind::Not, 1, [0., 0.]);
    assert!(c.connect(p(a, 0), p(b, 0)).is_err());
    assert!(c.wires.is_empty());
    assert!(c.connect(p(b, 0), p(b, 0)).is_err());
    assert!(c.connect(p(999, 0), p(b, 0)).is_err());
    let n = input(&mut c, 0, 1);
    connect(&mut c, n, 0, b, 0);
    c.wires.push(Wire {
        from: p(n, 0),
        to: p(b, 0),
    });
    assert!(c.validate().is_err());
    c.wires.pop();
    c.nodes.push(c.nodes[0].clone());
    assert!(c.validate().is_err());
}
#[test]
fn invalid_file_metadata_is_rejected() {
    let mut c = demos::alu64();
    c.version = 99;
    assert!(c.validate().is_err());
    c.version = 1;
    c.nodes[0].width = 0;
    assert!(c.validate().is_err());
    c.nodes[0].width = 65;
    assert!(c.validate().is_err());
    c.nodes[0].width = 64;
    c.nodes[0].position[0] = f32::NAN;
    assert!(c.validate().is_err());
}
#[test]
fn unknown_inputs_and_controlling_gate_values() {
    let mut c = Circuit::empty();
    let and = c.add(Kind::And, 1, [0., 0.]);
    let zero = input(&mut c, 0, 1);
    let mut s = Simulator::default();
    s.settle(&c).unwrap();
    assert_eq!(s.output(p(and, 0)), Signal::Unknown);
    connect(&mut c, zero, 0, and, 0);
    s.settle(&c).unwrap();
    assert_eq!(s.output(p(and, 0)), Signal::Value(0));
}
#[test]
fn storage_round_trip_and_replace_existing_file() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-storage");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("roundtrip.json");
    let c = demos::alu64();
    storage::save(&c, &path).unwrap();
    assert_eq!(storage::load(&path).unwrap(), c);
    let mut updated = c.clone();
    updated.name = "Изменённая схема".into();
    updated.nodes[0].value = u64::MAX;
    storage::save(&updated, &path).unwrap();
    assert_eq!(storage::load(&path).unwrap(), updated);
    std::fs::write(&path, b"{broken}").unwrap();
    assert!(storage::load(&path).is_err());
    std::fs::remove_file(path).unwrap();
}
#[test]
fn json_preserves_full_u64_precision_and_rejects_invalid_values() {
    let mut c = demos::alu64();
    c.nodes[0].value = u64::MAX;
    let text = serde_json::to_string(&c).unwrap();
    let decoded: Circuit = serde_json::from_str(&text).unwrap();
    assert_eq!(decoded, c);
    c.nodes[0].width = 1;
    assert!(c.validate().is_err());
}
#[test]
fn all_demos_validate() {
    for c in [
        demos::alu64(),
        demos::flip_flops(),
        demos::full_adder(),
        demos::counter(),
    ] {
        c.validate().unwrap();
    }
}
