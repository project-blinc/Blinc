"""Run the ABI encoder with probe-only accessors, leaving its checkout untouched."""
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[5]
abi = root.parent / "blinc_abi"
with tempfile.TemporaryDirectory(prefix="blinc-image-probe-") as directory:
    scratch = Path(directory)
    copy = scratch / "abi"
    copy.mkdir()
    shutil.copy2(abi / "Cargo.toml", copy / "Cargo.toml")
    shutil.copytree(abi / "src", copy / "src")
    context = copy / "src/context.rs"
    source = context.read_text()
    marker = "impl LayoutContext {"
    assert marker in source
    source = source.replace(marker, marker + '''
    pub fn probe_mapping(&mut self, node: Node, fit: i32, position: [f32; 4]) {
        let id = self.check(node).unwrap();
        self.tree.as_mut().unwrap().set_image_mapping(id, fit, position);
        self.revision += 1;
    }
    pub fn probe_background(&mut self, node: Node, position: [f32; 4], repeat: u8) {
        let id = self.check(node).unwrap();
        self.tree.as_mut().unwrap().background_mappings.insert(id, crate::types::ImageMapping { position, repeat });
        self.revision += 1;
    }
''', 1)
    context.write_text(source)
    manifest = '''[package]
name = "image_probe"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
taffy = { version = "0.14", features = ["content_size"] }
'''
    manifest += f'blinc_abi = {{ path = "{copy}", default-features = false, features = ["scene"] }}\n'
    manifest += f'blinc_layout = {{ path = "{root / "crates/blinc_layout"}" }}\n'
    manifest += '[patch."https://github.com/project-blinc/blinc"]\n'
    for name in ["blinc_core", "blinc_layout", "blinc_text", "blinc_svg", "blinc_image", "blinc_animation", "blinc_css", "blinc_theme", "blinc_paint", "blinc_platform", "blinc_icons", "blinc_tabler_icons"]:
        manifest += f'{name} = {{ path = "{root / "crates" / name}" }}\n'
    (scratch / "Cargo.toml").write_text(manifest)
    (scratch / "src").mkdir()
    shutil.copy2(Path(__file__).with_suffix(".rs"), scratch / "src/main.rs")
    subprocess.run(["cargo", "run", "--offline", "--target-dir", str(root / "target/abi-image-probe")], cwd=scratch, check=True)
    for fixture in (scratch / "out").glob("*.f32"):
        shutil.copy2(fixture, Path(__file__).parent / fixture.name)
