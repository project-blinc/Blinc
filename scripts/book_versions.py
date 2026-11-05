#!/usr/bin/env python3
"""Build, freeze and assemble mdBook editions without changing released snapshots."""

import argparse
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
META = "book-version.json"


def edition(value):
    if value != "dev" and not VERSION.fullmatch(value):
        raise ValueError(f"Invalid book edition: {value}")
    return value


def relative(value):
    path = PurePosixPath(value)
    if not value or path.is_absolute() or ".." in path.parts or "\\" in value:
        raise ValueError(f"Invalid snapshot path: {value}")
    return path


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def files(directory):
    return {p.relative_to(directory).as_posix(): checksum(p)
            for p in sorted(directory.rglob("*")) if p.is_file() and p != directory / META}


def set_config(text, section, key, value):
    header = f"[{section}]"
    if header not in text:
        text += f"\n{header}\n"
    start = text.index(header) + len(header)
    match = re.search(r"^\[", text[start:], re.M)
    end = start + match.start() if match else len(text)
    body = text[start:end]
    assignment = f"{key} = {json.dumps(value)}"
    pattern = rf"^{re.escape(key)}\s*=.*$"
    if re.search(pattern, body, re.M):
        body = re.sub(pattern, lambda _: assignment, body, count=1, flags=re.M)
    else:
        body = "\n" + assignment + body
    return text[:start] + body + text[end:]


def build(source, output, name, commit, source_ref="main"):
    name = edition(name)
    if not COMMIT.fullmatch(commit):
        raise ValueError("A full source commit is required")
    output = output.resolve()
    with tempfile.TemporaryDirectory(prefix="blinc-book-") as temporary:
        book = Path(temporary) / "book"
        shutil.copytree(source / "docs/book", book)
        config = (book / "book.toml").read_text()
        parsed = tomllib.loads(config)
        html = parsed.get("output", {}).get("html", {})
        theme = book / "theme"
        theme.mkdir(exist_ok=True)
        for filename in ["version-selector.js", "version-selector.css"]:
            shutil.copy2(ROOT / "docs/book/theme" / filename, theme / filename)
        for key, filename in [("additional-js", "version-selector.js"),
                              ("additional-css", "version-selector.css")]:
            additions = list(html.get(key, []))
            asset = f"theme/{filename}"
            if asset not in additions:
                additions.append(asset)
            config = set_config(config, "output.html", key, additions)
        config = set_config(config, "output.html", "site-url", f"/docs/{name}/")
        edit = html.get("edit-url-template", "")
        if edit:
            config = set_config(config, "output.html", "edit-url-template",
                                edit.replace("/edit/main/", f"/edit/{source_ref}/"))
        tomllib.loads(config)
        (book / "book.toml").write_text(config)
        subprocess.run(["mdbook", "build", str(book), "--dest-dir", str(output)], check=True)
    pages = sorted(p.relative_to(output).as_posix() for p in output.rglob("*.html"))
    metadata = {"schema": 1, "id": name, "commit": commit, "source_ref": source_ref, "pages": pages}
    (output / META).write_text(json.dumps(metadata, indent=2) + "\n")
    return metadata


def freeze(book, examples, archive):
    metadata = json.loads((book / META).read_text())
    if edition(metadata["id"]) == "dev":
        raise ValueError("Development editions cannot be frozen as releases")
    if archive.exists():
        raise ValueError(f"Refusing to overwrite release archive: {archive}")
    if not examples.is_dir() or not any(examples.iterdir()):
        raise ValueError("A release snapshot requires its built example assets")
    shutil.copytree(examples, book / "examples", dirs_exist_ok=True)
    class Frames(HTMLParser):
        def handle_starttag(self, tag, attributes):
            if tag == "iframe":
                self.sources.append(dict(attributes).get("src", ""))

    for chapter in metadata["pages"]:
        parser = Frames()
        parser.sources = []
        parser.feed((book / chapter).read_text())
        for source in parser.sources:
            url = urlsplit(source)
            if not source or url.scheme or url.netloc or source.startswith("/"):
                continue
            target = ((book / chapter).parent / unquote(url.path)).resolve()
            if not target.is_relative_to(book.resolve()) or not target.is_file():
                raise ValueError(f"Missing release demo: {chapter} -> {source}")
    metadata["sha256"] = files(book)
    (book / META).write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n")
    verify(book)
    archive.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="blinc-book-archive-", dir=archive.parent) as temporary:
        completed = Path(temporary) / archive.name
        with tarfile.open(completed, "w:gz") as target:
            for path in sorted(book.rglob("*")):
                if path.is_symlink():
                    raise ValueError(f"Release snapshot contains a symlink: {path}")
                target.add(path, arcname=path.relative_to(book).as_posix(), recursive=False)
        os.link(completed, archive)


