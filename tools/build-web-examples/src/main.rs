//! `blinc-build-web-examples` — codegen tool that auto-discovers
//! cross-target examples in `examples/blinc_app_examples/examples/*.rs` and
//! emits one wasm wrapper crate per example under
//! `examples/_generated/<name>/`.
//!
//! See `docs/book/src/contributing/examples.md` for the convention
//! an example must satisfy to be picked up by this tool. The
//! short version: the example file must define a top-level
//! `pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder`.
//!
//! # Discovery
//!
//! Every `.rs` file under `examples/blinc_app_examples/examples/` is
//! syntactically parsed. A file is selected if:
//!
//! 1. Its top `//!` doc-comment block does NOT contain `no-web:`
//!    (explicit opt-out marker — the codegen tool skips the file
//!    gracefully and doesn't error).
//! 2. It contains exactly one top-level `pub fn build_ui` item.
//!
//! Files that don't match are silently ignored, which is how we
//! keep examples like `multi_window_demo.rs` (inherently desktop-
//! only) out of the web build without a separate allowlist.
//!
//! # Output
//!
//! For each matched example, the tool writes:
//!
//! - `examples/_generated/<name>/Cargo.toml` — crate manifest
//!   derived from a template; dependency set is inferred from
//!   `use blinc_cn::` / `use blinc_icons::` / etc. imports in the
//!   example source.
//! - `examples/_generated/<name>/build.rs` — strips `//!` inner
//!   doc comments from the upstream example source and writes the
//!   result into `$OUT_DIR/example.rs`. See the hand-crafted
//!   `examples/_generated/scroll/build.rs` for the rationale (rustc
//!   limitation rust-lang/rust#66043 prevents `include!` from
//!   expanding into inner-doc-comment regions).
//! - `examples/_generated/<name>/src/lib.rs` — the wrapper. Gated
//!   `#![cfg(target_arch = "wasm32")]`; brings the example into a
//!   private `mod example { include!(…$OUT_DIR/example.rs) }`; runs
//!   `WebApp::run_with_setup` against `example::build_ui` from a
//!   `#[wasm_bindgen(start)]` entry point.
//! - `examples/_generated/<name>/index.html` — canvas host page
//!   with the WebGPU probe and module bootstrap. Title comes from
//!   the example's first doc-comment line.
//! - `examples/_generated/<name>/serve.sh` — the same static-file
//!   server script every other web example ships.
//!
//! # Idempotency + pruning
//!
//! Running the tool twice produces the same output. After
//! generating fresh wrappers, the tool walks `examples/_generated/`
//! and deletes any subdirectory whose matching upstream example
//! either no longer exists or no longer satisfies the convention
//! (the `pub fn build_ui` check). This keeps the generated tree in
//! sync with the source of truth without manual cleanup.
//!
//! # Running
//!
//! ```ignore
//! # From the workspace root:
//! cargo run -p blinc-build-web-examples
//! ```
//!
//! The tool is intentionally not in `default-members`, so regular
//! `cargo build` / `cargo check --workspace` does not pull it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// Constants / paths
// ============================================================================

/// Directory holding the upstream cross-target examples.
const EXAMPLES_DIR: &str = "examples/blinc_app_examples/examples";

/// Directory where this tool writes generated wrapper crates.
const GENERATED_DIR: &str = "examples/_generated";

/// mdBook source root. The tool writes the auto-generated example
/// gallery pages here and patches `SUMMARY.md` to include them.
const BOOK_SRC_DIR: &str = "docs/book/src";

/// Base GitHub URL for "view source" links in the gallery. Points at
/// `main` so the links track the current state of the repo.
const SOURCE_LINK_BASE: &str = "https://github.com/project-blinc/Blinc/blob/main";

/// Markers that bracket the gallery section inside `SUMMARY.md`. The
/// tool rewrites everything between these two lines on each run; the
/// rest of SUMMARY.md is left untouched. Keep both markers on their
/// own lines with exact whitespace.
const SUMMARY_BEGIN: &str = "<!-- begin:web-examples -->";
const SUMMARY_END: &str = "<!-- end:web-examples -->";

