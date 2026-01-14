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
//!   ├── linux/
//!   └── wasm/
//! - assets/          - Static assets

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::config::BlincProject;

/// Create a new Blinc project with full workspace structure
pub fn create_project(path: &Path, name: &str, template: &str, org: &str) -> Result<()> {
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
    fs::create_dir_all(path.join("platforms/wasm"))?;

    // Create .blincproj
    let project = BlincProject::new(name).with_all_platforms(name, org);
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

# Web (WASM)
blinc build --target wasm --release
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
    ├── linux/           # Linux desktop config
    └── wasm/            # Web/WASM build files
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

    // WASM/Web
    create_wasm_files(path, name)?;

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

fn create_wasm_files(path: &Path, name: &str) -> Result<()> {
    let wasm_path = path.join("platforms/wasm");
    let binary_name = name.to_lowercase().replace(' ', "_").replace('-', "_");

    // index.html - Main HTML entry point
    fs::write(
        wasm_path.join("index.html"),
        format!(
            r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <meta name="theme-color" content="#000000">
    <meta name="description" content="{name} - A Blinc Application">
    <title>{name}</title>
    <link rel="manifest" href="manifest.json">
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        html, body {{
            width: 100%;
            height: 100%;
            overflow: hidden;
            background: #000;
        }}
        #blinc-canvas {{
            width: 100%;
            height: 100%;
            display: block;
        }}
        .loading {{
            position: fixed;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            color: #fff;
            font-family: system-ui, sans-serif;
            font-size: 18px;
        }}
    </style>
</head>
<body>
    <div id="loading" class="loading">Loading...</div>
    <canvas id="blinc-canvas"></canvas>

    <script type="module">
        // Import WASM module
        import init, {{ run }} from './{binary_name}.js';

        async function main() {{
            try {{
                // Initialize WASM module
                await init();

                // Hide loading indicator
                document.getElementById('loading').style.display = 'none';

                // Run the application
                run();
            }} catch (error) {{
                console.error('Failed to start application:', error);
                document.getElementById('loading').textContent =
                    'Failed to load. Please ensure your browser supports WebGPU or WebGL2.';
            }}
        }}

        main();
    </script>
</body>
</html>
"##
        ),
    )?;

    // manifest.json - PWA manifest
    fs::write(
        wasm_path.join("manifest.json"),
        format!(
            r##"{{
    "name": "{name}",
    "short_name": "{name}",
    "description": "A Blinc Application",
    "start_url": "/",
    "display": "standalone",
    "orientation": "any",
    "background_color": "#000000",
    "theme_color": "#000000",
    "icons": [
        {{
            "src": "icons/icon-192.png",
            "sizes": "192x192",
            "type": "image/png"
        }},
        {{
            "src": "icons/icon-512.png",
            "sizes": "512x512",
            "type": "image/png"
        }}
    ]
}}
"##
        ),
    )?;

    // service-worker.js - Basic service worker for offline support
    fs::write(
        wasm_path.join("service-worker.js"),
        format!(
            r#"// {name} Service Worker
const CACHE_NAME = '{binary_name}-v1';
const ASSETS = [
    '/',
    '/index.html',
    '/{binary_name}.js',
    '/{binary_name}_bg.wasm',
];

self.addEventListener('install', (event) => {{
    event.waitUntil(
        caches.open(CACHE_NAME)
            .then((cache) => cache.addAll(ASSETS))
    );
}});

self.addEventListener('fetch', (event) => {{
    event.respondWith(
        caches.match(event.request)
            .then((response) => response || fetch(event.request))
    );
}});
"#
        ),
    )?;

    // Create icons directory
    fs::create_dir_all(wasm_path.join("icons"))?;

    // README
    fs::write(
        wasm_path.join("README.md"),
        format!(
            r#"# {name} - Web (WASM)

Web/WASM platform files for {name}.

## Building

```bash
# From project root
blinc build --target wasm --release
```

## Development Server

```bash
# Start development server with hot reload
blinc dev --target wasm
```

## Files

- `index.html` - HTML entry point
- `manifest.json` - PWA manifest
- `service-worker.js` - Service worker for offline support
- `icons/` - PWA icons (add your icons here)

## Browser Requirements

- WebGPU support (preferred) or WebGL2 fallback
- Minimum browser versions:
  - Chrome 89+
  - Firefox 89+
  - Safari 15+
  - Edge 89+

## GPU Backend

The application uses WebGPU when available, with automatic fallback to WebGL2.
Configure the preferred backend in `.blincproj`:

```toml
[platforms.wasm]
gpu_backend = "webgpu"  # or "webgl"
```
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

/// Create a new Rust-first Blinc project
///
/// This creates a native Rust project with Cargo.toml instead of .blinc DSL files.
/// Ideal for testing mobile platforms with full control over the Rust code.
pub fn create_rust_project(path: &Path, name: &str, org: &str) -> Result<()> {
    let package_name = name.replace('-', "_").replace(' ', "_").to_lowercase();

    // Get blinc workspace path (relative to the generated project)
    let blinc_path = std::env::var("BLINC_PATH").unwrap_or_else(|_| {
        // Try to find the blinc workspace relative to the CLI binary
        let exe_path = std::env::current_exe().unwrap_or_default();
        exe_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "../../..".to_string())
    });

    // Create directory structure
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("platforms/android/app/src/main/kotlin/com/blinc"))?;
    fs::create_dir_all(path.join("platforms/ios/BlincApp"))?;

    // Create Cargo.toml
    fs::write(
        path.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2021"

[lib]
name = "{package_name}"
path = "src/main.rs"
crate-type = ["cdylib", "staticlib", "rlib"]

[[bin]]
name = "{package_name}_desktop"
path = "src/main.rs"
required-features = ["desktop"]

[dependencies]
blinc_app = {{ path = "{blinc_path}/crates/blinc_app" }}
blinc_core = {{ path = "{blinc_path}/crates/blinc_core" }}
blinc_layout = {{ path = "{blinc_path}/crates/blinc_layout" }}
tracing = "0.1"
tracing-subscriber = "0.3"

[target.'cfg(target_os = "android")'.dependencies]
blinc_platform_android = {{ path = "{blinc_path}/extensions/blinc_platform_android" }}
android-activity = {{ version = "0.6", features = ["native-activity"] }}
log = "0.4"
android_logger = "0.14"

[target.'cfg(target_os = "ios")'.dependencies]
blinc_platform_ios = {{ path = "{blinc_path}/extensions/blinc_platform_ios" }}

[target.'cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))'.dependencies]
blinc_platform_desktop = {{ path = "{blinc_path}/extensions/blinc_platform_desktop" }}

[features]
default = ["desktop"]
desktop = []
android = []
ios = []

[profile.release]
lto = "thin"
opt-level = "z"
strip = true

[profile.dev]
opt-level = 1

[package.metadata.android]
package = "{org}.{package_name}"
apk_label = "{name}"
target_sdk_version = 34
min_sdk_version = 24

[package.metadata.android.application]
theme = "@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen"
"#
        ),
    )?;

    // Create src/main.rs
    fs::write(
        path.join("src/main.rs"),
        format!(
            r#"//! {name}
//!
//! A Blinc UI application with desktop, Android, and iOS support.

use blinc_app::prelude::*;
use blinc_app::windowed::{{WindowedApp, WindowedContext}};
use blinc_core::reactive::State;

/// Counter button with stateful hover/press states
fn counter_button(label: &str, count: State<i32>, delta: i32) -> impl ElementBuilder {{
    let label = label.to_string();

    let count = count.clone();
    stateful::<ButtonState>()
        .on_state(move |ctx| {{
            let bg = match ctx.state() {{
                ButtonState::Idle => Color::rgba(0.3, 0.3, 0.4, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.4, 0.5, 1.0),
                ButtonState::Pressed => Color::rgba(0.2, 0.2, 0.3, 1.0),
                ButtonState::Disabled => Color::rgba(0.2, 0.2, 0.2, 0.5),
            }};

            div()
                .w(80.0)
                .h(50.0)
                .rounded(8.0)
                .bg(bg)
                .items_center()
                .justify_center()
                .cursor(CursorStyle::Pointer)
                .child(text(&label).size(24.0).color(Color::WHITE))
        }})
        .on_click(move |_| {{
            count.set(count.get() + delta);
        }})
}}

/// Counter display that reacts to count changes
fn counter_display(count: State<i32>) -> impl ElementBuilder {{
    stateful::<NoState>()
        .deps([count.signal_id()])
        .on_state(move |_ctx| {{
            div().child(
                text(format!("Count: {{}}", count.get()))
                    .size(48.0)
                    .color(Color::rgba(0.4, 0.8, 1.0, 1.0)),
            )
        }})
}}

/// Main application UI
fn app_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {{
    let count = ctx.use_state_keyed("count", || 0i32);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(20.0)
        .child(
            text("{name}")
                .size(32.0)
                .color(Color::WHITE),
        )
        .child(counter_display(count.clone()))
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(counter_button("-", count.clone(), -1))
                .child(counter_button("+", count.clone(), 1)),
        )
}}

// =============================================================================
// Desktop Entry Point
// =============================================================================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn main() -> Result<()> {{
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {{
        title: "{name}".to_string(),
        width: 400,
        height: 600,
        ..Default::default()
    }};

    WindowedApp::run(config, |ctx| app_ui(ctx))
}}

