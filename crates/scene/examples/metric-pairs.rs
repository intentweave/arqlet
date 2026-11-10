//! Focused native measurement; core metric code remains clock- and I/O-free.
use arqlet_scene::metrics::{
    measure_variants, measure_variants_exhaustive, measure_variants_indexed,
    measure_variants_with_pair_work, Drawing, PairMode,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Read, time::Instant};

#[derive(Deserialize)]
struct Case {
    name: String,
    drawing: Drawing,
}

fn run() -> Result<Value, String> {
    let reps: usize = std::env::args()
        .nth(1)
        .ok_or("Usage: metric-pairs REPS < cases.json")?
        .parse()
        .map_err(|_| "Invalid repetitions")?;
    if reps == 0 {
        return Err("Repetitions must be positive".into());
    }
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    let cases: Vec<Case> = serde_json::from_str(&input).map_err(|e| e.to_string())?;
    let variants = [(12., false), (24., true)];
    let mut results = Vec::new();
    for case in cases {
        let (expected, old_work) =
            measure_variants_with_pair_work(&case.drawing, &variants, PairMode::Legacy)?;
        let (actual, work) =
            measure_variants_with_pair_work(&case.drawing, &variants, PairMode::Indexed)?;
        let (shared, shared_work) =
            measure_variants_with_pair_work(&case.drawing, &variants, PairMode::Shared)?;
        if expected != actual
            || shared != expected
            || serde_json::to_vec(&expected).map_err(|e| e.to_string())?
                != serde_json::to_vec(&shared).map_err(|e| e.to_string())?
            || serde_json::to_vec(&expected).map_err(|e| e.to_string())?
                != serde_json::to_vec(&actual).map_err(|e| e.to_string())?
            || old_work.candidate_pairs != work.candidate_pairs
        {
            return Err(format!("Metric or pair-work mismatch in {}", case.name));
        }
        let mut samples = [Vec::new(), Vec::new(), Vec::new()];
        for round in 0..=reps {
            let mut order = [0, 1, 2];
            order.rotate_left(round % 3);
            if round % 2 == 1 {
                order.reverse();
            }
            for arm in order {
                let started = Instant::now();
                let measured = if arm == 0 {
                    measure_variants_exhaustive(&case.drawing, &variants)?
                } else if arm == 1 {
                    measure_variants(&case.drawing, &variants)?
                } else {
                    measure_variants_indexed(&case.drawing, &variants)?
                };
                let ms = started.elapsed().as_secs_f64() * 1000.;
                if measured != expected {
                    return Err(format!("Repeated metric mismatch in {}", case.name));
                }
                if round > 0 {
                    samples[arm].push(ms);
                }
            }
        }
        results.push(json!({
            "name":case.name,"nodes":case.drawing.nodes.len(),"edges":case.drawing.edges.len(),
            "exhaustiveMs":samples[0],"sharedMs":samples[1],"indexedMs":samples[2],"metrics":actual,
            "exhaustiveWork":work_json(&old_work),
            "sharedWork":work_json(&shared_work),
            "indexedWork":work_json(&work),
        }));
    }
    Ok(json!({"reps":reps,"variants":variants,"cases":results}))
}

fn main() {
    match run() {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn work_json(work: &arqlet_scene::metrics::PairWork) -> Value {
    json!({"boundsTests":work.bounds_tests,"candidatePairs":work.candidate_pairs,
        "segmentPairVisits":work.segment_pair_visits,"segmentBoundsComputed":work.segment_bounds_computed})
}
