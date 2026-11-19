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

## Registering component semantics

Components can supply their own accessibility description. Register it with
`Div::accessibility(...)`, or implement `ElementBuilder::accessibility_semantics()`
for a custom element. Store the descriptor on the element and return an Arc
clone from the trait method. This preserves provider identity across unchanged
builds. The description declares the role, child traversal,
advertised actions and live-region defaults. A live region announces changing
content to a screen reader.

```rust,ignore
use blinc::layout::accessibility::{
    AccessibilityLive, AccessibilityRole, AccessibilitySemantics,
};

let activity = div()
    .accessibility(
        AccessibilitySemantics::new(AccessibilityRole::Custom("activity-feed"))
            .children(true)
            .live(AccessibilityLive::Polite, false),
    )
    .attr("aria-label", "Workspace activity")
    .child(text("Build complete"));
```

Custom roles do not need an entry in a core lookup table. Desktop adapters
expose an unknown custom role as a named group. Use a standard role such as
`Button` or `Slider` when the element has that behavior.

`AccessibilitySemantics::binding(...)` accepts an `AccessibilityBinding` for a
live value and its action handler. This uses the same provider interface as
built-in text fields. Read Signal or Computed values in the provider and apply
assistive edits through the same state and constraints as ordinary input.
The description's `.actions(...)` method declares the supported actions.
An element still needs handlers for the actions that it advertises.

`Div::accessibility_role(...)` is a shorter form for a standard role. A component
can override child traversal with `.children(...)`: containers keep their child
controls, while an atomic control exposes one accessible element. Registration
survives boxed elements and incremental updates. Changes to semantic metadata
are detected even when appearance stays the same.

CN dialog, popover, tooltip and toast builders register their own descriptions.
The shared role types describe standard semantics, rather than CN widget types.
Tag names and `role` attributes remain the fallback for other elements.

## Containers and announcements

The snapshot keeps parent IDs and reading order for menus, trees, toolbars,
dialogs and regions. Checkbox and radio menu items retain distinct roles.
Dialogs use their title as their accessible name and their description as a
hint. An alert dialog has a separate role from an ordinary dialog.

Toasts use `Status` with polite announcements. Error toasts use `Alert` with
assertive announcements. These defaults follow
[WAI-ARIA live-region roles](https://www.w3.org/TR/wai-aria-1.2/#status).
Use `aria-live`, `aria-atomic` and `aria-busy` to override announcement defaults.
Atomic toast messages announce their whole content. Busy messages wait until ready.
Unchanged repaints do not resend the same native update.

## Platform integration

Desktop windows create an AccessKit bridge before they become visible. Semantic
collection starts when a native accessibility client requests the tree. Native
requests reach the UI thread and use the existing widget handlers and focus
router. Updates preserve Blinc stable IDs and send changed native nodes.
Deactivation releases the native-node cache.

The desktop bridge targets macOS accessibility, Windows UI Automation and
Linux AT-SPI. The native AppKit probe queries a hidden window and checks its
hierarchy, actions, editing and object identity. Compilation and semantic tests
do not establish screen-reader behavior on every platform. Test with VoiceOver,
NVDA or Orca for the platform that you ship.

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

The browser canvas and Android runners still require platform adapters
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
