//! Native precomputation of a bounded graph view.
use std::{env, fs, process};
fn main() {
    // A clock for phase timings (`timings: true` on a request); the layout never reads it.
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(std::time::Instant::now);
    arqlet_model::clock::install(|| {
        ORIGIN
            .get()
            .map_or(0., |o| o.elapsed().as_secs_f64() * 1000.)
    });
    if let Err(e) = run() {
        eprintln!("{e}");
        process::exit(1)
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 4 {
        return Err("Usage: snapshot GRAPH.json OUTPUT.json NODE_LIMIT".into());
    }
    let s = arqlet_runtime::import_view(&fs::read_to_string(&args[1])?, args[3].parse()?)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    serde_json::to_writer_pretty(&mut file, &s)?;
    println!(
        "{} nodes, {} resolved edges, {} unresolved edges omitted",
        s.graph.nodes.len(),
        s.graph.edges.len(),
        s.omitted_unresolved_edges
    );
    Ok(())
}
