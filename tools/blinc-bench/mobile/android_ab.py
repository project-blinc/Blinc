#!/usr/bin/env python3
"""Run bench APKs on an Android phone, one after another, and print fps, CPU and memory for each.

    ANDROID_SERIAL=<serial> android_ab.py SCENE CARDS APK[:LOG] [APK[:LOG] ...]

Each APK is installed over the previous one, launched with the scene set through the `debug.*`
system properties the bench library reads, and measured for 20 s after a 10 s warm-up. Name an APK
twice to repeat it; alternate two builds to compare them. LOG `all` keeps the runner's own
unfiltered log. Nothing is captured from the screen.

An install that times out or fails, or an app whose pid is replaced during the window, is retried
up to ATTEMPTS times. A variant that still fails is reported and the rest run, and so is one whose
app dies during the run, with the crash report's abort message; the exit status is 1 if any did.
A death is not retried, since it may be the build under test. What is measured:

- fps: present times of the app's layer in `dumpsys SurfaceFlinger --latency`.
- CPU: utime + stime of the process from /proc/<pid>/stat, in cores.
- memory: TOTAL PSS and Graphics from `dumpsys meminfo`.
"""

import os
import re
import subprocess
import sys
import time

SERIAL = os.environ.get("ANDROID_SERIAL") or sys.exit("set ANDROID_SERIAL")
ADB = os.path.expanduser("~/Library/Android/sdk/platform-tools/adb")
PKG = "com.blinc.example"
WARM, WINDOW = 10, 20
ATTEMPTS = 3


class Flake(Exception):
    """A step that fails now and then for a reason outside the build under test."""


class Gone(Exception):
    """The measured process has no /proc entry."""


class Died(Exception):
    """The app crashed or was killed during the run; the message is the result line."""


def adb(*args, errors=False, timeout=120):
    try:
        r = subprocess.run([ADB, "-s", SERIAL, *args], capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise Flake(f"adb {args[0]} timed out after {timeout} s")
    return (r.stdout + r.stderr if errors else r.stdout).strip()


def cpu_ticks(pid):
    # utime and stime are fields 14 and 15; the fields after the parenthesised name start at 3.
    f = adb("shell", f"cat /proc/{pid}/stat").rpartition(")")[2].split()
    if len(f) < 13:
        raise Gone(pid)
    return int(f[11]) + int(f[12])


def crash_reason():
    out = adb("logcat", "-b", "crash", "-d", "-v", "brief")
    text = "\n".join(re.sub(r"^\w/\S+\s*\(\s*\d+\): ?", "", l) for l in out.splitlines())
    m = re.search(r"Abort message: '(.*?)'\n", text, re.S)
    if m:
        return " ".join(m.group(1).split())
    m = re.search(r"Fatal signal \d+ \(\w+\)", text)
    return m.group(0) if m else "no crash report"


def fps():
    layers = adb("shell", "dumpsys SurfaceFlinger --list")
    m = re.search(r"VRI-" + re.escape(PKG) + r"[^ ]*#\d+", layers)
    if not m:
        return 0.0
    out = adb("shell", f"dumpsys SurfaceFlinger --latency '{m.group(0)}'").splitlines()
    times = []
    for line in out[1:]:
        p = line.split()
        if len(p) == 3 and p[1] not in ("0", "9223372036854775807"):
            times.append(int(p[1]))
    if len(times) < 3:
        return 0.0
    span = (times[-1] - times[0]) / 1e9
    return (len(times) - 1) / span if span > 0 else 0.0


def launch():
    # Right after `install -r` the first launch can start late or not at all.
    started = adb("shell", f"am start -n {PKG}/.MainActivity")
    for i in range(30):
        pid = adb("shell", f"pidof {PKG}")
        if pid:
            return pid, started
        if i == 8:
            started = adb("shell", f"am start -n {PKG}/.MainActivity")
        time.sleep(1)
    return "", started


def run(apk, scene, cards, log):
    adb("shell", f"am force-stop {PKG}")
    installed = adb("install", "-r", apk, errors=True, timeout=60)
    if "Success" not in installed:
        raise Flake(f"install: {installed[-200:]!r}")
    for key, value in (("", scene), ("_cards", cards), ("_log", log)):
        adb("shell", f"setprop debug.blinc_bench{key} '{value}'")
    adb("logcat", "-c")
    pid, started = launch()
    if not pid:
        return f"{apk}: no process; am start: {started[-200:]!r}"
    name = os.path.basename(apk)
    try:
        time.sleep(WARM)
        t0, c0 = time.time(), cpu_ticks(pid)
        time.sleep(WINDOW / 2)
        rate = fps()
        time.sleep(WINDOW / 2 - 1)
        t1, c1 = time.time(), cpu_ticks(pid)
    except Gone:
        now = adb("shell", f"pidof {PKG}")
        if now:
            raise Flake(f"process {pid} was replaced by {now}")
        raise Died(
            f"{name:12} log={log or 'warn':4} {scene} x{cards}: process {pid} died during the run: "
            + crash_reason()
        )
    mem = adb("shell", f"dumpsys meminfo {pid}")
    pss = re.search(r"TOTAL PSS:\s+(\d+)", mem)
    gfx = re.search(r"Graphics:\s+(\d+)", mem)
    alive = bool(adb("shell", f"pidof {PKG}"))
    return (
        f"{name:12} log={log or 'warn':4} {scene} x{cards}: fps {rate:5.1f}  "
        f"cpu {(c1 - c0) / 100.0 / (t1 - t0):4.2f} cores  "
        f"pss {int(pss.group(1)) // 1024 if pss else -1} MB  "
        f"gfx {int(gfx.group(1)) // 1024 if gfx else -1} MB  alive {alive}"
    )


def measure(apk, scene, cards, log):
    for attempt in range(1, ATTEMPTS + 1):
        try:
            return run(apk, scene, cards, log), True
        except Flake as e:
            print(f"{os.path.basename(apk)}: attempt {attempt} of {ATTEMPTS}: {e}", file=sys.stderr, flush=True)
        except Died as e:
            return str(e), False
    return None, False


def reset():
    try:
        adb("shell", f"am force-stop {PKG}")
        for key in ("", "_cards", "_log"):
            adb("shell", f"setprop debug.blinc_bench{key} ''")
    except Flake as e:
        print(f"reset: {e}; the debug.blinc_bench* properties may still be set", file=sys.stderr)


if __name__ == "__main__":
    scene, cards, *variants = sys.argv[1:]
    failed = 0
    try:
        for v in variants:
            apk, _, log = v.partition(":")
            line, ok = measure(apk, scene, cards, log)
            if line is None:
                line = f"{os.path.basename(apk):12} log={log or 'warn':4} {scene} x{cards}: FAILED"
            failed += not ok
            print(line, flush=True)
    finally:
        reset()
    sys.exit(1 if failed else 0)
