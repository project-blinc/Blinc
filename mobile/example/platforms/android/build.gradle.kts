plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.22" apply false
}

tasks.register<Exec>("buildRust") {
    description = "Build Rust library for Android"
    group = "rust"
    workingDir = file("../..")
    commandLine("cargo", "ndk", "-t", "arm64-v8a", "-o", "platforms/android/app/src/main/jniLibs", "build", "--release")
}
