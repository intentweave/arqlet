//! All integration tests of the runtime in one test binary: one compile and one link after a change instead of one per
//! file, and the tests of all files run in parallel. Each file stays a module of its own (its `include_str!` paths are
//! relative to it). Run one file's tests with `cargo test -p arqlet-runtime --test all <module>::`.

#[path = "abstract_pipeline.rs"]
mod abstract_pipeline;
#[path = "circle_containers.rs"]
mod circle_containers;
#[path = "containers_straight.rs"]
mod containers_straight;
#[path = "curation_labels.rs"]
mod curation_labels;
#[path = "curation_locks.rs"]
mod curation_locks;
#[path = "curation_reroute.rs"]
mod curation_reroute;
#[path = "directed_repair.rs"]
mod directed_repair;
#[path = "drag_region.rs"]
mod drag_region;
#[path = "edge_weights.rs"]
mod edge_weights;
#[path = "elk.rs"]
mod elk;
#[path = "engine_choice.rs"]
mod engine_choice;
#[path = "explicit_constraints.rs"]
mod explicit_constraints;
#[path = "filter.rs"]
mod filter;
#[path = "force_directed.rs"]
mod force_directed;
#[path = "geometry_update.rs"]
mod geometry_update;
#[path = "graph_roundtrip.rs"]
mod graph_roundtrip;
#[path = "interactive_continuation.rs"]
mod interactive_continuation;
#[path = "layered_construction.rs"]
mod layered_construction;
#[path = "local_edit_large.rs"]
mod local_edit_large;
#[path = "move_gesture.rs"]
mod move_gesture;
#[path = "move_only.rs"]
mod move_only;
#[path = "objectives.rs"]
mod objectives;
#[path = "option_defaults.rs"]
mod option_defaults;
#[path = "overlaps.rs"]
mod overlaps;
#[path = "overview.rs"]
mod overview;
#[path = "parallel_straight.rs"]
mod parallel_straight;
#[path = "petri_shapes.rs"]
mod petri_shapes;
#[path = "pin_search.rs"]
mod pin_search;
#[path = "pinned_overlap.rs"]
mod pinned_overlap;
#[path = "pins.rs"]
mod pins;
#[path = "reference.rs"]
mod reference;
#[path = "reference_corpus.rs"]
mod reference_corpus;
#[path = "reproduce.rs"]
mod reproduce;
#[path = "route_crossings.rs"]
mod route_crossings;
#[path = "rows.rs"]
mod rows;
#[path = "shape_change.rs"]
mod shape_change;
#[path = "shape_geometry.rs"]
mod shape_geometry;
#[path = "single_pass.rs"]
mod single_pass;
#[path = "sparse_stress.rs"]
mod sparse_stress;
#[path = "structured_communities.rs"]
mod structured_communities;
