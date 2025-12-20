//! Doctor command - diagnose platform setup and dependencies
//!
//! Checks that all required tools and SDKs are properly installed
//! for each target platform.

use std::env;
use std::path::PathBuf;
use std::process::Command;

// ANSI color codes
mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const RED: &str = "\x1b[31m";
    pub const GRAY: &str = "\x1b[90m";
    pub const BOLD: &str = "\x1b[1m";
    pub const CYAN: &str = "\x1b[36m";
}

/// Result of a single check
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
    NotApplicable,
}

impl CheckResult {
    fn ok(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Ok,
            message: message.to_string(),
            hint: None,
        }
    }

    fn warning(name: &str, message: &str, hint: &str) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warning,
            message: message.to_string(),
            hint: Some(hint.to_string()),
        }
    }

    fn error(name: &str, message: &str, hint: &str) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Error,
            message: message.to_string(),
            hint: Some(hint.to_string()),
        }
    }

    fn not_applicable(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::NotApplicable,
            message: message.to_string(),
            hint: None,
        }
    }

    pub fn colored_icon(&self) -> String {
        match self.status {
            CheckStatus::Ok => format!("{}✓{}", colors::GREEN, colors::RESET),
            CheckStatus::Warning => format!("{}!{}", colors::YELLOW, colors::RESET),
            CheckStatus::Error => format!("{}✗{}", colors::RED, colors::RESET),
            CheckStatus::NotApplicable => format!("{}-{}", colors::GRAY, colors::RESET),
        }
    }
}

/// Category of checks
pub struct CheckCategory {
    pub name: String,
    pub checks: Vec<CheckResult>,
}

impl CheckCategory {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            checks: Vec::new(),
        }
    }

    fn add(&mut self, check: CheckResult) {
        self.checks.push(check);
    }

    pub fn status(&self) -> CheckStatus {
        let mut has_warning = false;
        for check in &self.checks {
            match check.status {
                CheckStatus::Error => return CheckStatus::Error,
                CheckStatus::Warning => has_warning = true,
                _ => {}
            }
        }
        if has_warning {
            CheckStatus::Warning
        } else {
            CheckStatus::Ok
        }
    }

    pub fn colored_icon(&self) -> String {
        match self.status() {
            CheckStatus::Ok => format!("{}✓{}", colors::GREEN, colors::RESET),
            CheckStatus::Warning => format!("{}!{}", colors::YELLOW, colors::RESET),
            CheckStatus::Error => format!("{}✗{}", colors::RED, colors::RESET),
            CheckStatus::NotApplicable => format!("{}-{}", colors::GRAY, colors::RESET),
        }
    }
}

/// Run all doctor checks
pub fn run_doctor() -> Vec<CheckCategory> {
    let mut categories = Vec::new();

    categories.push(check_blinc_ecosystem());
    categories.push(check_rust_toolchain());
    categories.push(check_desktop_platform());
    categories.push(check_android_platform());
    categories.push(check_ios_platform());

    categories
}

/// Check Blinc runtime and compiler ecosystem
fn check_blinc_ecosystem() -> CheckCategory {
    let mut cat = CheckCategory::new("Blinc Ecosystem");

    // Get version info (shared across CLI and runtime since they're in the same repo)
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = option_env!("BLINC_GIT_HASH").unwrap_or("unknown");

    // Check blinc CLI version
    cat.add(CheckResult::ok(
        "Blinc CLI",
        &format!("v{} ({})", version, git_hash),
    ));

    // Blinc Runtime (same repo, same version)
    cat.add(CheckResult::ok(
        "Blinc Runtime",
        &format!("v{} ({})", version, git_hash),
    ));

    // TODO: Check for Zyntax compiler when available
    // For now, indicate it's pending
    cat.add(CheckResult::warning(
        "Zyntax Compiler",
        "not yet available",
        "Zyntax Grammar2 compiler is in development",
    ));

    // Check for blinc.toml in current directory (optional)
    let cwd = std::env::current_dir();
    if let Ok(dir) = cwd {
        let config_path = dir.join("blinc.toml");
        if config_path.exists() {
            cat.add(CheckResult::ok(
                "Project config",
                &format!("blinc.toml found in {}", dir.display()),
            ));
        } else {
            cat.add(CheckResult::not_applicable(
                "Project config",
                "no blinc.toml in current directory",
            ));
        }
    }

    cat
}