/// Crates we know how to infer as wrapper dependencies. Each entry
/// is `(use_path_prefix, cargo_package_name, relative_path_from_wrapper_dir)`.
///
/// The inference is deliberately conservative — we only add a dep
/// if the example actually uses the crate, so wrappers for simple
/// examples stay small. A new workspace crate that examples start
/// depending on needs one line added here.
/// Each entry is `(prefix in source, package name, full TOML
/// dependency spec)`. The spec is emitted verbatim as the right-hand
/// side of `package_name = ...` in the generated wrapper's
/// `Cargo.toml`, so it can be either a `path` dep on a workspace
/// crate or a `git` dep on a downstream `/packages/*` crate that
/// lives in its own upstream repo. The spec must be a valid TOML
/// inline table starting with `{` and ending with `}`.
///
/// When bumping a `git` rev here, also bump the matching entries in
/// `examples/blinc_app_examples/Cargo.toml` so the desktop and wasm
/// builds stay on the same package commit.
const INFERABLE_DEPS: &[(&str, &str, &str)] = &[
    (
        "blinc_animation::",
        "blinc_animation",
        r#"{ path = "../../../crates/blinc_animation" }"#,
    ),
    (
        "blinc_cn::",
        "blinc_cn",
        r#"{ path = "../../../crates/blinc_cn" }"#,
    ),
    (
        "blinc_icons::",
        "blinc_icons",
        r#"{ path = "../../../crates/blinc_icons" }"#,
    ),
    (
        "blinc_tabler_icons::",
        "blinc_tabler_icons",
        r#"{ path = "../../../crates/blinc_tabler_icons" }"#,
    ),
    (
        "blinc_canvas_kit::",
        "blinc_canvas_kit",
        // Standalone downstream package (lives at `packages/blinc_canvas_kit/`,
        // gitignored). Workspace `[patch]` redirects the git URL to the local
        // path so wasm wrappers iterate without a push/rev cycle. Bump the
        // rev when the published repo gets new releases. Carries `InputState`
        // / `DivInputExt` / the gamepad types since the blinc_input absorption
        // — wrappers that use `blinc_canvas_kit::InputState` resolve through
        // this single entry.
        r#"{ git = "https://github.com/project-blinc/blinc_canvas_kit.git", rev = "61f2ea3d666656fc3c704a31445f6e71b1aa321e" }"#,
    ),
    (
        "blinc_gpu::",
        "blinc_gpu",
        // Custom-render-pass demos (`gpu_pass_demo`) import `blinc_gpu`
        // directly. `default-features = false` is required because the
        // default `desktop` feature pulls vulkan / metal / dx12 / winit
        // backends that don't compile on `wasm32-unknown-unknown`. The
        // `web` feature enables `webgpu` + `webgl` wgpu backends — same
        // set blinc_app's `web` feature chains in transitively, but
        // wrappers depending on `blinc_gpu` by name need their own copy.
        r#"{ path = "../../../crates/blinc_gpu", default-features = false, features = ["web"] }"#,
    ),
    (
        "blinc_theme::",
        "blinc_theme",
        r#"{ path = "../../../crates/blinc_theme" }"#,
    ),
    (
        "blinc_text::",
        "blinc_text",
        r#"{ path = "../../../crates/blinc_text" }"#,
    ),
    (
        "blinc_paint::",
        "blinc_paint",
        r#"{ path = "../../../crates/blinc_paint" }"#,
    ),
    (
        "blinc_router::",
        "blinc_router",
        r#"{ path = "../../../crates/blinc_router" }"#,
    ),
    (
        "blinc_svg::",
        "blinc_svg",
        r#"{ path = "../../../crates/blinc_svg" }"#,
    ),
    (
        "blinc_image::",
        "blinc_image",
        r#"{ path = "../../../crates/blinc_image" }"#,
    ),
    (
        "blinc_media::",
        "blinc_media",
        r#"{ path = "../../../crates/blinc_media" }"#,
    ),
    (
        "blinc_platform::",
        "blinc_platform",
        r#"{ path = "../../../crates/blinc_platform" }"#,
    ),
    (
        "blinc_macros::",
        "blinc_macros",
        r#"{ path = "../../../crates/blinc_macros" }"#,
    ),
    // Downstream `/packages/*` crates live in their own upstream repos
    // and are gitignored locally — wasm wrappers can't path-dep on
    // them. Pin to the same git revs the workspace's
    // `examples/blinc_app_examples/Cargo.toml` uses; bump in lockstep.
    (
        "blinc_game_kit::",
        "blinc_game_kit",
        // Merged glTF loader + runtime skeleton poser (replaces the
        // retired separate `blinc_gltf` + `blinc_skeleton` repos).
        // Same API behind the `gltf::` / `skeleton::` sub-modules.
        // Features flow through from the example crate's `gltf`
        // feature: platform-assets + bc-encode.
        r#"{ git = "https://github.com/project-blinc/blinc_game_kit.git", rev = "2db35f96ec552f39dec85d4f63914dfb70f5aa82", features = ["platform-assets", "bc-encode"] }"#,
    ),
    (
        "blinc_node_editor::",
        "blinc_node_editor",
        // Metadata-driven node-graph editor toolkit. Lives in its own
        // repo (`github.com/project-blinc/blinc_node_editor`) and is
        // pinned to the same rev `examples/blinc_app_examples/Cargo.toml`
        // uses. Bump in lockstep.
        r#"{ git = "https://github.com/project-blinc/blinc_node_editor.git", rev = "248be85d54d2c0b1b02e872a02a1e43a962373e0" }"#,
    ),
    (
        "blinc_portal_ui::",
        "blinc_portal_ui",
        // Immediate-mode widget toolkit pulled by node_editor_demo for
        // its node body closures. Same downstream-repo pattern as
        // canvas_kit / node_editor — pinned to the workspace rev.
        r#"{ git = "https://github.com/project-blinc/blinc_portal_ui.git", rev = "41c40a9e3da33148eb789e096debbd8ac9eb56a8" }"#,
    ),
    // `blinc_input` was absorbed into `blinc_canvas_kit::input` and the
    // standalone repo archived. Demos that previously imported
    // `blinc_input::InputState` now resolve it through the canvas_kit
    // entry above. No source-scan entry needed.
    // Non-`blinc_*` crate that the 3D animation demos use to get a
    // wasm32-safe monotonic clock. `std::time::Instant::now()`
    // panics on `wasm32-unknown-unknown`; `web_time::Instant` wraps
    // `performance.now()` with the same API. Detected by source
    // scan so demos that don't animate don't pay the extra dep.
    ("web_time::", "web-time", r#""1.1""#),
    // wgpu + bytemuck: pulled in only by demos that drive a
    // `CustomRenderPass` directly (currently `gpu_pass_demo`). The
    // workspace pins `wgpu = "=26.0.1"` so the wrapper agrees on
    // ABI with `blinc_gpu`; backend features (`webgpu`, `webgl`)
    // come transitively via `blinc_gpu`'s `web` feature, so we
    // don't enable them here. `bytemuck` carries the `derive`
    // feature for `Pod` / `Zeroable` proc-macros.
    ("wgpu::", "wgpu", r#""=26.0.1""#),
    (
        "bytemuck::",
        "bytemuck",
        r#"{ version = "1.14", features = ["derive"] }"#,
    ),
];

// ============================================================================
// Discovery + metadata extraction
// ============================================================================

/// Everything the codegen stage needs to know about one example.
#[derive(Debug)]
struct ExampleMeta {
    /// Base name of the source file without `.rs`, e.g. `"scroll"`.
    name: String,
    /// Source file path relative to the workspace root.
    source_path: PathBuf,
    /// First non-empty line of the `//!` doc comment block. Used as
    /// the display title in the index.html `<title>` and in the
    /// gallery page.
    title: String,
    /// Short description lifted from subsequent `//!` lines in the
    /// doc block. Stops at the first `Run with:` line (which every
    /// example uses as a footer) or at the end of the doc block.
    /// Rendered verbatim as markdown on the gallery page.
    description: String,
    /// Extra `blinc_*` crates this example imports, in deterministic
    /// order. Each entry is `(package_name, full_toml_spec)` where
    /// `spec` is the inline-table right-hand side of the dependency
    /// declaration — `{ path = "..." }` for workspace crates,
    /// `{ git = "...", rev = "..." }` for `/packages/*` crates.
    extra_deps: Vec<(String, String)>,
    /// Image asset paths referenced in the example source. Detected
    /// by scanning for string literals that match
    /// `examples/blinc_app_examples/examples/assets/`. On the web target these
    /// are fetched via `preload_assets` at startup (the browser
    /// serves them from the same origin as the wasm).
    image_assets: Vec<String>,
    /// Window width/height from `WindowConfig { width: N, height: M }`
    /// in the example's `fn main`. Extracted so the generated
    /// `index.html` can set `data-width`/`data-height` on the canvas
    /// for consistent rendering in docs/book iframes.
    window_size: Option<(u32, u32)>,
    /// `true` when the example defines `pub fn theme_bundle() ->
    /// ThemeBundle`. When set, the wasm wrapper hands the bundle to
    /// `ThemeState::init` before `WebApp::run` so the example gets
    /// the same theme + `with_css(...)` registration on web that it
    /// gets on desktop via `WindowedApp::run_with_theme`. Examples
    /// without this function inherit `platform_theme_bundle()` and
    /// no extra CSS.
    has_theme_bundle: bool,
    /// `true` when the example defines `pub fn theme_color_scheme()
    /// -> ColorScheme`. When set, the wasm wrapper passes the
    /// example's chosen scheme to `ThemeState::init` instead of
    /// auto-detecting from `prefers-color-scheme`. Lets demos pin
    /// a specific scheme (e.g. `ColorScheme::Dark`) so desktop and
    /// web render identically regardless of the user's OS pref.
    has_theme_color_scheme: bool,
}

/// Walk `EXAMPLES_DIR`, parse each `.rs`, and return metadata for
/// every file that passes the convention check.
fn discover_examples(workspace_root: &Path) -> Vec<ExampleMeta> {
    let examples_root = workspace_root.join(EXAMPLES_DIR);
    let entries = match fs::read_dir(&examples_root) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "blinc-build-web-examples: cannot read {}: {e}",
                examples_root.display()
            );
            return Vec::new();
        }
    };

    let mut found: Vec<ExampleMeta> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };

        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  skip {stem}: cannot read {}: {e}", path.display());
                continue;
            }
        };

        // Opt-out marker: any `//! no-web:` line in the upstream doc
        // block. This is how examples that can't run on the web
        // target (multi-window, filesystem assets, etc.) declare
        // themselves out without a separate manifest.
        if source
            .lines()
            .take_while(|l| l.trim_start().starts_with("//!") || l.trim().is_empty())
            .any(|l| l.contains("no-web:"))
        {
            println!("  skip {stem}: `//! no-web:` opt-out");
            continue;
        }

        let ast = match syn::parse_file(&source) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("  skip {stem}: parse error {e}");
                continue;
            }
        };

        if !has_pub_build_ui(&ast) {
            // Common case for examples that haven't been migrated to
            // the convention yet. Not an error — just not picked up.
            continue;
        }

        let title = extract_title(&source).unwrap_or_else(|| format_fallback_title(&stem));
        let description = extract_description(&source);
        let mut extra_deps = infer_extra_deps(&source);
        let image_assets = detect_image_assets(&source);
        let window_size = extract_window_size(&source);
        let has_theme_bundle = has_pub_theme_bundle(&ast);
        let has_theme_color_scheme = has_pub_theme_color_scheme(&ast);
        // The wrapper directly references `blinc_theme::ThemeState`
        // and `blinc_theme::detect_system_color_scheme` when the
        // example exports `theme_bundle()`. Force-add the dep even
        // if the example source happens not to mention `blinc_theme::`
        // by name (it usually does — but don't depend on it). Also
        // pull it in when only `theme_color_scheme()` is exported —
        // the generated wrapper still calls into `blinc_theme`.
        if (has_theme_bundle || has_theme_color_scheme)
            && !extra_deps.iter().any(|(p, _)| p == "blinc_theme")
        {
            extra_deps.push((
                "blinc_theme".to_string(),
                r#"{ path = "../../../crates/blinc_theme" }"#.to_string(),
            ));
            extra_deps.sort_by(|a, b| a.0.cmp(&b.0));
        }

        let relative_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_path_buf();

        if !image_assets.is_empty() {
            println!(
                "  discovered {stem} ({title}) [{} image(s)]",
                image_assets.len()
            );
        } else {
            println!("  discovered {stem} ({title})");
        }
        found.push(ExampleMeta {
            name: stem,
            source_path: relative_path,
            title,
            description,
            extra_deps,
            image_assets,
            window_size,
            has_theme_bundle,
            has_theme_color_scheme,
        });
    }

    // Deterministic order so re-runs produce byte-identical output.
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Returns true if the parsed syn AST contains a top-level item
/// matching `pub fn build_ui(...)`. We don't verify the argument or
/// return types — the upstream desktop build will fail loudly if
/// the signature drifts from the convention, and the wrapper build
/// will fail at `WebApp::run_with_setup(…, build_ui)` if the
/// signature doesn't satisfy its `FnMut(&mut WindowedContext) -> E
/// where E: ElementBuilder` bound. No need to duplicate either
/// check here.
fn has_pub_build_ui(ast: &syn::File) -> bool {
    ast.items.iter().any(|item| {
        if let syn::Item::Fn(f) = item {
            matches!(f.vis, syn::Visibility::Public(_)) && f.sig.ident == "build_ui"
        } else {
            false
        }
    })
}

/// Returns true if the parsed syn AST exposes a top-level
/// `pub fn theme_bundle()`. Examples that need a non-default
/// `ThemeBundle` (e.g. `cn_demo` registering `cn_styles::CN_STYLES`)
/// define this so the wasm wrapper can hand the bundle to
/// `ThemeState::init` before `WebApp::run` — same effect as the
/// desktop's `WindowedApp::run_with_theme`. Examples that don't
/// define it inherit the auto-init platform bundle.
fn has_pub_theme_bundle(ast: &syn::File) -> bool {
    ast.items.iter().any(|item| {
        if let syn::Item::Fn(f) = item {
            matches!(f.vis, syn::Visibility::Public(_)) && f.sig.ident == "theme_bundle"
        } else {
            false
        }
    })
}

/// Mirror of [`has_pub_theme_bundle`] for the optional
/// `pub fn theme_color_scheme() -> ColorScheme` accessor. When an
/// example defines this, the wasm wrapper hands its return value
/// to `ThemeState::init` instead of falling back to
/// `detect_system_color_scheme()`. Lets demos pin a specific
/// scheme (e.g. `ColorScheme::Dark`) so desktop and web agree
/// regardless of the user's OS preference.
fn has_pub_theme_color_scheme(ast: &syn::File) -> bool {
    ast.items.iter().any(|item| {
        if let syn::Item::Fn(f) = item {
            matches!(f.vis, syn::Visibility::Public(_)) && f.sig.ident == "theme_color_scheme"
        } else {
            false
        }
    })
}

/// Pull the first non-empty `//!` doc-comment line from the top of
/// the file. Used as the gallery title. We look at the raw source
/// rather than the AST because `syn` collapses inner doc comments
/// into attribute nodes and we want to preserve the original
/// single-line text without round-tripping.
fn extract_title(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(body) = trimmed.strip_prefix("//!") {
            let body = body.trim();
            if !body.is_empty() {
                // Strip a trailing "Example" suffix if present, to
                // make the gallery titles feel less repetitive
                // ("Scroll Container" vs "Scroll Container Example").
                let cleaned = body
                    .strip_suffix(" Example")
                    .or_else(|| body.strip_suffix(" Demo"))
                    .unwrap_or(body);
                return Some(cleaned.to_string());
            }
        } else if trimmed.is_empty() {
            continue;
        } else {
            // First non-comment, non-blank line — we're past the doc
            // block and haven't found anything. Bail.
            return None;
        }
    }
    None
}

