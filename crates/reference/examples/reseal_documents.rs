//! One-off migration: a stored reference document whose graph changed its written form (`hybrid` became `auto`, the same
//! free direction with the compact choice) is rewritten with its input hash and payload hash recomputed. Usage:
//! `cargo run -p arqlet-reference --example reseal_documents -- FILE...`. Each file is rewritten in place.
use arqlet_reference::canonical::canonical_json;
use arqlet_reference::document::{input_hash, seal, ReferenceDocument};

fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut document: ReferenceDocument = serde_json::from_str(&text).unwrap();
        document.input_hash = input_hash(&document.input).unwrap();
        seal(&mut document).unwrap();
        let value = serde_json::to_value(&document).unwrap();
        std::fs::write(&path, canonical_json(&value).unwrap() + "\n").unwrap();
        println!("resealed {path}");
    }
}
