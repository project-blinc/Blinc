//! Build script for blinc_tabler_icons
//!
//! Parses Tabler SVG files and generates Rust const declarations
//! for both outline and filled icon variants.

use roxmltree::Document;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/tabler");

    generate_module(
        Path::new("assets/tabler/outline"),
        Path::new("src/outline.rs"),
        "outline",
    );
    generate_module(
        Path::new("assets/tabler/filled"),
        Path::new("src/filled.rs"),
        "filled",
    );
}

fn generate_module(icons_dir: &Path, out_path: &Path, variant: &str) {
    if !icons_dir.exists() {
        eprintln!(
            "Warning: {} directory not found, creating empty {}.rs",
            icons_dir.display(),
            variant
        );
        fs::write(
            out_path,
            format!(
                "//! Generated {variant} icon constants - no icons found\n\npub const HOME: &str = \"\";\n"
            ),
        )
        .expect("Failed to write generated file");
        return;
    }

    let mut output = format!(
        "//! Generated Tabler {variant} icon constants
//!
//! Auto-generated from Tabler SVG files - DO NOT EDIT
//!
//! Each icon is a `&'static str` containing the SVG inner elements.
//! Use `blinc_tabler_icons::to_svg()` (outline) or `to_svg_filled()` (filled)
//! to wrap in a complete SVG tag.
//!
//! Unused icons are automatically eliminated by DCE (Dead Code Elimination).

"
    );

    let mut icons: Vec<(String, String, String)> = Vec::new();

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
            icons.push((const_name, path_data, doc));
        }
    }

    icons.sort_by(|a, b| a.0.cmp(&b.0));

    println!("Generated {} {} icon constants", icons.len(), variant);

    for (const_name, path_data, doc) in &icons {
        output.push_str(doc);
        output.push('\n');
        output.push_str(&format!(
            "pub const {}: &str = r#\"{}\"#;\n\n",
            const_name, path_data
        ));
    }

    fs::write(out_path, output).expect("Failed to write generated file");
}

/// Parse an SVG file and extract the inner elements as a string
fn parse_svg(content: &str, file_name: &str) -> Option<(String, String)> {
    let content = content.trim_start();

    let doc = match Document::parse(content) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Warning: Failed to parse {}.svg: {}", file_name, e);
            return None;
        }
    };

    let svg = doc.root_element();

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

    let const_name = file_name.to_uppercase().replace(['-', '.'], "_");

    Some((const_name, elements.join("")))
}
