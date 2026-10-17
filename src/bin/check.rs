use circuit_lab::{
    demos,
    model::Kind,
    simulator::{Signal, Simulator},
    storage,
};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--export-demos") {
        let dir = Path::new(args.get(1).map(String::as_str).unwrap_or("examples"));
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for (name, c) in [
            ("alu64.json", demos::alu64()),
            ("full-adder.json", demos::full_adder()),
            ("flip-flops.json", demos::flip_flops()),
            ("counter.json", demos::counter()),
        ] {
            storage::save(&c, &dir.join(name))?;
            println!("Exported {name}");
        }
        return Ok(());
    }
    let mut c = if let Some(path) = args.first() {
        storage::load(Path::new(path))?
    } else {
        demos::alu64()
    };
    let mut sim = Simulator::default();
    sim.reset(&mut c)?;
    let steps: usize = args
        .get(1)
        .map(|s| {
            s.parse()
                .map_err(|_| "Steps must be a non-negative integer")
        })
        .transpose()?
        .unwrap_or(0);
    for _ in 0..steps {
        sim.step(&mut c)?;
    }
    println!(
        "{} | {} nodes | {} wires | step {}",
        c.name,
        c.nodes.len(),
        c.wires.len(),
        sim.ticks
    );
    let mut failed = false;
    for n in c.nodes.iter().filter(|n| n.kind == Kind::Output) {
        let v = sim.reading(&c, n);
        println!("{} = {}", n.label, v.text());
        failed |= !matches!(v, Signal::Value(_));
    }
    if failed {
        return Err("Some outputs contain unknown or invalid signals".into());
    }
    Ok(())
}