/// Extract the example's short description from its top-of-file
/// doc comment block. The description is everything inside the
/// `//!` block after the title line, with `//!` prefixes stripped.
/// Blank `//!` lines become paragraph breaks; lines that start
/// with `- ` stay as bullet list entries.
///
/// `Run with:` paragraphs are SKIPPED in place rather than
/// terminating the description so examples whose convention is
///
///   //! Title
///   //!
///   //! Run with:
///   //! ```
///   //! cargo run ...
///   //! ```
///   //!
///   //! What it shows:
///   //! ...
///
/// still emit the "What it shows" body to the gallery. A `Run with:`
/// block ends at the next blank `//!` line (or, when wrapped in a
/// `///! `\`\`\`` ` fence, at the closing fence).
///
/// If the example has no description (i.e. only a title line in
/// its doc block), this returns an empty string — callers fall back
/// to a generic "see source" blurb.
fn extract_description(source: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut past_title = false;
    let mut skipping_run_with = false;
    let mut in_run_with_fence = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(body) = trimmed.strip_prefix("//!") else {
            // First non-comment line ends the doc block.
            if !trimmed.is_empty() {
                break;
            }
            continue;
        };
        let body = body.strip_prefix(' ').unwrap_or(body);

        if !past_title {
            // Skip the title line itself — we already captured it
            // via `extract_title`. Blank line right after the title
            // flips the state machine into description-collection.
            if body.trim().is_empty() {
                past_title = true;
            }
            continue;
        }

        // Skip a `Run with:` paragraph in place — the gallery page
        // already links the source file, so reproducing the cargo
        // invocation here is noise. Detection runs at the start of
        // each line so the skip works whether `Run with:` lives at
        // the end of the doc block or in the middle (between the
        // tagline and a "What it shows:" section).
        //
        // Two shapes are accepted:
        //   - inline:  `Run with: cargo run ...`
        //   - block:   `Run with:\n\n\`\`\`sh\ncargo ...\n\`\`\``
        //
        // Blank `//!` lines are NOT a terminator while we're
        // skipping — a `Run with:` line followed by one blank
        // `//!` then a `\`\`\`sh` fence is the common shape.
        // We exit the skip when the fence closes or when we hit
        // the first non-blank non-fence content (treating that
        // content as the next paragraph and letting it fall
        // through to normal collection).
        if skipping_run_with {
            let bt = body.trim_start();
            if in_run_with_fence {
                if bt.starts_with("```") {
                    in_run_with_fence = false;
                    skipping_run_with = false;
                }
                continue;
            }
            if bt.starts_with("```") {
                in_run_with_fence = true;
                continue;
            }
            if body.trim().is_empty() {
                continue;
            }
            // Non-blank non-fence content — the Run-with block is
            // implicitly closed. End the skip and let this line
            // through to the normal description collector below.
            skipping_run_with = false;
        }
        if body.trim_start().starts_with("Run with:") {
            skipping_run_with = true;
            // Drop the trailing blank that just preceded the
            // `Run with:` so paragraphs around it merge cleanly.
            while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                lines.pop();
            }
            continue;
        }

        lines.push(body.to_string());
    }

    // Trim trailing blank lines so the rendered markdown doesn't
    // end with gratuitous whitespace.
    while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    // Trim leading blanks left by a Run-with block sitting between
    // the title and the first prose paragraph.
    while lines.first().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.remove(0);
    }

    lines.join("\n")
}

/// Fallback title for examples whose doc block is missing or
/// doesn't have a leading text line. We take the file stem, replace
/// underscores with spaces, and title-case each word.
fn format_fallback_title(stem: &str) -> String {
    stem.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Scan the example source for image asset paths. Looks for quoted
/// strings containing `examples/blinc_app_examples/examples/assets/` — the
/// convention all desktop examples use for bundled images. Returns
/// deduplicated, sorted paths.
/// Extract `width` and `height` from a `WindowConfig { ... }` block
/// in the example source. Looks for `width: N` and `height: N` lines
/// near a `WindowConfig` struct literal.
/// Extract `width`, `height`, and `resizable` from a `WindowConfig { ... }`
/// block in the example source. Only returns a fixed size when
/// `resizable` is explicitly `false` — when `true` (or defaulted),
/// the canvas should fill the viewport like a normal web page.
fn extract_window_size(source: &str) -> Option<(u32, u32)> {
    // Find the WindowConfig block
    let config_start = source.find("WindowConfig")?;
    let block = &source[config_start..];
    let brace_start = block.find('{')?;
    let brace_end = block.find('}')?;
    let block = &block[brace_start..=brace_end];

    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut resizable = true; // default

    for line in block.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        if let Some(rest) = trimmed.strip_prefix("width:") {
            width = rest.trim().parse().ok();
        } else if let Some(rest) = trimmed.strip_prefix("height:") {
            height = rest.trim().parse().ok();
        } else if let Some(rest) = trimmed.strip_prefix("resizable:") {
            resizable = rest.trim() != "false";
        }
    }

    // Only lock to fixed size when resizable is explicitly false.
    // Resizable demos should fill the viewport on web.
    if resizable {
        return None;
    }

    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some((w, h)),
        _ => None,
    }
}

/// Classify an asset URL for the purpose of `<link rel="preload">`.
///
/// Media URLs are consumed by the browser's `<video>` / `<audio>`
/// elements (or the wasm `<video>.src` streaming path in Blinc),
/// not by `fetch()`. Declaring them with `as="fetch"` makes the
/// browser log the classic "preloaded but not used within a few
/// seconds" warning once the timeout fires, because nothing
/// ever claims the preload at that MIME type. Matching the `as=`
/// to the actual consumer fixes the warning and lets the browser
/// dedupe the preload with the media element's range request.
fn preload_as_attr(url: &str) -> &'static str {
    let lower = url.to_lowercase();
    const VIDEO: &[&str] = &[".mp4", ".webm", ".mov", ".m4v"];
    const AUDIO: &[&str] = &[".mp3", ".wav", ".flac", ".ogg", ".oga", ".m4a"];
    if VIDEO.iter().any(|ext| lower.ends_with(ext)) {
        "video"
    } else if AUDIO.iter().any(|ext| lower.ends_with(ext)) {
        "audio"
    } else {
        "fetch"
    }
}

/// True when the URL targets a `<video>` / `<audio>` element.
/// Media is excluded from `WebAssetLoader::preload` because the
/// media pipeline does its own HTTP range-request streaming — the
/// `<video>.src = url` path doesn't need the bytes cached in the
/// wasm-side `HashMap` first, and caching them there would
/// duplicate a 50-MB download for no benefit.
fn is_media_url(url: &str) -> bool {
    preload_as_attr(url) != "fetch"
}

fn detect_image_assets(source: &str) -> Vec<String> {
    let mut raw_paths = std::collections::BTreeSet::new();
    let asset_prefix = "examples/blinc_app_examples/examples/assets/";

    // Match string literals like "examples/blinc_app_examples/examples/assets/foo.webp"
    // or directory constants like "examples/blinc_app_examples/examples/assets/3d/DamagedHelmet"
    for segment in source.split('"') {
        let trimmed = segment.trim();
        if trimmed.starts_with(asset_prefix) && !trimmed.contains('\n') {
            raw_paths.insert(trimmed.to_string());
        }
    }
    // Match markdown image syntax: ![alt](examples/blinc_app_examples/examples/assets/foo.webp)
    for segment in source.split('(') {
        let trimmed = segment.trim();
        if trimmed.starts_with(asset_prefix)
            && let Some(end) = trimmed.find(')')
        {
            raw_paths.insert(trimmed[..end].to_string());
        }
    }

    // Expand directory paths: if a detected path is a directory on disk,
    // include all files under it recursively. This handles the common
    // pattern of `const DIR: &str = "crates/.../assets/3d/Model"` where
    // the individual file paths are constructed via `format!()` and
    // aren't detectable as string literals.
    //
    // `.gltf` references also expand to their containing directory —
    // a `.gltf` is always a multi-file asset (sibling `.bin` buffer
    // plus a `textures/` subdir) and `blinc_gltf::load_asset` resolves
    // each sibling via the platform asset loader. On web that loader
    // is `WebAssetLoader` which REQUIRES each path to have been
    // preloaded. Without this expansion the main `.gltf` file
    // preloads but its buffer and textures panic at runtime.
    let mut out = std::collections::BTreeSet::new();
    for path in &raw_paths {
        let p = Path::new(path);
        if p.is_dir() {
            if let Ok(entries) = list_files_recursive(p) {
                for file_path in entries {
                    if let Some(s) = file_path.to_str() {
                        out.insert(s.to_string());
                    }
                }
            }
        } else if p.is_file() {
            out.insert(path.clone());
            let is_gltf_json = path.ends_with(".gltf");
            if is_gltf_json
                && let Some(parent) = p.parent()
                && let Ok(entries) = list_files_recursive(parent)
            {
                for file_path in entries {
                    if let Some(s) = file_path.to_str() {
                        out.insert(s.to_string());
                    }
                }
            }
        }
        // If path doesn't exist on disk (e.g. scanned from source but
        // not checked out), include it anyway — the staging step will
        // skip missing files gracefully.
        if !p.exists() {
            out.insert(path.clone());
        }
    }
    out.into_iter().collect()
}

/// Recursively list all files under `dir`, returning paths relative to
/// the current working directory (which is the workspace root).
fn list_files_recursive(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(list_files_recursive(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

/// Scan the example source for `use <crate>::` patterns from our
/// inferable-dep allowlist. Returns the matched entries as a sorted
/// deduplicated list. The scan is a simple substring search — it
/// intentionally doesn't try to be smart about `use foo::bar` vs
/// fully-qualified paths, since both count as "this example
/// depends on `foo`".
fn infer_extra_deps(source: &str) -> Vec<(String, String)> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (prefix, package, spec) in INFERABLE_DEPS {
        if source.contains(prefix) {
            out.insert((*package).to_string(), (*spec).to_string());
        }
    }
    out.into_iter().collect()
}

// ============================================================================
// Codegen — one wrapper crate per discovered example
// ============================================================================

/// Emit the full set of wrapper files for one example. Overwrites
/// any existing files in the wrapper directory so re-runs pick up
/// edits to the upstream example immediately.
fn generate_wrapper(workspace_root: &Path, meta: &ExampleMeta) -> std::io::Result<()> {
    let wrapper_dir = workspace_root.join(GENERATED_DIR).join(&meta.name);
    let src_dir = wrapper_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let crate_name = crate_name_for(&meta.name);
    let source_path_str = meta.source_path.to_string_lossy().replace('\\', "/");
    // build.rs reads EXAMPLE_PATH relative to the wrapper dir
    // (`CARGO_MANIFEST_DIR`), which is three levels down from the
    // workspace root (`examples/_generated/<name>/`). So the
    // relative path from the wrapper up to the example is:
    //     ../../../<source_path_from_workspace_root>
    let example_path_from_wrapper = format!("../../../{source_path_str}");

    fs::write(
        wrapper_dir.join("Cargo.toml"),
        render_cargo_toml(&crate_name, meta),
    )?;
    fs::write(
        wrapper_dir.join("build.rs"),
        render_build_rs(&example_path_from_wrapper, &meta.name),
    )?;
    fs::write(
        src_dir.join("lib.rs"),
        render_lib_rs(
            &meta.name,
            &crate_name,
            &meta.image_assets,
            meta.has_theme_bundle,
            meta.has_theme_color_scheme,
        ),
    )?;
    fs::write(
        wrapper_dir.join("index.html"),
        render_index_html(
            &meta.title,
            &crate_name,
            meta.window_size,
            &meta.image_assets,
        ),
    )?;

    let serve_sh_path = wrapper_dir.join("serve.sh");
    fs::write(&serve_sh_path, render_serve_sh())?;
    // Make serve.sh executable (best effort; Windows ignores this).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&serve_sh_path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&serve_sh_path, perms);
        }
    }

    // A `.gitignore` in each wrapper excludes the `pkg/` directory
    // produced by wasm-pack. Matches what `examples/web_hello` etc.
    // ship.
    fs::write(wrapper_dir.join(".gitignore"), "pkg/\n")?;

    Ok(())
}

