//! Measurement of Louvain against Leiden (#237), not a check: ignored by default and run by
//! `benchmarks/overview/communities.py`, which passes the graphs and writes the results file. For every container that
//! `with_communities` would split (more children than the threshold, at least half of them linked), both algorithms run
//! on the links between its children; one JSON line per container and algorithm on stdout: modularity of the final
//! partition (resolution 1), communities and their sizes, levels, communities without a link between their parts (final
//! level and any level), work units and time. Then one line per graph and algorithm for the whole clustering
//! (`with_communities_within`): clusters added, work and time.
//!
//!   ARQLET_COMMUNITY_EVAL='name=path.json;…' ARQLET_COMMUNITY_THRESHOLD=120 \
//!     cargo test --release -p arqlet-layout --test community_eval -- --ignored --nocapture
use arqlet_layout::overview::{
    community_levels, hierarchy, with_communities_within, Budget, CommunityAlgorithm, Container,
    Detection, Hubs, Limit,
};
use arqlet_model::Graph;
use std::time::Instant;

/// The final community of every member, and how many communities each level has.
fn flatten(k: usize, levels: &[Vec<usize>]) -> Vec<usize> {
    let mut of: Vec<usize> = (0..k).collect();
    for level in levels {
        for c in of.iter_mut() {
            *c = level[*c];
        }
    }
    of
}

fn modularity(k: usize, links: &[(usize, usize, f64)], of: &[usize]) -> f64 {
    let mut degree = vec![0f64; k];
    let mut total = 0.;
    let communities = of.iter().copied().max().map_or(0, |m| m + 1);
    let mut inside = vec![0f64; communities];
    for &(a, b, w) in links {
        degree[a] += w;
        degree[b] += w;
        total += 2. * w;
        if of[a] == of[b] {
            inside[of[a]] += 2. * w;
        }
    }
    if total == 0. {
        return 0.;
    }
    let mut tot = vec![0f64; communities];
    for i in 0..k {
        tot[of[i]] += degree[i];
    }
    (0..communities)
        .map(|c| inside[c] / total - (tot[c] / total) * (tot[c] / total))
        .sum()
}