// =============================================================================
// Android Entry Point
// =============================================================================

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {{
    use android_logger::Config;
    use log::LevelFilter;

    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Info)
            .with_tag("{package_name}"),
    );

    log::info!("Starting {name} on Android");

    blinc_app::AndroidApp::run(app, |ctx| app_ui(ctx)).expect("Failed to run Android app");
}}

#[cfg(target_os = "android")]
fn main() {{}}

// =============================================================================
// iOS Entry Point
// =============================================================================

#[cfg(target_os = "ios")]
fn main() {{}}
"#
        ),
    )?;

    // Create blinc.toml
    fs::write(
        path.join("blinc.toml"),
        format!(
            r#"# Blinc Project Configuration (Rust)
# Generated by: blinc new --rust

[project]
name = "{name}"
version = "0.1.0"
template = "rust"
entry = "Cargo.toml"

[targets]
default = "desktop"
supported = ["desktop", "android", "ios"]

[targets.desktop]
enabled = true
command = "cargo run --features desktop"

[targets.android]
enabled = true
platform_dir = "platforms/android"

[targets.ios]
enabled = true
platform_dir = "platforms/ios"

[build]
blinc_path = "{blinc_path}"
"#
        ),
    )?;

    // Create Android platform files
    create_rust_android_files(path, name, &package_name, org)?;

    // Create iOS platform files
    create_rust_ios_files(path, name, &package_name, org)?;

    // Create README
    fs::write(
        path.join("README.md"),
        format!(
            r#"# {name}

A Blinc UI application with cross-platform support for desktop, Android, and iOS.

## Quick Start

### Desktop

```bash
cargo run --features desktop
```

### Android

```bash
# Build Rust library
cargo ndk -t arm64-v8a build --lib

# Build and install APK
cd platforms/android
./gradlew installDebug
```

### iOS

```bash
# Build Rust library
cargo lipo --release

# Open Xcode project and run
```

## Project Structure

```
{name}/
├── Cargo.toml           # Rust project configuration
├── blinc.toml           # Blinc toolchain configuration
├── src/
│   └── main.rs          # Application code
└── platforms/
    ├── android/         # Android Gradle project
    └── ios/             # iOS Swift files
```
"#
        ),
    )?;

    // Create .gitignore
    fs::write(
        path.join(".gitignore"),
        r#"# Rust
/target/
Cargo.lock

# Android
/platforms/android/.gradle/
/platforms/android/build/
/platforms/android/app/build/
/platforms/android/app/src/main/jniLibs/
*.apk

# iOS
/platforms/ios/build/
*.xcworkspace
*.xcuserdata

# IDE
.idea/
.vscode/
*.swp

# OS
.DS_Store
"#,
    )?;

    Ok(())
}

