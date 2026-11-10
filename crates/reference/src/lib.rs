//! Curated diagram references: canonical JSON, SHA-256 identities and validation of the reference document.
//!
//! Provisional C0 contract. The browser editor and the eval runner use it only through the runtime dispatcher.

pub mod canonical;
pub mod document;
pub mod ports;
pub mod route;
pub mod svg;

pub use canonical::{canonical_json, sha256_hex, CANONICAL_FORMAT};
pub use document::{
    canonical_text, create, payload_hash, refresh, reroute, seal, validate, ReferenceDocument,
    Report,
};
