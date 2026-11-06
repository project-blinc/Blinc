#!/usr/bin/env python3
"""Move book demo WASM modules and large sample assets to a content-addressed asset origin.

`externalize` removes every examples/*/pkg/*_bg.wasm and every sample asset of 1 MiB or more from
an assembled docs tree, stores each once as <objects>/wasm/<sha256>.wasm or
<objects>/asset/<sha256><extension>, and points the demo page at <base>/<key>. The page loads the
module through init() and remaps the asset fetches and preload links, so frozen release builds
need no change. Editions that ship the same bytes share one object. Media files stay with the
page, which streams them through the element. `verify` checks the uploaded objects over HTTP.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
import urllib.error
import urllib.request

INIT = "await init();"
ASSETS = "examples/blinc_app_examples/examples/assets"
LARGE = 1024 * 1024
MEDIA = {".mp4", ".webm", ".mov", ".mp3", ".wav", ".flac", ".ogg"}
SHIM = (
    "<script>(function(){var m=%s,b=document.baseURI,f=window.fetch,t={};"
    "for(var k in m)t[new URL(k,b).href]=m[k];"
    "window.fetch=function(i,n){var u=typeof i==='string'?i:i instanceof URL?i.href:i&&i.url,"
    "r=u&&t[new URL(u,b).href];"
    "return f.call(this,r?(typeof i==='string'||i instanceof URL?r:new Request(r,i)):i,n)}})();</script>"
)
WASM = "application/wasm"
IMMUTABLE = "public, max-age=31536000, immutable"


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def base_url(value):
    if (not re.fullmatch(r"https://[A-Za-z0-9.-]+(:\d+)?(/[A-Za-z0-9._~-]+)*/?", value)
            or re.search(r"/\.\.?(/|$)", value)):
        raise ValueError(f"The asset base must be an https origin and optional path: {value}")
    return value.rstrip("/")


def store(file, directory, suffix, objects, stored):
    digest = checksum(file)
    key = f"{directory}/{digest}{suffix}"
    target = objects / key
    if not target.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(file, target)
    stored[key] = {"sha256": digest, "bytes": file.stat().st_size}
    return key


def externalize(docs, base, objects):
    """Rewrite every demo under `docs`; return the object list as {key: {sha256, bytes}}."""
    base = base_url(base)
    stored = {}
    for page in sorted(docs.rglob("examples/*/index.html")):
        modules = sorted((page.parent / "pkg").glob("*_bg.wasm"))
        if len(modules) != 1:
            raise ValueError(f"{page}: expected one WASM module, found {len(modules)}")
        text = page.read_text()
        if text.count(INIT) != 1:
            raise ValueError(f"{page}: expected exactly one `{INIT}` to point at the asset origin")
        remap = {}
        for file in sorted((page.parent / ASSETS).rglob("*")):
            if file.is_file() and file.stat().st_size >= LARGE and file.suffix.lower() not in MEDIA:
                path = file.relative_to(page.parent).as_posix()
                remap[path] = f"{base}/{store(file, 'asset', file.suffix.lower(), objects, stored)}"
                file.unlink()
        for path, url in remap.items():
            text = text.replace(f'href="{path}"', f'href="{url}"')
        key = store(modules[0], "wasm", ".wasm", objects, stored)
        text = text.replace(INIT, f'await init({{ module_or_path: "{base}/{key}" }});')
        if remap:
            if "<script" not in text:
                raise ValueError(f"{page}: no script to place the asset remap before")
            text = text.replace("<script", SHIM % json.dumps(remap) + "\n    <script", 1)
        page.write_text(text)
        modules[0].unlink()
    return stored


def verify(base, manifest, origin):
    """HEAD each object; return a list of problems, empty when all are servable."""
    base = base_url(base)
    problems = []
    for key, item in sorted(manifest.items()):
        request = urllib.request.Request(f"{base}/{key}", method="HEAD", headers={"Origin": origin, "User-Agent": "blinc-asset-check"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                headers = response.headers
        except urllib.error.URLError as error:
            problems.append(f"{key}: {error}")
            continue
        if key.startswith("wasm/") and headers.get_content_type() != WASM:
            problems.append(f"{key}: content type {headers.get_content_type()}")
        if "immutable" not in (headers.get("Cache-Control") or ""):
            problems.append(f"{key}: cache policy {headers.get('Cache-Control')}")
        if headers.get("Access-Control-Allow-Origin") not in ("*", origin):
            problems.append(f"{key}: no CORS grant for {origin}")
        if headers.get("Content-Length") != str(item["bytes"]):
            problems.append(f"{key}: length {headers.get('Content-Length')}, expected {item['bytes']}")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    move = commands.add_parser("externalize")
    move.add_argument("--docs", type=Path, required=True)
    move.add_argument("--base", required=True)
    move.add_argument("--objects", type=Path, required=True)
    check = commands.add_parser("verify")
    check.add_argument("--base", required=True)
    check.add_argument("--objects", type=Path, required=True)
    check.add_argument("--origin", required=True)
    args = parser.parse_args()
    manifest_file = args.objects / "objects.json"
    if args.command == "externalize":
        manifest = externalize(args.docs, args.base, args.objects)
        manifest_file.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        total = sum(item["bytes"] for item in manifest.values())
        print(f"{len(manifest)} objects, {total / 1048576:.0f} MiB")
        return 0
    problems = verify(args.base, json.loads(manifest_file.read_text()), args.origin)
    for problem in problems:
        print(problem, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
