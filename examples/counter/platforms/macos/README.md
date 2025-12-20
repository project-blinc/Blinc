# counter - macOS

macOS platform files for counter.

## Building

```bash
# From project root
blinc build --target macos --release
```

## App Bundle Structure

The build will create `counter.app` with:
```
counter.app/
├── Contents/
│   ├── Info.plist
│   ├── MacOS/
│   │   └── counter     # Executable
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