/// Convert an example file stem (`keyframe_canvas`) into a cargo
/// package name (`blinc-example-keyframe-canvas-web`). Cargo
/// package names use hyphens, not underscores; the `example` infix
/// keeps these generated crates from colliding with any real
/// `blinc-<name>-web` crate that happens to exist.
fn crate_name_for(stem: &str) -> String {
    format!("blinc-example-{}-web", stem.replace('_', "-"))
}

// ============================================================================
// Templates — each `render_*` returns the full file contents
// ============================================================================

fn render_cargo_toml(crate_name: &str, meta: &ExampleMeta) -> String {
    let mut extra_deps = String::new();
    // Each `extra_deps` entry is `(package_name, full_toml_spec)` —
    // the spec already includes the leading `{` and trailing `}` and
    // can be either `{ path = "..." }` or `{ git = "...", rev = "..." }`.
    for (package, spec) in &meta.extra_deps {
        extra_deps.push_str(&format!("{package} = {spec}\n"));
    }

    format!(
        r#"[package]
# Auto-generated by `tools/build-web-examples`. DO NOT EDIT by hand —
# the next run of the tool will overwrite your changes. To tweak the
# wrapper shape, edit the templates in
# `tools/build-web-examples/src/main.rs`.
#
# Source of truth for this wrapper's behavior is
# `{source_path}` — the upstream example file
# included via `build.rs` + `include!` into `src/lib.rs`.
name = "{crate_name}"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[package.metadata.wasm-pack.profile.release]
# `--all-features` is required because rustc's wasm32 codegen
# emits `memory.copy` instructions, which wasm-opt rejects unless
# bulk-memory (and other current proposals) are enabled. The
# bundled wasm-opt 116 in wasm-pack 0.13.1 segfaults on the
# larger generated modules (e.g. cn_demo) — CI installs a fresh
# binaryen via apt so wasm-pack picks the system binary up
# instead of the bundled crashy one.
#
# TODO(wasm-size): size-focused flags (`-Oz`, `--strip-debug`,
# `--strip-producers`) combined with rustc-side `opt-level = "z"` +
# `lto = "fat"` + `panic = "abort"` produced a ~22% smaller wasm
# locally but corrupted the binary on CI — the deployed file had
# an invalid value-type byte near offset 201 and the browser
# refused it with `invalid value type 0x0 @+201`. Suspect
# interaction between apt-installed binaryen on the Ubuntu runner
# and one of the new flags. Before retrying: pin a specific
# binaryen version in CI, bisect which of the flags causes the
# corruption (likely `--strip-producers` or the fat-LTO output),
# and add a wasm-validate post-step so the CI build fails loudly
# instead of shipping broken bytes to Pages.
wasm-opt = ['-O', '--all-features']

[package.metadata.wasm-pack.profile.dev]
wasm-opt = false

# Strictly a wasm32 wrapper. The desktop side of the same example
# is `cargo run -p blinc_app_examples --example {name} --features windowed`.
# Native builds of this wrapper are intentionally no-ops — the whole
# crate is `#![cfg(target_arch = "wasm32")]`-gated inside `src/lib.rs`.
[target.'cfg(target_arch = "wasm32")'.dependencies]
blinc_app = {{ path = "../../../crates/blinc_app", default-features = false, features = ["web"] }}
blinc_layout = {{ path = "../../../crates/blinc_layout", features=["media"] }}
blinc_core = {{ path = "../../../crates/blinc_core" }}
{extra_deps}wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
web-sys = {{ version = "0.3", features = ["console", "Window"] }}
console_error_panic_hook = "0.1"
tracing = {{ workspace = true }}
tracing-wasm = "0.2"
tracing-subscriber = {{ workspace = true, features = ["registry"] }}
# Enable getrandom 0.3's `wasm_js` backend for any demo that
# transitively pulls it (today: node_editor_demo via ahash). The
# feature wires up the wasm_js codepath; the `--cfg getrandom_backend`
# rustflag in the workspace `.cargo/config.toml` selects it. Both are
# required per getrandom 0.3's own diagnostic. No-op for demos that
# resolve only getrandom 0.2.
getrandom = {{ version = "0.3", features = ["wasm_js"] }}
"#,
        crate_name = crate_name,
        name = meta.name,
        source_path = meta.source_path.display(),
        extra_deps = extra_deps,
    )
}

