#!/usr/bin/env python3
"""Exercise the native editing connection in one isolated Android emulator."""

import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target/android-ime"
SOURCE = ROOT / "mobile/tests/ime/android"
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
JAVA = Path(os.environ.get("BLINC_TEST_JAVA_HOME", "/Applications/Android Studio.app/Contents/jbr/Contents/Home"))


def prepare(native=False):
    project = OUT / "project"
    project.mkdir(parents=True, exist_ok=True)
    example = ROOT / "mobile/example/platforms/android"
    for name in ["gradlew", "gradlew.bat"]:
        shutil.copy2(example / name, project / name)
    shutil.copytree(example / "gradle", project / "gradle", dirs_exist_ok=True)
    (project / "settings.gradle.kts").write_text('''pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositories { google(); mavenCentral() } }
rootProject.name = "BlincImeTests"
include(":app")
''')
    (project / "build.gradle.kts").write_text('''plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "1.9.25" apply false
}
''')
    (project / "gradle.properties").write_text("org.gradle.jvmargs=-Xmx1536m\nkotlin.compiler.execution.strategy=in-process\nandroid.useAndroidX=true\n")
    (project / "local.properties").write_text(f"sdk.dir={SDK}\n")
    app = project / "app"
    main = app / "src/main"
    kotlin = main / "kotlin/com/blinc"
    tests = app / "src/androidTest/kotlin/com/blinc"
    kotlin.mkdir(parents=True, exist_ok=True)
    tests.mkdir(parents=True, exist_ok=True)
    if native:
        shutil.copyfile(SOURCE / "NativeImeTests.kt", tests / "NativeImeTests.kt")
        host = ROOT / "mobile/example/platforms/android/app/src/main/kotlin/com/blinc"
        shutil.copyfile(host / "MainActivity.kt", kotlin / "MainActivity.kt")
        shutil.copyfile(SOURCE / "ImeTestService.kt", kotlin / "ImeTestService.kt")
        shutil.copyfile(ROOT / "extensions/blinc_platform_android/templates/BlincNativeBridge.kt", kotlin / "BlincNativeBridge.kt")
    else:
        shutil.copyfile(SOURCE / "ImeActivity.kt", kotlin / "ImeActivity.kt")
        shutil.copyfile(SOURCE / "ImeTests.kt", tests / "ImeTests.kt")
    shutil.copyfile(ROOT / "extensions/blinc_platform_android/templates/BlincIme.kt", kotlin / "BlincIme.kt")
    gradle = '''plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
android {
    namespace = "com.blinc.imefixture"
    useLibrary("android.test.runner")
    useLibrary("android.test.base")
    compileSdk = 35
    defaultConfig {
        applicationId = "com.blinc.imefixture"
        minSdk = 24
        targetSdk = 35
        testInstrumentationRunner = "android.test.InstrumentationTestRunner"
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_1_8; targetCompatibility = JavaVersion.VERSION_1_8 }
    kotlinOptions { jvmTarget = "1.8" }
}
dependencies { implementation("androidx.core:core-ktx:1.12.0") }
'''
    (app / "build.gradle.kts").write_text(gradle.replace("com.blinc.imefixture", "com.blinc.nativeimefixture") if native else gradle)
    activity = ('<activity android:name="com.blinc.example.MainActivity" android:exported="true">'
                '<meta-data android:name="android.app.lib_name" android:value="example" /></activity>') if native else (
                '<activity android:name="com.blinc.ImeActivity" android:exported="true" />')
    if native:
        xml = main / "res/xml"
        xml.mkdir(parents=True, exist_ok=True)
        (xml / "test_ime.xml").write_text('<input-method xmlns:android="http://schemas.android.com/apk/res/android" android:supportsSwitchingToNextInputMethod="false" />')
        activity += ('<service android:name="com.blinc.ImeTestService" android:permission="android.permission.BIND_INPUT_METHOD" android:exported="true">'
                     '<intent-filter><action android:name="android.view.InputMethod" /></intent-filter>'
                     '<meta-data android:name="android.view.im" android:resource="@xml/test_ime" /></service>')
    (main / "AndroidManifest.xml").write_text('''<manifest xmlns:android="http://schemas.android.com/apk/res/android">
<application android:theme="@android:style/Theme.Material.NoActionBar" android:label="Blinc IME tests">
<uses-library android:name="android.test.runner" />
<uses-library android:name="android.test.base" />
''' + activity + '</application></manifest>\n')
    return project


