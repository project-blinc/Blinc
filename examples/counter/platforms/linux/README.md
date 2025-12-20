# counter - Linux

Linux platform files for counter.

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
cp counter.desktop ~/.local/share/applications/
```

## Configuration

- `counter.desktop` - Desktop entry for app launchers
- `counter.metainfo.xml` - AppStream metadata for software centers
