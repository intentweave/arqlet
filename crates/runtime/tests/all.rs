//! All integration tests of the runtime in one test binary: one compile and one link after a change instead of one per
//! file, and the tests of all files run in parallel. Each file stays a module of its own (its `include_str!` paths are
//! relative to it). Run one file's tests with `cargo test -p arqlet-runtime --test all <module>::`.

#[path = "abstract_pipeline.rs"]
mod abstract_pipeline;
#[path = "bookmark.rs"]
mod bookmark;
#[path = "bus_line.rs"]
mod bus_line;
#[path = "change_log.rs"]
mod change_log;
#[path = "channels_by_demand.rs"]
mod channels_by_demand;
#[path = "circle_containers.rs"]
mod circle_containers;
#[path = "community_groups.rs"]
mod community_groups;
#[path = "community_hubs.rs"]
mod community_hubs;
#[path = "containers_straight.rs"]
mod containers_straight;
#[path = "content.rs"]
mod content;
#[path = "curation_labels.rs"]
mod curation_labels;
#[path = "curation_locks.rs"]
mod curation_locks;
#[path = "curation_reroute.rs"]
mod curation_reroute;
#[path = "details.rs"]
mod details;
#[path = "directed_repair.rs"]
mod directed_repair;
#[path = "directions_first.rs"]
mod directions_first;
#[path = "drag_region.rs"]
mod drag_region;
#[path = "edge_weights.rs"]
mod edge_weights;
#[path = "elk.rs"]
mod elk;
#[path = "engine_choice.rs"]
mod engine_choice;
#[path = "estimate.rs"]
mod estimate;
#[path = "explicit_constraints.rs"]
mod explicit_constraints;
#[path = "filter.rs"]
mod filter;
#[path = "find.rs"]
mod find;
#[path = "force_directed.rs"]
mod force_directed;
#[path = "geometry_update.rs"]
mod geometry_update;
#[path = "graph_patch.rs"]
mod graph_patch;
#[path = "graph_roundtrip.rs"]
mod graph_roundtrip;
#[path = "holding.rs"]
mod holding;
#[path = "import_limit.rs"]
mod import_limit;
#[path = "interactive_continuation.rs"]
mod interactive_continuation;
#[path = "isolated_grid_from.rs"]
mod isolated_grid_from;
#[path = "lanes.rs"]
mod lanes;
#[path = "layered_construction.rs"]
mod layered_construction;
#[path = "level_budget.rs"]
mod level_budget;
#[path = "local_edit_large.rs"]
mod local_edit_large;
#[path = "membership_hulls.rs"]
mod membership_hulls;
#[path = "memberships.rs"]
mod memberships;
#[path = "memory_release.rs"]
mod memory_release;
#[path = "move_gesture.rs"]
mod move_gesture;
#[path = "move_only.rs"]
mod move_only;
#[path = "neighbourhood.rs"]
mod neighbourhood;
#[path = "nested_edges_by_detour.rs"]
mod nested_edges_by_detour;
#[path = "objectives.rs"]
mod objectives;
#[path = "option_defaults.rs"]
mod option_defaults;
#[path = "organic_forces.rs"]
mod organic_forces;
#[path = "overlaps.rs"]
mod overlaps;
#[path = "overview.rs"]
mod overview;
#[path = "overview_budget.rs"]
mod overview_budget;
#[path = "pack_components.rs"]
mod pack_components;
#[path = "paging.rs"]
mod paging;
#[path = "parallel_straight.rs"]
mod parallel_straight;
#[path = "peek.rs"]
mod peek;
#[path = "petri_shapes.rs"]
mod petri_shapes;
#[path = "pin_search.rs"]
mod pin_search;
#[path = "pinned_overlap.rs"]
mod pinned_overlap;
#[path = "pins.rs"]
mod pins;
#[path = "query.rs"]
mod query;
#[path = "reference.rs"]
mod reference;
#[path = "reference_corpus.rs"]
mod reference_corpus;
#[path = "region_child_ports.rs"]
mod region_child_ports;
#[path = "region_ports.rs"]
mod region_ports;
#[path = "reproduce.rs"]
mod reproduce;
#[path = "route_crossings.rs"]
mod route_crossings;
#[path = "rows.rs"]
mod rows;
#[path = "session_store.rs"]
mod session_store;
#[path = "shape_change.rs"]
mod shape_change;
#[path = "shape_geometry.rs"]
mod shape_geometry;
#[path = "single_pass.rs"]
mod single_pass;
#[path = "single_pass_reversed.rs"]
mod single_pass_reversed;
#[path = "source_delivery.rs"]
mod source_delivery;
#[path = "sparse_stress.rs"]
mod sparse_stress;
#[path = "stacked_containers.rs"]
mod stacked_containers;
#[path = "straight_detours.rs"]
mod straight_detours;
#[path = "structure_cache.rs"]
mod structure_cache;
#[path = "structured_communities.rs"]
mod structured_communities;
