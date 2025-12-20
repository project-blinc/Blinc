//! Project creation and scaffolding
//!
//! Creates opinionated workspace structure for Blinc projects:
//! - .blincproj       - Project configuration
//! - src/             - Source files
//! - plugins/         - Local plugins
//! - platforms/       - Platform-specific code
//!   ├── android/
//!   ├── ios/
//!   ├── macos/
//!   ├── windows/
//!   └── linux/
//! - assets/          - Static assets

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::config::BlincProject;

/// Create a new Blinc project with full workspace structure
pub fn create_project(path: &Path, name: &str, template: &str) -> Result<()> {
    // Create directory structure
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("assets"))?;
    fs::create_dir_all(path.join("plugins"))?;

    // Create platform directories
    fs::create_dir_all(path.join("platforms/android"))?;
    fs::create_dir_all(path.join("platforms/ios"))?;
    fs::create_dir_all(path.join("platforms/macos"))?;
    fs::create_dir_all(path.join("platforms/windows"))?;
    fs::create_dir_all(path.join("platforms/linux"))?;

    // Create .blincproj
    let project = BlincProject::new(name).with_all_platforms(name);
    fs::write(path.join(".blincproj"), project.to_toml()?)?;

    // Create main file based on template
    let main_content = match template {
        "minimal" => template_minimal(name),
        "counter" => template_counter(name),
        _ => template_default(name),
    };

    fs::write(path.join("src/main.blinc"), main_content)?;

    // Create platform entry points
    create_platform_files(path, name)?;

    // Create plugins README
    fs::write(
        path.join("plugins/README.md"),
        r#"# Plugins

Place your local Blinc plugins here. Each plugin should be in its own directory.

## Creating a Plugin

```bash
cd plugins
blinc plugin new my_plugin
```

## Using a Plugin

Add to your `.blincproj`:

```toml
[[dependencies.plugins]]
name = "my_plugin"
path = "plugins/my_plugin"
```
"#,
    )?;

    // Create .gitignore
    fs::write(
        path.join(".gitignore"),
        r#"# Blinc build artifacts
/target/
*.zrtl

# Platform-specific build outputs
/platforms/android/build/
/platforms/android/.gradle/
/platforms/ios/build/
/platforms/ios/Pods/
/platforms/macos/build/
/platforms/windows/build/
/platforms/linux/build/

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Secrets
*.keystore
*.jks
*.p12
*.mobileprovision
"#,
    )?;

    // Create README
    fs::write(
        path.join("README.md"),
        format!(
            r#"# {name}

A Blinc UI application.

## Development

```bash
blinc dev
```

## Build

```bash
# Desktop (current platform)
blinc build --release

# Mobile
blinc build --target android --release
blinc build --target ios --release
```

## Project Structure

```
{name}/
├── .blincproj           # Project configuration
├── src/
│   └── main.blinc       # Application entry point
├── assets/              # Static assets (images, fonts, etc.)
├── plugins/             # Local plugins
└── platforms/           # Platform-specific code
    ├── android/         # Android project files
    ├── ios/             # iOS/Xcode project files
    ├── macos/           # macOS app bundle config
    ├── windows/         # Windows executable config
    └── linux/           # Linux desktop config
```

## Configuration

Edit `.blincproj` to configure:
- Project metadata (name, version, description)
- Platform-specific settings (package IDs, SDK versions)
- Dependencies (plugins, external packages)
"#,
        ),
    )?;

    Ok(())
}

/// Create platform-specific files
fn create_platform_files(path: &Path, name: &str) -> Result<()> {
    let package_name = name.replace('-', "_").replace(' ', "_").to_lowercase();

    // Android
    create_android_files(path, name, &package_name)?;

    // iOS
    create_ios_files(path, name, &package_name)?;

    // macOS
    create_macos_files(path, name, &package_name)?;

    // Windows
    create_windows_files(path, name)?;

    // Linux
    create_linux_files(path, name)?;

    Ok(())
}

