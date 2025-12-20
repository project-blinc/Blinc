# counter

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
counter/
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