def main():
    global OUT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--native", action="store_true", help="Test the full JNI/Rust/GPU host as well as the native editor")
    args = parser.parse_args()
    if args.native:
        OUT = ROOT / "target/android-ime-native"
    OUT.mkdir(parents=True, exist_ok=True)
    lock = (OUT / ".runner.lock").open("a")
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    project = prepare(args.native)
    env = dict(os.environ, JAVA_HOME=str(JAVA), ANDROID_HOME=str(SDK), ANDROID_AVD_HOME=str(OUT / "avd"))
    ndk = SDK / "ndk/27.0.12077973"
    # The emulator's Vulkan driver crashes on debug object names; use production GPU flags.
    env.update(CARGO_BUILD_JOBS="2", CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_DEV_OPT_LEVEL="0", CARGO_PROFILE_DEV_CODEGEN_UNITS="1",
               CARGO_PROFILE_DEV_DEBUG_ASSERTIONS="false",
               CARGO_TARGET_DIR=str(OUT / "rust"), ANDROID_NDK_HOME=str(ndk),
               PATH=str(ndk / "toolchains/llvm/prebuilt/darwin-x86_64/bin") + os.pathsep + env["PATH"])

    def run(name, command, timeout=300, **kwargs):
        print(name, flush=True)
        path = OUT / f"{name}.log"
        with path.open("w") as log:
            result = subprocess.run(command, env=env, cwd=project, stdout=log, stderr=subprocess.STDOUT, text=True, timeout=timeout, **kwargs)
        if result.returncode:
            raise RuntimeError(f"{name} failed; see {OUT / (name + '.log')}")
        return path.read_text()

    if args.native:
        fixture = ROOT / "mobile/tests/accessibility"
        shutil.copyfile(ROOT / "Cargo.lock", fixture / "Cargo.lock")
        run("rust-build", ["cargo", "build", "--offline", "--manifest-path", str(fixture / "Cargo.toml"), "--target", "aarch64-linux-android"], timeout=900)
        libraries = project / "app/src/main/jniLibs/arm64-v8a"
        libraries.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(OUT / "rust/aarch64-linux-android/debug/libexample.so", libraries / "libexample.so")
    run("build", [str(project / "gradlew"), ":app:assembleDebug", ":app:assembleDebugAndroidTest", "--no-daemon", "--max-workers=2"], timeout=600)
    if args.build_only:
        return
    avd = OUT / "avd"
    avd.mkdir(exist_ok=True)
    if not (avd / "BlincIMEQA.ini").exists():
        run("create-device", [str(SDK / "cmdline-tools/latest/bin/avdmanager"), "create", "avd", "-n", "BlincIMEQA",
            "-k", "system-images;android-36;google_apis_playstore;arm64-v8a", "--device", "pixel_5"], input="no\n")
    adb = str(SDK / "platform-tools/adb")
    serial = "emulator-5582"
    devices = run("devices", [adb, "devices"])
    if serial in devices:
        raise RuntimeError(f"{serial} is already in use; leaving that device alone")
    with (OUT / "emulator.log").open("w") as log:
        previous_ime = None
        process = subprocess.Popen([str(SDK / "emulator/emulator"), "-avd", "BlincIMEQA", "-port", "5582",
            "-no-window", "-no-audio", "-no-boot-anim", "-no-snapshot", "-cores", "2", "-memory", "2048"],
            env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            run("wait-device", [adb, "-s", serial, "wait-for-device"], timeout=180)
            deadline = time.monotonic() + 180
            while True:
                boot = subprocess.run([adb, "-s", serial, "shell", "getprop", "sys.boot_completed"], env=env, capture_output=True, text=True)
                if boot.returncode == 0 and boot.stdout.strip() == "1":
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Dedicated emulator did not finish booting")
                time.sleep(1)
            if args.native:
                fonts = run("system-fonts", [adb, "-s", serial, "shell", "ls", "/system/fonts"])
                if "NotoColorEmoji.ttf" in fonts:
                    run("emoji-font", [adb, "-s", serial, "pull", "/system/fonts/NotoColorEmoji.ttf", str(OUT / "NotoColorEmoji.ttf")])
            for name in ["app-debug.apk", "app-debug-androidTest.apk"]:
                path = next((project / "app/build/outputs/apk").rglob(name))
                run("install-" + name, [adb, "-s", serial, "install", "-r", str(path)])
            package = "com.blinc.nativeimefixture" if args.native else "com.blinc.imefixture"
            if args.native:
                previous_ime = run("previous-ime", [adb, "-s", serial, "shell", "settings", "get", "secure", "default_input_method"]).strip()
                component = package + "/com.blinc.ImeTestService"
                run("enable-test-ime", [adb, "-s", serial, "shell", "ime", "enable", component])
                run("select-test-ime", [adb, "-s", serial, "shell", "ime", "set", component])
                run("show-test-ime", [adb, "-s", serial, "shell", "settings", "put", "secure", "show_ime_with_hard_keyboard", "1"])
            result = run("input-connection-tests", [adb, "-s", serial, "shell", "am", "instrument", "-w",
                package + ".test/android.test.InstrumentationTestRunner"])
            run("device-log", [adb, "-s", serial, "logcat", "-d", "-v", "threadtime"])
            expected = "OK (1 test)" if args.native else "OK (3 tests)"
            if expected not in result or "FAILURES" in result:
                raise RuntimeError(f"Input connection tests failed; see {OUT / 'input-connection-tests.log'}")
            if args.native:
                run("rendered-edit", [adb, "-s", serial, "pull", f"/sdcard/Android/data/{package}/files/ime-native.png", str(OUT / "ime-native.png")])
                run("rendered-bounds", [adb, "-s", serial, "pull", f"/sdcard/Android/data/{package}/files/ime-native-bounds.json", str(OUT / "ime-native-bounds.json")])
                from PIL import Image
                bounds = json.loads((OUT / "ime-native-bounds.json").read_text())
                with Image.open(OUT / "ime-native.png") as screenshot:
                    pixels = screenshot.convert("RGB").crop(bounds)
                    pixels.save(OUT / "ime-input.png")
                    orange = sum(1 for r, g, b in pixels.get_flattened_data() if r > 150 and 60 < g < 200 and b < 100)
                (OUT / "emoji-pixels.json").write_text(json.dumps({"bounds": bounds, "orange_pixels": orange}) + "\n")
                if orange < 20:
                    raise RuntimeError("The typed fox has no visible colour pixels in the GPU input")
        finally:
            if previous_ime and previous_ime != "null":
                subprocess.run([adb, "-s", serial, "shell", "ime", "set", previous_ime], env=env, capture_output=True, timeout=30)
            subprocess.run([adb, "-s", serial, "emu", "kill"], env=env, capture_output=True, timeout=30)
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=30)


if __name__ == "__main__":
    main()
