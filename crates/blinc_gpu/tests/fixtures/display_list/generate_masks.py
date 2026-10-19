"""Run the unchanged ABI encoder against the local Blinc crates."""
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[5]
with tempfile.TemporaryDirectory(prefix="blinc-masks-probe-") as directory:
    scratch = Path(directory)
    manifest = f'''[package]
name = "masks_probe"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
taffy = {{ version = "0.14", features = ["content_size"] }}
blinc_abi = {{ path = "{root.parent / 'blinc_abi'}", default-features = false, features = ["scene"] }}
blinc_layout = {{ path = "{root / 'crates/blinc_layout'}" }}
[patch."https://github.com/project-blinc/blinc"]
'''
    for name in ["blinc_core", "blinc_layout", "blinc_text", "blinc_svg", "blinc_image", "blinc_animation", "blinc_css", "blinc_theme", "blinc_paint", "blinc_platform", "blinc_icons", "blinc_tabler_icons"]:
        manifest += f'{name} = {{ path = "{root / "crates" / name}" }}\n'
    (scratch / "Cargo.toml").write_text(manifest)
    (scratch / "src").mkdir()
    shutil.copy2(Path(__file__).with_suffix(".rs"), scratch / "src/main.rs")
    subprocess.run(["cargo", "run", "--offline", "--target-dir", str(root / "target/abi-image-probe")], cwd=scratch, check=True)
    for fixture in (scratch / "out").iterdir():
        shutil.copy2(fixture, Path(__file__).parent / fixture.name)