def verify(book):
    metadata = json.loads((book / META).read_text())
    if metadata.get("schema") != 1:
        raise ValueError("Unsupported book snapshot schema")
    edition(metadata["id"])
    if not COMMIT.fullmatch(metadata["commit"]):
        raise ValueError("Snapshot has no valid source commit")
    pages = metadata["pages"]
    if "index.html" not in pages:
        raise ValueError("Snapshot has no book index")
    for page in pages:
        relative(page)
        if not (book / page).is_file():
            raise ValueError(f"Missing chapter: {page}")
    expected = metadata.get("sha256")
    if expected is not None and files(book) != expected:
        raise ValueError("Release snapshot checksums do not match")
    return metadata


def unpack(archive, output):
    output.mkdir(parents=True)
    with tarfile.open(archive, "r:gz") as source:
        for member in source.getmembers():
            target = output / relative(member.name)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as stream, target.open("wb") as destination:
                    shutil.copyfileobj(stream, destination)
            else:
                raise ValueError(f"Unsupported snapshot entry: {member.name}")
    return verify(output)


def assemble(dev, archives, output):
    dev = dev.resolve()
    output = output.resolve()
    if verify(dev)["id"] != "dev":
        raise ValueError("The development input must be labelled dev")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="blinc-books-", dir=output.parent) as temporary:
        staging = Path(temporary) / "docs"
        releases = Path(temporary) / "releases"
        editions = []
        for archive in sorted(archives.glob("blinc-book-*.tar.gz")):
            folder = releases / archive.name.removesuffix(".tar.gz")
            metadata = unpack(archive, folder)
            name = edition(metadata["id"])
            if archive.name != f"blinc-book-{name}.tar.gz":
                raise ValueError("Release archive filename does not match its edition")
            if name == "dev" or not metadata.get("sha256"):
                raise ValueError("Only checksummed release snapshots can be assembled")
            if any(item["id"] == name for item in editions):
                raise ValueError(f"Duplicate book edition: {name}")
            editions.append(metadata)
        if not editions:
            raise ValueError("No released book snapshot is available; refusing to replace stable docs")
        editions.sort(key=lambda item: tuple(map(int, item["id"].split("."))), reverse=True)
        stable = editions[0]["id"]
        shutil.copytree(releases / f"blinc-book-{stable}", staging)
        for metadata in editions:
            name = metadata["id"]
            shutil.copytree(releases / f"blinc-book-{name}", staging / name)
        shutil.copytree(dev, staging / "dev")
        editions.append(verify(dev))
        manifest = {"schema": 1, "stable": stable, "editions": [
            {"id": item["id"], "url": f"/docs/{item['id']}/", "pages": item["pages"]}
            for item in editions
        ]}
        (staging / "versions.json").write_text(json.dumps(manifest, indent=2) + "\n")
        if output.exists():
            backup = Path(temporary) / "previous"
            output.rename(backup)
            try:
                staging.rename(output)
            except OSError:
                backup.rename(output)
                raise
        else:
            staging.rename(output)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build_args = commands.add_parser("build")
    build_args.add_argument("--source", type=Path, default=ROOT)
    build_args.add_argument("--output", type=Path, required=True)
    build_args.add_argument("--edition", required=True)
    build_args.add_argument("--commit", required=True)
    build_args.add_argument("--source-ref", default="main")
    freeze_args = commands.add_parser("freeze")
    freeze_args.add_argument("--book", type=Path, required=True)
    freeze_args.add_argument("--examples", type=Path, required=True)
    freeze_args.add_argument("--archive", type=Path, required=True)
    stage_args = commands.add_parser("assemble")
    stage_args.add_argument("--dev", type=Path, required=True)
    stage_args.add_argument("--archives", type=Path, required=True)
    stage_args.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        build(args.source, args.output, args.edition, args.commit, args.source_ref)
    elif args.command == "freeze":
        freeze(args.book, args.examples, args.archive)
    else:
        assemble(args.dev, args.archives, args.output)


if __name__ == "__main__":
    main()
