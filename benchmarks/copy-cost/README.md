# copy-cost

Measures what a shared, in-place read of the canonical graph would avoid or pay (ADR 0021, section "The shared in-memory
graph"): the index of a graph, its validation, a full copy, and the projection of a view, against walking the graph in
place. Not a workspace member; the CI does not build it.

```sh
python3 benchmarks/copy-cost/gen.py                      # benchmarks/datasets/synthetic-{2k,20k,100k,200k}.json (git-ignored)
CARGO_TARGET_DIR=target cargo build --release --manifest-path benchmarks/copy-cost/Cargo.toml
for d in 2k 20k 100k 200k; do target/release/copy-cost benchmarks/datasets/synthetic-$d.json 15; done
```

Run it on an otherwise idle machine and record the machine, the revision, the load and the seeds with the output, as
`benchmarks/README.md` asks. Each figure is the median of 15 runs after one unmeasured warm-up, with min and max. A wide
spread between min and max means the machine was not idle; do not use that run.

The graphs come from `benchmarks/generate.py::synthetic` (random hierarchy of three levels with locality, cycles,
self-loops, parallel and unresolved edges). 2k and 20k use the seeds of the repository's datasets (11, 12); 100k and 200k
use seeds 13 and 14 chosen for this measurement. "Level 1 collapsed" and "top level collapsed" collapse the containers
`g1-*` and `g2-*` of the generator.

The crate is outside the workspace, so its `Cargo.lock` names the version of `arqlet-model` and `arqlet-view` and goes
stale at every engine version bump (`--locked` then fails). Update it with
`cargo check --offline --manifest-path benchmarks/copy-cost/Cargo.toml` and commit the two changed version lines.

What is not measured: layout, routing, the view selection of an import, memory use, and any graph that is not this
generator's. The figures say how the copying and indexing cost grows with the graph; they say nothing about an engine
that does not exist yet.
