#!/usr/bin/env python3
"""Restore immutable book archives and build the newest release if it has none."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("book_versions", ROOT / "scripts/book_versions.py")
books = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(books)


def run(command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def gh_json(*arguments):
    return json.loads(subprocess.check_output(["gh", *arguments], text=True))


def validate_archive(archive, tag, version):
    with tempfile.TemporaryDirectory(prefix="blinc-book-verify-") as temporary:
        metadata = books.unpack(archive, Path(temporary) / "book")
        commit = subprocess.check_output(
            ["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"], cwd=ROOT, text=True).strip()
        if metadata["id"] != version or metadata["commit"] != commit:
            raise RuntimeError(f"Book archive does not match release tag {tag}")


def restore(repo, archives):
    releases = gh_json("release", "list", "--repo", repo, "--limit", "100",
                       "--exclude-drafts", "--exclude-pre-releases", "--json", "tagName")
    supported = []
    for release in releases:
        tag = release["tagName"]
        version = tag.removeprefix("v")
        if books.VERSION.fullmatch(version):
            supported.append((tuple(map(int, version.split("."))), tag, version))
    if not supported:
        raise RuntimeError("No published stable release is available for the book")
    supported.sort(reverse=True)
    restored = set()
    for _, tag, version in supported:
        filename = f"blinc-book-{version}.tar.gz"
        assets = gh_json("release", "view", tag, "--repo", repo, "--json", "assets")["assets"]
        if any(asset["name"] == filename for asset in assets):
            run(["gh", "release", "download", tag, "--repo", repo,
                 "--pattern", filename, "--dir", str(archives), "--skip-existing"])
            validate_archive(archives / filename, tag, version)
            restored.add(version)
    _, tag, version = supported[0]
    return tag, version, version in restored


def snapshot(tag, version, archives):
    archive = archives / f"blinc-book-{version}.tar.gz"
    if archive.exists():
        return archive
    work = ROOT / "target/book-release-source" / version
    work.parent.mkdir(parents=True, exist_ok=True)
    if work.exists():
        raise RuntimeError(f"Release worktree already exists: {work}")
    run(["git", "worktree", "add", "--detach", str(work), tag], cwd=ROOT)
    try:
        strip = work / "scripts/ci-strip-local-patch.sh"
        if strip.is_file():
            run(["bash", str(strip)], cwd=work)
        lock = work / "ci/workspace.lock"
        if lock.is_file():
            shutil.copy2(lock, work / "Cargo.lock")
        environment = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_TARGET_DIR=str(ROOT / "target"))
        examples = work / "target/book-examples"
        run(["cargo", "run", "--package", "blinc-build-web-examples", "--", "--build",
             "--stage-to", str(examples)], cwd=work, env=environment)
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=work, text=True).strip()
        output = work / "target/frozen-book"
        books.build(work, output, version, commit, tag)
        books.freeze(output, examples, archive)
    finally:
        run(["git", "worktree", "remove", "--force", str(work)], cwd=ROOT)
    return archive


def main():
    repo = os.environ["GITHUB_REPOSITORY"]
    archives = ROOT / "target/book-releases"
    archives.mkdir(parents=True, exist_ok=True)
    tag, version, published = restore(repo, archives)
    archive = archives / f"blinc-book-{version}.tar.gz"
    if not archive.exists():
        archive = snapshot(tag, version, archives)
    if not published:
        validate_archive(archive, tag, version)
        if os.environ.get("BLINC_BOOK_UPLOAD") == "true":
            # No clobber: a released snapshot is never replaced by a main build.
            run(["gh", "release", "upload", tag, str(archive), "--repo", repo])
    print(f"Stable book: {version}; archives: {archives}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Book release preparation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
