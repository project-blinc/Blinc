#!/usr/bin/env python3
"""Run the example app on an isolated Android emulator: launcher resume, config changes, touch scroll."""

import argparse
import fcntl
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from test_android_ime import read_png  # noqa: E402

OUT = ROOT / "target/android-resume"
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
JAVA = Path(os.environ.get("BLINC_TEST_JAVA_HOME", "/Applications/Android Studio.app/Contents/jbr/Contents/Home"))
PACKAGE = "com.blinc.example"
SERIAL = "emulator-5584"
WIDTH, HEIGHT = 360, 640


THUMB = (39, 39, 48)


def scrolled(rows):
    """Pixels the page is scrolled, read from the scroll indicator on the right edge.

    The indicator's length is the viewport's share of the page, so one pixel of its travel is
    page height over indicator length pixels of content; it is absent until the page has moved."""
    ys = [y for y, row in enumerate(rows) if row[-5] == THUMB]
    return 0 if not ys else ys[0] * len(rows) / len(ys)


def lit_fraction(rows):
    total = sum(len(row) for row in rows)
    return sum(1 for row in rows for r, g, b in row if r + g + b > 60) / total


class Device:
    def __init__(self, env):
        self.env = env
        self.adb = str(SDK / "platform-tools/adb")

    def run(self, *args, timeout=60):
        result = subprocess.run([self.adb, "-s", SERIAL, *args], env=self.env, capture_output=True, text=True, timeout=timeout)
        if result.returncode:
            raise RuntimeError(f"adb {' '.join(args)} failed: {result.stderr.strip()}")
        return result.stdout

    def shell(self, *args, timeout=60):
        return self.run("shell", *args, timeout=timeout)

    def pid(self):
        # pidof exits 1 while the app is not running.
        result = subprocess.run([self.adb, "-s", SERIAL, "shell", "pidof", PACKAGE], env=self.env, capture_output=True, text=True, timeout=60)
        return result.stdout.strip()

    def launch(self):
        # The intent and flags the home screen sends when its icon is tapped.
        result = subprocess.run([self.adb, "-s", SERIAL, "shell", "am", "start", "-a", "android.intent.action.MAIN",
                                 "-c", "android.intent.category.LAUNCHER", "-f", "0x10200000", "-n", f"{PACKAGE}/.MainActivity"],
                                env=self.env, capture_output=True, text=True, timeout=60)
        with (OUT / "launch.log").open("a") as log:
            log.write(result.stdout + result.stderr + f"exit {result.returncode}\n")

    def screen(self, name):
        path = OUT / f"{name}.png"
        result = subprocess.run([self.adb, "-s", SERIAL, "exec-out", "screencap", "-p"], env=self.env, capture_output=True, timeout=60)
        path.write_bytes(result.stdout)
        return read_png(path)

    def wait_pid(self, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if self.pid():
                return self.pid()
            time.sleep(0.5)
        raise RuntimeError("The app process did not start")


def check(results, name, ok, detail):
    results.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + ": " + detail, flush=True)


