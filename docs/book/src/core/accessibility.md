# Accessibility

Blinc provides keyboard navigation through its event router and a semantic
snapshot for platform accessibility adapters. These are separate capabilities:
Tab navigation alone does not expose controls to a screen reader.

## Names and roles

Standard buttons use their visible text as their accessible name. A checkbox
combines its box and label into one accessible element. Text fields expose their
current value separately from their name; `cn::input(&data).label("Name")`
supplies the accessible name as well as the visible label.

`cn::slider(value).label("Volume")` exposes one adjustable control. Its value
follows the bound number, and assistive increment/decrement uses the configured
step, bounds and change callback without requiring keyboard focus.

For a core text input, bind a name explicitly. It accepts a string, Signal or
Computed:

```rust,ignore
use blinc::layout::widgets::text_input::{text_input, text_input_data};

let data = text_input_data();
let field = text_input(&data).accessibility_label("Search projects");
```

Custom elements can declare `role`, `aria-label`, `aria-labelledby`,
`aria-description`, `aria-disabled`, `aria-selected`, `aria-checked` and
`aria-expanded` through `.attr(...)`. An `aria-labelledby` value contains the IDs
of elements whose text supplies the name. Give icon-only controls a name:

```rust,ignore
use blinc::layout::prelude::*;

let close = div()
    .attr("role", "button")
    .attr("tabindex", "0")
    .attr("aria-label", "Close panel")
    .child(text("×"));
```

The role describes behavior; it does not add a click handler or styling. Keep
the usual event handler and CSS for a custom control. `aria-hidden="true"`
removes a decorative subtree from the semantic snapshot. Disabled controls
remain discoverable, with their disabled state exposed, and reject actions.

## Platform integration

The iOS host's `BlincAccessibility.swift` creates native accessibility elements
for GPU-rendered content. It retains their objects by Blinc stable node ID,
updates their names, values, states and bounds after UI changes, and dispatches
activation through the widget's existing handlers. Native text-field proxies
provide editing to UIKit while Blinc continues to draw the field. Return uses
the existing key handler, including `cn::input(...).on_submit(|shift| ...)`.
Editing uses the widget's document, constraints and change callback, including
native marked-text and candidate replacement. Accessibility snapshots contain
masked password values; the keyboard connection receives the private document
separately. See [mobile input methods](../widgets/inputs.md#mobile-input-methods)
for the editing and commit behavior.

New CLI-generated iOS projects include this adapter. Existing hosts must add
`BlincAccessibility.swift` from `extensions/blinc_platform_ios/swift/`, include
the accessibility and IME declarations from its bridging header, and retain a
`BlincAccessibility(host: view, context: ctx)` instance. Call `synchronize()` after
`blinc_build_frame(ctx)` and `disconnect()` before destroying the context.
Update the native keyboard bridge alongside the host so it reuses an accessible
text field when that field owns the keyboard.

The iOS hosts scale their logical viewport using the user's Dynamic Type
preference. Text, containing controls, accessibility frames, touch coordinates
and keyboard insets use the same scale. The native editing proxy uses scaled
font metrics too. Keep layouts responsive and provide scrolling when larger
content no longer fits; fixed-size content can still overflow.

The automated iOS fixture covers buttons, a labelled single-line input,
checkbox state, adjustable slider values, disabled actions, stable native
objects and larger-text geometry. The snapshot currently exposes visible
content. Assistive scrolling through offscreen content, providers for other editable widgets, advanced
composite-widget semantics and screen-reader focus restoration across overlays
still need platform validation and integration; this adapter is not a claim of
complete library coverage.

The browser canvas, Android and desktop runners still require platform adapters
for the semantic snapshot. A role stored in a Blinc tree is not automatically
an HTML ARIA element or an operating-system accessibility node.

## Validation

Use `python3 scripts/test_ios_accessibility.py` in the repository to build the
current-source fixture and run its semantic/action assertions, larger-text
geometry checks, native assistive-action tests and Apple's audit in one dedicated
iOS Simulator. The result bundle includes rendered screenshots and the native
accessibility hierarchy. The runner shuts down its Simulator on completion and
does not start an Android emulator.

Keep explicit assertions for names, roles, values and actions alongside general
audits: a general audit can pass when no custom controls are exposed. Physical
iOS VoiceOver validation remains necessary; Simulator results do not establish
real-device screen-reader behavior.
