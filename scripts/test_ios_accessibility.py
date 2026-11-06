#!/usr/bin/env python3
"""Build a current-source Blinc fixture and audit it in one headless iOS Simulator.

Uses the existing UIKit host, a separate bundle ID and generated Xcode project.
Memory use is recorded for diagnostics without imposing an execution limit.
The dedicated simulator is shut down after the run, including failed tests.
"""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "mobile/tests/accessibility"
OUT = ROOT / "target/ios-accessibility"
BUNDLE = "com.blinc.accessibility.fixture"
SCHEME = "BlincAccessibility"
DEVICE_NAME = "Blinc Accessibility QA"


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def memory():
    pressure = int(output("sysctl", "-n", "kern.memorystatus_vm_pressure_level"))
    swap = output("sysctl", "-n", "vm.swapusage")
    free = output("memory_pressure", "-Q")
    return {
        "pressure": pressure,
        "swap_used_mb": float(re.search(r"used = ([\d.]+)M", swap)[1]),
        "free_percent": int(re.search(r"free percentage: (\d+)%", free)[1]),
    }


def encode(value):
    if isinstance(value, dict):
        return "{\n" + "\n".join(f"{json.dumps(k)} = {encode(v)};" for k, v in value.items()) + "\n}"
    if isinstance(value, list):
        return "(" + ",\n".join(encode(v) for v in value) + ")"
    return json.dumps(value)