fn create_android_files(path: &Path, name: &str, package_name: &str) -> Result<()> {
    let android_path = path.join("platforms/android");

    // Create basic Android structure
    fs::create_dir_all(android_path.join("app/src/main/java"))?;
    fs::create_dir_all(android_path.join("app/src/main/res/values"))?;

    // settings.gradle.kts
    fs::write(
        android_path.join("settings.gradle.kts"),
        format!(
            r#"rootProject.name = "{name}"
include(":app")
"#
        ),
    )?;

    // build.gradle.kts (root)
    fs::write(
        android_path.join("build.gradle.kts"),
        r#"// Top-level build file for Blinc Android project
plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.20" apply false
}
"#,
    )?;

    // app/build.gradle.kts
    fs::write(
        android_path.join("app/build.gradle.kts"),
        format!(
            r#"plugins {{
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}}

android {{
    namespace = "com.example.{package_name}"
    compileSdk = 35

    defaultConfig {{
        applicationId = "com.example.{package_name}"
        minSdk = 24
        targetSdk = 35
        versionCode = 1
        versionName = "1.0.0"
    }}

    buildTypes {{
        release {{
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }}
    }}

    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }}

    kotlinOptions {{
        jvmTarget = "17"
    }}
}}

dependencies {{
    // Blinc runtime will be added here
}}
"#
        ),
    )?;

    // AndroidManifest.xml
    fs::write(
        android_path.join("app/src/main/AndroidManifest.xml"),
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <application
        android:allowBackup="true"
        android:label="{name}"
        android:supportsRtl="true"
        android:theme="@style/Theme.Blinc">

        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:configChanges="orientation|screenSize|screenLayout|keyboardHidden">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>

    </application>

</manifest>
"#
        ),
    )?;

    // MainActivity.kt placeholder
    let package_path = format!("com/example/{}", package_name);
    fs::create_dir_all(android_path.join(format!("app/src/main/java/{}", package_path)))?;
    fs::write(
        android_path.join(format!(
            "app/src/main/java/{}/MainActivity.kt",
            package_path
        )),
        format!(
            r#"package com.example.{package_name}

import android.app.Activity
import android.os.Bundle

/**
 * Main entry point for the Blinc Android application.
 * The Blinc runtime will initialize and render the UI here.
 */
class MainActivity : Activity() {{
    override fun onCreate(savedInstanceState: Bundle?) {{
        super.onCreate(savedInstanceState)
        // Blinc runtime initialization will be added here
    }}
}}
"#
        ),
    )?;

    // res/values/themes.xml
    fs::write(
        android_path.join("app/src/main/res/values/themes.xml"),
        r#"<?xml version="1.0" encoding="utf-8"?>
<resources>
    <style name="Theme.Blinc" parent="android:Theme.Material.Light.NoActionBar">
        <item name="android:windowFullscreen">false</item>
    </style>
</resources>
"#,
    )?;

    // gradle.properties
    fs::write(
        android_path.join("gradle.properties"),
        r#"# Blinc Android project properties
org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
android.useAndroidX=true
kotlin.code.style=official
android.nonTransitiveRClass=true
"#,
    )?;

    // proguard-rules.pro
    fs::write(
        android_path.join("app/proguard-rules.pro"),
        r#"# Blinc ProGuard rules
# Keep Blinc runtime classes
-keep class blinc.** { *; }

# Keep native methods
-keepclasseswithmembernames class * {
    native <methods>;
}
"#,
    )?;

    // README
    fs::write(
        android_path.join("README.md"),
        format!(
            r#"# {name} - Android

Android platform files for {name}.

## Building

```bash
# From project root
blinc build --target android --release

# Or using Gradle directly
cd platforms/android
./gradlew assembleRelease
```

## Requirements

- Android SDK with API 35
- Gradle 8.x
- JDK 17+

## Configuration

Edit `app/build.gradle.kts` to modify:
- Package name
- Min/Target SDK versions
- Build settings
"#
        ),
    )?;

    Ok(())
}

fn create_ios_files(path: &Path, name: &str, package_name: &str) -> Result<()> {
    let ios_path = path.join("platforms/ios");

    // Create Xcode project structure
    let xcodeproj = ios_path.join(format!("{}.xcodeproj", name));
    fs::create_dir_all(&xcodeproj)?;
    fs::create_dir_all(ios_path.join(name))?;

    // Info.plist
    fs::write(
        ios_path.join(format!("{}/Info.plist", name)),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>com.example.{package_name}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
    <key>UILaunchStoryboardName</key>
    <string>LaunchScreen</string>
    <key>UIRequiredDeviceCapabilities</key>
    <array>
        <string>arm64</string>
    </array>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
    <key>MinimumOSVersion</key>
    <string>15.0</string>
</dict>
</plist>
"#
        ),
    )?;

    // AppDelegate.swift
    fs::write(
        ios_path.join(format!("{}/AppDelegate.swift", name)),
        r#"import UIKit

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        window = UIWindow(frame: UIScreen.main.bounds)
        window?.rootViewController = BlincViewController()
        window?.makeKeyAndVisible()
        return true
    }
}

