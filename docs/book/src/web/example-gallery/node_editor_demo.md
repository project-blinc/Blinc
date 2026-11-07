# Node-editor demo — pre-wired graph with three node types, typed

What it shows:

- **Metadata-driven**: nodes render from declarative
  `NodeTemplate`s. The editor never hardcodes shape — adding a new
  template adds a new node type.
- **Generic over port kind**: hosts impl `PortKind` for their own
  port-type enum. Here, [`DemoPort`] models `Number` / `String` /
  `Boolean`; the editor delegates compatibility to
  `DemoPort::compatible_with`.
- **Theme-aware**: chrome (background, header, border, badge) pulls
  from `ThemeState`. Switch theme bundles to recolor; squircle
  profile, shadows, typography, and spacing tokens flow through.
- **Group with badge**: two nodes are wrapped in a group with a
  status badge in the header.
- **Drag-to-connect**: drag from an output port to an input port;
  the validator accepts compatible kinds.
- **Pan + zoom + selection**: inherited from `blinc_canvas_kit`.

<div class="blinc-example" data-example="node_editor_demo" data-title="Blinc node_editor_demo example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/node_editor_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/node_editor_demo.rs)