def prepare():
    shutil.copyfile(ROOT / "Cargo.lock", FIXTURE / "Cargo.lock")
    project = OUT / "project"
    shutil.copytree(
        ROOT / "mobile/example/platforms/ios", project,
        ignore=shutil.ignore_patterns("libs", "xcuserdata", "build", ".DS_Store"),
        dirs_exist_ok=True,
    )
    pbx = project / "BlincApp.xcodeproj/project.pbxproj"
    data = json.loads(output("plutil", "-convert", "json", "-o", "-", str(pbx)))
    objects = data["objects"]
    main_id = next(k for k, v in objects.items() if v["isa"] == "PBXNativeTarget")
    main = objects[main_id]
    root = objects[data["rootObject"]]
    main["productName"] = "BlincApp"
    for item in objects.values():
        if item["isa"] == "XCBuildConfiguration":
            settings = item["buildSettings"]
            settings["IPHONEOS_DEPLOYMENT_TARGET"] = "17.0"
            if "PRODUCT_BUNDLE_IDENTIFIER" in settings:
                settings["PRODUCT_BUNDLE_IDENTIFIER"] = BUNDLE
                settings["ENABLE_TESTABILITY"] = "YES"
    for file in objects.values():
        if file.get("name") == "BlincAccessibility.swift":
            file["path"] = str(ROOT / "extensions/blinc_platform_ios/swift/BlincAccessibility.swift")
            file["sourceTree"] = "<absolute>"
        if file.get("path") == "libs/simulator/libexample.a":
            file["path"] = str(OUT / "rust/aarch64-apple-ios-sim/debug/libexample.a")
            file["sourceTree"] = "<absolute>"
    library = project / "libs/simulator"
    library.mkdir(parents=True, exist_ok=True)
    link = library / "libexample.a"
    if link.is_symlink():
        link.unlink()
    link.symlink_to(OUT / "rust/aarch64-apple-ios-sim/debug/libexample.a")

    def add(key_name, **fields):
        key = hashlib.sha256(key_name.encode()).hexdigest()[:24].upper()
        objects[key] = fields
        return key

    tests = project / "BlincAccessibilityUITests"
    tests.mkdir(exist_ok=True)
    shutil.copyfile(FIXTURE / "ios/AccessibilityTests.swift", tests / "AccessibilityTests.swift")
    source = add("test-source", isa="PBXFileReference", lastKnownFileType="sourcecode.swift",
                 path="AccessibilityTests.swift", sourceTree="<group>")
    build_file = add("test-build-file", isa="PBXBuildFile", fileRef=source)
    group = add("test-group", isa="PBXGroup", children=[source], path="BlincAccessibilityUITests", sourceTree="<group>")
    objects[root["mainGroup"]]["children"].append(group)
    product = add("test-product", isa="PBXFileReference", explicitFileType="wrapper.cfbundle",
                  path="BlincAccessibilityUITests.xctest", sourceTree="BUILT_PRODUCTS_DIR", includeInIndex=0)
    objects[root["productRefGroup"]]["children"].append(product)
    sources = add("test-sources", isa="PBXSourcesBuildPhase", buildActionMask=2147483647,
                  files=[build_file], runOnlyForDeploymentPostprocessing=0)
    frameworks = add("test-frameworks", isa="PBXFrameworksBuildPhase", buildActionMask=2147483647,
                     files=[], runOnlyForDeploymentPostprocessing=0)
    proxy = add("test-proxy", isa="PBXContainerItemProxy", containerPortal=data["rootObject"],
                proxyType=1, remoteGlobalIDString=main_id, remoteInfo=main["name"])
    dependency = add("test-dependency", isa="PBXTargetDependency", target=main_id, targetProxy=proxy)
    configs = []
    for name in ["Debug", "Release"]:
        configs.append(add("test-config-" + name, isa="XCBuildConfiguration", name=name, buildSettings={
            "PRODUCT_NAME": "$(TARGET_NAME)", "PRODUCT_BUNDLE_IDENTIFIER": BUNDLE + ".uitests",
            "GENERATE_INFOPLIST_FILE": "YES", "SWIFT_VERSION": "5.0",
            "IPHONEOS_DEPLOYMENT_TARGET": "17.0", "SUPPORTED_PLATFORMS": "iphonesimulator",
            "SDKROOT": "iphonesimulator", "TARGETED_DEVICE_FAMILY": "1",
            "TEST_TARGET_NAME": main["name"], "CODE_SIGNING_ALLOWED": "NO",
            "LD_RUNPATH_SEARCH_PATHS": ["$(inherited)", "@executable_path/Frameworks", "@loader_path/Frameworks"],
        }))
    config_list = add("test-configs", isa="XCConfigurationList", buildConfigurations=configs,
                      defaultConfigurationIsVisible=0, defaultConfigurationName="Debug")
    target = add("test-target", isa="PBXNativeTarget", name="BlincAccessibilityUITests",
                 buildConfigurationList=config_list, buildPhases=[sources, frameworks],
                 buildRules=[], dependencies=[dependency], productName="BlincAccessibilityUITests",
                 productReference=product, productType="com.apple.product-type.bundle.ui-testing")
    root["targets"].append(target)
    unit_source_path = tests / "AccessibilityActions.swift"
    shutil.copyfile(FIXTURE / "ios/AccessibilityActions.swift", unit_source_path)
    unit_source = add("unit-source", isa="PBXFileReference", lastKnownFileType="sourcecode.swift",
                      path="AccessibilityActions.swift", sourceTree="<group>")
    objects[group]["children"].append(unit_source)
    unit_build = add("unit-build", isa="PBXBuildFile", fileRef=unit_source)
    unit_sources = add("unit-sources", isa="PBXSourcesBuildPhase", buildActionMask=2147483647,
                       files=[unit_build], runOnlyForDeploymentPostprocessing=0)
    unit_product = add("unit-product", isa="PBXFileReference", explicitFileType="wrapper.cfbundle",
                      path="BlincAccessibilityActions.xctest", sourceTree="BUILT_PRODUCTS_DIR", includeInIndex=0)
    objects[root["productRefGroup"]]["children"].append(unit_product)
    unit_configs = []
    for name in ["Debug", "Release"]:
        unit_configs.append(add("unit-config-" + name, isa="XCBuildConfiguration", name=name, buildSettings={
            "PRODUCT_NAME": "$(TARGET_NAME)", "PRODUCT_BUNDLE_IDENTIFIER": BUNDLE + ".actions",
            "GENERATE_INFOPLIST_FILE": "YES", "SWIFT_VERSION": "5.0",
            "IPHONEOS_DEPLOYMENT_TARGET": "17.0", "SUPPORTED_PLATFORMS": "iphonesimulator",
            "SDKROOT": "iphonesimulator", "TARGETED_DEVICE_FAMILY": "1", "CODE_SIGNING_ALLOWED": "NO",
            "TEST_HOST": "$(BUILT_PRODUCTS_DIR)/BlincApp.app/BlincApp", "BUNDLE_LOADER": "$(TEST_HOST)",
            "LD_RUNPATH_SEARCH_PATHS": ["$(inherited)", "@executable_path/Frameworks", "@loader_path/Frameworks"],
        }))
    unit_config_list = add("unit-configs", isa="XCConfigurationList", buildConfigurations=unit_configs,
                           defaultConfigurationIsVisible=0, defaultConfigurationName="Debug")
    unit_target = add("unit-target", isa="PBXNativeTarget", name="BlincAccessibilityActions",
                      buildConfigurationList=unit_config_list, buildPhases=[unit_sources], buildRules=[],
                      dependencies=[dependency], productName="BlincAccessibilityActions",
                      productReference=unit_product, productType="com.apple.product-type.bundle.unit-test")
    root["targets"].append(unit_target)
    root.setdefault("attributes", {}).setdefault("TargetAttributes", {})[target] = {
        "CreatedOnToolsVersion": "26.4", "TestTargetID": main_id,
    }
    pbx.write_text("// !$*UTF8*$!\n" + encode(data) + "\n")
    subprocess.run(["plutil", "-lint", str(pbx)], check=True)

    scheme = ET.Element("Scheme", LastUpgradeVersion="2640", version="1.3")
    action = ET.SubElement(scheme, "BuildAction", parallelizeBuildables="NO", buildImplicitDependencies="YES")
    entries = ET.SubElement(action, "BuildActionEntries")

    def reference(parent, identifier, name):
        return ET.SubElement(parent, "BuildableReference", BuildableIdentifier="primary",
                             BlueprintIdentifier=identifier, BuildableName=name,
                             BlueprintName=objects[identifier]["name"], ReferencedContainer="container:BlincApp.xcodeproj")

    for identifier, name in [(main_id, "BlincApp.app"), (target, "BlincAccessibilityUITests.xctest"), (unit_target, "BlincAccessibilityActions.xctest")]:
        entry = ET.SubElement(entries, "BuildActionEntry", buildForTesting="YES", buildForRunning="YES",
                              buildForProfiling="NO", buildForArchiving="NO", buildForAnalyzing="NO")
        reference(entry, identifier, name)
    test = ET.SubElement(scheme, "TestAction", buildConfiguration="Debug", shouldUseLaunchSchemeArgsEnv="YES")
    testable = ET.SubElement(ET.SubElement(test, "Testables"), "TestableReference", skipped="NO", parallelizable="NO")
    reference(testable, target, "BlincAccessibilityUITests.xctest")
    testable = ET.SubElement(test.find("Testables"), "TestableReference", skipped="NO", parallelizable="NO")
    reference(testable, unit_target, "BlincAccessibilityActions.xctest")
    variables = ET.SubElement(test, "EnvironmentVariables")
    for key in ["BLINC_GPU_MAX_PRIMITIVES", "BLINC_GPU_MAX_GLYPHS"]:
        ET.SubElement(variables, "EnvironmentVariable", key=key, value="1024", isEnabled="YES")
    path = project / "BlincApp.xcodeproj/xcshareddata/xcschemes"
    path.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(scheme).write(path / f"{SCHEME}.xcscheme", encoding="utf-8", xml_declaration=True)
    return project / "BlincApp.xcodeproj"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    lock = (OUT / ".runner.lock").open("a")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        parser.error("Another simulator accessibility run is already active.")
    project = prepare()
    if args.prepare_only:
        lock.close()
        return 0
    baseline = memory()
    peak_swap = baseline["swap_used_mb"]
    device = None
    events = []
    report = {"simulator_started": False, "tests_started": False, "outcome": "prepared"}
    env = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_DEV_OPT_LEVEL="0", CARGO_PROFILE_DEV_CODEGEN_UNITS="1",
               CARGO_TARGET_DIR=str(OUT / "rust"), IPHONEOS_DEPLOYMENT_TARGET="17.0")

    def record_memory(name):
        nonlocal peak_swap
        sample = memory()
        peak_swap = max(peak_swap, sample["swap_used_mb"])
        events.append({"phase": name, **sample})

    def run(name, command, *, timeout=900):
        record_memory(name)
        print(name, flush=True)
        with (OUT / f"{name}.log").open("w") as log:
            process = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, env=env, start_new_session=True)
            if name == "accessibility-tests":
                report["tests_started"] = True
            started = time.monotonic()
            sampled = started
            try:
                while process.poll() is None:
                    if time.monotonic() - sampled >= 10:
                        record_memory(name)
                        sampled = time.monotonic()
                    if time.monotonic() - started > timeout:
                        raise RuntimeError(f"{name} exceeded {timeout}s")
                    time.sleep(1)
                record_memory(name)
                return process.returncode
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()

    def require(name, command, **options):
        if run(name, command, **options):
            raise RuntimeError(f"{name} failed; see {OUT / (name + '.log')}")

    try:
        require("rust-build", ["cargo", "build", "--offline", "--manifest-path", str(FIXTURE / "Cargo.toml"),
                               "--lib", "--target", "aarch64-apple-ios-sim"])
        base = ["xcodebuild", "-project", str(project), "-scheme", SCHEME, "-configuration", "Debug",
                "-sdk", "iphonesimulator", "-derivedDataPath", str(OUT / "derived-data"), "-jobs", "1",
                "ARCHS=arm64", "ONLY_ACTIVE_ARCH=YES", "CODE_SIGNING_ALLOWED=NO", "COMPILER_INDEX_STORE_ENABLE=NO"]
        require("xcode-build", base + ["build-for-testing", "-destination", "generic/platform=iOS Simulator"])
        devices = json.loads(output("xcrun", "simctl", "list", "devices", "available", "--json"))["devices"]
        if any(d["state"] != "Shutdown" for values in devices.values() for d in values):
            raise RuntimeError("A simulator is already active; keeping this run from starting another.")
        runtime = "com.apple.CoreSimulator.SimRuntime.iOS-17-5"
        if runtime not in devices:
            raise RuntimeError("Install an iOS17.5 runtime or select an installed runtime in this runner.")
        own = next((d for d in devices[runtime] if d["name"] == DEVICE_NAME), None)
        if own and own["state"] != "Shutdown":
            raise RuntimeError("The dedicated test device is already active; not starting a concurrent run.")
        device = own["udid"] if own else output("xcrun", "simctl", "create", DEVICE_NAME,
            "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation", runtime)
        require("simulator-boot", ["xcrun", "simctl", "boot", device], timeout=120)
        report["simulator_started"] = True
        require("simulator-ready", ["xcrun", "simctl", "bootstatus", device, "-b"], timeout=180)
        result = OUT / f"results-{time.time_ns()}.xcresult"
        status = run("accessibility-tests", base + ["test-without-building", "-destination", f"platform=iOS Simulator,id={device}",
            "-parallel-testing-enabled", "NO", "-maximum-concurrent-test-simulator-destinations", "1",
            "-resultBundlePath", str(result)], timeout=300)
        if result.exists():
            require("export-attachments", ["xcrun", "xcresulttool", "export", "attachments", "--path", str(result),
                                           "--output-path", str(OUT / (result.stem + "-attachments"))])
            summary = output("xcrun", "xcresulttool", "get", "test-results", "summary", "--path", str(result))
            (OUT / (result.stem + "-summary.json")).write_text(summary + "\n")
        report["outcome"] = "tests-failed" if status else "passed"
        return status
    except RuntimeError as error:
        report["outcome"] = "setup-failed"
        report["reason"] = str(error)
        print(error, file=sys.stderr)
        return 1
    finally:
        if device:
            shutdown = subprocess.run(["xcrun", "simctl", "shutdown", device], capture_output=True)
            report["shutdown_returncode"] = shutdown.returncode
        (OUT / "memory.json").write_text(json.dumps({
            **report, "baseline": baseline, "peak_swap_mb": peak_swap, "final": memory(), "samples": events,
        }, indent=2) + "\n")
        lock.close()


if __name__ == "__main__":
    raise SystemExit(main())