/// View controller that hosts the Blinc rendering surface
class BlincViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        // Blinc runtime initialization will be added here
    }
}
"#,
    )?;

    // LaunchScreen.storyboard
    fs::write(
        ios_path.join(format!("{}/LaunchScreen.storyboard", name)),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<document type="com.apple.InterfaceBuilder3.CocoaTouch.Storyboard.XIB" version="3.0" toolsVersion="21701" targetRuntime="iOS.CocoaTouch" propertyAccessControl="none" useAutolayout="YES" launchScreen="YES" useTraitCollections="YES" useSafeAreas="YES" colorMatched="YES" initialViewController="01J-lp-oVM">
    <device id="retina6_1" orientation="portrait" appearance="light"/>
    <dependencies>
        <deployment identifier="iOS"/>
        <plugIn identifier="com.apple.InterfaceBuilder.IBCocoaTouchPlugin" version="21678"/>
        <capability name="Safe area layout guides" minToolsVersion="9.0"/>
        <capability name="documents saved in the Xcode 8 format" minToolsVersion="8.0"/>
    </dependencies>
    <scenes>
        <scene sceneID="EHf-IW-A2E">
            <objects>
                <viewController id="01J-lp-oVM" sceneMemberID="viewController">
                    <view key="view" contentMode="scaleToFill" id="Ze5-6b-2t3">
                        <rect key="frame" x="0.0" y="0.0" width="414" height="896"/>
                        <autoresizingMask key="autoresizingMask" widthSizable="YES" heightSizable="YES"/>
                        <viewLayoutGuide key="safeArea" id="6Tk-OE-BBY"/>
                        <color key="backgroundColor" systemColor="systemBackgroundColor"/>
                    </view>
                </viewController>
                <placeholder placeholderIdentifier="IBFirstResponder" id="iYj-Kq-Ea1" userLabel="First Responder" sceneMemberID="firstResponder"/>
            </objects>
        </scene>
    </scenes>
</document>
"#,
    )?;

    // README
    fs::write(
        ios_path.join("README.md"),
        format!(
            r#"# {name} - iOS

iOS platform files for {name}.

## Building

```bash
# From project root
blinc build --target ios --release
```

## Requirements

- Xcode 15+
- iOS SDK 15.0+
- Apple Developer account (for device deployment)

## Configuration

Edit `{name}/Info.plist` to modify:
- Bundle identifier
- Version information
- Required capabilities
"#
        ),
    )?;

    Ok(())
}

fn create_macos_files(path: &Path, name: &str, package_name: &str) -> Result<()> {
    let macos_path = path.join("platforms/macos");

    // Info.plist for macOS app bundle
    fs::write(
        macos_path.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>com.example.{package_name}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
"#
        ),
    )?;

    // Entitlements
    fs::write(
        macos_path.join("entitlements.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <true/>
    <key>com.apple.security.files.user-selected.read-only</key>
    <true/>
    <key>com.apple.security.network.client</key>
    <true/>
</dict>
</plist>
"#,
    )?;

    // README
    fs::write(
        macos_path.join("README.md"),
        format!(
            r#"# {name} - macOS

macOS platform files for {name}.

## Building

```bash
# From project root
blinc build --target macos --release
```

## App Bundle Structure

The build will create `{name}.app` with:
```
{name}.app/
├── Contents/
│   ├── Info.plist
│   ├── MacOS/
│   │   └── {name}     # Executable
│   └── Resources/
│       └── ...        # Assets
```

## Configuration

Edit `Info.plist` to modify:
- Bundle identifier
- Version information
- Minimum macOS version

Edit `entitlements.plist` to modify:
- App sandbox settings
- Hardened runtime capabilities
"#
        ),
    )?;

    Ok(())
}

