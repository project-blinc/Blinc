"""Validate the release crate graph before invoking Cargo in dependency order."""

import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time


# Keep experimental and independently maintained packages outside this release set.
RELEASE_CRATES = (
    "blinc_macros", "blinc_flow", "blinc_platform", "blinc_icons", "blinc_core",
    "blinc_animation", "blinc_paint", "blinc_svg", "blinc_text",
    "blinc_theme", "blinc_css", "blinc_image", "blinc_media", "blinc_layout",
    "blinc_noto_emoji", "blinc_noto_symbols", "blinc_gpu", "blinc_cn",
    "blinc_tabler_icons", "blinc_router", "blinc_platform_desktop",
    "blinc_platform_android", "blinc_platform_ios", "blinc_platform_web",
    "blinc_app", "blinc", "blinc_cli",
)
ROOT = Path(__file__).resolve().parent.parent


def release_plan(metadata, selected=RELEASE_CRATES):
    members = set(metadata["workspace_members"])
    packages = {p["name"]: p for p in metadata["packages"] if p["id"] in members}
    selected = tuple(selected)
    if len(set(selected)) != len(selected):
        raise ValueError("Duplicate crate in release selection")
    dependencies = {}
    for name in selected:
        if name not in packages:
            raise ValueError(f"{name} is not a workspace package")
        package = packages[name]
        registries = package.get("publish")
        if registries is not None and "crates-io" not in registries:
            raise ValueError(f"{name} cannot be published to crates.io")
        dependencies[name] = []
        for dep in package["dependencies"]:
            if dep["kind"] == "dev":
                continue
            if dep.get("path") is not None:
                if dep["name"] not in selected or dep["name"] not in packages:
                    raise ValueError(f"{name} requires unselected local crate {dep['name']}")
                if dep["req"] == "*":
                    raise ValueError(f"{name} needs a release version for {dep['name']}")
                version = packages[dep["name"]]["version"]
                if dep["req"] not in {version, f"^{version}", f"={version}", f"~{version}"}:
                    raise ValueError(f"{name} must pin {dep['name']} to release {version}")
                dependencies[name].append(dep["name"])
            elif (dep.get("source") or "").startswith("git+") and dep["req"] == "*":
                raise ValueError(f"{name} has an unversioned git dependency {dep['name']}")

    ordered = []
    visiting = set()
    visited = set()

    def visit(name):
        if name in visiting:
            raise ValueError(f"Release dependency cycle at {name}")
        if name in visited:
            return
        visiting.add(name)
        for dependency in dependencies[name]:
            visit(dependency)
        visiting.remove(name)
        visited.add(name)
        ordered.append(packages[name])

    for name in selected:
        visit(name)
    return ordered


def execute_plan(plan, dry_run, wait):
    for index, package in enumerate(plan):
        name = package["name"]
        print(f"{'Verifying' if dry_run else 'Publishing'} {name}@{package['version']}", flush=True)
        command = ["cargo", "publish", "--locked", "--package", name]
        if dry_run:
            command.append("--dry-run")
        result = subprocess.run(command, cwd=ROOT, check=False)
        if result.returncode:
            raise RuntimeError(f"Cargo failed for {name} (exit {result.returncode}); release stopped")
        if not dry_run and index + 1 < len(plan) and wait:
            time.sleep(wait)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--plan", action="store_true", help="print validated crate order (default)")
    mode.add_argument("--dry-run", action="store_true", help="verify packages without upload")
    mode.add_argument("--publish", action="store_true", help="upload with Cargo verification enabled")
    parser.add_argument("--start-at", help="resume at this crate after earlier versions are available")
    parser.add_argument("--wait", type=float, default=20, help="seconds between successful uploads")
    args = parser.parse_args(argv)
    if not math.isfinite(args.wait) or args.wait < 0:
        parser.error("--wait must be finite and nonnegative")
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"],
            cwd=ROOT, check=True, capture_output=True, text=True,
        )
        plan = release_plan(json.loads(result.stdout))
        if args.start_at:
            names = [p["name"] for p in plan]
            if args.start_at not in names:
                raise ValueError(f"Unknown release crate {args.start_at}")
            plan = plan[names.index(args.start_at):]
        for index, package in enumerate(plan, 1):
            print(f"{index:2}. {package['name']}@{package['version']}")
        if args.publish or args.dry_run:
            execute_plan(plan, args.dry_run, args.wait)
            print("Package verification complete." if args.dry_run else "Release uploads complete.")
        else:
            print("Plan only. Use --dry-run to verify packages or --publish to upload.")
        return 0
    except (ValueError, RuntimeError, subprocess.CalledProcessError, OSError) as error:
        print(f"Release failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
