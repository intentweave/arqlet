//! Cost of the copies and indexes that a shared read of the canonical graph would avoid or pay (ADR 0021).
//!
//! `cargo run --release -- GRAPH.json [reps]`. Prints one block per graph: one unmeasured warm-up, then `reps` measured
//! runs, median with min and max. Run on an otherwise idle machine; the machine line is the caller's to record.
use arqlet_model::{Graph, GraphIndex};
use arqlet_view::{project, project_indexed};
use std::collections::BTreeSet;
use std::hint::black_box;
use std::time::Instant;

type Stats = (f64, f64, f64);

fn stats(mut samples: Vec<f64>) -> Stats {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in timings"));
    (
        samples[samples.len() / 2],
        samples[0],
        samples[samples.len() - 1],
    )
}

fn time<T>(reps: usize, mut f: impl FnMut() -> T) -> Stats {
    black_box(f());
    let mut samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = Instant::now();
        black_box(f());
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    stats(samples)
}

fn row(name: &str, note: &str, s: Stats) {
    println!(
        "  {name:62} median {:>9.2} ms  min {:>9.2}  max {:>9.2}  {note}",
        s.0, s.1, s.2
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: copy-cost GRAPH.json [reps]");
    let reps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(15);
    let text = std::fs::read_to_string(path).expect("readable graph file");
    let start = Instant::now();
    let g: Graph = serde_json::from_str(&text).expect("a graph");
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    let name = path.rsplit('/').next().unwrap_or(path);
    println!(
        "{name}: nodes={} edges={} reps={reps} (JSON parse into Graph, single run: {parse_ms:.1} ms)",
        g.nodes.len(),
        g.edges.len()
    );

    let index = GraphIndex::of(&g);
    // Containers by level, from the ids of `benchmarks/generate.py::synthetic`: g0 innermost groups, g2 top level.
    let level = |prefix: &str| -> BTreeSet<String> {
        g.nodes
            .iter()
            .filter(|n| n.id.starts_with(prefix))
            .map(|n| n.id.clone())
            .collect()
    };
    let none = BTreeSet::new();
    let mid = level("g1-");
    let top = level("g2-");

    row(
        "in place: walk every node and edge by index",
        "(lower bound of a shared read)",
        time(reps, || {
            let mut sum = 0usize;
            for (i, n) in g.nodes.iter().enumerate() {
                sum += n.id.len() + index.parent[i].unwrap_or(0);
            }
            for (a, b) in &index.ends {
                sum += a.unwrap_or(0) + b.unwrap_or(0);
            }
            sum
        }),
    );
    row(
        "GraphIndex::of(&graph)",
        "",
        time(reps, || GraphIndex::of(&g)),
    );
    row(
        "arqlet_model::validate(&graph)",
        "",
        time(reps, || arqlet_model::validate(&g).expect("valid graph")),
    );
    row("Graph::clone() (a full copy)", "", time(reps, || g.clone()));
    for (label, set) in [
        ("nothing collapsed", &none),
        ("level 1 collapsed", &mid),
        ("top level collapsed", &top),
    ] {
        let shown = project_indexed(&g, &index, set).expect("projection");
        let note = format!(
            "-> view nodes={} edges={}",
            shown.nodes.len(),
            shown.edges.len()
        );
        row(
            &format!("project_indexed, {label}"),
            &note,
            time(reps, || {
                project_indexed(&g, &index, set).expect("projection")
            }),
        );
    }
    row(
        "project (validate + index + project), nothing collapsed",
        "",
        time(reps, || project(&g, &none).expect("projection")),
    );
    row(
        "project (validate + index + project), top level collapsed",
        "",
        time(reps, || project(&g, &top).expect("projection")),
    );
}