/// Check Rust toolchain
fn check_rust_toolchain() -> CheckCategory {
    let mut cat = CheckCategory::new("Rust Toolchain");

    // Check rustc
    match get_command_version("rustc", &["--version"]) {
        Some(version) => {
            cat.add(CheckResult::ok("Rust compiler", &version));
        }
        None => {
            cat.add(CheckResult::error(
                "Rust compiler",
                "rustc not found",
                "Install Rust from https://rustup.rs",
            ));
        }
    }

    // Check cargo
    match get_command_version("cargo", &["--version"]) {
        Some(version) => {
            cat.add(CheckResult::ok("Cargo", &version));
        }
        None => {
            cat.add(CheckResult::error(
                "Cargo",
                "cargo not found",
                "Install Rust from https://rustup.rs",
            ));
        }
    }

    // Check rustup
    match get_command_version("rustup", &["--version"]) {
        Some(version) => {
            cat.add(CheckResult::ok("Rustup", &version));
        }
        None => {
            cat.add(CheckResult::warning(
                "Rustup",
                "rustup not found",
                "Install rustup from https://rustup.rs for easier target management",
            ));
        }
    }

    // Check installed targets
    if let Some(targets) = get_installed_targets() {
        let target_count = targets.len();
        let android_targets: Vec<_> = targets.iter().filter(|t| t.contains("android")).collect();
        let ios_targets: Vec<_> = targets.iter().filter(|t| t.contains("apple-ios")).collect();

        let mut target_info = format!("{} targets installed", target_count);
        if !android_targets.is_empty() {
            target_info.push_str(&format!(", {} Android", android_targets.len()));
        }
        if !ios_targets.is_empty() {
            target_info.push_str(&format!(", {} iOS", ios_targets.len()));
        }

        cat.add(CheckResult::ok("Rust targets", &target_info));
    }

    cat
}

