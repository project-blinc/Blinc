#!/usr/bin/env python3
"""Run bench APKs on an Android phone, one after another, and print fps, CPU and memory for each.

    ANDROID_SERIAL=<serial> android_ab.py SCENE CARDS APK[:LOG] [APK[:LOG] ...]

Each APK is installed over the previous one, launched with the scene set through the `debug.*`
system properties the bench library reads, and measured for 20 s after a 10 s warm-up. Name an APK
twice to repeat it; alternate two builds to compare them. LOG `all` keeps the runner's own
unfiltered log. Nothing is captured from the screen:

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


def adb(*args):
    r = subprocess.run([ADB, "-s", SERIAL, *args], capture_output=True, text=True, timeout=120)
    return r.stdout.strip()


def cpu_ticks(pid):
    f = adb("shell", f"cat /proc/{pid}/stat").split()
    return int(f[13]) + int(f[14])


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
    adb("install", "-r", apk)
    for key, value in (("", scene), ("_cards", cards), ("_log", log)):
        adb("shell", f"setprop debug.blinc_bench{key} '{value}'")
    adb("logcat", "-c")
    pid, started = launch()
    if not pid:
        return f"{apk}: no process; am start: {started[-200:]!r}"
    time.sleep(WARM)
    t0, c0 = time.time(), cpu_ticks(pid)
    time.sleep(WINDOW / 2)
    rate = fps()
    time.sleep(WINDOW / 2 - 1)
    t1, c1 = time.time(), cpu_ticks(pid)
    mem = adb("shell", f"dumpsys meminfo {pid}")
    pss = re.search(r"TOTAL PSS:\s+(\d+)", mem)
    gfx = re.search(r"Graphics:\s+(\d+)", mem)
    alive = bool(adb("shell", f"pidof {PKG}"))
    name = os.path.basename(apk)
    return (
        f"{name:12} log={log or 'warn':4} {scene} x{cards}: fps {rate:5.1f}  "
        f"cpu {(c1 - c0) / 100.0 / (t1 - t0):4.2f} cores  "
        f"pss {int(pss.group(1)) // 1024 if pss else -1} MB  "
        f"gfx {int(gfx.group(1)) // 1024 if gfx else -1} MB  alive {alive}"
    )


if __name__ == "__main__":
    scene, cards, *variants = sys.argv[1:]
    try:
        for v in variants:
            apk, _, log = v.partition(":")
            print(run(apk, scene, cards, log), flush=True)
    finally:
        adb("shell", f"am force-stop {PKG}")
        for key in ("", "_cards", "_log"):
            adb("shell", f"setprop debug.blinc_bench{key} ''")