fn create_rust_android_files(path: &Path, name: &str, package_name: &str, org: &str) -> Result<()> {
    let android_path = path.join("platforms/android");

    // settings.gradle.kts
    fs::write(
        android_path.join("settings.gradle.kts"),
        format!(
            r#"pluginManagement {{
    repositories {{
        google()
        mavenCentral()
        gradlePluginPortal()
    }}
}}

dependencyResolutionManagement {{
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {{
        google()
        mavenCentral()
    }}
}}

rootProject.name = "{name}"
include(":app")
"#
        ),
    )?;

    // build.gradle.kts (root)
    fs::write(
        android_path.join("build.gradle.kts"),
        r#"plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.22" apply false
}

tasks.register("buildRust") {
    description = "Build Rust library for Android"
    group = "rust"

    doLast {
        exec {
            workingDir = file("../..")
            commandLine("cargo", "ndk", "-t", "arm64-v8a", "build", "--lib")
        }
    }
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
    namespace = "{org}.{package_name}"
    compileSdk = 34

    defaultConfig {{
        applicationId = "{org}.{package_name}"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"

        ndk {{
            abiFilters += listOf("arm64-v8a")
        }}
    }}

    buildTypes {{
        release {{
            isMinifyEnabled = false
        }}
    }}

    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }}

    kotlinOptions {{
        jvmTarget = "1.8"
    }}

    sourceSets {{
        getByName("main") {{
            jniLibs.srcDirs("src/main/jniLibs")
        }}
    }}
}}

dependencies {{
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.appcompat:appcompat:1.6.1")
}}

tasks.register<Copy>("copyRustLibs") {{
    val rustTargetDir = file("../../../../target")
    val jniLibsDir = file("src/main/jniLibs")

    from("$rustTargetDir/aarch64-linux-android/debug") {{
        include("lib{package_name}.so")
        into("arm64-v8a")
    }}

    into(jniLibsDir)
}}

tasks.named("preBuild") {{
    dependsOn("copyRustLibs")
}}
"#
        ),
    )?;

    // AndroidManifest.xml
    fs::create_dir_all(android_path.join("app/src/main"))?;
    fs::write(
        android_path.join("app/src/main/AndroidManifest.xml"),
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-feature android:glEsVersion="0x00030000" android:required="true" />
    <uses-permission android:name="android.permission.VIBRATE" />

    <application
        android:allowBackup="true"
        android:label="{name}"
        android:theme="@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen"
        android:hardwareAccelerated="true">

        <activity
            android:name=".MainActivity"
            android:configChanges="orientation|screenSize|screenLayout|keyboardHidden"
            android:exported="true">

            <meta-data
                android:name="android.app.lib_name"
                android:value="{package_name}" />

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

    // MainActivity.kt
    let kotlin_path = android_path.join("app/src/main/kotlin/com/blinc");
    fs::create_dir_all(&kotlin_path)?;
    fs::write(
        kotlin_path.join("MainActivity.kt"),
        format!(
            r#"package {org}.{package_name}

import android.app.NativeActivity
import android.os.Bundle

class MainActivity : NativeActivity() {{
    companion object {{
        init {{
            System.loadLibrary("{package_name}")
        }}
    }}

    override fun onCreate(savedInstanceState: Bundle?) {{
        super.onCreate(savedInstanceState)
    }}
}}
"#
        ),
    )?;

    // gradle.properties
    fs::write(
        android_path.join("gradle.properties"),
        r#"org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
android.useAndroidX=true
kotlin.code.style=official
"#,
    )?;

    Ok(())
}