/// Check desktop platform requirements
fn check_desktop_platform() -> CheckCategory {
    let mut cat = CheckCategory::new("Desktop Platform");

    let os = env::consts::OS;
    cat.add(CheckResult::ok(
        "Host OS",
        &format!("{} ({})", os, env::consts::ARCH),
    ));

    // Platform-specific checks
    match os {
        "macos" => {
            // Check xcode-select path
            let xcode_select_result = Command::new("xcode-select").arg("-p").output();

            match xcode_select_result {
                Ok(output) if output.status.success() => {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

                    // Check if the path actually exists
                    if PathBuf::from(&path).exists() {
                        cat.add(CheckResult::ok("xcode-select", &format!("set to {}", path)));
                    } else {
                        cat.add(CheckResult::error(
                            "xcode-select",
                            &format!("path does not exist: {}", path),
                            "Run: sudo xcode-select --reset",
                        ));
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if stderr.contains("no developer tools were found") {
                        cat.add(CheckResult::error(
                            "xcode-select",
                            "no developer tools installed",
                            "Run: xcode-select --install",
                        ));
                    } else {
                        cat.add(CheckResult::error(
                            "xcode-select",
                            "not configured",
                            "Run: xcode-select --install",
                        ));
                    }
                }
                Err(_) => {
                    cat.add(CheckResult::error(
                        "xcode-select",
                        "command not found",
                        "Xcode Command Line Tools are not installed",
                    ));
                }
            }

            // Check if CLI tools are actually installed by verifying clang exists
            let clang_check = Command::new("xcrun").args(["--find", "clang"]).output();

            match clang_check {
                Ok(output) if output.status.success() => {
                    cat.add(CheckResult::ok(
                        "Xcode CLI Tools",
                        "clang available via xcrun",
                    ));
                }
                _ => {
                    cat.add(CheckResult::error(
                        "Xcode CLI Tools",
                        "not properly installed",
                        "Run: xcode-select --install",
                    ));
                }
            }
        }
        "linux" => {
            // Check for required libraries
            let has_pkg_config = Command::new("pkg-config").arg("--version").output().is_ok();
            if has_pkg_config {
                cat.add(CheckResult::ok("pkg-config", "available"));
            } else {
                cat.add(CheckResult::warning(
                    "pkg-config",
                    "not found",
                    "Install via: apt install pkg-config (Debian/Ubuntu) or dnf install pkgconfig (Fedora)",
                ));
            }

            // Check for GTK/Wayland libraries (common for UI)
            let libs_to_check = ["gtk+-3.0", "wayland-client"];
            for lib in libs_to_check {
                if check_pkg_config_lib(lib) {
                    cat.add(CheckResult::ok(lib, "available"));
                } else {
                    cat.add(CheckResult::warning(
                        lib,
                        "not found (optional)",
                        &format!("May be needed for some features. Install via package manager."),
                    ));
                }
            }
        }
        "windows" => {
            // Check for Visual Studio Build Tools
            match get_command_version("cl", &[]) {
                Some(_) => {
                    cat.add(CheckResult::ok("MSVC", "available"));
                }
                None => {
                    // Check if we're in a VS developer command prompt
                    if env::var("VSINSTALLDIR").is_ok() {
                        cat.add(CheckResult::ok("MSVC", "Visual Studio detected"));
                    } else {
                        cat.add(CheckResult::warning(
                            "MSVC",
                            "Visual Studio Build Tools not in PATH",
                            "Install Visual Studio Build Tools or run from Developer Command Prompt",
                        ));
                    }
                }
            }
        }
        _ => {
            cat.add(CheckResult::warning(
                "Platform",
                &format!("Unknown platform: {}", os),
                "Desktop support may be limited",
            ));
        }
    }

    cat
}

/// Check Android platform requirements
fn check_android_platform() -> CheckCategory {
    let mut cat = CheckCategory::new("Android Platform");

    // Check ANDROID_HOME or ANDROID_SDK_ROOT
    let sdk_path = find_android_sdk();
    match &sdk_path {
        Some(path) => {
            cat.add(CheckResult::ok(
                "Android SDK",
                &format!("{}", path.display()),
            ));

            // Check for NDK
            let ndk_path = find_android_ndk(path);
            match &ndk_path {
                Some(ndk) => {
                    let version = ndk
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    cat.add(CheckResult::ok(
                        "Android NDK",
                        &format!("{} at {}", version, ndk.display()),
                    ));

                    // Check for clang
                    let toolchain_bin = get_ndk_toolchain_bin(ndk);
                    if let Some(bin) = toolchain_bin {
                        let clang = bin.join("aarch64-linux-android35-clang");
                        if clang.exists() {
                            cat.add(CheckResult::ok("NDK Clang", "API 35 toolchain available"));
                        } else {
                            // Try other API levels
                            let found_api = (21..=35).rev().find(|api| {
                                bin.join(format!("aarch64-linux-android{}-clang", api))
                                    .exists()
                            });
                            match found_api {
                                Some(api) => {
                                    cat.add(CheckResult::ok(
                                        "NDK Clang",
                                        &format!("API {} toolchain available", api),
                                    ));
                                }
                                None => {
                                    cat.add(CheckResult::warning(
                                        "NDK Clang",
                                        "clang not found in NDK",
                                        "NDK may be corrupted. Try reinstalling.",
                                    ));
                                }
                            }
                        }
                    }
                }
                None => {
                    cat.add(CheckResult::error(
                        "Android NDK",
                        "not found",
                        "Install NDK via: sdkmanager 'ndk;27.0.12077973'",
                    ));
                }
            }

            // Check for platform-tools
            let adb = path.join("platform-tools").join("adb");
            if adb.exists() {
                if let Some(version) = get_command_version(adb.to_str().unwrap(), &["--version"]) {
                    let first_line = version.lines().next().unwrap_or(&version);
                    cat.add(CheckResult::ok("ADB", first_line));
                } else {
                    cat.add(CheckResult::ok("ADB", "available"));
                }
            } else {
                cat.add(CheckResult::warning(
                    "ADB",
                    "not found",
                    "Install via: sdkmanager 'platform-tools'",
                ));
            }
        }
        None => {
            cat.add(CheckResult::error(
                "Android SDK",
                "not found",
                "Set ANDROID_HOME environment variable or install Android Studio",
            ));
            cat.add(CheckResult::not_applicable("Android NDK", "SDK not found"));
            cat.add(CheckResult::not_applicable("ADB", "SDK not found"));
        }
    }

    // Check Rust Android targets
    if let Some(targets) = get_installed_targets() {
        let android_targets: Vec<_> = targets
            .iter()
            .filter(|t| t.contains("android"))
            .cloned()
            .collect();

        if android_targets.is_empty() {
            cat.add(CheckResult::error(
                "Rust Android targets",
                "none installed",
                "Run: rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android",
            ));
        } else {
            cat.add(CheckResult::ok(
                "Rust Android targets",
                &android_targets.join(", "),
            ));
        }
    }

    // Check Java (required for Android SDK tools)
    match get_command_version("java", &["-version"]) {
        Some(version) => {
            let first_line = version.lines().next().unwrap_or(&version);
            cat.add(CheckResult::ok("Java", first_line));
        }
        None => {
            cat.add(CheckResult::warning(
                "Java",
                "not found",
                "Java is required for some Android SDK tools. Install JDK 17+",
            ));
        }
    }

    cat
}

/// Check iOS platform requirements (macOS only)
fn check_ios_platform() -> CheckCategory {
    let mut cat = CheckCategory::new("iOS Platform");

    if env::consts::OS != "macos" {
        cat.add(CheckResult::not_applicable(
            "iOS development",
            "only available on macOS",
        ));
        return cat;
    }

    // Check Xcode
    match get_command_version("xcodebuild", &["-version"]) {
        Some(version) => {
            let first_line = version.lines().next().unwrap_or(&version);
            cat.add(CheckResult::ok("Xcode", first_line));
        }
        None => {
            cat.add(CheckResult::error(
                "Xcode",
                "not installed",
                "Install Xcode from the Mac App Store",
            ));
            cat.add(CheckResult::not_applicable(
                "iOS Simulator",
                "Xcode not installed",
            ));
            cat.add(CheckResult::not_applicable(
                "Rust iOS targets",
                "Xcode not installed",
            ));
            return cat;
        }
    }

    // Check for iOS Simulator
    let simctl_output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "available", "-j"])
        .output();

    match simctl_output {
        Ok(output) if output.status.success() => {
            cat.add(CheckResult::ok("iOS Simulator", "available"));
        }
        _ => {
            cat.add(CheckResult::warning(
                "iOS Simulator",
                "could not list simulators",
                "Run: xcodebuild -downloadPlatform iOS",
            ));
        }
    }

    // Check Rust iOS targets
    if let Some(targets) = get_installed_targets() {
        let ios_targets: Vec<_> = targets
            .iter()
            .filter(|t| t.contains("apple-ios"))
            .cloned()
            .collect();

        if ios_targets.is_empty() {
            cat.add(CheckResult::error(
                "Rust iOS targets",
                "none installed",
                "Run: rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios",
            ));
        } else {
            cat.add(CheckResult::ok("Rust iOS targets", &ios_targets.join(", ")));
        }
    }

    cat
}