def prepare():
    project = OUT / "project"
    shutil.rmtree(project, ignore_errors=True)
    shutil.copytree(ROOT / "mobile/example/platforms/android", project,
                    ignore=shutil.ignore_patterns("build", ".gradle", ".DS_Store", "local.properties"))
    (project / "local.properties").write_text(f"sdk.dir={SDK}\n")
    libraries = project / "app/src/main/jniLibs/arm64-v8a"
    libraries.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(OUT / "rust/aarch64-linux-android/release/libexample.so", libraries / "libexample.so")
    return project


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--cycles", type=int, default=5, help="launcher resume cycles")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    lock = (OUT / ".runner.lock").open("a")
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    if not (OUT / "rust/aarch64-linux-android/release/libexample.so").exists():
        raise RuntimeError(f"Build the example first; see {OUT / 'rust-build.log'}")
    env = dict(os.environ, JAVA_HOME=str(JAVA), ANDROID_HOME=str(SDK), ANDROID_AVD_HOME=str(OUT / "avd"))
    project = prepare()

    def run(name, command, timeout=300, **kwargs):
        print(name, flush=True)
        with (OUT / f"{name}.log").open("w") as log:
            result = subprocess.run(command, env=env, cwd=project, stdout=log, stderr=subprocess.STDOUT, text=True, timeout=timeout, **kwargs)
        if result.returncode:
            raise RuntimeError(f"{name} failed; see {OUT / (name + '.log')}")

    run("build", [str(project / "gradlew"), ":app:assembleDebug", "--no-daemon", "--max-workers=2"], timeout=900)
    if args.build_only:
        return 0
    avd = OUT / "avd"
    avd.mkdir(exist_ok=True)
    if not (avd / "BlincResumeQA.ini").exists():
        run("create-device", [str(SDK / "cmdline-tools/latest/bin/avdmanager"), "create", "avd", "-n", "BlincResumeQA",
            "-k", "system-images;android-36;google_apis_playstore;arm64-v8a", "--device", "pixel_5"], input="no\n")
    device = Device(env)
    if SERIAL in subprocess.run([device.adb, "devices"], env=env, capture_output=True, text=True).stdout:
        raise RuntimeError(f"{SERIAL} is already in use; leaving that device alone")
    results = []
    with (OUT / "emulator.log").open("w") as log:
        process = subprocess.Popen([str(SDK / "emulator/emulator"), "-avd", "BlincResumeQA", "-port", "5584",
            "-no-window", "-no-audio", "-gpu", "host", "-no-boot-anim", "-no-snapshot", "-cores", "2", "-memory", "2048"],
            env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            subprocess.run([device.adb, "-s", SERIAL, "wait-for-device"], env=env, timeout=180)
            deadline = time.monotonic() + 240
            while True:
                boot = subprocess.run([device.adb, "-s", SERIAL, "shell", "getprop", "sys.boot_completed"],
                                      env=env, capture_output=True, text=True)
                if boot.returncode == 0 and boot.stdout.strip() == "1":
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Dedicated emulator did not finish booting")
                time.sleep(1)
            # A cold boot keeps installing and updating packages for a while; an app launched into
            # that churn is killed by the input-dispatch timeout for reasons that are the system's.
            time.sleep(60)
            # A headless emulator starts with its display off, which starves a surface that presents.
            device.shell("svc", "power", "stayon", "true")
            device.shell("input", "keyevent", "KEYCODE_WAKEUP")
            device.shell("wm", "dismiss-keyguard")
            # A small screen keeps the captures cheap to decode.
            device.shell("wm", "size", f"{WIDTH}x{HEIGHT}")
            device.shell("wm", "density", "160")
            apk = next((project / "app/build/outputs/apk").rglob("app-debug.apk"))
            device.run("install", "-r", str(apk), timeout=180)
            device.run("logcat", "-c")
            (OUT / "power.log").write_text(device.shell("dumpsys", "power"))
            device.launch()
            first = device.wait_pid()
            time.sleep(10)
            check(results, "first launch renders", lit_fraction(device.screen("launch")) > 0.05, "screen is not black")

            same = True
            for cycle in range(args.cycles):
                device.shell("input", "keyevent", "KEYCODE_HOME")
                time.sleep(2)
                device.launch()
                time.sleep(4)
                if device.pid() != first:
                    same = False
                    break
            check(results, "launcher resume keeps the process", same, f"pid {first} across {args.cycles} resumes")
            check(results, "resumed app still renders", lit_fraction(device.screen("resumed")) > 0.05, "screen is not black")

            device.shell("settings", "put", "system", "font_scale", "1.3")
            device.shell("cmd", "uimode", "night", "yes")
            time.sleep(4)
            check(results, "font and theme change keep the process", device.pid() == first, f"pid {first}")
            device.shell("settings", "put", "system", "font_scale", "1.0")
            device.shell("cmd", "uimode", "night", "no")
            time.sleep(3)

            log_text = device.run("logcat", "-d", "-v", "threadtime", timeout=120)
            (OUT / "device-log.log").write_text(log_text)
            crashed = "Abort message" in log_text or ("FATAL EXCEPTION" in log_text and PACKAGE in log_text)
            check(results, "no crash in the device log", not crashed, "no abort or fatal exception")

            # Touches start in the left margin, which no control covers, so none of them takes focus.
            x = 12
            device.screen("drag-before")
            device.shell("input", "swipe", str(x), "520", str(x), "320", "2000")
            time.sleep(2.5)
            dragged = scrolled(device.screen("drag-after"))
            check(results, "a slow drag moves the content with the finger", 180 <= dragged <= 280,
                  f"content moved {dragged:.0f}px for a 200px drag")

            # A quick flick must carry on after the finger lifts, every time, in both directions.
            moves = []
            position = dragged
            for flick in range(4):
                start, end = (560, 310) if flick % 2 == 0 else (310, 560)
                device.shell("input", "swipe", str(x), str(start), str(x), str(end), "100")
                time.sleep(3)
                now = scrolled(device.screen(f"flick{flick}"))
                moves.append(round(abs(now - position)))
                position = now
            check(results, "every flick keeps coasting after release", all(move > 250 * 1.3 for move in moves),
                  f"content moved {moves}px for 250px flicks")
        finally:
            if not all(ok for _, ok, _ in results):
                # The bug report carries the ANR thread dumps, which name where the app was stuck.
                subprocess.run([device.adb, "-s", SERIAL, "bugreport", str(OUT / "bugreport.zip")],
                               env=env, capture_output=True, timeout=600)
            subprocess.run([device.adb, "-s", SERIAL, "emu", "kill"], env=env, capture_output=True, timeout=30)
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=30)
    return 0 if all(ok for _, ok, _ in results) else 1


if __name__ == "__main__":
    sys.exit(main())
