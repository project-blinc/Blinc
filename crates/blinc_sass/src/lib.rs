//! Compile a crate's SCSS sources into the CSS it embeds, at build time.
//!
//! A crate's `build.rs` calls [`build`], and its stylesheet constant is
//! `include_str!(concat!(env!("OUT_DIR"), "/name.css"))`. Nothing is compiled at run time, and an
//! application's own CSS stays plain CSS.
//!
//! The sources are modules: `styles/index.scss` `@use`s one partial per component, and a partial
//! can `@use "blinc/mixins"` and `@use "blinc/tokens"`, which this crate carries so that every
//! stylesheet shares them. The compiler is [grass](https://docs.rs/grass), a Sass compiler written
//! in Rust.
//!
//! Output is Blinc's CSS and nothing else: no `@charset`, expanded style, and optionally wrapped
//! in an `@layer`. A partial is plain SCSS at the top level, so the layer is added here and not in
//! the sources.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Where the shared partials appear to the compiler. A `@use "blinc/mixins"` finds
/// `<ROOT>/blinc/_mixins.scss`, which is held in memory.
const ROOT: &str = "/__blinc_sass__";

/// The shared partials, by path under [`ROOT`].
const SHARED: &[(&str, &str)] = &[
    ("blinc/_mixins.scss", include_str!("../scss/_mixins.scss")),
    ("blinc/_tokens.scss", include_str!("../scss/_tokens.scss")),
];

/// A stylesheet that did not compile.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// A path with `/` separators, so a lookup does not depend on the platform.
fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The shared partials from memory, and everything else from disk.
#[derive(Debug)]
struct Sources(HashMap<String, &'static str>);

impl Sources {
    fn new() -> Self {
        Self(
            SHARED
                .iter()
                .map(|(path, text)| (format!("{ROOT}/{path}"), *text))
                .collect(),
        )
    }

    fn is_shared(path: &str) -> bool {
        path == ROOT || path.starts_with(&format!("{ROOT}/"))
    }
}

impl grass::Fs for Sources {
    fn is_dir(&self, path: &Path) -> bool {
        let path = key(path);
        if Self::is_shared(&path) {
            let prefix = format!("{path}/");
            return path == ROOT || self.0.keys().any(|k| k.starts_with(&prefix));
        }
        Path::new(&path).is_dir()
    }

    fn is_file(&self, path: &Path) -> bool {
        let path = key(path);
        self.0.contains_key(&path) || (!Self::is_shared(&path) && Path::new(&path).is_file())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.0.get(&key(path)) {
            Some(text) => Ok(text.as_bytes().to_vec()),
            None => std::fs::read(path),
        }
    }
}

/// Compile the SCSS at `entry` to CSS, wrapped in `@layer <layer> { ... }` when `layer` is given.
pub fn compile(entry: impl AsRef<Path>, layer: Option<&str>) -> Result<String, Error> {
    let sources = Sources::new();
    let options = grass::Options::default()
        .fs(&sources)
        .load_path(ROOT)
        .style(grass::OutputStyle::Expanded)
        // Blinc's parser skips the rule a leading `@charset` is attached to.
        .allows_charset(false);
    let css = grass::from_path(entry.as_ref(), &options).map_err(|e| Error(e.to_string()))?;
    Ok(match layer {
        Some(layer) => format!("@layer {layer} {{\n{css}}}\n"),
        None => css,
    })
}

/// The `.scss` files under `dir`, sorted, for what has to depend on all of them.
pub fn sources(dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "scss") {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir.as_ref(), &mut out)?;
    out.sort();
    Ok(out)
}