// Helper functions

fn get_command_version(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let output = if stdout.trim().is_empty() {
                stderr.to_string()
            } else {
                stdout.to_string()
            };
            if output.trim().is_empty() {
                None
            } else {
                Some(output.trim().to_string())
            }
        })
}

fn get_installed_targets() -> Option<Vec<String>> {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
}

fn find_android_sdk() -> Option<PathBuf> {
    // Check environment variables
    if let Ok(path) = env::var("ANDROID_HOME") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    if let Ok(path) = env::var("ANDROID_SDK_ROOT") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // Check common locations
    let home = dirs::home_dir()?;
    let common_paths = [
        home.join("Library/Android/sdk"), // macOS
        home.join("Android/Sdk"),         // Linux
        PathBuf::from("C:\\Users")
            .join(env::var("USERNAME").unwrap_or_default())
            .join("AppData\\Local\\Android\\Sdk"), // Windows
    ];

    common_paths.into_iter().find(|p| p.exists())
}

fn find_android_ndk(sdk_path: &PathBuf) -> Option<PathBuf> {
    // Check ANDROID_NDK_HOME
    if let Ok(path) = env::var("ANDROID_NDK_HOME") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // Look in SDK ndk directory
    let ndk_dir = sdk_path.join("ndk");
    if ndk_dir.exists() {
        // Find the latest version
        if let Ok(entries) = std::fs::read_dir(&ndk_dir) {
            let mut versions: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .collect();

            versions.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

            if let Some(latest) = versions.first() {
                return Some(latest.path());
            }
        }
    }

    None
}

