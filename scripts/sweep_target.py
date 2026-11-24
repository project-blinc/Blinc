#!/usr/bin/env python3
"""Delete build output that the given Cargo commands no longer use.

Cargo keeps a unit's old build folder when its features, profile settings, crate type or
toolchain change, so target/ grows with every such change. This script runs each command
with JSON output and keeps the `target/<profile>/build/<pkg>/<hash>` folders the command
reports, also for units that are already up to date. In the profiles those commands use, it
lists every other hash folder, and deletes them with --delete. Other profiles stay as they
are. After a sweep, the same commands compile nothing.

    scripts/sweep_target.py                       # dry run of `cargo test --workspace --no-run`
    scripts/sweep_target.py --delete -- cargo test --workspace --no-run \\
        ';;' cargo build --profile release-debug -p blinc_app_examples --example cn_demo --features cn

Commands are separated by ';;'. A command that fails stops the sweep before anything is
deleted.
"""

import json
import os
import shutil
import subprocess
import sys

DEFAULT = [['cargo', 'test', '--workspace', '--no-run']]


def parse(args):
    delete = args[:1] == ['--delete']
    if delete:
        args = args[1:]
    if not args:
        return delete, DEFAULT
    if args[0] != '--':
        sys.exit(__doc__)
    commands, cur = [], []
    for a in args[1:]:
        if a == ';;':
            commands.append(cur)
            cur = []
        else:
            cur.append(a)
    if cur:
        commands.append(cur)
    return delete, commands


def unit_dir(path):
    """The `.../build/<pkg>/<hash>` folder that holds `path`, if any."""
    parts = path.split(os.sep)
    if 'build' not in parts:
        return None
    i = len(parts) - 1 - parts[::-1].index('build')
    if i + 2 >= len(parts):
        return None
    return os.sep.join(parts[: i + 3])


def used(commands):
    """The unit folders the commands report, and the (name, size) of each binary copied out
    of its unit folder (into examples/ or the profile folder)."""
    keep, copied = set(), set()
    for cmd in commands:
        out = subprocess.run(cmd + ['--message-format=json'], capture_output=True, text=True)
        if out.returncode != 0:
            sys.exit(f'failed: {" ".join(cmd)}\n{out.stderr[-2000:]}')
        for line in out.stdout.splitlines():
            try:
                m = json.loads(line)
            except ValueError:
                continue
            paths = m.get('filenames', []) + [m.get('executable') or '', m.get('out_dir') or '']
            for path in filter(None, paths):
                d = unit_dir(path)
                if d:
                    keep.add(d)
                elif os.path.exists(path):
                    # The copy can be a clone with its own inode, so match it by name and size.
                    copied.add((os.path.basename(path), os.stat(path).st_size))
    return keep, copied


def holds_copied(folder, copied):
    for root, _, files in os.walk(folder):
        for f in files:
            if (f, os.stat(os.path.join(root, f)).st_size) in copied:
                return True
    return False


def has_files(folder):
    return any(files for _, _, files in os.walk(folder))


def size_kb(folder):
    out = subprocess.run(['du', '-sk', folder], capture_output=True, text=True).stdout
    return int(out.split()[0])


def main():
    delete, commands = parse(sys.argv[1:])
    keep, copied = used(commands)
    profiles = sorted({os.sep.join(d.split(os.sep)[:-3]) for d in keep})
    stale = []
    for profile in profiles:
        build = os.path.join(profile, 'build')
        for pkg in sorted(os.listdir(build)):
            for h in sorted(os.listdir(os.path.join(build, pkg))):
                d = os.path.join(build, pkg, h)
                # Cargo leaves empty `out` folders for units it does not report.
                if d not in keep and has_files(d) and not holds_copied(d, copied):
                    stale.append((size_kb(d), d))
    total = sum(n for n, _ in stale)
    print(f'kept {len(keep)} unit folders in {", ".join(profiles)}')
    print(f'stale: {len(stale)} folders, {total / 1e6:.2f} GB')
    for n, d in sorted(stale, reverse=True)[:10]:
        print(f'  {n / 1e3:8.0f} MB  {d}')
    if delete:
        for _, d in stale:
            shutil.rmtree(d)
        print('deleted')
    elif stale:
        print('dry run: pass --delete to remove them')


if __name__ == '__main__':
    main()