fn render_build_rs(example_path_from_wrapper: &str, stem: &str) -> String {
    // Double-hash raw-string delimiter (`r##"..."##`) because the
    // template body contains the two-char sequence `"#` inside
    // `starts_with("#![")`, which would otherwise terminate a
    // single-hash `r#"..."#` literal early.
    format!(
        r##"//! Auto-generated by `tools/build-web-examples`.
//!
//! Pre-processes the upstream example source for consumption by
//! `include!` inside `src/lib.rs`. Strips inner doc comments (`//!`)
//! because `include!` can't paste them into a non-module-start
//! context (rust-lang/rust#66043), while preserving line numbers
//! via blank-line replacement so panic backtraces still point at
//! the right line in the upstream file.
//!
//! Emits `cargo:rerun-if-changed` so cargo re-runs this script
//! whenever the upstream example changes. The wrapper's copy of
//! the example lives in `$OUT_DIR/example.rs` and never ends up in
//! source control.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Path to the upstream example, relative to this wrapper's
/// `CARGO_MANIFEST_DIR` (i.e. `examples/_generated/{stem}/`).
const EXAMPLE_PATH: &str = "{example_path}";

fn main() {{
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo"));
    let example_path = manifest_dir.join(EXAMPLE_PATH);

    println!("cargo:rerun-if-changed={{}}", example_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let raw = fs::read_to_string(&example_path)
        .unwrap_or_else(|e| panic!("read {{}}: {{e}}", example_path.display()));

    // Drop inner-scope tokens that `include!` can't paste outside
    // the literal start of a module: `//!` doc comments and
    // `#![…]` inner attributes. Both are valid at the top of the
    // upstream example file but illegal when pasted inside the
    // wrapper's `mod example {{ include!(…) }}` body (rustc bug
    // rust-lang/rust#66043). Blank-line replacement preserves
    // line numbers so panic backtraces still point at the right
    // line in the original example.
    //
    // `#![…]` can span multiple lines (e.g. `#![allow(
    //     foo,
    //     bar,
    // )]`), so we track an open-bracket counter while we're
    // inside an inner attribute and keep blanking until brackets
    // balance back to zero.
    let mut bracket_depth: i32 = 0;
    let stripped: String = raw
        .lines()
        .map(|line| {{
            let trimmed = line.trim_start();
            if bracket_depth > 0 {{
                // Mid-attribute continuation line: blank it AND
                // update the bracket balance from this line's
                // contents, so a closing `)]` on its own line ends
                // the attribute cleanly.
                bracket_depth += line.chars().filter(|c| *c == '[').count() as i32;
                bracket_depth -= line.chars().filter(|c| *c == ']').count() as i32;
                ""
            }} else if trimmed.starts_with("//!") {{
                ""
            }} else if trimmed.starts_with("#![") {{
                // Single-line or multi-line inner attribute. Count
                // brackets on the first line to know whether we
                // need to keep eating continuation lines.
                bracket_depth += line.chars().filter(|c| *c == '[').count() as i32;
                bracket_depth -= line.chars().filter(|c| *c == ']').count() as i32;
                ""
            }} else {{
                line
            }}
        }})
        .collect::<Vec<_>>()
        .join("\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    let out_path = out_dir.join("example.rs");
    fs::write(&out_path, stripped)
        .unwrap_or_else(|e| panic!("write {{}}: {{e}}", out_path.display()));
}}
"##,
        example_path = example_path_from_wrapper,
        stem = stem,
    )
}

fn render_lib_rs(
    stem: &str,
    crate_name: &str,
    image_assets: &[String],
    has_theme_bundle: bool,
    has_theme_color_scheme: bool,
) -> String {
    format!(
        r#"//! Auto-generated by `tools/build-web-examples`.
//!
//! Wasm wrapper for the `{stem}` example. The entire crate is
//! `#![cfg(target_arch = "wasm32")]`-gated — native builds of this
//! wrapper are no-ops. The desktop side of the same example is
//! `cargo run -p blinc_app_examples --example {stem} --features windowed`.
//!
//! Flow:
//!
//! 1. `build.rs` reads the upstream example file, strips `//!`
//!    inner doc comments (workaround for rust-lang/rust#66043),
//!    and writes the result to `$OUT_DIR/example.rs`.
//! 2. `mod example {{ include!(…$OUT_DIR/example.rs) }}` pulls the
//!    cleaned example in under a private sub-module. Wrapping in
//!    an inner mod gives the `include!` expansion its own module
//!    namespace and sidesteps symbol collisions with this file.
//! 3. `pub fn build_ui` from the example becomes reachable as
//!    `example::build_ui`, which the `#[wasm_bindgen(start)]` shim
//!    hands to `WebApp::run_with_setup`.
//!
//! See `tools/build-web-examples/src/main.rs` for the codegen
//! logic, and `docs/book/src/contributing/examples.md` for the
//! convention examples must satisfy to be picked up.

#![cfg(target_arch = "wasm32")]

// The upstream example may have carried `#![allow(...)]` inner
// attributes (commonly `deprecated`, `dead_code`, various
// `clippy::*` lints) that `build.rs` stripped because `include!`
// can't paste inner attributes mid-module. Without a
// replacement, every wrapper would re-trip those lints here.
// An outer `#[allow(...)]` on the `mod example` item scopes the
// same suppression to every item inside the included example.
//
// `clippy::all` + `clippy::pedantic` are intentionally wide: example
// code is allowed to be loose about naming, explicit types, and
// idioms in service of clarity. Production crates don't inherit
// this allow — only the auto-generated wrapper does.
#[allow(dead_code, deprecated, unused_imports, unused_variables, unused_mut)]
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod example {{
    include!(concat!(env!("OUT_DIR"), "/example.rs"));
}}

use blinc_app::web::WebApp;
use example::build_ui;
use wasm_bindgen::prelude::*;

/// Bundled fonts from `assets/fonts/` at the workspace root.
/// Browsers can't hand wgpu their system fonts, so font bytes are
/// included in the wasm binary. These match the fonts the desktop
/// runner preloads via `preload_fonts` at app.rs:110-121.
const ARIAL_TTF: &[u8] = include_bytes!("../../../../assets/fonts/Arial.ttf");
const FIRA_CODE_TTF: &[u8] = include_bytes!("../../../../assets/fonts/FiraCode-Regular.ttf");
const JETBRAINS_MONO_TTF: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf");

#[wasm_bindgen(start)]
pub fn _start() {{
    console_error_panic_hook::set_once();

    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );

{theme_init_block}    wasm_bindgen_futures::spawn_local(async {{
        let result = WebApp::run_with_async_setup(
            "blinc-canvas",
            |app| Box::pin(async move {{
                app.load_font_data(ARIAL_TTF.to_vec());
                app.load_font_data(FIRA_CODE_TTF.to_vec());
                app.load_font_data(JETBRAINS_MONO_TTF.to_vec());
{preload_block}
                Ok(())
            }}),
            // Closure wrapper: on edition 2024, a free fn
            // `build_ui(ctx) -> impl Element` captures `ctx`'s
            // lifetime in the return type, which breaks the
            // higher-ranked `FnMut` bound on `run_with_async_setup`.
            // The closure form bypasses the inference failure.
            |ctx| build_ui(ctx),
        )
        .await;

        if let Err(e) = result {{
            web_sys::console::error_1(
                &format!("{crate_name}: WebApp::run failed: {{e}}").into(),
            );
        }}
    }});
}}
"#,
        stem = stem,
        crate_name = crate_name,
        theme_init_block = if has_theme_bundle {
            // Install the example's bundle before WebApp::new()'s
            // auto-init runs — its `try_get().is_none()` guard skips
            // when we've already initialised. Without this, the
            // bundle's `with_css(...)` payload (cn_styles, app
            // overrides) never lands.
            //
            // Color-scheme source mirrors what the desktop's
            // `WindowedApp::run_with_theme` would receive: the
            // example's own `theme_color_scheme()` accessor when
            // defined (so demos can pin a specific scheme like
            // `ColorScheme::Dark` and have web honour it), else the
            // OS preference via `detect_system_color_scheme()`.
            let scheme_expr = if has_theme_color_scheme {
                "example::theme_color_scheme()"
            } else {
                "blinc_theme::detect_system_color_scheme()"
            };
            format!(
                "    blinc_theme::ThemeState::init(\n        example::theme_bundle(),\n        {scheme_expr},\n    );\n\n"
            )
        } else {
            String::new()
        },
        preload_block = {
            // Media URLs (video/audio) stream via `<video>.src` /
            // `<audio>.src` — they don't need to be materialised in
            // the wasm `WebAssetLoader` cache. Including them here
            // would double-fetch a 50 MB clip on cold load, and
            // still leave the `<link rel=preload as=video>` entry
            // as the only consumer the browser ever sees.
            let fetch_urls: Vec<&String> =
                image_assets.iter().filter(|p| !is_media_url(p)).collect();
            if fetch_urls.is_empty() {
                String::new()
            } else {
                let urls: Vec<String> = fetch_urls
                    .iter()
                    .map(|p| format!("                        \"{}\"", p))
                    .collect();
                // Background-spawn the preload instead of awaiting it.
                // Without this the setup closure blocks before the first
                // frame is painted, leaving the user staring at a blank
                // canvas for the entire download duration (~74 MB on
                // buster_drone = several seconds on a slow connection).
                //
                // The cloned `asset_loader_handle()` lives in the
                // `spawn_local` task. Callers query progress via
                // `app.preload_progress()` — an `Arc<PreloadProgress>`
                // safe to poll every frame — to build a loading overlay.
                // Assets are available to synchronous `load_asset` calls
                // only after the preload task's fetches resolve (check
                // `progress.is_complete()` before calling).
                format!(
                    r#"                let loader = app.asset_loader_handle();
                wasm_bindgen_futures::spawn_local(async move {{
                    let urls = [
{urls}
                    ];
                    if let Err(e) = loader.preload(&urls).await {{
                        web_sys::console::error_1(
                            &format!("preload failed: {{e}}").into(),
                        );
                    }}
                }});
"#,
                    urls = urls.join(",\n")
                )
            }
        },
    )
}

fn render_index_html(
    title: &str,
    crate_name: &str,
    window_size: Option<(u32, u32)>,
    image_assets: &[String],
) -> String {
    // Convert crate name back to the JS import shim filename. wasm-pack
    // emits `<package_name_with_underscores>.js`, not hyphens.
    let js_name = crate_name.replace('-', "_");
    let canvas_attrs = match window_size {
        Some((w, h)) => format!(
            r#" data-width="{w}" data-height="{h}" data-dpr="1" style="width:{w}px;height:{h}px""#
        ),
        None => String::new(),
    };
    let canvas_css = match window_size {
        Some(_) => "display: block; /* fixed size via data- attrs */",
        None => "display: block; width: 100vw; height: 100vh;",
    };

    // `<link rel="preload" as="fetch" crossorigin>` lets the browser
    // begin downloading each asset URL in parallel with the wasm
    // bundle, rather than waiting until wasm initialises and calls
    // `fetch()` itself. On a cached reload these are served from
    // disk cache; on a cold load they overlap with the ~100–500 ms
    // wasm compile window. The wasm code's later `fetch()` calls
    // see the same URL and dedupe against the in-flight request —
    // no double-download.
    //
    // `as="fetch"` matches the `RequestMode::Cors` + default
    // `RequestCredentials::SameOrigin` the wasm preloader uses; any
    // mismatch makes the browser discard the preload and re-fetch,
    // so these attributes need to stay in lockstep with
    // `WebAssetLoader::fetch_bytes`. The `crossorigin` attribute
    // must be present (even empty) for `as="fetch"` preloads to be
    // honored by the cache.
    // Partition assets by consumer type so the `<link rel="preload">`
    // `as=` attribute matches the later request — a mismatch means
    // the browser discards the preload + re-fetches, or logs the
    // "preloaded but not used" warning after the timeout.
    //
    // - Media (`.mp4` / `.webm` / `.mov` / `.mp3` / `.wav` /
    //   `.flac` / `.ogg`): consumed via `<video>.src` or
    //   `<audio>.src` streaming — `as="video"` / `as="audio"`. The
    //   `crossorigin` attribute stays off because media elements
    //   default to the no-credentials mode already.
    // - Everything else: consumed via the wasm `fetch()` preload
    //   path with `credentials=omit` — `as="fetch" crossorigin`,
    //   matching `WebAssetLoader::fetch_bytes`.
    // `fetchpriority="high"` on the fetch preloads prevents Chrome's
    // resource scheduler from deprioritising some preloads behind
    // others — without it, on HTTP/2 a large first asset (e.g. a
    // glTF `scene.bin`) can dominate bandwidth while later textures
    // sit in "Pending" for multiple seconds despite stream
    // multiplexing being available. Making them all "high" keeps
    // the server round-robining bytes across the 29 streams so
    // every asset finishes in roughly the same wall-clock window
    // instead of one starving the others.
    //
    // Not applied to video/audio preloads: those are consumed by
    // the media element which has its own priority model, and the
    // attribute isn't meaningful for `as="video"` / `as="audio"`.
    let preload_links = if image_assets.is_empty() {
        String::new()
    } else {
        image_assets
            .iter()
            .map(|p| match preload_as_attr(p) {
                "video" => {
                    format!(r#"    <link rel="preload" as="video" href="{p}" />"#)
                }
                "audio" => {
                    format!(r#"    <link rel="preload" as="audio" href="{p}" />"#)
                }
                _ => format!(
                    r#"    <link rel="preload" as="fetch" crossorigin fetchpriority="high" href="{p}" />"#
                ),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Blinc · {title}</title>
{preload_links}
    <style>
      html, body {{
        margin: 0;
        padding: 0;
        height: 100%;
        background: #14141c;
        color: #ededf0;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
      }}
      body {{
        display: flex;
        flex-direction: column;
        align-items: stretch;
        justify-content: stretch;
      }}
      #blinc-canvas {{
        {canvas_css}
      }}
      #unsupported {{
        display: none;
        position: absolute;
        inset: 0;
        align-items: center;
        justify-content: center;
        flex-direction: column;
        gap: 12px;
        font-size: 14px;
        line-height: 1.5;
        text-align: center;
        padding: 24px;
      }}
      #unsupported a {{ color: #6db3ff; }}
      .no-webgpu #blinc-canvas {{ display: none; }}
      .no-webgpu #unsupported {{ display: flex; }}
    </style>
  </head>
  <body>
    <canvas id="blinc-canvas"{canvas_attrs}></canvas>

    <div id="unsupported">
      <strong>WebGPU not available</strong>
      <span>
        Blinc's web target requires WebGPU.
        Try Chrome / Edge 113+, or enable WebGPU in
        <a href="https://caniuse.com/webgpu">your browser</a>.
      </span>
    </div>

    <script type="module">
      const hasWebGPU = "gpu" in navigator;
      const probeCanvas = document.createElement("canvas");
      const hasWebGL2 = !!probeCanvas.getContext("webgl2");

      if (!hasWebGPU && !hasWebGL2) {{
        document.body.classList.add("no-webgpu");
      }} else {{
        const {{ default: init }} = await import("./pkg/{js_name}.js");
        await init();
      }}
    </script>
  </body>
</html>
"#,
        title = title,
        js_name = js_name,
        canvas_css = canvas_css,
        canvas_attrs = canvas_attrs,
        preload_links = preload_links,
    )
}

fn render_serve_sh() -> &'static str {
    r#"#!/usr/bin/env bash
# Auto-generated by `tools/build-web-examples`.
#
# Serve this example over HTTP for local iteration. Run
# `wasm-pack build --target web --release` first to populate pkg/.

set -euo pipefail

cd "$(dirname "$0")"

PORT="${1:-8000}"

if [ ! -d pkg ]; then
  echo "pkg/ not found. Run \`wasm-pack build --target web --release\` first." >&2
  exit 64
fi

URL="http://localhost:${PORT}/"
echo "Serving $(pwd) on ${URL}"
echo "Press Ctrl-C to stop."
echo

if command -v python3 >/dev/null 2>&1; then
  exec python3 -m http.server "${PORT}"
elif command -v python >/dev/null 2>&1; then
  exec python -m http.server "${PORT}"
elif command -v ruby >/dev/null 2>&1; then
  exec ruby -run -e httpd . -p "${PORT}"
elif command -v npx >/dev/null 2>&1; then
  exec npx --yes http-server -p "${PORT}" -c-1 .
else
  echo "No static HTTP server found. Install one of:" >&2
  echo "  python3, python, ruby, or npx (Node.js)" >&2
  exit 127
fi
"#
}

