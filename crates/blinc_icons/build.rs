//! Build script for blinc_icons
//!
//! Parses Lucide SVG files and generates Rust const declarations.

use roxmltree::Document;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/lucide");

    let icons_dir = Path::new("assets/lucide");
    let out_path = Path::new("src/icons.rs");

    // Check if icons directory exists
    if !icons_dir.exists() {
        eprintln!("Warning: assets/lucide directory not found, creating empty icons.rs");
        fs::write(
            out_path,
            "//! Generated icon constants - no icons found\n\npub const CHECK: &str = \"\";\n",
        )
        .expect("Failed to write icons.rs");
        return;
    }

    let mut output = String::from(
        "//! Generated Lucide icon constants
//!
//! Auto-generated from Lucide SVG files - DO NOT EDIT
//!
//! Each icon is a `&'static str` containing the SVG inner elements.
//! Use `blinc_icons::to_svg()` to wrap in a complete SVG tag.
//!
//! Unused icons are automatically eliminated by DCE (Dead Code Elimination).

",
    );

    // Collect all icons
    let mut icons: Vec<(String, String, String, String)> = Vec::new(); // (const_name, path_data, doc_comment, lucide_name)

    for entry in walkdir::WalkDir::new(icons_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "svg"))
    {
        let file_name = entry.path().file_stem().unwrap().to_str().unwrap();
        let content = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Failed to read {}: {}", entry.path().display(), e);
                continue;
            }
        };

        if let Some((const_name, path_data)) = parse_svg(&content, file_name) {
            let doc = format!("/// {}", file_name.replace('-', " "));
            icons.push((const_name, path_data, doc, file_name.to_string()));
        }
    }

    // Sort alphabetically for consistent output
    icons.sort_by(|a, b| a.0.cmp(&b.0));

    println!("Generated {} icon constants", icons.len());

    // Generate const declarations
    for (const_name, path_data, doc, _) in &icons {
        output.push_str(doc);
        output.push('\n');
        output.push_str(&format!(
            "pub const {}: &str = r#\"{}\"#;\n\n",
            const_name, path_data
        ));
    }

    // Name lookup, behind a feature: the match references every
    // constant, so enabling it keeps the whole set in the binary and
    // gives up the DCE the plain consts are there for. Worth it only
    // when icon names arrive as data — a DSL prop, a config file — and
    // the call site cannot name a const.
    output.push_str(
        "/// Look an icon up by its Lucide name, as in `by_name(\"house\")`.
///
/// Names are the kebab-case Lucide file names. Returns the same path
/// data as the matching constant, for wrapping with [`crate::to_svg`].
///
/// Enabling the `registry` feature that gates this pulls every icon
/// into the binary. Name a constant directly where you can.
#[cfg(feature = \"registry\")]
pub fn by_name(name: &str) -> Option<&'static str> {
    Some(match name {
",
    );
    for (const_name, _, _, lucide_name) in &icons {
        output.push_str(&format!("        \"{lucide_name}\" => {const_name},\n"));
    }
    output.push_str(
        "        _ => return None,
    })
}

/// Every Lucide name this crate ships, sorted. Same feature trade-off
/// as [`by_name`].
#[cfg(feature = \"registry\")]
pub const NAMES: &[&str] = &[
",
    );
    for (_, _, _, lucide_name) in &icons {
        output.push_str(&format!("    \"{lucide_name}\",\n"));
    }
    output.push_str("];\n");

    fs::write(out_path, output).expect("Failed to write icons.rs");
}

/// Parse an SVG file and extract the inner elements as a string
fn parse_svg(content: &str, file_name: &str) -> Option<(String, String)> {
    // Trim leading whitespace that might come before the XML declaration
    let content = content.trim_start();

    let doc = match Document::parse(content) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Warning: Failed to parse {}.svg: {}", file_name, e);
            return None;
        }
    };

    let svg = doc.root_element();

    // Collect all inner elements
    let mut elements = Vec::new();

    for node in svg.children() {
        if node.is_element() {
            match node.tag_name().name() {
                "path" => {
                    if let Some(d) = node.attribute("d") {
                        elements.push(format!(r#"<path d="{}"/>"#, d));
                    }
                }
                "line" => {
                    let x1 = node.attribute("x1").unwrap_or("0");
                    let y1 = node.attribute("y1").unwrap_or("0");
                    let x2 = node.attribute("x2").unwrap_or("0");
                    let y2 = node.attribute("y2").unwrap_or("0");
                    elements.push(format!(
                        r#"<line x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
                        x1, y1, x2, y2
                    ));
                }
                "circle" => {
                    let cx = node.attribute("cx").unwrap_or("0");
                    let cy = node.attribute("cy").unwrap_or("0");
                    let r = node.attribute("r").unwrap_or("0");
                    elements.push(format!(r#"<circle cx="{}" cy="{}" r="{}"/>"#, cx, cy, r));
                }
                "rect" => {
                    let x = node.attribute("x").unwrap_or("0");
                    let y = node.attribute("y").unwrap_or("0");
                    let width = node.attribute("width").unwrap_or("0");
                    let height = node.attribute("height").unwrap_or("0");
                    let rx = node.attribute("rx");
                    let ry = node.attribute("ry");

                    let mut rect = format!(
                        r#"<rect x="{}" y="{}" width="{}" height="{}""#,
                        x, y, width, height
                    );
                    if let Some(rx) = rx {
                        rect.push_str(&format!(r#" rx="{}""#, rx));
                    }
                    if let Some(ry) = ry {
                        rect.push_str(&format!(r#" ry="{}""#, ry));
                    }
                    rect.push_str("/>");
                    elements.push(rect);
                }
                "polyline" => {
                    if let Some(points) = node.attribute("points") {
                        elements.push(format!(r#"<polyline points="{}"/>"#, points));
                    }
                }
                "polygon" => {
                    if let Some(points) = node.attribute("points") {
                        elements.push(format!(r#"<polygon points="{}"/>"#, points));
                    }
                }
                "ellipse" => {
                    let cx = node.attribute("cx").unwrap_or("0");
                    let cy = node.attribute("cy").unwrap_or("0");
                    let rx = node.attribute("rx").unwrap_or("0");
                    let ry = node.attribute("ry").unwrap_or("0");
                    elements.push(format!(
                        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
                        cx, cy, rx, ry
                    ));
                }
                _ => {
                    // Skip unknown elements (like <title>, <desc>, etc.)
                }
            }
        }
    }

    if elements.is_empty() {
        eprintln!("Warning: No drawable elements found in {}.svg", file_name);
        return None;
    }

    // Convert filename to SCREAMING_SNAKE_CASE const name
    let const_name = file_name.to_uppercase().replace(['-', '.'], "_");

    Some((const_name, elements.join("")))
}