fn create_rust_ios_files(path: &Path, name: &str, package_name: &str, org: &str) -> Result<()> {
    let ios_path = path.join("platforms/ios");
    let app_path = ios_path.join("BlincApp");
    let xcodeproj = ios_path.join("BlincApp.xcodeproj");

    fs::create_dir_all(&app_path)?;
    fs::create_dir_all(&xcodeproj)?;
    fs::create_dir_all(ios_path.join("libs/device"))?;
    fs::create_dir_all(ios_path.join("libs/simulator"))?;

    // AppDelegate.swift
    fs::write(
        app_path.join("AppDelegate.swift"),
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
"#,
    )?;

    // BlincMetalView.swift
    fs::write(
        app_path.join("BlincMetalView.swift"),
        r#"import UIKit
import Metal
import QuartzCore

class BlincMetalView: UIView {
    private(set) var metalDevice: MTLDevice?
    private(set) var commandQueue: MTLCommandQueue?

    var metalLayer: CAMetalLayer { return layer as! CAMetalLayer }
    var preferredFramesPerSecond: Int = 60

    override class var layerClass: AnyClass { return CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    private func commonInit() {
        guard let device = MTLCreateSystemDefaultDevice() else {
            print("Error: Metal is not supported on this device")
            return
        }
        metalDevice = device
        commandQueue = device.makeCommandQueue()
        metalLayer.device = device
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = true
        metalLayer.contentsScale = UIScreen.main.scale
        if #available(iOS 10.3, *) { metalLayer.displaySyncEnabled = true }
        isOpaque = true
        backgroundColor = .black
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let scale = UIScreen.main.scale
        let drawableSize = CGSize(width: bounds.width * scale, height: bounds.height * scale)
        if metalLayer.drawableSize != drawableSize { metalLayer.drawableSize = drawableSize }
    }

    func nextDrawable() -> CAMetalDrawable? { return metalLayer.nextDrawable() }
    var drawableSize: CGSize { return metalLayer.drawableSize }
    var pixelFormat: MTLPixelFormat { return metalLayer.pixelFormat }
}
"#,
    )?;

    // BlincViewController.swift
    fs::write(
        app_path.join("BlincViewController.swift"),
        r#"import UIKit
import MetalKit

class BlincViewController: UIViewController {
    private var metalView: BlincMetalView!
    private var displayLink: CADisplayLink?
    private var renderContext: OpaquePointer?
    private var gpuRenderer: OpaquePointer?
    private var isVisible = false
    private var touchIds: [ObjectIdentifier: UInt64] = [:]
    private var nextTouchId: UInt64 = 1

    override func viewDidLoad() {
        super.viewDidLoad()
        metalView = BlincMetalView(frame: view.bounds)
        metalView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(metalView)
        initializeBlinc()
        setupDisplayLink()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        isVisible = true
        displayLink?.isPaused = false
        if let ctx = renderContext { blinc_set_focused(ctx, true) }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        isVisible = false
        displayLink?.isPaused = true
        if let ctx = renderContext { blinc_set_focused(ctx, false) }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let scale = UIScreen.main.scale
        let width = UInt32(view.bounds.width * scale)
        let height = UInt32(view.bounds.height * scale)
        if let ctx = renderContext { blinc_update_size(ctx, width, height, Double(scale)) }
        if let gpu = gpuRenderer { blinc_gpu_resize(gpu, width, height) }
    }

    deinit {
        displayLink?.invalidate()
        if let gpu = gpuRenderer { blinc_destroy_gpu(gpu) }
        if let ctx = renderContext { blinc_destroy_context(ctx) }
    }

    private func initializeBlinc() {
        let scale = UIScreen.main.scale
        let width = UInt32(view.bounds.width * scale)
        let height = UInt32(view.bounds.height * scale)
        guard let ctx = blinc_create_context(width, height, Double(scale)) else {
            print("Error: Failed to create Blinc render context")
            return
        }
        renderContext = ctx
        let metalLayer = metalView.metalLayer
        guard let gpu = blinc_init_gpu(ctx, Unmanaged.passUnretained(metalLayer).toOpaque(), width, height) else {
            print("Error: Failed to initialize Blinc GPU renderer")
            return
        }
        gpuRenderer = gpu
        print("Blinc initialized: \(width)x\(height) @ \(scale)x")
    }

    private func setupDisplayLink() {
        displayLink = CADisplayLink(target: self, selector: #selector(displayLinkFired))
        if #available(iOS 15.0, *) {
            displayLink?.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 60)
        } else {
            displayLink?.preferredFramesPerSecond = 60
        }
        displayLink?.add(to: .main, forMode: .common)
    }

    @objc private func displayLinkFired() {
        guard isVisible, let ctx = renderContext, let gpu = gpuRenderer else { return }
        guard blinc_needs_render(ctx) else { return }
        blinc_build_frame(ctx)
        _ = blinc_render_frame(gpu)
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = renderContext else { return }
        for touch in touches {
            let point = touch.location(in: view)
            blinc_handle_touch(ctx, getTouchId(for: touch), Float(point.x), Float(point.y), 0)
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = renderContext else { return }
        for touch in touches {
            let point = touch.location(in: view)
            blinc_handle_touch(ctx, getTouchId(for: touch), Float(point.x), Float(point.y), 1)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = renderContext else { return }
        for touch in touches {
            let point = touch.location(in: view)
            blinc_handle_touch(ctx, getTouchId(for: touch), Float(point.x), Float(point.y), 2)
            removeTouchId(for: touch)
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = renderContext else { return }
        for touch in touches {
            blinc_handle_touch(ctx, getTouchId(for: touch), Float(touch.location(in: view).x), Float(touch.location(in: view).y), 3)
            removeTouchId(for: touch)
        }
    }

    private func getTouchId(for touch: UITouch) -> UInt64 {
        let id = ObjectIdentifier(touch)
        if let existing = touchIds[id] { return existing }
        let newId = nextTouchId
        nextTouchId += 1
        touchIds[id] = newId
        return newId
    }

    private func removeTouchId(for touch: UITouch) { touchIds.removeValue(forKey: ObjectIdentifier(touch)) }
    override var prefersStatusBarHidden: Bool { return true }
    override var preferredStatusBarStyle: UIStatusBarStyle { return .lightContent }
    override var preferredScreenEdgesDeferringSystemGestures: UIRectEdge { return .all }
}
"#,
    )?;

    // Blinc-Bridging-Header.h
    fs::write(
        app_path.join("Blinc-Bridging-Header.h"),
        format!(
            r#"// Blinc-Bridging-Header.h - Rust FFI declarations for {name}

#ifndef Blinc_Bridging_Header_h
#define Blinc_Bridging_Header_h

#include <stdint.h>
#include <stdbool.h>

typedef struct IOSRenderContext IOSRenderContext;
typedef struct WindowedContext WindowedContext;
typedef struct IOSGpuRenderer IOSGpuRenderer;
typedef void (*UIBuilderFn)(WindowedContext* ctx);

// Context Lifecycle
IOSRenderContext* blinc_create_context(uint32_t width, uint32_t height, double scale_factor);
void blinc_destroy_context(IOSRenderContext* ctx);

// Rendering
bool blinc_needs_render(IOSRenderContext* ctx);
void blinc_set_ui_builder(UIBuilderFn builder);
void blinc_build_frame(IOSRenderContext* ctx);
bool blinc_tick_animations(IOSRenderContext* ctx);

// Window Size
void blinc_update_size(IOSRenderContext* ctx, uint32_t width, uint32_t height, double scale_factor);
float blinc_get_width(IOSRenderContext* ctx);
float blinc_get_height(IOSRenderContext* ctx);
uint32_t blinc_get_physical_width(IOSRenderContext* ctx);
uint32_t blinc_get_physical_height(IOSRenderContext* ctx);
double blinc_get_scale_factor(IOSRenderContext* ctx);

// Input Events
void blinc_handle_touch(IOSRenderContext* ctx, uint64_t touch_id, float x, float y, int32_t phase);
void blinc_set_focused(IOSRenderContext* ctx, bool focused);

// State Management
void blinc_mark_dirty(IOSRenderContext* ctx);
void blinc_clear_dirty(IOSRenderContext* ctx);
WindowedContext* blinc_get_windowed_context(IOSRenderContext* ctx);

// GPU Rendering
IOSGpuRenderer* blinc_init_gpu(IOSRenderContext* ctx, void* metal_layer, uint32_t width, uint32_t height);
void blinc_gpu_resize(IOSGpuRenderer* gpu, uint32_t width, uint32_t height);
bool blinc_render_frame(IOSGpuRenderer* gpu);
void blinc_destroy_gpu(IOSGpuRenderer* gpu);

#endif
"#
        ),
    )?;

    // Info.plist
    fs::write(
        app_path.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>$(DEVELOPMENT_LANGUAGE)</string>
    <key>CFBundleDisplayName</key>
    <string>{name}</string>
    <key>CFBundleExecutable</key>
    <string>$(EXECUTABLE_NAME)</string>
    <key>CFBundleIdentifier</key>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$(PRODUCT_NAME)</string>
    <key>CFBundlePackageType</key>
    <string>$(PRODUCT_BUNDLE_PACKAGE_TYPE)</string>
    <key>CFBundleShortVersionString</key>
    <string>$(MARKETING_VERSION)</string>
    <key>CFBundleVersion</key>
    <string>$(CURRENT_PROJECT_VERSION)</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
    <key>UILaunchScreen</key>
    <dict/>
    <key>UIRequiredDeviceCapabilities</key>
    <array>
        <string>metal</string>
        <string>arm64</string>
    </array>
    <key>UIRequiresFullScreen</key>
    <true/>
    <key>UIStatusBarHidden</key>
    <true/>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
</dict>
</plist>
"#
        ),
    )?;

    // project.pbxproj - Xcode project file
    fs::write(
        xcodeproj.join("project.pbxproj"),
        format!(
            r#"// !$*UTF8*$!
{{
	archiveVersion = 1;
	classes = {{}};
	objectVersion = 56;
	objects = {{

/* Begin PBXBuildFile section */
		A1000001 /* AppDelegate.swift in Sources */ = {{isa = PBXBuildFile; fileRef = A2000001; }};
		A1000002 /* BlincViewController.swift in Sources */ = {{isa = PBXBuildFile; fileRef = A2000002; }};
		A1000003 /* BlincMetalView.swift in Sources */ = {{isa = PBXBuildFile; fileRef = A2000003; }};
		A1000004 /* lib{package_name}.a in Frameworks */ = {{isa = PBXBuildFile; fileRef = A2000004; }};
		A1000005 /* Metal.framework in Frameworks */ = {{isa = PBXBuildFile; fileRef = A2000005; }};
		A1000006 /* MetalKit.framework in Frameworks */ = {{isa = PBXBuildFile; fileRef = A2000006; }};
		A1000007 /* QuartzCore.framework in Frameworks */ = {{isa = PBXBuildFile; fileRef = A2000007; }};
/* End PBXBuildFile section */

/* Begin PBXFileReference section */
		A2000001 /* AppDelegate.swift */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = AppDelegate.swift; sourceTree = "<group>"; }};
		A2000002 /* BlincViewController.swift */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = BlincViewController.swift; sourceTree = "<group>"; }};
		A2000003 /* BlincMetalView.swift */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = BlincMetalView.swift; sourceTree = "<group>"; }};
		A2000004 /* lib{package_name}.a */ = {{isa = PBXFileReference; lastKnownFileType = archive.ar; name = "lib{package_name}.a"; path = "libs/simulator/lib{package_name}.a"; sourceTree = "<group>"; }};
		A2000005 /* Metal.framework */ = {{isa = PBXFileReference; lastKnownFileType = wrapper.framework; name = Metal.framework; path = System/Library/Frameworks/Metal.framework; sourceTree = SDKROOT; }};
		A2000006 /* MetalKit.framework */ = {{isa = PBXFileReference; lastKnownFileType = wrapper.framework; name = MetalKit.framework; path = System/Library/Frameworks/MetalKit.framework; sourceTree = SDKROOT; }};
		A2000007 /* QuartzCore.framework */ = {{isa = PBXFileReference; lastKnownFileType = wrapper.framework; name = QuartzCore.framework; path = System/Library/Frameworks/QuartzCore.framework; sourceTree = SDKROOT; }};
		A2000008 /* Info.plist */ = {{isa = PBXFileReference; lastKnownFileType = text.plist.xml; path = Info.plist; sourceTree = "<group>"; }};
		A2000009 /* Blinc-Bridging-Header.h */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.c.h; path = "Blinc-Bridging-Header.h"; sourceTree = "<group>"; }};
		A3000001 /* BlincApp.app */ = {{isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = BlincApp.app; sourceTree = BUILT_PRODUCTS_DIR; }};
/* End PBXFileReference section */

/* Begin PBXFrameworksBuildPhase section */
		A4000001 /* Frameworks */ = {{
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (A1000004, A1000005, A1000006, A1000007);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXFrameworksBuildPhase section */

/* Begin PBXGroup section */
		A5000001 = {{
			isa = PBXGroup;
			children = (A5000002, A5000003, A5000004);
			sourceTree = "<group>";
		}};
		A5000002 /* BlincApp */ = {{
			isa = PBXGroup;
			children = (A2000001, A2000002, A2000003, A2000008, A2000009);
			path = BlincApp;
			sourceTree = "<group>";
		}};
		A5000003 /* Frameworks */ = {{
			isa = PBXGroup;
			children = (A2000004, A2000005, A2000006, A2000007);
			name = Frameworks;
			sourceTree = "<group>";
		}};
		A5000004 /* Products */ = {{
			isa = PBXGroup;
			children = (A3000001);
			name = Products;
			sourceTree = "<group>";
		}};
/* End PBXGroup section */

/* Begin PBXNativeTarget section */
		A6000001 /* BlincApp */ = {{
			isa = PBXNativeTarget;
			buildConfigurationList = A8000001;
			buildPhases = (A6000002, A4000001);
			buildRules = ();
			dependencies = ();
			name = BlincApp;
			productName = BlincApp;
			productReference = A3000001;
			productType = "com.apple.product-type.application";
		}};
/* End PBXNativeTarget section */

/* Begin PBXProject section */
		A7000001 /* Project object */ = {{
			isa = PBXProject;
			attributes = {{
				BuildIndependentTargetsInParallel = 1;
				LastSwiftUpdateCheck = 1500;
				LastUpgradeCheck = 1500;
			}};
			buildConfigurationList = A8000002;
			compatibilityVersion = "Xcode 14.0";
			developmentRegion = en;
			hasScannedForEncodings = 0;
			knownRegions = (en, Base);
			mainGroup = A5000001;
			productRefGroup = A5000004;
			projectDirPath = "";
			projectRoot = "";
			targets = (A6000001);
		}};
/* End PBXProject section */

/* Begin PBXSourcesBuildPhase section */
		A6000002 /* Sources */ = {{
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (A1000001, A1000002, A1000003);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXSourcesBuildPhase section */

/* Begin XCBuildConfiguration section */
		A9000001 /* Debug */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;
				CODE_SIGN_STYLE = Automatic;
				CURRENT_PROJECT_VERSION = 1;
				INFOPLIST_FILE = BlincApp/Info.plist;
				IPHONEOS_DEPLOYMENT_TARGET = 15.0;
				LD_RUNPATH_SEARCH_PATHS = ("$(inherited)", "@executable_path/Frameworks");
				LIBRARY_SEARCH_PATHS = ("$(inherited)", "$(PROJECT_DIR)/libs/simulator");
				MARKETING_VERSION = 1.0;
				OTHER_LDFLAGS = ("-l{package_name}");
				PRODUCT_BUNDLE_IDENTIFIER = "{org}.{package_name}";
				PRODUCT_NAME = "$(TARGET_NAME)";
				SDKROOT = iphoneos;
				SUPPORTED_PLATFORMS = "iphonesimulator iphoneos";
				SWIFT_OBJC_BRIDGING_HEADER = "BlincApp/Blinc-Bridging-Header.h";
				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			}};
			name = Debug;
		}};
		A9000002 /* Release */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;
				CODE_SIGN_STYLE = Automatic;
				CURRENT_PROJECT_VERSION = 1;
				INFOPLIST_FILE = BlincApp/Info.plist;
				IPHONEOS_DEPLOYMENT_TARGET = 15.0;
				LD_RUNPATH_SEARCH_PATHS = ("$(inherited)", "@executable_path/Frameworks");
				LIBRARY_SEARCH_PATHS = ("$(inherited)", "$(PROJECT_DIR)/libs/device");
				MARKETING_VERSION = 1.0;
				OTHER_LDFLAGS = ("-l{package_name}");
				PRODUCT_BUNDLE_IDENTIFIER = "{org}.{package_name}";
				PRODUCT_NAME = "$(TARGET_NAME)";
				SDKROOT = iphoneos;
				SUPPORTED_PLATFORMS = "iphonesimulator iphoneos";
				SWIFT_OBJC_BRIDGING_HEADER = "BlincApp/Blinc-Bridging-Header.h";
				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			}};
			name = Release;
		}};
		A9000003 /* Debug */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				CLANG_WARN_BLOCK_CAPTURE_AUTORELEASING = YES;
				CLANG_WARN_BOOL_CONVERSION = YES;
				CLANG_WARN_COMMA = YES;
				CLANG_WARN_CONSTANT_CONVERSION = YES;
				CLANG_WARN_DEPRECATED_OBJC_IMPLEMENTATIONS = YES;
				CLANG_WARN_DIRECT_OBJC_ISA_USAGE = YES_ERROR;
				CLANG_WARN_EMPTY_BODY = YES;
				CLANG_WARN_ENUM_CONVERSION = YES;
				CLANG_WARN_INFINITE_RECURSION = YES;
				CLANG_WARN_INT_CONVERSION = YES;
				CLANG_WARN_OBJC_ROOT_CLASS = YES_ERROR;
				CLANG_WARN_UNREACHABLE_CODE = YES;
				COPY_PHASE_STRIP = NO;
				DEBUG_INFORMATION_FORMAT = dwarf;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				ENABLE_TESTABILITY = YES;
				GCC_C_LANGUAGE_STANDARD = gnu17;
				GCC_DYNAMIC_NO_PIC = NO;
				GCC_NO_COMMON_BLOCKS = YES;
				GCC_OPTIMIZATION_LEVEL = 0;
				GCC_WARN_64_TO_32_BIT_CONVERSION = YES;
				GCC_WARN_ABOUT_RETURN_TYPE = YES_ERROR;
				GCC_WARN_UNDECLARED_SELECTOR = YES;
				GCC_WARN_UNINITIALIZED_AUTOS = YES_AGGRESSIVE;
				GCC_WARN_UNUSED_FUNCTION = YES;
				GCC_WARN_UNUSED_VARIABLE = YES;
				MTL_ENABLE_DEBUG_INFO = INCLUDE_SOURCE;
				MTL_FAST_MATH = YES;
				ONLY_ACTIVE_ARCH = YES;
				SDKROOT = iphoneos;
				SWIFT_ACTIVE_COMPILATION_CONDITIONS = "DEBUG $(inherited)";
				SWIFT_OPTIMIZATION_LEVEL = "-Onone";
			}};
			name = Debug;
		}};
		A9000004 /* Release */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				CLANG_WARN_BLOCK_CAPTURE_AUTORELEASING = YES;
				CLANG_WARN_BOOL_CONVERSION = YES;
				CLANG_WARN_COMMA = YES;
				CLANG_WARN_CONSTANT_CONVERSION = YES;
				CLANG_WARN_DEPRECATED_OBJC_IMPLEMENTATIONS = YES;
				CLANG_WARN_DIRECT_OBJC_ISA_USAGE = YES_ERROR;
				CLANG_WARN_EMPTY_BODY = YES;
				CLANG_WARN_ENUM_CONVERSION = YES;
				CLANG_WARN_INFINITE_RECURSION = YES;
				CLANG_WARN_INT_CONVERSION = YES;
				CLANG_WARN_OBJC_ROOT_CLASS = YES_ERROR;
				CLANG_WARN_UNREACHABLE_CODE = YES;
				COPY_PHASE_STRIP = NO;
				DEBUG_INFORMATION_FORMAT = "dwarf-with-dsym";
				ENABLE_NS_ASSERTIONS = NO;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				GCC_C_LANGUAGE_STANDARD = gnu17;
				GCC_NO_COMMON_BLOCKS = YES;
				GCC_WARN_64_TO_32_BIT_CONVERSION = YES;
				GCC_WARN_ABOUT_RETURN_TYPE = YES_ERROR;
				GCC_WARN_UNDECLARED_SELECTOR = YES;
				GCC_WARN_UNINITIALIZED_AUTOS = YES_AGGRESSIVE;
				GCC_WARN_UNUSED_FUNCTION = YES;
				GCC_WARN_UNUSED_VARIABLE = YES;
				MTL_ENABLE_DEBUG_INFO = NO;
				MTL_FAST_MATH = YES;
				SDKROOT = iphoneos;
				SWIFT_COMPILATION_MODE = wholemodule;
				VALIDATE_PRODUCT = YES;
			}};
			name = Release;
		}};