fn create_windows_files(path: &Path, name: &str) -> Result<()> {
    let windows_path = path.join("platforms/windows");

    // Windows resource file
    fs::write(
        windows_path.join("app.rc"),
        format!(
            r#"// Windows Resource File for {name}

#include <windows.h>

// Version info
VS_VERSION_INFO VERSIONINFO
FILEVERSION     1,0,0,0
PRODUCTVERSION  1,0,0,0
FILEFLAGSMASK   VS_FFI_FILEFLAGSMASK
FILEFLAGS       0
FILEOS          VOS__WINDOWS32
FILETYPE        VFT_APP
FILESUBTYPE     VFT2_UNKNOWN
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904E4"
        BEGIN
            VALUE "CompanyName",      "\0"
            VALUE "FileDescription",  "{name}\0"
            VALUE "FileVersion",      "1.0.0\0"
            VALUE "InternalName",     "{name}\0"
            VALUE "LegalCopyright",   "\0"
            VALUE "OriginalFilename", "{name}.exe\0"
            VALUE "ProductName",      "{name}\0"
            VALUE "ProductVersion",   "1.0.0\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1252
    END
END

// Application icon
// 1 ICON "icon.ico"
"#
        ),
    )?;

    // Windows manifest
    fs::write(
        windows_path.join("app.manifest"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <assemblyIdentity
        version="1.0.0.0"
        processorArchitecture="*"
        name="{name}"
        type="win32"
    />
    <description>{name}</description>
    <dependency>
        <dependentAssembly>
            <assemblyIdentity
                type="win32"
                name="Microsoft.Windows.Common-Controls"
                version="6.0.0.0"
                processorArchitecture="*"
                publicKeyToken="6595b64144ccf1df"
                language="*"
            />
        </dependentAssembly>
    </dependency>
    <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
        <application>
            <supportedOS Id="{{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}}"/>
            <supportedOS Id="{{1f676c76-80e1-4239-95bb-83d0f6d0da78}}"/>
            <supportedOS Id="{{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}}"/>
            <supportedOS Id="{{35138b9a-5d96-4fbd-8e2d-a2440225f93a}}"/>
            <supportedOS Id="{{e2011457-1546-43c5-a5fe-008deee3d3f0}}"/>
        </application>
    </compatibility>
    <application xmlns="urn:schemas-microsoft-com:asm.v3">
        <windowsSettings>
            <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
            <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
        </windowsSettings>
    </application>
</assembly>
"#
        ),
    )?;

    // README
    fs::write(
        windows_path.join("README.md"),
        format!(
            r#"# {name} - Windows

Windows platform files for {name}.

## Building

```bash
# From project root
blinc build --target windows --release
```

## Configuration

- `app.rc` - Windows resource file (version info, icon)
- `app.manifest` - Application manifest (DPI awareness, dependencies)

## Adding an Icon

1. Place `icon.ico` in this directory
2. Uncomment the icon line in `app.rc`
3. Rebuild the project
"#
        ),
    )?;

    Ok(())
}

fn create_linux_files(path: &Path, name: &str) -> Result<()> {
    let linux_path = path.join("platforms/linux");
    let binary_name = name.to_lowercase().replace(' ', "_").replace('-', "_");

    // Desktop entry file
    fs::write(
        linux_path.join(format!("{}.desktop", binary_name)),
        format!(
            r#"[Desktop Entry]
Type=Application
Name={name}
Comment=A Blinc application
Exec={binary_name}
Icon={binary_name}
Terminal=false
Categories=Utility;
StartupWMClass={name}
"#
        ),
    )?;

    // AppStream metadata
    fs::write(
        linux_path.join(format!("{}.metainfo.xml", binary_name)),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
    <id>com.example.{binary_name}</id>
    <name>{name}</name>
    <summary>A Blinc application</summary>
    <metadata_license>CC0-1.0</metadata_license>
    <project_license>MIT</project_license>
    <description>
        <p>
            {name} is a cross-platform application built with Blinc.
        </p>
    </description>
    <launchable type="desktop-id">{binary_name}.desktop</launchable>
    <provides>
        <binary>{binary_name}</binary>
    </provides>
</component>
"#
        ),
    )?;

    // README
    fs::write(
        linux_path.join("README.md"),
        format!(
            r#"# {name} - Linux

Linux platform files for {name}.

## Building

```bash
# From project root
blinc build --target linux --release
```

## Installation

The desktop entry file can be installed to:
- User: `~/.local/share/applications/`
- System: `/usr/share/applications/`

```bash
# User installation
cp {binary_name}.desktop ~/.local/share/applications/
```

## Configuration

- `{binary_name}.desktop` - Desktop entry for app launchers
- `{binary_name}.metainfo.xml` - AppStream metadata for software centers
"#
        ),
    )?;

    Ok(())
}

