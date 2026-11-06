use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn generated_ios_host_typechecks_with_accessibility_adapter() {
    let root = std::env::temp_dir().join(format!(
        "blinc-ios-generator-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    super::create_rust_ios_files(&root, "Accessibility", "accessibility", "com.blinc.test")
        .unwrap();
    let canonical = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/blinc_platform_ios/swift/BlincAccessibility.swift");
    if canonical.exists() {
        assert_eq!(
            fs::read(canonical).unwrap(),
            fs::read(root.join("platforms/ios/BlincApp/BlincAccessibility.swift")).unwrap()
        );
    }
    let sdk = Command::new("xcrun")
        .args(["--sdk", "iphonesimulator", "--show-sdk-path"])
        .output()
        .unwrap();
    assert!(sdk.status.success());
    let app = root.join("platforms/ios/BlincApp");
    let result = Command::new("xcrun")
        .args([
            "swiftc",
            "-typecheck",
            "-sdk",
            std::str::from_utf8(&sdk.stdout).unwrap().trim(),
            "-target",
            "arm64-apple-ios17.0-simulator",
            "-module-cache-path",
        ])
        .arg(root.join("modules"))
        .arg("-import-objc-header")
        .arg(app.join("Blinc-Bridging-Header.h"))
        .args(
            [
                "AppDelegate.swift",
                "BlincViewController.swift",
                "BlincMetalView.swift",
                "BlincAccessibility.swift",
            ]
            .map(|s| app.join(s)),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new("plutil")
        .arg("-lint")
        .arg(root.join("platforms/ios/BlincApp.xcodeproj/project.pbxproj"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    fs::remove_dir_all(&root).unwrap();
}