/// Communities (of at least two members) whose members the links among them do not connect, and their members.
fn disconnected(k: usize, links: &[(usize, usize, f64)], of: &[usize]) -> (usize, usize) {
    let mut adjacent = vec![Vec::new(); k];
    for &(a, b, _) in links {
        if of[a] == of[b] && a != b {
            adjacent[a].push(b);
            adjacent[b].push(a);
        }
    }
    let communities = of.iter().copied().max().map_or(0, |m| m + 1);
    let mut pieces = vec![0usize; communities];
    let mut size = vec![0usize; communities];
    let mut seen = vec![false; k];
    for first in 0..k {
        size[of[first]] += 1;
        if seen[first] {
            continue;
        }
        pieces[of[first]] += 1;
        seen[first] = true;
        let mut stack = vec![first];
        while let Some(i) = stack.pop() {
            for &j in &adjacent[i] {
                if !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    let bad: Vec<usize> = (0..communities).filter(|&c| pieces[c] > 1).collect();
    (bad.len(), bad.iter().map(|&c| size[c]).sum())
}

fn detection(algorithm: CommunityAlgorithm) -> Detection {
    Detection {
        algorithm,
        resolution: 1.,
    }
}

fn name(algorithm: CommunityAlgorithm) -> &'static str {
    match algorithm {
        CommunityAlgorithm::Louvain => "louvain",
        CommunityAlgorithm::Leiden => "leiden",
    }
}

#[test]
#[ignore]
fn louvain_against_leiden() {
    let Ok(spec) = std::env::var("ARQLET_COMMUNITY_EVAL") else {
        eprintln!("ARQLET_COMMUNITY_EVAL not set: nothing measured");
        return;
    };
    let threshold: usize = std::env::var("ARQLET_COMMUNITY_THRESHOLD")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(120);
    let reps: usize = std::env::var("ARQLET_COMMUNITY_REPS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(3);
    let algorithms = [CommunityAlgorithm::Louvain, CommunityAlgorithm::Leiden];
    for item in spec.split(';').filter(|s| !s.is_empty()) {
        let (dataset, path) = item.split_once('=').expect("name=path");
        let text = std::fs::read_to_string(path).expect("graph file");
        let graph: Graph = serde_json::from_str(&text).expect("graph");
        let h = hierarchy(&graph);
        let containers: Vec<Container> = (0..h.ids.len())
            .filter(|&i| h.members(Container::Node(i)).len() > threshold)
            .map(Container::Node)
            .chain((h.members(Container::Root).len() > threshold).then_some(Container::Root))
            .collect();
        for container in containers {
            let k = h.members(container).len();
            let local: std::collections::BTreeMap<usize, usize> = h
                .members(container)
                .iter()
                .enumerate()
                .map(|(i, &m)| (m, i))
                .collect();
            let links: Vec<(usize, usize, f64)> = h
                .container_links(container)
                .iter()
                .map(|&(a, b, w)| (local[&a], local[&b], w))
                .collect();
            let mut linked = vec![false; k];
            for &(a, b, _) in &links {
                linked[a] = true;
                linked[b] = true;
            }
            let linked_count = linked.iter().filter(|l| **l).count();
            // As `with_communities`: a container of mostly unlinked children is not split.
            if linked_count * 2 < k {
                continue;
            }
            for algorithm in algorithms {
                let mut times = Vec::new();
                let mut levels = Vec::new();
                let mut work = 0;
                for _ in 0..reps {
                    let mut meter = Limit::new(u64::MAX);
                    let started = Instant::now();
                    levels =
                        community_levels(k, &links, &detection(algorithm), &mut meter).unwrap();
                    times.push(started.elapsed().as_secs_f64() * 1e3);
                    work = meter.used;
                }
                times.sort_by(f64::total_cmp);
                let of = flatten(k, &levels);
                // Only linked members count for the communities (those without links stay alone in both).
                let count = of.iter().copied().max().map_or(0, |m| m + 1);
                let mut sizes = vec![0usize; count];
                for (i, &c) in of.iter().enumerate() {
                    if linked[i] {
                        sizes[c] += 1;
                    }
                }
                let mut sizes: Vec<usize> = sizes.into_iter().filter(|&s| s > 0).collect();
                sizes.sort_unstable_by(|a, b| b.cmp(a));
                let multi = sizes.iter().filter(|&&s| s >= 2).count();
                let (bad, bad_members) = disconnected(k, &links, &of);
                // Any level: a community of a lower level that is not connected.
                let mut bad_any_level = 0;
                for l in 1..=levels.len() {
                    bad_any_level += disconnected(k, &links, &flatten(k, &levels[..l])).0;
                }
                let at_least_8 = sizes.iter().filter(|&&s| s >= 8).count();
                println!(
                    "{}",
                    serde_json::json!({
                        "row": "container",
                        "dataset": dataset,
                        "container": match container { Container::Root => "#".to_string(), Container::Node(i) => h.ids[i].clone() },
                        "children": k,
                        "linked": linked_count,
                        "links": links.len(),
                        "algorithm": name(algorithm),
                        "modularity": (modularity(k, &links, &of) * 1e6).round() / 1e6,
                        "levels": levels.len(),
                        "communities": multi,
                        "communitiesAtLeast8": at_least_8,
                        "largest": sizes.first().copied().unwrap_or(0),
                        "medianSize": if multi > 0 { sizes[multi / 2] } else { 0 },
                        "disconnected": bad,
                        "disconnectedMembers": bad_members,
                        "disconnectedAnyLevel": bad_any_level,
                        "work": work,
                        "medianMs": (times[times.len() / 2] * 1e3).round() / 1e3,
                    })
                );
            }
        }
        for algorithm in algorithms {
            let mut times = Vec::new();
            let mut added = 0;
            let mut work = 0;
            for _ in 0..reps {
                let mut budget = Budget::unlimited();
                let base = hierarchy(&graph);
                let started = Instant::now();
                let out = with_communities_within(
                    base,
                    threshold,
                    &Hubs::default(),
                    &detection(algorithm),
                    &mut budget,
                )
                .unwrap();
                times.push(started.elapsed().as_secs_f64() * 1e3);
                added = out.ids.len() - out.real;
                work = budget.clustering;
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({
                    "row": "graph",
                    "dataset": dataset,
                    "nodes": graph.nodes.len(),
                    "edges": graph.edges.len(),
                    "threshold": threshold,
                    "algorithm": name(algorithm),
                    "clustersAdded": added,
                    "work": work,
                    "medianMs": (times[times.len() / 2] * 1e3).round() / 1e3,
                })
            );
        }
    }
}
