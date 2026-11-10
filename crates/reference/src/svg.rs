//! SVG encoding of a curated reference document (C0). The visible drawing is derived from the document's curated
//! geometry; the canonical document is embedded in `<metadata id="layerweave-reference">` as hex, with its payload
//! hash. `import` accepts only text that `export` produces for the embedded document, byte for byte, so a tampered
//! drawing, a tampered payload, non-canonical JSON or unsafe markup is rejected. Pure string handling: no files, no
//! network, no scripts.
use std::fmt::Write;

use crate::canonical::canonical_json;
use crate::document::{self, ReferenceDocument};

pub const METADATA_ID: &str = "layerweave-reference";
/// Largest accepted SVG text, in bytes.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Markup that a reference never needs and that could execute or load something.
const UNSAFE: [&str; 5] = [
    "<!doctype",
    "<!entity",
    "<script",
    "<foreignobject",
    "<iframe",
];

/// Writes the SVG encoding of a document. The document is validated first.
pub fn export(document: &ReferenceDocument) -> Result<String, String> {
    let value = serde_json::to_value(document).map_err(|e| e.to_string())?;
    let canonical = document::canonical_text(&value)?;
    let payload = document::payload_hash(document)?;
    let (min_x, min_y, width, height) = bounds(document);
    let mut out = String::new();
    writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{min_x} {min_y} {width} {height}" width="{width}" height="{height}">"#
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        out,
        r#"<metadata id="{METADATA_ID}" data-format="{}" data-payload-hash="{payload}" data-encoding="hex">{}</metadata>"#,
        document::FORMAT,
        hex_encode(canonical.as_bytes())
    )
    .map_err(|e| e.to_string())?;
    let labels: std::collections::BTreeMap<&str, &str> = document
        .input
        .graph
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.as_str(),
                if n.label.is_empty() {
                    n.id.as_str()
                } else {
                    n.label.as_str()
                },
            )
        })
        .collect();
    for (id, rect) in &document.curated.nodes {
        writeln!(
            out,
            r##"<rect data-node="{}" x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#333"/>"##,
            escape(id),
            rect.x,
            rect.y,
            rect.width,
            rect.height
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            out,
            r#"<text x="{}" y="{}" font-size="12">{}</text>"#,
            rect.x + 4.,
            rect.y + 16.,
            escape(labels.get(id.as_str()).copied().unwrap_or(id))
        )
        .map_err(|e| e.to_string())?;
    }
    let rounded = rounded_routes(document);
    for (id, points) in &document.curated.routes {
        let points = match rounded.get(id) {
            Some(obstacles) => arqlet_scene::realize::round_corners(
                points,
                arqlet_scene::realize::CORNER_RADIUS,
                obstacles,
            ),
            None => points.clone(),
        };
        let list: Vec<String> = points.iter().map(|p| format!("{},{}", p.x, p.y)).collect();
        writeln!(
            out,
            r##"<polyline data-edge="{}" points="{}" fill="none" stroke="#666"/>"##,
            escape(id),
            list.join(" ")
        )
        .map_err(|e| e.to_string())?;
    }
    for (id, port) in &document.curated.ports {
        writeln!(
            out,
            r#"<circle data-port="{}" cx="{}" cy="{}" r="2"/>"#,
            escape(id),
            port.position.x,
            port.position.y
        )
        .map_err(|e| e.to_string())?;
    }
    out.push_str("</svg>\n");
    Ok(out)
}

/// The routes drawn with rounded corners (by the routing styles of the baseline view), each with the nodes its arcs must keep out of:
/// every node except the two ends and the containers around them. The arcs are those of the scene (`realize::round_corners`), so the
/// export draws the path the viewer draws.
fn rounded_routes(
    document: &ReferenceDocument,
) -> std::collections::BTreeMap<String, Vec<arqlet_scene::geom::Rect>> {
    let Ok(scene) = arqlet_scene::build_scene(&document.baseline) else {
        return Default::default();
    };
    let graph = &document.input.graph;
    let parent: std::collections::BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let edges: std::collections::BTreeMap<&str, &arqlet_model::Edge> =
        graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    scene
        .routes
        .iter()
        .filter(|r| r.rounded)
        .filter_map(|r| {
            let edge = edges.get(r.id.as_str())?;
            let mut own = std::collections::BTreeSet::new();
            for end in [Some(edge.source.as_str()), edge.target.as_deref()] {
                let mut at = end;
                while let Some(id) = at {
                    own.insert(id);
                    at = parent.get(id).copied().flatten();
                }
            }
            let obstacles = document
                .curated
                .nodes
                .iter()
                .filter(|(id, _)| {
                    !own.contains(id.as_str())
                        || id.as_str() == edge.source
                        || edge.target.as_deref() == Some(id.as_str())
                })
                .map(|(_, rect)| arqlet_scene::geom::Rect {
                    x0: rect.x,
                    y0: rect.y,
                    x1: rect.x + rect.width,
                    y1: rect.y + rect.height,
                })
                .collect();
            Some((r.id.clone(), obstacles))
        })
        .collect()
}