/// For a `build.rs`: compile `entry` (relative to the crate) into `$OUT_DIR/<out>`, rebuilding
/// when anything under its directory changes. A stylesheet that does not compile fails the build
/// with the compiler's message, which names the file, line and column.
pub fn build(entry: &str, out: &str, layer: Option<&str>) {
    let dir = Path::new(entry).parent().unwrap_or_else(|| Path::new("."));
    println!("cargo:rerun-if-changed={}", dir.display());
    let css = compile(entry, layer).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let out_dir = std::env::var_os("OUT_DIR").expect("build.rs runs under cargo");
    std::fs::write(Path::new(&out_dir).join(out), css).expect("write the compiled stylesheet");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with `files`, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str, files: &[(&str, &str)]) -> Self {
            let dir =
                std::env::temp_dir().join(format!("blinc_sass_{name}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            for (path, text) in files {
                let path = dir.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            Self(dir)
        }

        fn entry(&self) -> PathBuf {
            self.0.join("index.scss")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn partials_use_the_shared_mixins_and_tokens() {
        let dir = Scratch::new(
            "shared",
            &[
                ("index.scss", "@use \"button\";\n"),
                (
                    "_button.scss",
                    "@use \"blinc/mixins\" as *;\n@use \"blinc/tokens\" as t;\n@use \"sass:map\";\n\
                     .cn-button { @include transition(background-color, color); background: tint(5%); \
                     height: map.get(map.get(t.$control-sizes, sm), height); }\n",
                ),
            ],
        );
        let css = compile(dir.entry(), None).unwrap();
        assert!(
            css.contains(
                "transition: background-color var(--duration-fast) var(--ease-state), \
                 color var(--duration-fast) var(--ease-state);"
            ),
            "{css}"
        );
        assert!(
            css.contains("background: color-mix(in srgb, var(--text-primary) 5%, transparent);")
        );
        assert!(css.contains("height: 32px;"));
    }

    #[test]
    fn the_output_is_wrapped_in_a_layer_and_has_no_charset() {
        let dir = Scratch::new(
            "layer",
            &[("index.scss", "/* — a dash */\n.a { content: \"é\"; }\n")],
        );
        let css = compile(dir.entry(), Some("components")).unwrap();
        assert!(css.starts_with("@layer components {\n"), "{css}");
        assert!(css.trim_end().ends_with('}'));
        assert!(!css.contains("@charset"));
        assert!(
            compile(dir.entry(), None)
                .unwrap()
                .starts_with("/* — a dash */")
        );
    }

    #[test]
    fn blinc_css_passes_through_unchanged() {
        let css = ".a:hover:not(:disabled) { background: color-mix(in srgb, var(--a, var(--b)) 44%, var(--c)) !important; \
                   overflow-fade: 0px 24px 0px 24px; }\n@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }\n";
        let dir = Scratch::new("passthrough", &[("index.scss", css)]);
        let out = compile(dir.entry(), None).unwrap();
        for part in [
            ".a:hover:not(:disabled)",
            "color-mix(in srgb, var(--a, var(--b)) 44%, var(--c)) !important",
            "overflow-fade: 0px 24px 0px 24px;",
            "@keyframes spin",
            "transform: rotate(360deg);",
        ] {
            assert!(out.contains(part), "{part} not in {out}");
        }
    }

    #[test]
    fn a_keyframes_mixin_writes_its_steps() {
        let dir = Scratch::new(
            "keyframes",
            &[(
                "index.scss",
                "@use \"blinc/mixins\" as *;\n@include keyframes(cn-spin, (from: (transform: rotate(0deg)), to: (transform: rotate(360deg))));\n",
            )],
        );
        let css = compile(dir.entry(), None).unwrap();
        assert!(css.contains("@keyframes cn-spin"), "{css}");
        assert!(
            css.contains("from {\n    transform: rotate(0deg);"),
            "{css}"
        );
    }

    #[test]
    fn an_error_names_the_file_and_line() {
        let dir = Scratch::new(
            "error",
            &[
                ("index.scss", "@use \"card\";\n"),
                (
                    "_card.scss",
                    ".cn-card {\n  color: red;\n  height: $missing;\n}\n",
                ),
            ],
        );
        let message = compile(dir.entry(), None).unwrap_err().to_string();
        assert!(message.contains("_card.scss:3"), "{message}");
        assert!(message.contains("Undefined variable"), "{message}");
    }

    #[test]
    fn sources_lists_every_scss_file_in_order() {
        let dir = Scratch::new(
            "sources",
            &[
                ("index.scss", ""),
                ("components/_b.scss", ""),
                ("components/_a.scss", ""),
                ("notes.md", ""),
            ],
        );
        let names: Vec<_> = sources(&dir.0)
            .unwrap()
            .iter()
            .map(|p| {
                p.strip_prefix(&dir.0)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            names,
            ["components/_a.scss", "components/_b.scss", "index.scss"]
        );
    }
}
