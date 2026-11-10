//! Baseline for interactive edits: how long a full recomputation takes per operation, per scene.
//!
//! Usage: `cargo run --release -p arqlet-runtime --example interaction_baseline -- benchmarks/scenes`
//! Output: one TSV line per scene and operation (median milliseconds of `RUNS` runs, the outcome or the refusal).
//! Operations the engine does not have (moving a node) are not measured; the report says so.

use arqlet_model::{
    DirectionConstraint, LayoutStrategy, NodeSize, Orientation, RoutingConstraint, RoutingStyle,
    StrategyConstraint,
};
use arqlet_model::{MovePhase, MoveStep, Point, Snapshot};
use arqlet_runtime::{
    import_view, measure_snapshot, move_node, set_direction, set_routing, set_strategy,
};
use std::collections::BTreeMap;
use std::time::Instant;

const RUNS: usize = 5;

fn median_ms(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn timed<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..RUNS {
        let start = Instant::now();
        last = Some(f());
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    (median_ms(times), last.expect("at least one run"))
}

fn outcome<T, E: std::fmt::Display>(result: &Result<T, E>) -> String {
    match result {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("refused: {e}"),
    }
}

/// One drag, phase by phase: begin, four updates, finish keeping the pin, a second begin and a finish releasing it. Each phase
/// is timed once per run; every run starts from the same snapshot. Also counts the unrelated nodes whose boxes move.
fn drag(start: &Snapshot, node: &str) -> (Vec<(String, f64, String)>, usize) {
    let origin = start.boxes[node].clone();
    let mut phases: Vec<(String, Vec<f64>, String)> = Vec::new();
    let mut moved_unrelated = 0;
    for _ in 0..RUNS {
        let mut current = start.clone();
        let mut seq = 0;
        let mut run = |current: &mut Snapshot,
                       seq: &mut u64,
                       label: &str,
                       phase: MovePhase,
                       position: Option<Point>,
                       release: bool,
                       phases: &mut Vec<(String, Vec<f64>, String)>| {
            *seq += 1;
            let request = MoveStep {
                gesture_id: "bench".into(),
                seq: *seq,
                phase,
                node: node.into(),
                position,
                release,
                others: Default::default(),
            };
            let (ms, result) =
                timed(|| move_node(current, current.revision, current.layout_version, &request));
            let text = outcome(&result);
            if let Ok(next) = &result {
                *current = next.clone();
            }
            match phases.iter_mut().find(|(l, _, _)| l == label) {
                Some((_, times, _)) => times.push(ms),
                None => phases.push((label.to_string(), vec![ms], text)),
            }
        };
        let at = |x: f64, y: f64| Some(Point { x, y });
        run(
            &mut current,
            &mut seq,
            "move-begin",
            MovePhase::Begin,
            at(origin.x, origin.y),
            false,
            &mut phases,
        );
        for k in 1..=4 {
            let position = at(origin.x + 15. * k as f64, origin.y + 9. * k as f64);
            run(
                &mut current,
                &mut seq,
                "move-update",
                MovePhase::Update,
                position,
                false,
                &mut phases,
            );
        }
        run(
            &mut current,
            &mut seq,
            "move-finish-keep",
            MovePhase::Finish,
            None,
            false,
            &mut phases,
        );
        let unrelated = start
            .boxes
            .iter()
            .filter(|(id, rect)| {
                id.as_str() != node
                    && !current.boxes.get(*id).is_some_and(|b| {
                        b.x == rect.x
                            && b.y == rect.y
                            && b.width == rect.width
                            && b.height == rect.height
                    })
            })
            .count();
        moved_unrelated = moved_unrelated.max(unrelated);
        run(
            &mut current,
            &mut seq,
            "move-begin-release",
            MovePhase::Begin,
            at(origin.x, origin.y),
            false,
            &mut phases,
        );
        run(
            &mut current,
            &mut seq,
            "move-finish-release",
            MovePhase::Finish,
            None,
            true,
            &mut phases,
        );
    }
    let rows = phases
        .into_iter()
        .map(|(label, times, text)| (label, median_ms(times), text))
        .collect();
    (rows, moved_unrelated)
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "benchmarks/scenes".into());
    let mut scenes: Vec<_> = std::fs::read_dir(&dir)
        .expect("scene directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    scenes.sort();
    println!("scene\toperation\tmedian_ms\toutcome");
    for path in scenes {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).expect("scene file");
        let (import_ms, imported) = timed(|| import_view(&text, 500));
        let snapshot = match imported {
            Ok(snapshot) => snapshot,
            Err(e) => {
                println!("{name}\timport\t{import_ms:.3}\trefused: {e}");
                continue;
            }
        };
        println!("{name}\timport\t{import_ms:.3}\tok");
        let (rev, layout) = (snapshot.revision, snapshot.layout_version);
        let first = snapshot.graph.nodes[0].id.clone();

        let mut sizes = BTreeMap::new();
        sizes.insert(
            first.clone(),
            NodeSize {
                width: 220.,
                height: 90.,
            },
        );
        let (ms, result) = timed(|| measure_snapshot(&snapshot, rev, layout, sizes.clone()));
        println!("{name}\tresize-measure\t{ms:.3}\t{}", outcome(&result));

        let (ms, result) = timed(|| {
            set_routing(
                &snapshot,
                rev,
                layout,
                None,
                Some(RoutingConstraint::Fixed(RoutingStyle::Straight)),
            )
        });
        println!(
            "{name}\troot-routing-straight\t{ms:.3}\t{}",
            outcome(&result)
        );

        let (ms, result) = timed(|| {
            set_direction(
                &snapshot,
                rev,
                layout,
                None,
                Some(DirectionConstraint::Fixed(Orientation::Tb)),
            )
        });
        println!("{name}\troot-direction-tb\t{ms:.3}\t{}", outcome(&result));

        let (ms, result) = timed(|| {
            set_strategy(
                &snapshot,
                rev,
                layout,
                None,
                Some(StrategyConstraint::Fixed(LayoutStrategy::Tree)),
            )
        });
        println!("{name}\troot-strategy-tree\t{ms:.3}\t{}", outcome(&result));
        // A drag on the first leaf, phase by phase, as move-node steps on the same snapshot.
        let leaf = snapshot
            .graph
            .nodes
            .iter()
            .find(|n| {
                !snapshot
                    .graph
                    .nodes
                    .iter()
                    .any(|m| m.parent_id.as_deref() == Some(n.id.as_str()))
            })
            .map(|n| n.id.clone());
        if let Some(leaf) = leaf {
            let (rows, unrelated) = drag(&snapshot, &leaf);
            for (label, ms, text) in rows {
                println!("{name}\t{label}\t{ms:.3}\t{text}");
            }
            println!("{name}\tstability-unrelated-moved\t{unrelated}\tcount");
        }
    }
}
