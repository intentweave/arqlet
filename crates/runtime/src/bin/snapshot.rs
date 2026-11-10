//! Native precomputation of a bounded graph view.
use std::{env, fs, process};
fn main() {
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