// ============================================================================
// Gallery — mdBook markdown emission + SUMMARY.md patching
// ============================================================================
//
// The tool writes three things under `docs/book/src/web/`:
//
//   1. `example-gallery.md` — the gallery landing page, a grid of
//      cards linking to each per-example sub-page. Auto-generated
//      from the discovered example list.
//   2. `example-gallery/<name>.md` — one page per example: title,
//      description, lazy-loaded iframe, "view source" GitHub link.
//   3. `SUMMARY.md` — patched between the
//      `<!-- begin:web-examples -->` / `<!-- end:web-examples -->`
//      markers to add nested entries for each example page. The
//      rest of SUMMARY.md is left untouched.
//
// The per-example sub-pages use an iframe with `loading="lazy"` so
// browsing the gallery doesn't spawn 40+ WebGPU contexts at once
// (Chrome refuses to allocate that many). The iframe src points at
// `../../examples/<name>/index.html`, which resolves against the
// book's HTML output root — CI copies `examples/_generated/<name>/`
// into `target/book/examples/<name>/` after the mdbook build so
// the relative path lines up at serve time.

/// Path (relative to the workspace root) of the gallery's sub-page
/// directory under the mdBook source tree.
const GALLERY_SUBDIR: &str = "docs/book/src/web/example-gallery";

/// Path (relative to the workspace root) of the gallery index page.
const GALLERY_INDEX: &str = "docs/book/src/web/example-gallery.md";

/// Path (relative to the workspace root) of `SUMMARY.md`.
const SUMMARY_PATH: &str = "docs/book/src/SUMMARY.md";

/// Write the gallery index page, one sub-page per example, and
/// patch SUMMARY.md to include them. Idempotent.
fn emit_gallery(workspace_root: &Path, examples: &[ExampleMeta]) -> std::io::Result<()> {
    let gallery_dir = workspace_root.join(GALLERY_SUBDIR);
    fs::create_dir_all(&gallery_dir)?;

    // Remove any stale sub-pages before writing new ones, so a
    // rename / removal upstream cleans up here.
    if let Ok(entries) = fs::read_dir(&gallery_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let _ = fs::remove_file(&path);
        }
    }

    // Per-example sub-pages.
    for meta in examples {
        let page = render_gallery_page(meta);
        fs::write(gallery_dir.join(format!("{}.md", meta.name)), page)?;
    }

    // Gallery index page (the landing grid).
    let index = render_gallery_index(examples);
    fs::write(workspace_root.join(GALLERY_INDEX), index)?;

    // Patch SUMMARY.md between the begin/end markers.
    patch_summary(workspace_root, examples)?;

    Ok(())
}

/// Render the per-example gallery sub-page. Keeps the iframe at a
/// fixed height that matches the default wgpu canvas aspect ratio
/// well enough for most demos; authors can tune per-example heights
/// later via a manifest field if the default feels cramped.
fn render_gallery_page(meta: &ExampleMeta) -> String {
    let source_url = format!(
        "{SOURCE_LINK_BASE}/{path}",
        path = meta.source_path.to_string_lossy().replace('\\', "/"),
    );
    // Gallery sub-pages live at `docs/book/src/web/example-gallery/<name>.md`,
    // which renders to `target/book/web/example-gallery/<name>.html`.
    // CI stages wasm-pack output at `target/book/examples/<name>/`.
    // So the iframe path is `../../examples/<name>/index.html`.
    let iframe_src = format!("../../examples/{name}/index.html", name = meta.name);

    let description = if meta.description.trim().is_empty() {
        String::from(
            "This example is auto-generated from the cross-target source \
             in `examples/blinc_app_examples/examples/`. See the linked source file \
             for the full details.",
        )
    } else {
        meta.description.clone()
    };

    format!(
        r#"# {title}

{description}

<iframe
  src="{iframe_src}"
  width="100%"
  height="560"
  loading="lazy"
  style="border:1px solid #45475a;border-radius:8px;background:#181825;"
  title="Blinc {name} example"
></iframe>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab]({iframe_src}) · [View source on GitHub]({source_url})
"#,
        title = meta.title,
        description = description,
        iframe_src = iframe_src,
        source_url = source_url,
        name = meta.name,
    )
}

/// Render the gallery index page. Alphabetised grid of cards, one
/// per example. Each card links to the per-example sub-page.
fn render_gallery_index(examples: &[ExampleMeta]) -> String {
    let mut body = String::new();
    body.push_str(
        "# Example Gallery\n\n\
         Every example in [`examples/blinc_app_examples/examples/`](https://github.com/project-blinc/Blinc/tree/main/examples/blinc_app_examples/examples)\n\
         that follows the cross-target convention is auto-built for the web\n\
         target by `tools/build-web-examples` and embedded below. The same\n\
         `build_ui` function that runs on desktop, iOS, and Android runs\n\
         here — no per-target forks. See the\n\
         [Contributing → Examples](../contributing/examples.md) page for the\n\
         convention that makes this work.\n\n\
         Click any card to open the example in a focused view with a\n\
         lazy-loaded iframe. Each demo spawns its own WebGPU context, so\n\
         loading more than ~8 at once will start hitting Chrome's\n\
         per-tab GPU context limit — the per-example pages keep that\n\
         manageable.\n\n\
         ## Examples\n\n",
    );

    for meta in examples {
        let first_desc_line = meta
            .description
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with('-'))
            .unwrap_or("Auto-built from the cross-target source.")
            .to_string();
        body.push_str(&format!(
            "- [**{title}**](./example-gallery/{name}.md) — {blurb}\n",
            title = meta.title,
            name = meta.name,
            blurb = first_desc_line,
        ));
    }

    body
}

/// Rewrite `SUMMARY.md` between the begin/end markers with a nested
/// list of the discovered examples. Preserves everything outside
/// the markers. Fails (but warns) if either marker is missing.
fn patch_summary(workspace_root: &Path, examples: &[ExampleMeta]) -> std::io::Result<()> {
    let path = workspace_root.join(SUMMARY_PATH);
    let Ok(content) = fs::read_to_string(&path) else {
        eprintln!(
            "  WARN: {} not found — skipping SUMMARY patch",
            path.display()
        );
        return Ok(());
    };

    let Some(begin_idx) = content.find(SUMMARY_BEGIN) else {
        eprintln!(
            "  WARN: {} missing `{SUMMARY_BEGIN}` marker — skipping SUMMARY patch",
            path.display()
        );
        return Ok(());
    };
    let Some(end_idx) = content.find(SUMMARY_END) else {
        eprintln!(
            "  WARN: {} missing `{SUMMARY_END}` marker — skipping SUMMARY patch",
            path.display()
        );
        return Ok(());
    };
    if end_idx <= begin_idx {
        eprintln!(
            "  WARN: {} has `{SUMMARY_END}` before `{SUMMARY_BEGIN}` — skipping SUMMARY patch",
            path.display()
        );
        return Ok(());
    }

    // Figure out the indentation of the begin marker line so the
    // generated list entries line up with whatever surrounds it.
    // Lines in SUMMARY.md typically start at column 0 or use `- `
    // bullet indentation.
    let prefix_end = begin_idx + SUMMARY_BEGIN.len();
    // Walk backwards from begin_idx to the start of the line.
    let line_start = content[..begin_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = content[line_start..begin_idx]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();

    // Build the replacement block. The gallery index is listed as
    // the parent entry; each example sub-page is a nested child.
    let mut generated = String::new();
    generated.push_str(&format!(
        "{indent}- [Example Gallery](./web/example-gallery.md)\n"
    ));
    for meta in examples {
        generated.push_str(&format!(
            "{indent}  - [{title}](./web/example-gallery/{name}.md)\n",
            title = meta.title,
            name = meta.name,
        ));
    }

    let mut new_content = String::new();
    // Content up to and including the begin marker line.
    new_content.push_str(&content[..prefix_end]);
    new_content.push('\n');
    new_content.push_str(&generated);
    // Re-indent the end marker to match the begin marker.
    new_content.push_str(&indent);
    new_content.push_str(&content[end_idx..]);

    fs::write(&path, new_content)?;
    println!("  patched {}", path.display());
    Ok(())
}

// ============================================================================
// Incremental build planning — skip wasm-pack on wrappers whose inputs haven't
// changed since the last successful run
// ============================================================================
//
// Each successful `--build` run stamps the current `git rev-parse
// HEAD` into `examples/_generated/.last-build-sha`. The next run
// reads that stamp, runs `git diff --name-only $LAST_SHA HEAD` (and
// against the working tree) and uses the changed-file list to
// decide which wrappers actually need a fresh `wasm-pack build`.
//
// File classification:
//
//   - `examples/blinc_app_examples/examples/<name>.rs` → only `<name>` rebuilds
//   - `Cargo.lock` / `crates/!blinc_app/**` / `extensions/**` /
//     `tools/build-web-examples/**` / wrapper templates →
//     "shared change", every wrapper rebuilds
//   - anything else → ignored
//
// On a fresh checkout (no stamp file) or when git isn't available
// (e.g. running outside a git checkout, or `--force-rebuild` is
// passed) the planner falls back to rebuilding everything.
//
// `wasm-pack build --target web --release` is what dominates the
// per-wrapper time even when cargo's incremental build does
// nothing — `wasm-bindgen` + `wasm-opt` re-run unconditionally on
// every invocation. Skipping the call entirely for unchanged
// wrappers is the difference between a no-op CI run finishing in
// seconds vs. ~20 minutes.

/// Marker file in `examples/_generated/` recording the git commit
/// SHA at which the previous successful `--build` run finished.
/// Plain text, single line, no newline.
const LAST_BUILD_SHA_PATH: &str = "examples/_generated/.last-build-sha";

/// Decision the planner returns for one example.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildAction {
    /// Run `wasm-pack build` for this wrapper.
    Rebuild,
    /// Skip — `pkg/` from the previous run is still good.
    Skip,
}

