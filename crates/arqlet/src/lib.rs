//! Arqlet: an incremental graph-layout engine for nested graphs with ports.
//!
//! **Provisional.** Arqlet is a prototype; every API, the JSON protocol and the snapshot format may change in any
//! release. This crate re-exports the engine's crates under one name.
//!
//! Every operation is a JSON request to one dispatcher, [`dispatch`]: the same protocol runs natively, in the browser
//! (WASM, npm package `@arqlet/engine-client`) and behind a backend. The response is `{"ok": true, ...}` or
//! `{"ok": false, "error": ...}`; it never panics on bad input.
//!
//! ```
//! // A graph in the JSON form ELK uses: two nodes and a cycle between them.
//! let request = r#"{"op": "elkLayout", "graph": {
//!   "id": "root",
//!   "children": [{"id": "a", "width": 40, "height": 30}, {"id": "b", "width": 40, "height": 30}],
//!   "edges": [{"id": "e1", "sources": ["a"], "targets": ["b"]},
//!             {"id": "e2", "sources": ["b"], "targets": ["a"]}]
//! }}"#;
//! let response = arqlet::dispatch(request);
//! // The graph comes back with positions (`x`, `y`) and edge `sections`.
//! assert!(response.contains(r#""ok":true"#));
//! ```
//!
//! The same requests exist as Rust values ([`protocol::Request`], provisional, ADR 0019): a host in Rust builds them
//! without JSON and gets a [`protocol::Response`].
//!
//! ```
//! use arqlet::protocol::{handle, Request, Response};
//! let graph = serde_json::from_str(r#"{"nodes": [
//!   {"id": "g", "kind": "group", "label": "G", "parentId": null},
//!   {"id": "a", "kind": "fn", "label": "A", "parentId": "g"},
//!   {"id": "b", "kind": "fn", "label": "B", "parentId": "g"}],
//!   "edges": [{"id": "ab", "kind": "calls", "source": "a", "target": "b"}]}"#).unwrap();
//! let Response::Snapshot(snapshot) = handle(Request::Import { graph, limit: 100, options: None }) else { panic!() };
//! assert_eq!(snapshot.graph.nodes.len(), 3);
//! ```
//!
//! A [`Session`] (provisional, ADR 0019) keeps a view's snapshot inside the engine: requests carry only the change, and
//! answers bring the new versions and the scene.
//!
//! ```
//! let mut session = arqlet::Session::new();
//! let graph = r#"{"nodes": [{"id": "g", "kind": "group", "label": "G", "parentId": null},
//!   {"id": "a", "kind": "fn", "label": "A", "parentId": "g"}], "edges": []}"#;
//! session.run(&format!(r#"{{"op": "import", "graph": {graph}, "limit": 10}}"#));
//! let answer = session.run(r#"{"op": "toggle", "nodeId": "g"}"#);
//! assert!(answer.contains(r#""collapsedIds":["g"]"#));
//! ```
//!
//! The crates, in order of the pipeline:
//!
//! - [`model`]: canonical graph, identities, revisions and layout options;
//! - [`view`]: visible and aggregated projection;
//! - [`layout`]: layering, ordering, placement, ports and constraints;
//! - [`routing`]: edge routes and bundles;
//! - [`scene`]: renderer-independent geometry and metrics;
//! - [`topology`]: planarity embedding and straight-line grid drawings;
//! - [`reference`]: curated reference diagrams;
//! - [`elk`]: adapter for graphs in ELK's JSON form;
//! - [`runtime`]: dispatcher, snapshots, patches and incremental scheduling.

pub use arqlet_elk as elk;
pub use arqlet_layout as layout;
pub use arqlet_model as model;
pub use arqlet_reference as reference;
pub use arqlet_routing as routing;
pub use arqlet_runtime as runtime;
pub use arqlet_runtime::engine_session::EngineSession as Session;
pub use arqlet_runtime::protocol;
pub use arqlet_scene as scene;
pub use arqlet_topology as topology;
pub use arqlet_view as view;

pub use arqlet_runtime::{dispatch, dispatch_streaming};
