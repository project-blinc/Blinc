# Plugins

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
