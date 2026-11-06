use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, process::Command};

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn revision(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("cannot read source revision".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn files(dir: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            files(&path, output)?;
        } else {
            output.push(path);
        }
    }
    Ok(())
}
pub(super) fn copy_files(from: &Path, to: &Path, exclude: &[&str]) -> Result<()> {
    fs::create_dir_all(to)?;
    let mut paths = Vec::new();
    files(from, &mut paths)?;
    for path in paths {
        if path
            .components()
            .any(|p| exclude.contains(&p.as_os_str().to_str().unwrap_or("")))
        {
            continue;
        }
        let destination = to.join(path.strip_prefix(from)?);
        fs::create_dir_all(destination.parent().unwrap())?;
        fs::copy(path, destination)?;
    }
    Ok(())
}

pub(super) fn runtime_digest(root: &Path) -> Result<String> {
    let mut paths = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
    for dir in ["crates", "extensions", "examples/blinc_component_demos"] {
        let mut found = Vec::new();
        files(&root.join(dir), &mut found)?;
        paths.extend(found.into_iter().filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("rs" | "toml" | "css" | "wgsl" | "ttf" | "otf")
            )
        }));
    }
    let mut web = Vec::new();
    files(&root.join("web/components"), &mut web)?;
    paths.extend(web.into_iter().filter(|p| {
        !p.components()
            .any(|part| matches!(part.as_os_str().to_str(), Some("previews" | "tests")))
            && matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("js" | "css" | "html" | "json")
            )
    }));
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        hash.update(path.strip_prefix(root)?.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(fs::read(path)?);
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn fixture_digest(root: &Path, component: &Component) -> Result<String> {
    let mut hash = Sha256::new();
    let common = fs::read_to_string(root.join("examples/blinc_component_demos/src/demos.rs"))?;
    hash.update(
        common
            .split("pub fn build(")
            .next()
            .ok_or("fixture setup missing")?,
    );
    hash.update(serde_json::to_vec(component)?);
    hash.update(super::pages::fixture_source(root, component)?);
    hash.update(super::pages::function(&common, "bound_text")?);
    for file in [
        "examples/blinc_component_demos/src/capture.rs",
        "examples/blinc_component_demos/src/cursor.rs",
        "examples/blinc_component_demos/src/recording_motion.rs",
        "examples/blinc_component_demos/src/recording_config.rs",
        "examples/blinc_component_demos/assets/fennec-fox.svg",
        "crates/blinc_layout/src/ua_styles.rs",
        "examples/blinc_component_demos/assets/JetBrainsMono-Regular.ttf",
        "examples/blinc_component_demos/assets/landscape.data-uri",
        "examples/blinc_component_demos/assets/handpan.mp4",
        "examples/blinc_component_demos/assets/night-drive.flac",
        "examples/blinc_component_demos/assets/night-drive-waveform.json",
        "examples/blinc_component_demos/demo.css",
        "crates/blinc_cn/src/cn_styles.rs",
        "examples/blinc_component_demos/src/lib.rs",
        "examples/blinc_component_demos/assets/NotoSans-Regular.ttf",
        "examples/blinc_component_demos/assets/NotoSans-Bold.ttf",
        "examples/blinc_component_demos/assets/cursor.svg",
    ] {
        hash.update(fs::read(root.join(file))?);
        hash.update([0]);
    }
    // Fixture-specific pictures are fingerprinted here, so the shared list above stays stable
    // and only the fixture that shows one needs re-recording.
    if component.slug == "image" {
        hash.update(fs::read(
            root.join("examples/blinc_component_demos/assets/avatar.data-uri"),
        )?);
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn compile(root: &Path, input: &str) -> Result<()> {
    let dir = root.join("target/component-runtime");
    if fs::read_to_string(dir.join(".inputs-sha256"))
        .ok()
        .as_deref()
        == Some(input)
        && dir.join("blinc_component_demos_bg.wasm").is_file()
        && dir.join("blinc_component_demos.js").is_file()
    {
        println!("Worker runtime unchanged; reusing the local bundle.");
        return Ok(());
    }
    let status = Command::new("wasm-pack")
        .args(["build", "--target", "web", "--release", "--out-dir"])
        .arg(&dir)
        .args([".", "--", "--locked"])
        .env("CARGO_BUILD_JOBS", "2")
        .current_dir(root.join("examples/blinc_component_demos"))
        .status()?;
    if !status.success() {
        return Err("component wasm-pack build failed".into());
    }
    fs::write(dir.join(".inputs-sha256"), input)?;
    Ok(())
}
pub(super) fn record(root: &Path, catalog: &Catalog, only: Option<&str>) -> Result<()> {
    let dir = root.join("target/component-captures");
    let mut command = Command::new("cargo");
    command
        .args([
            "run",
            "--locked",
            "--offline",
            "-p",
            "blinc_component_demos",
            "--features",
            "capture",
            "--bin",
            "capture-components",
            "--",
        ])
        .arg(&dir)
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2");
    if let Some(slug) = only {
        command.arg(slug);
    }
    if !command.status()?.success() {
        return Err("component recording failed".into());
    }
    for c in catalog
        .fixtures()
        .iter()
        .filter(|c| only.is_none_or(|s| s == c.slug))
    {
        let fixture = fixture_digest(root, &c)?;
        let destination = root.join("web/components/previews").join(&c.slug);
        fs::create_dir_all(&destination)?;
        let mut hashes = BTreeMap::new();
        let animated = c.animated_preview();
        let files = if animated {
            vec!["preview.gif", "poster.png"]
        } else {
            vec!["poster.png"]
        };
        for file in files {
            let bytes = fs::read(dir.join(&c.slug).join(file))?;
            hashes.insert(file, digest(&bytes));
            fs::write(destination.join(file), bytes)?;
        }
        if !animated && destination.join("preview.gif").exists() {
            fs::remove_file(destination.join("preview.gif"))?;
        }
        let metadata = serde_json::json!({"fixture_sha256": fixture, "logical_width":c.viewport.width, "logical_height":c.viewport.height, "pixel_width":c.viewport.width*c.viewport.scale, "pixel_height":c.viewport.height*c.viewport.scale, "frames":if animated {c.recording.as_ref().and_then(|v| v["frames"].as_u64()).unwrap_or(48)} else {1}, "animated":animated, "delay_ms":if animated {60} else {0}, "theme":"hybrid", "scheme":"dark", "cursor":if c.capabilities.iter().any(|s| s == "pointer" || s == "keyboard") {"interaction-specific pointer movement and deliberate actions"} else {"none"}, "artifacts_sha256":hashes});
        fs::write(
            destination.join("recording.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
    }
    Ok(())
}
pub(super) fn preview_manifest(
    root: &Path,
    catalog: &Catalog,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut result = BTreeMap::new();
    for c in &catalog.fixtures() {
        let expected = fixture_digest(root, c)?;
        let dir = root.join("web/components/previews").join(&c.slug);
        let metadata: serde_json::Value = serde_json::from_slice(
            &fs::read(dir.join("recording.json"))
                .map_err(|_| format!("missing {} recording; run with --record", c.slug))?,
        )?;
        if metadata["fixture_sha256"] != expected {
            return Err(format!("{} recording is stale; regenerate with --record", c.slug).into());
        }
        let files = if c.animated_preview() {
            vec!["poster.png", "preview.gif"]
        } else {
            vec!["poster.png"]
        };
        for file in files {
            if metadata["artifacts_sha256"][file] != digest(&fs::read(dir.join(file))?) {
                return Err(format!("{} {file} recording checksum mismatch", c.slug).into());
            }
        }
        result.insert(c.slug.clone(), metadata);
    }
    Ok(result)
}
pub(super) fn artifact_manifest(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut paths = Vec::new();
    files(dir, &mut paths)?;
    let mut result = BTreeMap::new();
    for path in paths {
        let relative = path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
        if relative == "build-manifest.json" {
            continue;
        }
        result.insert(relative, digest(&fs::read(path)?));
    }
    Ok(result)
}