/* End XCBuildConfiguration section */

/* Begin XCConfigurationList section */
		A8000001 /* Build configuration list for PBXNativeTarget "BlincApp" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (A9000001, A9000002);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
		A8000002 /* Build configuration list for PBXProject "BlincApp" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (A9000003, A9000004);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
/* End XCConfigurationList section */

	}};
	rootObject = A7000001;
}}
"#
        ),
    )?;

    // build-ios.sh
    fs::write(
        ios_path.join("build-ios.sh"),
        format!(
            r#"#!/bin/bash
# Build script for {name} iOS

set -e

SCRIPT_DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
cd "$SCRIPT_DIR/../.."

LIB_NAME="lib{package_name}.a"
TARGET_ARM64="aarch64-apple-ios"
TARGET_SIM_ARM64="aarch64-apple-ios-sim"

BUILD_MODE="${{1:-debug}}"
CARGO_FLAGS=""
TARGET_DIR="debug"

if [ "$BUILD_MODE" = "release" ]; then
    CARGO_FLAGS="--release"
    TARGET_DIR="release"
    echo "Building in RELEASE mode..."
else
    echo "Building in DEBUG mode..."
fi

# Ensure iOS targets are installed
if ! rustup target list --installed | grep -q "$TARGET_ARM64"; then
    rustup target add "$TARGET_ARM64"
fi
if ! rustup target list --installed | grep -q "$TARGET_SIM_ARM64"; then
    rustup target add "$TARGET_SIM_ARM64"
fi

# Build for device and simulator
echo "Building for device ($TARGET_ARM64)..."
cargo build --features ios $CARGO_FLAGS --target "$TARGET_ARM64"

echo "Building for simulator ($TARGET_SIM_ARM64)..."
cargo build --features ios $CARGO_FLAGS --target "$TARGET_SIM_ARM64"

# Copy libraries
LIBS_DIR="$SCRIPT_DIR/libs"
mkdir -p "$LIBS_DIR/device" "$LIBS_DIR/simulator"
cp "target/$TARGET_ARM64/$TARGET_DIR/$LIB_NAME" "$LIBS_DIR/device/"
cp "target/$TARGET_SIM_ARM64/$TARGET_DIR/$LIB_NAME" "$LIBS_DIR/simulator/"

echo ""
echo "Build complete! Libraries at:"
echo "  Device:    $LIBS_DIR/device/$LIB_NAME"
echo "  Simulator: $LIBS_DIR/simulator/$LIB_NAME"
echo ""
echo "Open BlincApp.xcodeproj in Xcode to build and run."
"#
        ),
    )?;

    // README
    fs::write(
        ios_path.join("README.md"),
        format!(
            r#"# {name} - iOS

## Building

1. Build the Rust library:

```bash
cd platforms/ios
./build-ios.sh        # Debug build
./build-ios.sh release  # Release build
```

2. Open in Xcode:

```bash
open BlincApp.xcodeproj
```

3. Select target and run (Cmd+R)

## Requirements

- Xcode 15+
- iOS 15+ deployment target
- Rust iOS targets: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`
"#
        ),
    )?;

    Ok(())
}