fn get_ndk_toolchain_bin(ndk_path: &PathBuf) -> Option<PathBuf> {
    let os = match env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => return None,
    };

    let arch = match env::consts::ARCH {
        "x86_64" | "aarch64" => "x86_64", // NDK uses x86_64 even on arm macs
        _ => return None,
    };

    let bin = ndk_path
        .join("toolchains/llvm/prebuilt")
        .join(format!("{}-{}", os, arch))
        .join("bin");

    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

fn check_pkg_config_lib(lib: &str) -> bool {
    Command::new("pkg-config")
        .args(["--exists", lib])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Print doctor results to stdout
pub fn print_doctor_results(categories: &[CheckCategory]) {
    println!(
        "{}{}Blinc Doctor{}",
        colors::BOLD,
        colors::CYAN,
        colors::RESET
    );
    println!("============");
    println!();

    let mut total_errors = 0;
    let mut total_warnings = 0;

    for category in categories {
        let icon = category.colored_icon();
        println!(
            "[{}] {}{}{}",
            icon,
            colors::BOLD,
            category.name,
            colors::RESET
        );

        for check in &category.checks {
            let icon = check.colored_icon();
            println!("    [{}] {}: {}", icon, check.name, check.message);

            if let Some(hint) = &check.hint {
                println!("        {}→ {}{}", colors::CYAN, hint, colors::RESET);
            }

            match check.status {
                CheckStatus::Error => total_errors += 1,
                CheckStatus::Warning => total_warnings += 1,
                _ => {}
            }
        }

        println!();
    }

    // Summary
    println!("────────────────────────────────────────");
    if total_errors == 0 && total_warnings == 0 {
        println!(
            "{}{}✓ All checks passed!{} Your environment is ready.",
            colors::BOLD,
            colors::GREEN,
            colors::RESET
        );
    } else {
        if total_errors > 0 {
            println!(
                "{}{}✗ {} issue(s) found that need attention{}",
                colors::BOLD,
                colors::RED,
                total_errors,
                colors::RESET
            );
        }
        if total_warnings > 0 {
            println!(
                "{}{}! {} warning(s){} - optional improvements available",
                colors::BOLD,
                colors::YELLOW,
                total_warnings,
                colors::RESET
            );
        }
    }
}