/// Planner output: what to do for each discovered example, plus
/// the human-readable reason the plan came out that way (so the
/// CI log explains why it's rebuilding everything vs. one wrapper).
struct BuildPlan {
    actions: Vec<(String, BuildAction)>,
    reason: String,
}

/// Decide which wrappers need rebuilding by diffing the current
/// working tree against the last successful build SHA.
///
/// `force` short-circuits the planner and forces a full rebuild —
/// the `--force-rebuild` flag exposes this for the rare case where
/// a developer needs to bypass the cache.
fn plan_builds(workspace_root: &Path, examples: &[ExampleMeta], force: bool) -> BuildPlan {
    if force {
        return BuildPlan {
            actions: examples
                .iter()
                .map(|m| (m.name.clone(), BuildAction::Rebuild))
                .collect(),
            reason: "--force-rebuild".to_string(),
        };
    }

    let stamp_path = workspace_root.join(LAST_BUILD_SHA_PATH);
    let last_sha = match fs::read_to_string(&stamp_path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            return BuildPlan {
                actions: examples
                    .iter()
                    .map(|m| (m.name.clone(), BuildAction::Rebuild))
                    .collect(),
                reason: "no last-build-sha stamp (cold start)".to_string(),
            };
        }
    };

    if last_sha.is_empty() {
        return BuildPlan {
            actions: examples
                .iter()
                .map(|m| (m.name.clone(), BuildAction::Rebuild))
                .collect(),
            reason: "empty last-build-sha stamp".to_string(),
        };
    }

    // Anything modified since the last successful build, including
    // uncommitted local edits. We collect both the diff against the
    // stamped SHA *and* the diff against the working tree so a
    // developer iterating on an example without committing still
    // gets that example rebuilt.
    let mut changed: Vec<String> = Vec::new();
    let committed = git_changed_files(workspace_root, &[&last_sha, "HEAD"]);
    if let Some(list) = committed {
        changed.extend(list);
    } else {
        return BuildPlan {
            actions: examples
                .iter()
                .map(|m| (m.name.clone(), BuildAction::Rebuild))
                .collect(),
            reason: format!(
                "git diff against {last_sha} failed (shallow clone? rewritten history?)"
            ),
        };
    }
    if let Some(working_tree) = git_changed_files(workspace_root, &["HEAD"]) {
        changed.extend(working_tree);
    }
    // Also catch untracked files via `git ls-files --others --exclude-standard`,
    // which `git diff` doesn't report. New examples land in this bucket
    // before they're added to the index.
    if let Some(untracked) = git_untracked_files(workspace_root) {
        changed.extend(untracked);
    }
    changed.sort();
    changed.dedup();

    if changed.is_empty() {
        return BuildPlan {
            actions: examples
                .iter()
                .map(|m| (m.name.clone(), BuildAction::Skip))
                .collect(),
            reason: format!("no files changed since {last_sha}"),
        };
    }

    // Categorise. The example bucket maps file→stem; the shared
    // bucket is just a flag.
    let mut changed_examples: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut shared_changes: Vec<String> = Vec::new();
    for path in &changed {
        if let Some(rest) = path.strip_prefix("examples/blinc_app_examples/examples/") {
            // Only top-level *.rs files in that directory count;
            // subdirectories (like the `assets/` folder) don't.
            if let Some(stem) = rest.strip_suffix(".rs")
                && !stem.contains('/')
            {
                changed_examples.insert(stem.to_string());
                continue;
            }
            // Files under examples/blinc_app_examples/examples/ that aren't
            // top-level .rs (e.g. assets/) don't affect any
            // wrapper's wasm output, so they're ignored.
            continue;
        }
        if path == "Cargo.lock"
            || path.starts_with("crates/")
            || path.starts_with("extensions/")
            || path.starts_with("tools/build-web-examples/")
            || path == ".github/workflows/docs.yml"
        {
            shared_changes.push(path.clone());
        }
        // Anything else (docs/, README, etc.) is ignored — those
        // files don't affect what wasm-pack would produce.
    }

    if !shared_changes.is_empty() {
        return BuildPlan {
            actions: examples
                .iter()
                .map(|m| (m.name.clone(), BuildAction::Rebuild))
                .collect(),
            reason: format!(
                "{} shared file(s) changed (e.g. {}); rebuilding all wrappers",
                shared_changes.len(),
                shared_changes.first().map(|s| s.as_str()).unwrap_or("")
            ),
        };
    }

    let actions: Vec<(String, BuildAction)> = examples
        .iter()
        .map(|m| {
            let action = if changed_examples.contains(&m.name) {
                BuildAction::Rebuild
            } else {
                BuildAction::Skip
            };
            (m.name.clone(), action)
        })
        .collect();

    let rebuild_count = actions
        .iter()
        .filter(|(_, a)| *a == BuildAction::Rebuild)
        .count();
    BuildPlan {
        actions,
        reason: format!(
            "{} example file(s) changed since {last_sha}; rebuilding {} wrapper(s)",
            changed_examples.len(),
            rebuild_count
        ),
    }
}

/// Run `git diff --name-only <range...>` from `workspace_root` and
/// return the resulting list of paths (workspace-relative). Returns
/// `None` if git isn't available or the diff fails — the caller
/// treats that as "fall back to a full rebuild".
fn git_changed_files(workspace_root: &Path, args: &[&str]) -> Option<Vec<String>> {
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(workspace_root)
        .arg("diff")
        .arg("--name-only");
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
    )
}

/// Run `git ls-files --others --exclude-standard` to list untracked
/// (but not gitignored) files. Used so newly-added example files
/// register as "changed" before they're staged.
fn git_untracked_files(workspace_root: &Path) -> Option<Vec<String>> {
    let out = std::process::Command::new("git")
        .current_dir(workspace_root)
        .args(["ls-files", "--others", "--exclude-standard"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
    )
}

/// Read the current `git rev-parse HEAD` and stamp it into
/// `examples/_generated/.last-build-sha` so the next run can diff
/// against it. Failures (non-git checkout, IO error) are logged
/// but non-fatal.
fn write_last_build_sha(workspace_root: &Path) {
    let out = match std::process::Command::new("git")
        .current_dir(workspace_root)
        .args(["rev-parse", "HEAD"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => {
            eprintln!("  WARN: could not read git HEAD; skipping last-build-sha stamp");
            return;
        }
    };
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return;
    }
    let stamp = workspace_root.join(LAST_BUILD_SHA_PATH);
    if let Some(parent) = stamp.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&stamp, &sha) {
        eprintln!("  WARN: could not write {}: {e}", stamp.display());
    }
}

// ============================================================================
// Build + stage — optional wasm-pack integration behind `--build`
// ============================================================================

/// Run `wasm-pack build --target web --release` in each wrapper
/// directory whose inputs have changed since the last successful
/// build (per the [`plan_builds`] git-diff planner). Returns the
/// list of wrappers that failed so the caller can report a non-zero
/// exit code.
fn build_wrappers_with_wasm_pack(
    workspace_root: &Path,
    examples: &[ExampleMeta],
    force_rebuild: bool,
) -> Vec<String> {
    let plan = plan_builds(workspace_root, examples, force_rebuild);
    println!("  plan: {}", plan.reason);

    let mut failed = Vec::new();
    let mut skipped = 0usize;
    let mut built = 0usize;

    // Map name → action for quick lookup as we iterate examples in
    // their original (sorted) order.
    let action_for: std::collections::HashMap<&str, BuildAction> =
        plan.actions.iter().map(|(n, a)| (n.as_str(), *a)).collect();

    for meta in examples {
        let wrapper_dir = workspace_root.join(GENERATED_DIR).join(&meta.name);
        let pkg_dir = wrapper_dir.join("pkg");
        let action = action_for
            .get(meta.name.as_str())
            .copied()
            .unwrap_or(BuildAction::Rebuild);

        // A "skip" decision is only honoured if the previous run's
        // pkg/ output is actually still on disk. CI cache restores
        // sometimes drop wrappers; without this guard, the planner
        // would happily declare a wrapper up-to-date and the
        // staging step would then complain that pkg/ is missing.
        let pkg_intact = pkg_dir.join("package.json").exists();
        if action == BuildAction::Skip && pkg_intact {
            println!("  ✓ skip {} (no inputs changed)", meta.name);
            skipped += 1;
            continue;
        }
        if action == BuildAction::Skip && !pkg_intact {
            println!(
                "  ! rebuild {} (planner said skip, but pkg/ is missing)",
                meta.name
            );
        }

        println!("  wasm-pack build {}", meta.name);
        // CI pre-installs a matching `wasm-bindgen-cli` on PATH (see
        // `.github/workflows/docs.yml`) so wasm-pack's version probe
        // hits the system binary and skips the per-wrapper download.
        // Locally devs already have one on PATH from a previous run.
        // We avoid `--mode no-install` here because that mode looks
        // ONLY at wasm-pack's `~/.cache/.wasm-pack/` directory and
        // ignores PATH — it would error out even with the right
        // binary installed.
        let status = std::process::Command::new("wasm-pack")
            .args(["build", "--target", "web", "--release"])
            .current_dir(&wrapper_dir)
            .status();
        match status {
            Ok(s) if s.success() => {
                built += 1;
            }
            Ok(s) => {
                eprintln!("    FAILED: wasm-pack exited with {s}");
                failed.push(meta.name.clone());
            }
            Err(e) => {
                eprintln!("    FAILED: could not spawn wasm-pack: {e}");
                failed.push(meta.name.clone());
            }
        }
    }

    println!(
        "  summary: built {}, skipped {} (no input changes), failed {}",
        built,
        skipped,
        failed.len()
    );

    // Stamp the build SHA only if every wrapper succeeded. A
    // partial failure leaves the previous SHA in place so the next
    // run still rebuilds whatever broke.
    if failed.is_empty() {
        write_last_build_sha(workspace_root);
    }

    failed
}