/// Create a new ZRTL plugin project
pub fn create_plugin_project(path: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(path.join("src"))?;

    // Create Cargo.toml for the plugin
    fs::write(
        path.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "staticlib"]

[dependencies]
# Add your plugin dependencies here

[features]
default = []
"#,
            name
        ),
    )?;

    // Create lib.rs
    fs::write(
        path.join("src/lib.rs"),
        format!(
            r#"//! {} - Blinc ZRTL Plugin
//!
//! This plugin can be loaded dynamically or compiled statically.

/// Plugin initialization - called when the plugin is loaded
#[no_mangle]
pub extern "C" fn plugin_init() {{
    // Initialize your plugin here
}}

/// Plugin cleanup - called when the plugin is unloaded
#[no_mangle]
pub extern "C" fn plugin_cleanup() {{
    // Clean up resources here
}}

/// Example exported function
#[no_mangle]
pub extern "C" fn hello() -> *const std::ffi::c_char {{
    static HELLO: &[u8] = b"Hello from {}!\0";
    HELLO.as_ptr() as *const std::ffi::c_char
}}
"#,
            name, name
        ),
    )?;

    // Create README
    fs::write(
        path.join("README.md"),
        format!(
            r#"# {}

A Blinc ZRTL plugin.

## Building

### Dynamic (.zrtl)
```bash
blinc plugin build --mode dynamic
```

### Static
```bash
blinc plugin build --mode static
```

## Usage

Import in your Blinc application:

```blinc
import {} from "{}.zrtl"
```
"#,
            name, name, name
        ),
    )?;

    Ok(())
}

fn template_default(name: &str) -> String {
    format!(
        r#"// {name} - Blinc Application
//
// A simple Blinc application with reactive state and animations.

@widget App {{
    @state count: i32 = 0

    @spring scale: f32 = 1.0 {{
        stiffness: 400
        damping: 30
    }}

    @machine button_state {{
        initial: idle

        idle -> hovered: pointer_enter
        hovered -> idle: pointer_leave
        hovered -> pressed: pointer_down
        pressed -> hovered: pointer_up
    }}

    @render {{
        Column {{
            spacing: 20
            align: center

            Text {{
                content: "Welcome to {name}"
                font_size: 24
            }}

            Text {{
                content: "Count: {{count}}"
                font_size: 48
            }}

            Button {{
                label: "Increment"
                on_click: {{ count += 1 }}
                scale: scale
            }}
        }}
    }}
}}
"#
    )
}

fn template_minimal(name: &str) -> String {
    format!(
        r#"// {name} - Minimal Blinc Application

@widget App {{
    @render {{
        Text {{
            content: "Hello, Blinc!"
        }}
    }}
}}
"#
    )
}

fn template_counter(name: &str) -> String {
    format!(
        r#"// {name} - Counter Example
//
// Demonstrates reactive state and FSM-driven interactions.

@widget Counter {{
    @state count: i32 = 0

    @derived doubled: i32 = count * 2

    @machine state {{
        initial: idle

        idle -> active: pointer_enter
        active -> idle: pointer_leave
    }}

    @spring opacity: f32 = 1.0 {{
        stiffness: 300
        damping: 25
    }}

    @effect {{
        // Animate opacity based on state
        when state == active {{
            opacity = 1.0
        }} else {{
            opacity = 0.7
        }}
    }}

    @render {{
        Column {{
            spacing: 16
            padding: 24

            Row {{
                spacing: 12

                Button {{
                    label: "-"
                    on_click: {{ count -= 1 }}
                }}

                Text {{
                    content: "{{count}}"
                    font_size: 32
                    opacity: opacity
                }}

                Button {{
                    label: "+"
                    on_click: {{ count += 1 }}
                }}
            }}

            Text {{
                content: "Doubled: {{doubled}}"
                font_size: 14
                color: #666
            }}
        }}
    }}
}}

@widget App {{
    @render {{
        Center {{
            Counter {{}}
        }}
    }}
}}
"#
    )
}