/// Reads an SVG written by [`export`]. Every check failure is an explicit error.
pub fn import(text: &str) -> Result<ReferenceDocument, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("SVG exceeds {MAX_BYTES} bytes"));
    }
    let lower = text.to_ascii_lowercase();
    if let Some(bad) = UNSAFE.iter().find(|m| lower.contains(*m)) {
        return Err(format!("unsafe markup rejected: {bad}"));
    }
    let open = format!(r#"<metadata id="{METADATA_ID}" "#);
    if text.matches(&open).count() != 1 {
        return Err("expected exactly one layerweave-reference metadata element".into());
    }
    let start = text.find(&open).unwrap();
    let tag_end = text[start..].find('>').ok_or("unterminated metadata tag")? + start;
    let tag = &text[start..tag_end];
    if attribute(tag, "data-format") != Some(document::FORMAT) {
        return Err("metadata format is not a arqlet curated reference".into());
    }
    if attribute(tag, "data-encoding") != Some("hex") {
        return Err("metadata encoding must be hex".into());
    }
    let declared_payload =
        attribute(tag, "data-payload-hash").ok_or("metadata has no payload hash")?;
    let body_end = text[tag_end..]
        .find("</metadata>")
        .ok_or("unterminated metadata element")?
        + tag_end;
    let bytes = hex_decode(&text[tag_end + 1..body_end])?;
    let canonical = String::from_utf8(bytes).map_err(|_| "metadata is not UTF-8".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&canonical).map_err(|e| format!("metadata is not JSON: {e}"))?;
    if canonical_json(&value)? != canonical {
        return Err("metadata is not in canonical form".into());
    }
    let parsed: ReferenceDocument =
        serde_json::from_value(value).map_err(|e| format!("invalid reference: {e}"))?;
    document::canonical_text(&serde_json::to_value(&parsed).map_err(|e| e.to_string())?)?;
    let payload = document::payload_hash(&parsed)?;
    if payload != declared_payload || payload != parsed.integrity.payload_hash {
        return Err("payload hash does not match the embedded document".into());
    }
    if export(&parsed)? != text {
        return Err("visible drawing does not match the embedded document".into());
    }
    Ok(parsed)
}

fn bounds(document: &ReferenceDocument) -> (f64, f64, f64, f64) {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for rect in document.curated.nodes.values() {
        points.push((rect.x, rect.y));
        points.push((rect.x + rect.width, rect.y + rect.height));
    }
    for route in document.curated.routes.values() {
        points.extend(route.iter().map(|p| (p.x, p.y)));
    }
    if points.is_empty() {
        return (0., 0., 100., 100.);
    }
    let margin = 20.;
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - margin;
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) - margin;
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max) + margin;
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) + margin;
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(r#"{name}=""#);
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("hex metadata has odd length".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16)
                .ok()
                .filter(|_| {
                    text[i..i + 2]
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
                .ok_or_else(|| "hex metadata contains a non-hex character".to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saved reference from the synthetic fixtures; its geometry was edited by hand, which the round trip must keep.
    const FIXTURE: &str =
        include_str!("../../../examples/fixtures/reference/state-tcp-moved.lwref.json");

    fn fixture() -> ReferenceDocument {
        serde_json::from_str(FIXTURE).expect("fixture parses")
    }

    #[test]
    fn export_then_import_returns_the_same_document() {
        let document = fixture();
        let text = export(&document).unwrap();
        let back = import(&text).unwrap();
        assert_eq!(back.integrity.payload_hash, document.integrity.payload_hash);
        assert_eq!(export(&back).unwrap(), text, "the encoding is stable");
    }

    #[test]
    fn a_changed_visible_rectangle_is_rejected() {
        let text = export(&fixture()).unwrap();
        let tampered = text.replacen(r#"x="24""#, r#"x="25""#, 1);
        assert_ne!(tampered, text, "the fixture has a node at x=24");
        assert!(import(&tampered).is_err());
    }

    #[test]
    fn a_changed_payload_is_rejected() {
        let text = export(&fixture()).unwrap();
        let start = text.find(">").unwrap() + 1;
        let mut bytes = text.clone().into_bytes();
        // Flip one hex digit inside the metadata body (after its opening tag).
        let index = text[start..]
            .find('7')
            .map(|i| i + start)
            .unwrap_or(start + 2);
        bytes[index] = if bytes[index] == b'7' { b'8' } else { b'7' };
        assert!(import(&String::from_utf8(bytes).unwrap()).is_err());
    }

    #[test]
    fn a_changed_payload_hash_attribute_is_rejected() {
        let text = export(&fixture()).unwrap();
        let hash = document::payload_hash(&fixture()).unwrap();
        let tampered = text.replace(&hash, &"0".repeat(64));
        assert!(import(&tampered).is_err());
    }

    #[test]
    fn unsafe_markup_is_rejected() {
        let text = export(&fixture()).unwrap();
        let with_script = text.replacen("<svg", "<script>alert(1)</script><svg", 1);
        assert!(import(&with_script).unwrap_err().contains("unsafe"));
        let with_doctype = format!("<!DOCTYPE svg>\n{text}");
        assert!(import(&with_doctype).unwrap_err().contains("unsafe"));
    }

    #[test]
    fn missing_or_duplicated_metadata_is_rejected() {
        let text = export(&fixture()).unwrap();
        assert!(import("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").is_err());
        let duplicated = format!("{text}{text}");
        assert!(import(&duplicated).is_err());
    }

    #[test]
    fn a_non_canonical_embedding_is_rejected() {
        let document = fixture();
        let pretty =
            serde_json::to_string_pretty(&serde_json::to_value(&document).unwrap()).unwrap();
        let text = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><metadata id="{METADATA_ID}" data-format="{}" data-payload-hash="{}" data-encoding="hex">{}</metadata></svg>"#,
            document::FORMAT,
            document::payload_hash(&document).unwrap(),
            hex_encode(pretty.as_bytes())
        );
        assert!(import(&text).unwrap_err().contains("canonical"));
    }

    #[test]
    fn labels_are_escaped() {
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }
}
