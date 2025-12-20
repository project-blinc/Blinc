# counter - Windows

Windows platform files for counter.

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
