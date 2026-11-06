#[test]
fn generated_android_host_includes_the_native_editing_connection() {
    let root = std::env::temp_dir().join(format!("blinc-android-ime-{}", std::process::id()));
    std::fs::create_dir_all(root.join("platforms/android/app/src/main")).unwrap();
    super::create_rust_android_files(&root, "Editor", "editor", "com.blinc.test").unwrap();
    let kotlin = root.join("platforms/android/app/src/main/kotlin/com/blinc");
    let activity = std::fs::read_to_string(kotlin.join("MainActivity.kt")).unwrap();
    assert!(activity.contains("BlincNativeBridge.registerDefaults(this)"));
    let canonical = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/blinc_platform_android/templates");
    for name in ["BlincNativeBridge.kt", "BlincIme.kt"] {
        if canonical.exists() {
            assert_eq!(
                std::fs::read(canonical.join(name)).unwrap(),
                std::fs::read(kotlin.join(name)).unwrap()
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
