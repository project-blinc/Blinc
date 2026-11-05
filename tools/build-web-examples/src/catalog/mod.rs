//! Static component pages and the shared worker-demo build.
mod build;
mod charts;
mod pages;

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
pub(super) struct Catalog {
    schema_version: u32,
    pub api_version: String,
    pub components: Vec<Component>,
    pub coverage: Coverage,
}
#[derive(Deserialize)]
pub(super) struct Coverage {
    widgets: std::collections::BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Component {
    pub slug: String,
    pub title: String,
    pub library: String,
    pub category: String,
    pub tags: Vec<String>,
    pub since: Option<String>,
    pub maturity: String,
    pub description: String,
    pub doc: String,
    pub doc_section: Option<String>,
    pub entry: String,
    pub source: String,
    pub viewport: Viewport,
    pub capabilities: Vec<String>,
    pub props: Vec<Prop>,
    pub classes: Vec<String>,
    pub keyboard: String,
    pub tokens: String,
    pub notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ChartExample>,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub implementation: Vec<String>,
    #[serde(default)]
    pub recipe: Option<String>,
    #[serde(default)]
    pub controls: Vec<String>,
    #[serde(default)]
    pub recording: Option<serde_json::Value>,
    #[serde(default)]
    pub custom_css: Option<bool>,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct ChartExample {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}
impl Component {
    fn animated_preview(&self) -> bool {
        self.capabilities
            .iter()
            .any(|c| matches!(c.as_str(), "pointer" | "keyboard" | "animation"))
    }
}
impl Catalog {
    fn fixtures(&self) -> Vec<Component> {
        self.components
            .iter()
            .flat_map(|c| {
                std::iter::once(c.clone()).chain(
                    c.examples
                        .iter()
                        .map(|example| charts::component(c, example)),
                )
            })
            .collect()
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Prop {
    pub name: String,
    pub r#type: String,
    pub kind: String,
    pub example: String,
}

pub(super) fn run() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut args = std::env::args().skip(1);
    let mut output = None;
    let (mut compile, mut record, mut only) = (false, false, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog-to" => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--catalog-to needs a directory")?,
                ))
            }
            "--build" => compile = true,
            "--record" => record = true,
            "--only" => only = Some(args.next().ok_or("--only needs a slug")?),
            _ => return Err(format!("unknown catalog argument: {arg}").into()),
        }
    }
    let output = output.ok_or("--catalog-to needs a directory")?;
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    if output == root || output == root.join("web") || output.starts_with(root.join("web")) {
        return Err("stage the catalog into a build directory, outside web/".into());
    }
    let catalog: Catalog =
        serde_json::from_slice(&fs::read(root.join("web/components/catalog.json"))?)?;
    validate(root, &catalog)?;
    if let Some(slug) = &only {
        if !catalog.fixtures().iter().any(|c| &c.slug == slug) {
            return Err(format!("unknown slug: {slug}").into());
        }
    }
    if record {
        build::record(root, &catalog, only.as_deref())?;
    }
    let runtime_digest = build::runtime_digest(root)?;
    if compile {
        build::compile(root, &runtime_digest)?;
    }
    let revision = build::revision(root)?;
    let previews = build::preview_manifest(root, &catalog)?;
    let dir = output.join(format!(".components-build-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    build::copy_files(
        &root.join("web/components"),
        &dir,
        &["README.md", "catalog.json", "templates", "tests"],
    )?;
    let release = dir.join("releases").join(&runtime_digest);
    build::copy_files(
        &root.join("web/components"),
        &release,
        &[
            "README.md",
            "catalog.json",
            "templates",
            "previews",
            "tests",
        ],
    )?;
    fs::create_dir_all(output.join("assets"))?;
    fs::copy(
        root.join("web/assets/blinc_logo.svg"),
        output.join("assets/blinc_logo.svg"),
    )?;
    let css = default_css(root)?;
    let source_dir = dir.join("source");
    fs::create_dir_all(&source_dir)?;
    fs::write(source_dir.join("cn.css"), &css)?;
    fs::copy(
        root.join("examples/blinc_component_demos/src/demos.rs"),
        source_dir.join("demos.rs"),
    )?;
    fs::copy(
        root.join("examples/blinc_component_demos/demo.css"),
        source_dir.join("demo.css"),
    )?;
    fs::copy(
        root.join("examples/blinc_component_demos/assets/NotoSans-OFL.txt"),
        source_dir.join("NotoSans-OFL.txt"),
    )?;
    fs::copy(
        root.join("examples/blinc_component_demos/assets/JetBrainsMono-OFL.txt"),
        source_dir.join("JetBrainsMono-OFL.txt"),
    )?;
    build::copy_files(
        &root.join("examples/blinc_component_demos/assets"),
        &source_dir.join("assets"),
        &[],
    )?;
    fs::create_dir_all(dir.join("installation"))?;
    fs::write(
        dir.join("installation/index.html"),
        pages::installation(root, &catalog, &runtime_digest)?,
    )?;
    let mut pages = vec!["/components/installation/".to_string()];
    for component in &catalog.components {
        let component_dir = dir.join(&component.library).join(&component.slug);
        fs::create_dir_all(&component_dir)?;
        fs::write(
            component_dir.join("index.html"),
            pages::detail(root, &catalog, component, &revision, &runtime_digest, &css)?,
        )?;
        pages.push(format!(
            "/components/{}/{}/",
            component.library, component.slug
        ));
        charts::stage(
            root,
            &dir,
            &catalog,
            component,
            &revision,
            &runtime_digest,
            &css,
            &mut pages,
        )?;
    }
    fs::write(
        dir.join("index.html"),
        pages::index(&catalog, &runtime_digest),
    )?;
    let bundle = root.join("target/component-runtime");
    if bundle.join(".inputs-sha256").exists() {
        if fs::read_to_string(bundle.join(".inputs-sha256"))? != runtime_digest {
            return Err("worker bundle is stale; rerun with --build".into());
        }
        build::copy_files(
            &bundle,
            &release.join("runtime/pkg"),
            &[".inputs-sha256", "package.json", "README.md", ".gitignore"],
        )?;
    } else if compile {
        return Err("worker bundle missing after build".into());
    }
    fs::write(dir.join("sitemap.xml"), pages::sitemap(&catalog))?;
    let artifacts = build::artifact_manifest(&dir)?;
    let manifest = serde_json::json!({
        "schema_version": 1, "api_version": catalog.api_version,
        "runtime_channel": "unreleased", "source_revision": revision,
        "source_dirty": !std::process::Command::new("git").args(["status", "--porcelain", "--", "crates", "extensions", "examples/blinc_component_demos", "web/components"]).current_dir(root).output()?.stdout.is_empty(),
        "runtime_inputs_sha256": runtime_digest,
        "cargo_lock_sha256": build::digest(&fs::read(root.join("Cargo.lock"))?),
        "features": ["blinc_app/web"], "routes": pages,
        "previews": previews, "artifacts_sha256": artifacts,
        "runtime_built": bundle.join(".inputs-sha256").exists(),
        "components": catalog.components,
    });
    fs::write(
        dir.join("build-manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    let destination = output.join("components");
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(&dir, &destination)?;
    println!(
        "Catalog: {} component pages → {}",
        catalog.components.len(),
        destination.display()
    );
    Ok(())
}

fn validate(root: &Path, catalog: &Catalog) -> Result<()> {
    if catalog.schema_version != 1 {
        return Err("unsupported catalog schema".into());
    }
    let mut slugs = BTreeSet::new();
    let css = default_css(root)?;
    for c in &catalog.fixtures() {
        if !matches!(c.library.as_str(), "cn" | "layout")
            || c.slug.is_empty()
            || !c
                .slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(format!("invalid component route: {}/{}", c.library, c.slug).into());
        }
        if !slugs.insert(&c.slug) {
            return Err(format!("duplicate slug: {}", c.slug).into());
        }
        let source = fs::read_to_string(root.join(&c.source))?;
        if !source.contains(&format!("pub fn {}(", c.entry)) {
            return Err(format!("missing fixture: {}", c.entry).into());
        }
        if !root.join(&c.doc).is_file() || !root.join(&c.source).is_file() {
            return Err(format!("missing source for {}", c.slug).into());
        }
        for class in &c.classes {
            let part = class.trim_start_matches(['.', '#']);
            let mut implementation = String::new();
            for path in &c.implementation {
                implementation.push_str(&fs::read_to_string(root.join(path))?);
            }
            implementation.push_str(&fs::read_to_string(
                root.join("examples/blinc_component_demos/demo.css"),
            )?);
            if !css.contains(class)
                && !implementation.contains(part)
                && class != &format!(".demo--{}", c.slug)
            {
                return Err(format!("{} documents an unknown CSS class: {}", c.slug, class).into());
            }
        }
    }
    let public = syn::parse_file(&fs::read_to_string(
        root.join("crates/blinc_cn/src/components/mod.rs"),
    )?)?;
    for item in public.items {
        if let syn::Item::Mod(module) = item {
            if matches!(module.vis, syn::Visibility::Public(_))
                && !catalog
                    .components
                    .iter()
                    .any(|c| c.module == format!("cn::{}", module.ident))
            {
                return Err(format!("missing gallery family: cn::{}", module.ident).into());
            }
        }
    }
    let widgets = syn::parse_file(&fs::read_to_string(
        root.join("crates/blinc_layout/src/widgets/mod.rs"),
    )?)?;
    for item in widgets.items {
        if let syn::Item::Mod(module) = item {
            if !matches!(module.vis, syn::Visibility::Public(_)) {
                continue;
            }
            let name = module.ident.to_string();
            let entries = catalog
                .coverage
                .widgets
                .get(&name)
                .ok_or_else(|| format!("missing built-in widget coverage: {name}"))?;
            if entries.is_empty()
                || entries
                    .iter()
                    .any(|slug| !catalog.components.iter().any(|c| c.slug == *slug))
            {
                return Err(format!("invalid widget coverage for {name}").into());
            }
        }
    }
    Ok(())
}

fn default_css(root: &Path) -> Result<String> {
    let mut output = String::new();
    for path in [
        "crates/blinc_layout/src/ua_styles.rs",
        "crates/blinc_cn/src/cn_styles.rs",
    ] {
        let source = fs::read_to_string(root.join(path))?;
        let (_, body) = source
            .split_once("    r#\"\n")
            .ok_or("stylesheet literal missing")?;
        let (body, _) = body.rsplit_once("\"#,").ok_or("stylesheet end missing")?;
        output.push_str(body);
        output.push('\n');
    }
    Ok(format!("@layer components {{\n{output}\n}}\n"))
}