/// Copy each built wrapper's public-facing files (`index.html` +
/// `pkg/`) into `stage_dir/<name>/`. CI uses this to drop the
/// wasm artifacts into `target/book/examples/` so mdBook's iframe
/// references resolve at serve time.
fn stage_wrappers(
    workspace_root: &Path,
    examples: &[ExampleMeta],
    stage_dir: &Path,
) -> std::io::Result<()> {
    fs::create_dir_all(stage_dir)?;
    for meta in examples {
        let wrapper_dir = workspace_root.join(GENERATED_DIR).join(&meta.name);
        let pkg_dir = wrapper_dir.join("pkg");
        if !pkg_dir.exists() {
            eprintln!(
                "  skip stage {}: pkg/ missing (wasm-pack didn't run?)",
                meta.name
            );
            continue;
        }
        let dest = stage_dir.join(&meta.name);
        // Fresh target — clear anything left from a previous run.
        let _ = fs::remove_dir_all(&dest);
        fs::create_dir_all(&dest)?;
        copy_dir_recursive(&pkg_dir, &dest.join("pkg"))?;
        fs::copy(wrapper_dir.join("index.html"), dest.join("index.html"))?;
        println!("  staged {} → {}", meta.name, dest.display());
    }

    // Copy detected assets into each example's staging directory,
    // preserving the full relative path from the workspace root so
    // browser fetch() URLs like
    // `examples/blinc_app_examples/examples/assets/3d/DamagedHelmet/albedo.jpg`
    // resolve correctly from the example's served root.
    for meta in examples {
        if meta.image_assets.is_empty() {
            continue;
        }
        let dest_root = stage_dir.join(&meta.name);
        for asset_path in &meta.image_assets {
            let src_file = workspace_root.join(asset_path);
            if !src_file.exists() {
                continue;
            }
            let dest_file = dest_root.join(asset_path);
            if let Some(parent) = dest_file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src_file, &dest_file)?;
            println!("  staged asset {} → {}", asset_path, meta.name);
        }
    }

    Ok(())
}

/// Recursive copy helper. `std::fs` doesn't ship one.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ============================================================================
// Pruning — remove wrapper dirs whose upstream example has gone away
// ============================================================================

/// Delete any subdirectory of `examples/_generated/` whose base name
/// is not in `keep_names`. Skips the directory's own marker files
/// (`.gitignore`, `.gitkeep`) so the workspace member glob stays
/// resolvable on a fresh clone.
fn prune_stale_wrappers(workspace_root: &Path, keep_names: &[String]) -> std::io::Result<()> {
    let generated = workspace_root.join(GENERATED_DIR);
    let Ok(entries) = fs::read_dir(&generated) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Never touch the marker files that keep the directory
        // tracked in git.
        if name.starts_with('.') {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        if !keep_names.iter().any(|k| k == name) {
            println!("  prune stale wrapper {name}");
            fs::remove_dir_all(&path)?;
        }
    }
    Ok(())
}

// ============================================================================
// main
// ============================================================================

/// Command-line flags the tool understands. Parsed via a tiny
/// ad-hoc loop to avoid dragging `clap` into the dep tree for five
/// arguments.
#[derive(Default)]
struct Args {
    /// When set, run `wasm-pack build --target web --release` in
    /// each generated wrapper after the codegen stage finishes.
    /// CI uses this; local dev usually doesn't (developers run
    /// wasm-pack themselves while iterating on a single example).
    build: bool,
    /// When set, copy each built wrapper's `index.html` + `pkg/`
    /// into `<stage>/<example-name>/`. Implies `--build`. CI uses
    /// this to drop artifacts into `target/book/examples/` before
    /// uploading the pages artifact.
    stage_to: Option<PathBuf>,
    /// When set, skip writing the gallery markdown / SUMMARY patch.
    /// Useful for CI steps that just want fresh wrappers without
    /// touching the book source tree (e.g. a nightly lint run).
    no_gallery: bool,
    /// When set, bypass the git-diff incremental planner and run
    /// `wasm-pack build` for every wrapper unconditionally. Used
    /// when the cache is suspected stale or when iterating on the
    /// codegen tool itself.
    force_rebuild: bool,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--build" => args.build = true,
            "--stage-to" => {
                let val = raw.get(i + 1).unwrap_or_else(|| {
                    eprintln!("error: --stage-to requires a value");
                    std::process::exit(2);
                });
                args.stage_to = Some(PathBuf::from(val));
                args.build = true; // --stage-to implies --build
                i += 1;
            }
            "--no-gallery" => args.no_gallery = true,
            "--force-rebuild" => args.force_rebuild = true,
            "--help" | "-h" => {
                println!(
                    "blinc-build-web-examples — codegen for cross-target example wrappers\n\n\
                     USAGE:\n    \
                     cargo run -p blinc-build-web-examples -- [FLAGS]\n\n\
                     FLAGS:\n    \
                     --build              After generating wrappers, run\n    \
                                          `wasm-pack build --target web --release` in each.\n    \
                                          Skips wrappers whose inputs haven't changed since\n    \
                                          the last successful build (per `git diff` against\n    \
                                          `examples/_generated/.last-build-sha`).\n    \
                     --stage-to <dir>     After `--build`, copy `index.html` + `pkg/`\n    \
                                          from each wrapper into `<dir>/<example>/`. Implies --build.\n    \
                     --no-gallery         Skip writing the mdBook gallery pages and\n    \
                                          SUMMARY.md patch.\n    \
                     --force-rebuild      Bypass the git-diff incremental planner and\n    \
                                          rebuild every wrapper. Use when the cache is\n    \
                                          suspected stale or when iterating on the codegen\n    \
                                          tool itself.\n    \
                     --help, -h           Show this help and exit.\n"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("error: unknown argument `{other}` (try --help)");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    args
}

fn main() {
    let args = parse_args();

    // Resolve the workspace root from CARGO_MANIFEST_DIR. The tool's
    // own Cargo.toml lives at `tools/build-web-examples/`, so two
    // `..` jumps land us at the workspace root.
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect(
            "CARGO_MANIFEST_DIR set by cargo. Run this tool via `cargo run -p blinc-build-web-examples`.",
        ),
    );
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root two levels above tool crate")
        .to_path_buf();

    println!(
        "blinc-build-web-examples: workspace at {}",
        workspace_root.display()
    );
    println!(
        "  source    : {}",
        workspace_root.join(EXAMPLES_DIR).display()
    );
    println!(
        "  wrappers  : {}",
        workspace_root.join(GENERATED_DIR).display()
    );
    if !args.no_gallery {
        println!(
            "  book      : {}",
            workspace_root.join(BOOK_SRC_DIR).display()
        );
    }
    if args.build {
        println!("  wasm-pack : on");
    }
    if args.force_rebuild {
        println!("  force     : on (incremental planner bypassed)");
    }
    if let Some(stage) = args.stage_to.as_ref() {
        println!("  stage-to  : {}", stage.display());
    }
    println!();
    println!("Discovering examples…");
    let found = discover_examples(&workspace_root);

    println!();
    println!("Generating {} wrapper crate(s)…", found.len());
    let mut ok = 0usize;
    let mut failed = Vec::new();
    for meta in &found {
        match generate_wrapper(&workspace_root, meta) {
            Ok(()) => {
                println!("  wrote examples/_generated/{}/", meta.name);
                ok += 1;
            }
            Err(e) => {
                eprintln!("  FAILED {}: {e}", meta.name);
                failed.push(meta.name.clone());
            }
        }
    }

    println!();
    println!("Pruning stale wrappers…");
    let keep: Vec<String> = found.iter().map(|m| m.name.clone()).collect();
    if let Err(e) = prune_stale_wrappers(&workspace_root, &keep) {
        eprintln!("  WARN: prune failed: {e}");
    }

    if !args.no_gallery {
        println!();
        println!("Emitting gallery markdown + SUMMARY patch…");
        if let Err(e) = emit_gallery(&workspace_root, &found) {
            eprintln!("  WARN: gallery emission failed: {e}");
        }
    }

    let mut build_failures: Vec<String> = Vec::new();
    if args.build {
        println!();
        println!("Running wasm-pack build on {} wrapper(s)…", found.len());
        build_failures = build_wrappers_with_wasm_pack(&workspace_root, &found, args.force_rebuild);
    }

    if let Some(stage_dir) = args.stage_to.as_ref() {
        println!();
        println!("Staging built wrappers into {}…", stage_dir.display());
        // `stage_dir` may be relative; resolve against the current
        // working directory (which for `cargo run -p …` is the
        // workspace root, so this Just Works in CI).
        let stage_abs = if stage_dir.is_absolute() {
            stage_dir.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| workspace_root.clone())
                .join(stage_dir)
        };
        if let Err(e) = stage_wrappers(&workspace_root, &found, &stage_abs) {
            eprintln!("  WARN: stage failed: {e}");
        }
    }

    println!();
    println!(
        "Done. {ok}/{total} wrappers written.{extra}",
        ok = ok,
        total = found.len(),
        extra = if failed.is_empty() && build_failures.is_empty() {
            String::new()
        } else {
            let mut msg = String::new();
            if !failed.is_empty() {
                msg.push_str(&format!(" Codegen failures: {}.", failed.join(", ")));
            }
            if !build_failures.is_empty() {
                msg.push_str(&format!(
                    " wasm-pack failures: {}.",
                    build_failures.join(", ")
                ));
            }
            msg
        }
    );

    if !failed.is_empty() || !build_failures.is_empty() {
        std::process::exit(1);
    }
}
