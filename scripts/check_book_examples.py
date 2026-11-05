#!/usr/bin/env python3
"""Type-check the book's complete getting-started applications and typography examples."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
BOOK = ROOT / "docs/book/src"
BLOCKS = re.compile(r"^```(rust[^\n]*|css)\n(.*?)^```", re.M | re.S)


def blocks(chapter):
    return [(language, source) for language, source in BLOCKS.findall((BOOK / chapter).read_text())]


def check(directory, target=None):
    features = ["web" if target else "windowed", "cn", "svg", "icons"]
    manifest = f'''[package]
name = "blinc-book-examples"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
blinc = {{ path = {json.dumps(str(ROOT / "crates/blinc"))}, default-features = false, features = {json.dumps(features)} }}
'''
    if target:
        manifest += '''wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["console"] }
console_error_panic_hook = "0.1"
tracing = "0.1"
tracing-wasm = "0.2"
'''
    manifest += '''[profile.dev]
opt-level = 1
debug = "line-tables-only"
[profile.dev.package."*"]
debug = false
'''
    (directory / "Cargo.toml").write_text(manifest)
    if (ROOT / "Cargo.lock").is_file():
        shutil.copy2(ROOT / "Cargo.lock", directory / "Cargo.lock")
    source = directory / "src"
    source.mkdir()
    modules = []
    count = 0
    if target:
        rust = next(text for language, text in blocks("web/setup.md") if language.startswith("rust"))
        (source / "web.rs").write_text(rust)
        fonts = directory / "fonts"
        fonts.mkdir()
        shutil.copy2(ROOT / "examples/blinc_component_demos/assets/NotoSans-Regular.ttf", fonts / "Inter.ttf")
        modules.append("mod web;")
        count += 1
    else:
        for chapter in ["getting-started/installation.md", "getting-started/first-app.md", "cn/overview.md"]:
            for language, text in blocks(chapter):
                if language.startswith("rust") and "fn main()" in text:
                    modules.append(f"mod example_{count} {{\n{text}\n}}")
                    count += 1
                elif language == "css" and chapter.endswith("first-app.md"):
                    (source / "app.css").write_text(text)
        for language, text in blocks("cn/typography.md"):
            if language.startswith("rust"):
                modules.append(f"fn typography_{count}() {{ let _example = {{\n{text}\n}}; }}")
                count += 1
        for language, text in blocks("introduction.md"):
            if language.startswith("rust"):
                modules.append(f"fn introduction_{count}() {{ let _example = {{\n{text}\n}}; }}")
                count += 1
        for language, text in blocks("getting-started/project-structure.md"):
            if language.startswith("rust"):
                filename = re.match(r"// src/(.+)\n", text)[1]
                path = source / filename
                if filename == "main.rs":
                    modules.append(text.replace("fn main()", "fn project_structure_main()"))
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(text)
                count += 1
            elif language == "css":
                (source / "app.css").write_text(text)
    (source / "lib.rs").write_text("#![allow(unused)]\n" + "\n".join(modules))
    command = ["cargo", "check", "--manifest-path", str(directory / "Cargo.toml"), "--offline"]
    if target:
        command += ["--target", target]
    environment = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_TARGET_DIR=str(ROOT / "target"))
    subprocess.run(command, env=environment, check=True)
    print(f"Type-checked {count} book examples" + (f" for {target}" if target else ""))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wasm", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="blinc-book-examples-") as temporary:
        check(Path(temporary), "wasm32-unknown-unknown" if args.wasm else None)


if __name__ == "__main__":
    main()
