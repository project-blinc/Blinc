# Changelog

All notable changes to `blinc_layout` will be documented in this file.

## [Unreleased]

### Accessibility
- Platform-neutral accessibility snapshots use stable node IDs, expose control names, values and states, and route assistive actions to widget handlers. Text inputs provide reactive accessible names and masked value snapshots.

### Changed
- Scroll containers cull by default: `Scroll::viewport_cull` is on unless set `false`. A child is culled only when its whole subtree, stopping at descendants that clip, lies outside the viewport and overscan, so content overflowing into view stays drawn and clickable; culling does not see past transforms, motions, layout animations, or fixed and sticky children. Culled children are now also left out of text, SVG and image collection and hit testing, not only SDF paint. `RenderTree::draws_outside`, `moves_its_content` and `is_viewport_cull_scroll` are public.
- A node that clips and has no area paints none of its children, so a closed collapsible, accordion section or tree group draws nothing of its content.
- Touch momentum on iOS decays at 2 per second (`ScrollConfig::deceleration` 1000, UIKit's normal deceleration rate) instead of 3, so a flick glides about 1.5 times as far. Other platforms keep 1500.

### Fixed
- What an overlay layer or a Stateful refresh creates while building is disposed when it is superseded: the layer's on its next rebuild, a structural refresh's on the next structural refresh, an in-place refresh's on the next in-place one, and all of them when the node is removed or the tree dropped. They ran outside any owner, so the effects of every overlay ever opened kept running. `queue_subtree_rebuild_owned` queues a rebuild built inside an `Owner`; `PendingSubtreeRebuild` carries it and disposes it if dropped unapplied.
- A region that builds rows in the same update as a bound class or attribute change elsewhere no longer applies that change unseen: a subtree's style pass syncs bound classes only within the subtree, so the other node's transition still runs. Opening a collapsible built on first open turned its caret at once.
- A CSS `transform` transition slides a moved element (FLIP) when it carries an identity transform, as a bound transform at rest does, and an element moved again mid-slide carries on from where it is drawn instead of jumping to its last laid-out place.
- `DivRef::bounds`, `ElementHandle::bounds` and the context's bounds query report an element's position in the window, scrolled as drawn, as pointer events do; they gave it relative to its parent. Reordering tabs away from the window's left edge works in both directions.
- A node built after the first stylesheet pass (a region's new row, say) keeps the layout its class rules give it when a bound class leaves it: a tab added to a strip no longer loses its padding when it stops being the active one.
- A read-only code block's lines scroll sideways in their own scroll container, so its gutter no longer scrolls away with them.
- A tooltip overlay's wrappers take no pointer input.
- A scroll goes to the nearest container under the pointer that scrolls its axis: a horizontal scroller inside a vertical one takes sideways scrolls and lets vertical ones by to its parent.
- A code editor's `on_change` runs after the editor is unlocked, so a callback that reads or edits the editor no longer deadlocks the thread that delivers input.
- A class or attribute bound with `class_when` or `attr_when` is matched only against the rules that name it, and its descendants only when a rule names it in an ancestor position, so building a tree with bound classes no longer matches the whole stylesheet against each one's subtree.
- The scroll offset a frame paints is rounded to whole device pixels instead of whole points. At 3x a coasting scroll advanced two points a frame, then one, which read as judder as it slowed; it now moves in thirds of a point. Hit testing keeps the exact offset.
- A finger that rests for more than 100 ms before lifting no longer launches a fling from the speed of its last movement.
- A CSS font stack is measured and drawn in the first of its families that is installed, in layout, text inputs and text areas alike, instead of only its first name.
- The caret, selection, wrapping and click mapping of text inputs and text areas measure with the font family, weight, style and letter spacing the field's own rules set, as the layout measures the drawn text. The cn default `letter-spacing: var(--tracking-tight)` no longer leaves the caret to the right of the text, and a field whose family differs from the default no longer drifts. Values a field inherits from an ancestor are not read, as with its font size.
- A stylesheet rule on an id with `:hover`, `:active` or `:focus` is ordered against the widget's own rules by specificity: `#cb:hover { border-color }` beats the widget's `.blinc-checkbox:hover` rule. The rule stays a simple rule for animations and the state table, and is also a complex rule for ordering.
- A stylesheet rule on an id with `:checked` or `:disabled` (`#cb:checked`, `#btn:disabled`) applies. It was parsed as a pointer-state rule that nothing read, so the element kept the widget's own look. Such rules are now matched like any selector on the element's `checked` and `disabled` attributes: they outrank the widget's class rules and follow the attribute as it changes.
- Native IME candidates update single-line, multiline, code and rich text documents without duplicate commits. Composition defers change callbacks, groups supported undo history, respects UTF-16 selection boundaries and rolls back invalid single-line commits.
- Labelled checkboxes toggle once when a box click bubbles through the row, while label, keyboard and assistive activation keep their existing behavior.
- Explicit CSS colour importance survives base, state and dynamic-selector passes while border transitions keep their interpolated colours.
- Audio waveforms read live playback progress instead of a fixed ratio, follow pause/seek, and request animation frames only during playback.
- Virtual List updates its mounted window as it scrolls, measures variable row heights and preserves its offset through fresh builds and CSS changes. A bounded window renders both ends of large datasets.
- Textareas retain their revision source with their shared document, so typing repaints after a fresh tree build.
- Text inputs retain their revision and focus sources across fresh tree builds, so typing after a viewport resize updates the visible field as well as callbacks.
- Returning from a pressed scale to the identity transform does not restart hover colour transitions.
- Repeated hover evaluation preserves settled transition targets instead of restarting them. Complex state selectors respect component layers and application specificity.
- Overlay focus returns to stable trigger identities on dismissal, and nested scopes constrain Tab to the topmost active popup. Pointer focus persists without a keyboard ring. Menu roles support roving focus, submenu keys and keyboard context-menu activation.

### Added
- The code editor scrolls sideways: its code area is a horizontal scroll container inside the editor's vertical one, so it glides like vertical scrolling while the gutter stays, and the caret is kept in view as it moves. `CodeEditorData::h_scroll` holds its physics, and the code area carries `.blinc-code-area`.
- `Canvas::flow` also takes a `Signal` or `Computed` of an `Arc<FlowShader>` (anything `FlowSource`): the canvas draws the shader it holds when it paints and repaints when it changes, with no rebuild. `PropertyId::Paint` is the binding that asks for the repaint.
- `Code::bound` shows the text a signal or computed holds. Its lines are a region keyed by number and content, so a change rebuilds only the lines it touched, and the gutter follows the line count.
- The code editor shows diagnostics inline: `set_code_diagnostics` with `CodeDiagnostic`s (a span, a `DiagnosticSeverity` and a message) underlines each span, tints its line, marks the gutter, and shows the line's most severe message after its text, styled by `blinc-code-{underline,mark,line,lens}-{severity}` rules from the theme's `--error`, `--warning`, `--info` and `--text-tertiary`. `CodeEditorData` gains a public `diagnostics` field.
- Code blocks and the code editor are set in `--blinc-code-font-family` when the stylesheet gives one, the generic monospace face otherwise; the caret and selection are measured in the same face.
- `text_measure::TextFace` (family, weight, italic, letter spacing), `TextInputConfig::face` and `TextAreaConfig::face`, and `TextInputData::cursor_position_from_x_with`. Exhaustive struct literals of the two configs must initialize `face`; builder and `..Default::default()` construction remain valid.
- `RenderProps` carries optional `important_colors` metadata. Initialize it to `None` in exhaustive literals or use `..Default::default()`.
- CSS `:focus-visible` distinguishes keyboard focus from pointer focus.

## [0.6.0] - 2026-11-04

### Fixed
- CSS `scrollbar-width: none` hides scrollbars on existing native scroll containers during scrolling; `auto` and `thin` apply their width modes.
- `ScrollRef::set_scroll_offset` cancels active momentum before positioning the content, so it stays at the requested offset.
- Tab and Shift+Tab start with visible controls when focus is empty or has been scrolled offscreen, instead of returning to the top or a scrolled-away control.
- OTP slots participate in Tab traversal without redirecting keyboard focus to the first empty slot. Pointer clicks retain the first-empty-slot behaviour.
- A canvas paints in tree order under a box that comes after it. An independent canvas is replayed over the static cache, so a panel or HUD declared after it, overlapping it, was drawn under it, for a rectangle and for a path alike. A canvas with such a box after it now paints with its batch in tree order, which repaints the static cache with it each frame. Boxes only: text, SVG and images after a canvas still draw over it as before, and the foreground layer and overlays are drawn after canvases as ever.
- A rule keyed on a bound class or attribute applies `display`. `.x[data-hidden] { display: none }` hid a box built with the attribute, but one whose attribute followed a signal (`attr_when`, `class_when`) kept its box: the layer that applies a class's rules to the layout style left `display` out, so the visual properties changed and the display did not. The same holds for the state rules (`:hover`, `:checked`) that go through it.
- Property writes land in the order the values were produced. A batch of queued writes applied from inside another (a region reconciling while a drain runs, applying what a leaving row's `start` queued) holds the newer values, and the rest of the outer batch no longer overwrites them: a plain write queued before the region update used to land after the row's start write.
- A row built in a reconcile where another row's `start` ran reads its bound sources again once mounted. A computed made while the row was built, over a signal that start wrote, kept the value it was built with and never followed its sources: the write reached no binding, because none was registered yet.
- `Div::on_unmount` and `Stateful::on_unmount` handlers run. They were registered but nothing sent them the event in a running app. A node a rebuild, a region or an incremental update removes now runs its unmount handlers once, with its stable id and no live node, after the registry has dropped it; a node built again under the same stable id is a survivor and does not.
- An incremental update sees a change of class. Classes were in no diff hash, so an element whose only change was a class was reported unchanged: its classes stayed registered as they were and the stylesheet kept applying the old rules. A change of class now counts as a change, and is treated as one that may change layout, since the rules it selects can.
- `AmbientPaintState::in_overlay_subtree` records whether a dynamic region sits inside an overlay subtree, so the compositor can order promoted layers around overlay content.
- CSS colours follow CSS. `rgb()` and `rgba()` components are 0 to 255: any component at or under 1 was read as a fraction, so `rgb(1, 1, 1)` came out white. Named colours take their CSS values (`orange` is `#ffa500`, `purple` `#800080`, `gray` `#808080`; they were the `Color` constants), and all 148 are known, with `rebeccapurple`. `blinc_css` also reads `hsl()` and `hsla()`, the space form of `rgb()` and `hsl()` with `/ alpha`, percentage components, and 4-digit hex (`#f00a`). Fractional components such as `rgb(0.2, 0.4, 0.9)` are 0 to 255 now, like any other.
- `Code` and `CodeEditor` report their classes and id to the renderer, so `.class(..)` on them matches rules.
- `Link::on_click` keeps the link's id and classes; it dropped them.
- A `:root` block that sets ordinary properties beside its custom properties keeps the custom properties; they were dropped.
- A class or attribute bound to a signal is recorded on the element when the tree has no stylesheet, so queries and selectors see it. It was dropped until a stylesheet was set.
- Text that took its font size, weight, line height, letter spacing or family from a container's rule gets back the font it was built with when that rule stops applying. It kept the inherited value, so a tab that stopped being the active one stayed bold.
- A CSS transition or animation of a layout property (`width`, `flex-grow`, `padding` and the rest) ends on its final value. The frame it finished on was never written, so it stopped a step short.
- CSS animations of opacity or transform inside an overlay show. A subtree of plain elements was baked into a layer to be moved at composite time, which the overlay's own paint pass never did, so a popover, menu or overlay panel without text showed its base values for the whole animation.
- An element whose rule names a different `@keyframes` after a rebuild starts that animation, as CSS restarts an animation whose name changes; one still playing the same animation keeps it.
- A table cell (`widgets::table::TableCell`) reports its size, classes, id, handlers and refs to the renderer. They were set on its inner element but never read, so `.w(..)` and `.class(..)` on a cell, and rules for its class, had no effect.
- An attribute bound to two values at once (`data-state` set to `open` while `closed` is still listed) reads as the value set last. A child rule keyed on the new value (`.trigger[data-state="open"] > .chevron`) failed to apply when the new value landed first.
- A class added in place no longer overrides a later rule the element already matched. A checked switch that was disabled from the start showed the `:checked` fill over the later `:disabled` one; one disabled through a signal showed the disabled fill. Both follow the cascade now.
- An unfocused text field no longer keeps the window drawing. Every `TextInput`, `TextArea`, code editor and rich text editor mounts a caret canvas, and any visible canvas asked for a frame each vsync, so a form on screen held the CPU awake. Their carets now ask for frames only while they can show.
- A CSS transition of a transform now carries every component of it when any one changes. Easing only the scale of a translated element (a pressed checked switch thumb) used to rebuild the transform from the scale alone and drop the translation. A transform written from transition values is composed translate, rotate, skew, scale, so the translation is no longer scaled.
- An element that keeps a pointer state across a restyle now eases to its new look. Unchecking a hovered switch changes the rule that applies from `:checked:hover` to `:hover`; that was applied without a transition, so the fill snapped.

### Added
- `widgets::cursor::hold_blink(true)` holds every caret at the opacity it has when its blink resets, so frames compare the same however long a run takes. `blinc_recorder::Headless` turns it on.
- `text_input_focused(&data)` exposes text focus as a `Computed<bool>` for composing field groups and focus styling.
- Keyboard navigation: an element with the `tabindex=0` attribute is a tab stop (`tabindex=text` for a text input). Tab and Shift+Tab move focus through the tab stops in tree order, wrapping at the ends and scrolling the stop into view, and staying inside an open overlay (a dialog, a sheet) that holds tab stops; Enter and Space click the stop focused that way, and a press clears it. Tab leaves a text input or area and enters one as a press does; a code or rich-text editor keeps Tab. `:focus` matches the stop, so a rule draws the ring. `RenderTree::{tab_stops, handle_navigation_key, scroll_into_view}` and `EventRouter::{focus_via_keyboard, keyboard_focus, sync_text_focus}`; `event_router::keyboard_focused_node()` names the node Tab focused, for a key handler (key events reach every handler) to test itself against; the runners sync the router's focus with `sync_text_focus`, which leaves focus held by Tab alone. `link` is a tab stop.
- `Canvas::flow(shader)` draws a flow shader over the canvas's bounds, and `Canvas::rounded(r)` rounds its corners.
- `Canvas::flow_loop(FlowLoop)` says what drives a flow from frame to frame: `Auto` (a frame every vsync while the canvas is painted, if the shader reads `time` or `frame_index`), `Always`, `Never`, `While(closure)`, or `Clock(FlowClock)`. A `FlowClock` is the shader's `time` under code's control: `playing()`, `paused()`, `play`, `pause`, `seek`, `step(dt)` and `set_rate`, so a developer can advance a shader one chosen interval at a time. A paused clock draws again only when stepped or sought.
- `TextInput::invalid(..)` and `TextArea::invalid(..)` mark a field invalid whatever its value, as a field with an error is: a bool, Signal or Computed. The box carries `data-invalid` while it is; a `TextArea` now carries the attribute at all. `TextInputData::invalid` and `TextAreaState::invalid` hold it. `TextInput`, `TextArea` and `NumberInput` `placeholder(..)` take a signal too, followed in place.
- `Stylesheet::resolve_color(value)` resolves a CSS colour, `var(..)` chains included, against the variables the stylesheet defines and was parsed with, so drawing code such as a canvas can take its colours from the stylesheet.
- A CSS `animation` set by a rule keyed on more than a single class (an attribute such as `.grid[data-enter="next"]`, a compound, a descendant) plays. It starts when an element is built matching the rule, and when a bound attribute or class makes a different animation the one that applies; when none applies any more, the animation stops. Only single-class rules started before.
- Scroll gestures: `RenderTree::on_scroll_gesture(ScrollGesturePhase)` follows the platform's phases. A SCROLL carries `EventContext::scroll_gesture`, a number that changes with each touch, and `scroll_momentum`, whether it is momentum after the fingers lifted. A gesture taken by a scroll handler with no physics of its own stays with that handler until it ends, wherever the pointer goes; scroll containers still take what is under the pointer.
- A layout property bound to a signal (`.w(..)`, `.h(..)`, `.flex_grow_value(..)` and the rest) eases to each new value by the `transition` the element's rules give it, as a change of rule does.
- `VisualAnimationConfig::shared()`: an element rebuilt under another parent with the same key moves from where the last one was. It plays only when the key lands on another element; scrolling, or an ancestor moving, carries it along.
- Overlays and toasts can enter and leave by CSS rules: `OverlayBuilder::motion_class(class)` and `ToastBuilder::motion_class(class)` put the content in a wrapper of that class with `data-motion="enter"`, then `"exit"` while it closes, and keep it until its exit animation has played (`widgets::css_motion::CssMotion`). An overlay's backdrop carries `overlay-backdrop` and `{class}-backdrop`. `css_animation_playing(id)` says whether an element's CSS animation is playing.
- A `translate` in a `@keyframes` rule can be a percentage of the element's own size, `translateX(100%)`, resolved when the animation starts on it.
- `Div::on_scroll_end(handler)`: the end of a scroll gesture the element took, the fingers lifting or the momentum after them running out. An element with a scroll handler and no scroll physics of its own now receives `SCROLL_END`.
- `Div::on_scroll_x(handler)`: a scroll handler that takes horizontal scrolling only, such as a two-finger swipe. A scroll that is mostly vertical passes on to the nearest container that takes it, so the page still scrolls with the pointer over the element.
- `ScrollRef::watch_y(initial)`: the container's vertical offset as a `Signal<f32>` the renderer keeps up to date as it scrolls, for content that follows the scroll such as a virtual list's window. A wheel or touch scroll brings the ref up to date as it lands, not only on the next physics tick.
- `VisualAnimationConfig::timed(duration_ms, easing)` and `theme_timed()` ease a layout animation over a duration on a curve instead of a spring; `theme_timed()` takes the theme's normal duration and ease-out. A timed animation runs as a scheduler keyframe, so it asks for frames while it plays.
- `Reactive<T>` is `Clone` and has `now()`, the value it holds at the moment, so a widget can hand one bound prop to several bindings and read it in a handler.
- `Div::flex_grow_value` takes a signal or a computed as well as a number; a change patches the style and relayouts, so siblings growing by complementary fractions share their parent's width as it is laid out.
- `Div::max_w_full` sets `max-width: 100%`.
- A SCROLL event carries the target's bounds and the pointer's position within it (`bounds_x`, `bounds_width`, `local_x`, ...), as pointer events do; it carried only the window position.
- `Canvas::redraw_while` names when a canvas animates; outside it the canvas still repaints with other frames but asks for none. `RenderTree::canvas_wants_frames` reports whether any canvas from the last full paint asks for one.
- `RenderTree::motion_output_promoted` reports whether the last full paint drew every motion-bound node into a promoted composite layer and recorded no composite binding.
- Escape clears a search `TextInput` that has text, as on the web; an empty one, and every other field, still loses focus on Escape.
- `.class::placeholder { color }` and `.class::selection { background }` colour the placeholder and the selection of a `TextInput` or `TextArea`, as `#id::placeholder` already did; an id rule wins over a class rule. `Stylesheet::get_class_pseudo_element` reads them.
- `@layer name { ... }` in stylesheets: its class rules sit below every unlayered rule regardless of specificity. `CN_STYLES` is wrapped in one, so an app stylesheet themes cn components whichever was added first.

### Changed
- **Breaking:** flow shaders are not part of CSS. `@flow` blocks, the `flow:` property, `div().flow(..)`, `FlowRef`, `RenderProps::flow`, `ElementStyle::flow`, `css_style!`/`style!` `flow` entries and `Stylesheet::get_flow` are removed. A flow is drawn by a canvas: `canvas(..).flow(graph)`, with `.rounded(r)` for its corners. `flow!` is a procedural macro that transpiles the flow to WGSL at compile time and evaluates to a `blinc_core::FlowShader`, a constant expression (`blinc_macros`, re-exported here); `Canvas::flow` takes that and draws it with `DrawContext::draw_shader` after the canvas's callback, so the canvas's transform, clips and opacity apply; `RenderProps` has no flow field. The body is Rust tokens: step types take underscore names (`pattern_noise`) and colours are `0xrrggbb` integers, both also accepted in `@flow` source text. `flow_shader(source)` compiles `@flow` source text at run time. The parser and the WGSL generator live in the new `blinc_flow` crate. Pointer queries stay in CSS.
- **Breaking:** the CSS `theme(token)` function is removed, for colours (`theme(primary)`), radii (`theme(radius-lg)`) and shadows (`theme(shadow-md)`). A theme's tokens are `:root` variables, so write `var(--primary)`, `var(--radius-lg)`, `var(--shadow-md)`. The `--shadow-*` variables, which were the text `theme(shadow-md)`, are the real `box-shadow` lists now (`0px 4px 6px -1px rgba(0,0,0,0.3)`), so they read in any CSS engine and nest in other values. `blinc_css`'s value parsers no longer read the global `ThemeState`.
- **Breaking:** `widgets::rich_text_editor::selection_toolbar` takes the editor's version as a `&Signal<u32>`; it was a `&State<u32>`. The editor keeps its version in a keyed signal.
- `typography::heading(level, ..)` takes a string or a signal; a signal replaces the text in place. `h1`..`h6` are unchanged.
- **Breaking:** `widgets::checkbox`, `checkbox_labeled` and `widgets::toggle` take a `Signal<bool>` instead of a `&State<bool>`, `widgets::radio_group` a `Signal<String>`, `widgets::number_input` a `Signal<f64>`, and `widgets::input_otp` a `Signal<String>`. `TextInput::class_when` adds a class while a condition holds. `TextInput::disabled` and `TextArea::disabled` take a bool, Signal or Computed too. `.disabled(..)` and `.label(..)` take a value, Signal or Computed and follow it in place; the label's colour is the `.blinc-checkbox-label` rules, dimmed under `.blinc-checkbox-row:disabled`.
- `TextInput::border_width` sets the field's configured border width too, so the content's height follows it; `TextInput` and `TextArea` default to the theme's `RadiusToken::Lg` rather than a literal 8px.
- `TextArea` takes its box look from the same `.blinc-input` rules as `TextInput`, with a `disabled` attribute that follows its state, and no longer binds its own colours. The `TextAreaConfig` box colours (`bg_color`, `hover_bg_color`, `focused_bg_color`, `border_color`, `hover_border_color`, `focused_border_color`) are removed; use rules, or `.bg(..)` and `.border_color(..)` for inline style.
- `TextInput` is one element with the class `blinc-input`, carrying `disabled` and `data-invalid` attributes that follow its data; fill, border, hover and focus are the `.blinc-input` rules of `ua_styles::UA_STYLES`, with a `transition`, and the box no longer binds its own colours. `hover_border_color`, `focused_border_color`, `error_border_color`, `border_colors`, `hover_bg_color`, `focused_bg_color` and `bg_colors` are removed, and the `TextInputConfig` box colours with them; use rules. `idle_border_color` and `idle_bg_color` set inline style. Text, placeholder, caret and selection colours are unchanged.
- `RadioGroup`: each ring is an element with `checked` and `disabled` attributes; its look is the `.blinc-radio` rules of `ua_styles::UA_STYLES`, and the stylesheet-by-id override path is gone. `selected_color`, `border_color` and `hover_border_color` are removed; use rules.
- `Checkbox` is the box element carrying `checked` and `disabled` attributes, with its look in the `.blinc-checkbox` rules of `ua_styles::UA_STYLES` (the same look as `cn::checkbox`, which is now a thin layer on it). `checked_bg`, `unchecked_bg`, `check_color`, `label_color` and `border_color` are removed; use rules. `class`, `checked_class`, `mark_size` and `gap` are new, and `id` and `class` apply to the box. The mark is a bolder check, sized to the box's inner area, so it reads at small sizes.
- `Toggle` is one element carrying `data-state`, `checked`, `data-variant`, `data-size` and `disabled`; its look is the `.blinc-toggle` rules of `ua_styles::UA_STYLES`. `height`, `padding_x`, `label_font_size`, `rounded`, `border_width`, `bordered_off`, `off_bg`, `on_bg`, `off_fg`, `on_fg` and `border_color` are removed; use `variant`, `size` and rules.
- `Button` takes its looks from selectors. `:hover`, `:active` and `:disabled` rules, with CSS `transition`, restyle it; `button()` starts from the `.blinc-button` rules of `ua_styles::UA_STYLES`, which the runners register. `hover_color`, `pressed_color`, `disabled_color`, `disabled_text_color`, `disabled_border`, `flat_when_disabled`, `ButtonConfig` and `ButtonLook::text_color` are removed; write rules instead. `bg_color` and `text_color` stay as fixed values.
- The rich text editor follows the theme. Its document takes its text colour from `.blinc-rich-text`, and its parts from `.blinc-rich-text-marker`, `-quote-bar`, `-divider`, `-code` and `-selection`; the selection toolbar from `.blinc-rte-toolbar`, `.blinc-rte-button`, `.blinc-rte-pill`, `.blinc-rte-separator`, `.blinc-rte-swatch` and `.blinc-rte-link-input`, which were dark-only literals. The caret is drawn in `--blinc-rich-text-caret`, the text colour when unset. Typed text without a colour, and the palette's Default swatch, are transparent, so they take the editor's text colour; they were white. **Breaking:** `RichTextTheme`'s `text`, `muted`, `quote_border` and `divider` are `Option<Color>`, and `RichTextTheme::default()` sets none, so the editor follows the theme; it was `dark()`. `dark()` and `light()` remain as fixed palettes.
- The code block and code editor take their plain look from `.blinc-code` rules of `ua_styles::UA_STYLES`: the fill and text, `.blinc-code-line-number`, `.blinc-code-fold`, `.blinc-code-fold-hint`, `.blinc-code-active-line`, `.blinc-code-selection`, `.blinc-code-guide`, `.blinc-code-bracket`, `.blinc-code-find-match` (`data-active` on the current match) and the minimap's `.blinc-code-minimap`, `-slider` and `-line`. The caret is drawn in `--blinc-code-caret`, the accent when unset. A syntax theme still colours the text, fill and line numbers, inline. The bracket, search match and minimap colours were fixed literals and now follow the theme. The find bar's look moved into the same stylesheet, so it applies without being added. **Breaking:** `CODE_EDITOR_STYLES` is removed; drop the `add_css` call. `CodeConfig`'s `bg_color`, `text_color`, `line_number_color`, `cursor_color`, `selection_color` and `indent_guide_color` are `Option<Color>`, `None` by default; a colour set there, or with `code_bg` and `text_color`, is inline and wins over the rules. `gutter_bg_color` and `gutter_separator_color` are removed.
- Media controls take their colours from `.blinc-media-play-btn`, `.blinc-media-time`, `.blinc-media-volume`, `.blinc-media-mute-btn`, `.blinc-media-seek-rail`, `.blinc-media-seek-fill` and `.blinc-media-live-badge` rules; the drawn seek bar, volume level and waveform from the `--blinc-media-track`, `--blinc-media-buffered`, `--blinc-media-progress`, `--blinc-media-level`, `--blinc-media-level-rest`, `--blinc-waveform-played` and `--blinc-waveform-unplayed` variables, theme colours when unset. `MediaControls` and the waveform used fixed literals. `waveform(..).played_color` and `unplayed_color` still set the waveform's colours.
- Links, blockquotes, rules, lists and tables take their colours from the `.blinc-link`, `.blinc-blockquote`, `.blinc-hr`, `.blinc-list-marker`, `.blinc-task-box` (`checked` when done), `.blinc-thead`, `.blinc-tfoot`, `.blinc-th`, `.blinc-td`, `.blinc-tr-alt` and `.blinc-table-separator` rules of `ua_styles::UA_STYLES`, and follow the theme without a rebuild. A link turns `--primary-hover` on hover. **Breaking:** `LinkConfig::text_color`, `BlockquoteConfig::border_color` and `bg_color`, `HrConfig::color`, and `ListConfig::marker_color` are `Option<Color>`, `None` by default; a colour set there, or with `TableBuilder::header_bg`, is inline and wins over the rule. `Link::hover_color`, `LinkConfig::hover_color` and `TableBuilder::border_color`, which had no effect, are removed. `th_text` and `td_text` no longer set a colour.
- A rule in the `components` layer gives way to a value the element set with a fluent call.
- `:active` matches the pressed element and its ancestors, as on the web.
- A class or attribute that follows a signal restyles the node's descendants too (`.switch:checked > .thumb`), and eases through `transition` when it changes; what a node is built with does not ease. The full style pass applies such a class at mount, as `apply_stylesheet_base_styles` does.
- Rules a signal-driven class brings apply in cascade order (tier, specificity), whichever class came first.
- A transition that moves or scales its node is painted with the animation applied, not baked into a layer: the baked layer carried the final offset and the animated one together.
- `css!` now builds a `Stylesheet` of class, id and pseudo-class rules from CSS text. The previous `css!`, which returned one `ElementStyle`, is renamed `css_style!`.
- `Div::style(&ElementStyle)` records its properties as the element's default tier, below an application stylesheet. `RenderProps::tiers` records which writer set each visual property; nothing reads it yet.

### Fixed
- Glass background brushes share material detection, grouping, paint order and tree-derived nesting across painters. Brush shadow/border styling survives resolution, CSS backdrop updates retain authored brushes, and glass configuration changes invalidate visual caches.
- Compatible nested glass panels share native isolation and retain joint ancestor opacity/effects. Layout `ClipPath::Polygon` and `ClipPath::Path` resolve to native owner-aware path clips so translated glass and inherited content keep their clip frame.
- Supported background ancestors isolate their glass, native paint and collected content together at the background anchor. Ordered backdrop sampling preserves ancestor opacity/effects, sibling order and live CSS opacity endpoints.
- Compatible painted foreground ancestors and siblings share their glass content's visual group. Painted foreground containers also retain collected content order at opaque endpoints, including material changes beside independent glass panels.
- Supported first-level glass panels and transparent unary ancestors retain native and collected children in a foreground visual group. Glass-only trees visit foreground children and preserve their routing after glass decoration.
- Supported nested overlay roots retain native and collected-content groups instead of rejecting their enclosing overlay's composition.
- `Image::opacity` is owned once by `RenderProps`, so CSS and animation can replace it. Built-in `ImageRenderInfo::opacity` now exports neutral content alpha; custom image producers retain their independent intrinsic alpha.
- Supported native ancestors spanning inline portals retain collected-content groups and avoid independent animation promotion; unsupported portals keep their existing admission guards.
- Supported inline overlay roots keep collected content and native descendants in a late visual group above ordinary foreground. Portals remain ownership boundaries for ordinary ancestors.
- Native canvas groups retain their ownership at bound-opacity endpoints so direct writes recover their complete output after a settled zero.
- Property, text, class and region bindings stay in their owning tree namespace; replacement and drop release its subscriptions and queued work without disturbing another window. Stable node identities and slotmap lookups remain unchanged.
- Native groups accept layer effects and blends when their descendants and collected-content capabilities can share ordered composition.
- Image elements and Div image backgrounds can join eligible native opacity groups. Background capture precedes borders and children; consumers without image resolution retain fallback.
- Eligible SVG nodes can join native opacity groups when the consumer resolves raster images. Capture checks text/SVG capabilities independently and excludes explicit flow passes.
- Deferred text capture supports StyledText nodes under the same native-group eligibility and context capability rules as plain text.
- Plain Text nodes can join native opacity groups when the paint context resolves collected glyphs. Zero-opacity groups retain text ownership, and region repaint checks the current context's text capability before using a capture plan.
- Native Div/Canvas opacity groups capture foreground descendants together with background content and suppress independent texture promotion. Moving native ancestors retain their background before nested captures. Collected content, global overlays, glass, effects and 3D boundaries keep their existing fallback.

### Changed
- Document fluent grouped and nested glass usage, shared panel opacity and both glass material and background brush forms.
- Queued property and subtree records carry `tree_scope`; zero retains legacy unscoped writes. `RenderTree::with_property_updates` scopes custom producers, and `take_pending_property_updates` drains only that tree's work.
- Promoted CSS animations and transitions retain completion bookkeeping and write their final properties once. Playing baked sources remain eligible for composite replay.
- Partial opacity uses an explicit native group for leaves and single-child wrappers, preserving overlapping draws. Other paint passes retain inherited opacity when they do not own the ancestor's commands.
- Dynamic canvases retain callback output in motion, overlay and explicit composite scratch owners. Their commands, clips and image payloads stay in the same batch, with fresh paint requested instead of independent replay.
- Dynamic canvases inside static native groups paint with their ancestor commands and stay out of independent overlay/region collection. Their retained-owner flag resets on each full paint.
- Retained foreground motion and CSS records keep invalidation metadata without background ranges. Static-owned opacity bindings and retained foreground CSS groups survive zero-opacity frames; ancestor groups suppress separate foreground promotion.
- Motion binding metadata identifies static-owned ranges. Ancestor group ownership blocks descendant CSS/motion texture promotion, keeps opacity endpoint groups, and requests repaint for retained motion ranges.
- Canvas paint records retain visit order independently of primitive counts and include dynamic callbacks that emit only images or no current primitives.
- Composite bake regions include tessellated paths and nested effect margins in their screen-space bounds.
- CSS paint metadata records composite scratch ownership, and scratch motion records stay out of the dynamic-cache patcher. Promoted-root animation opacity remains on the live blit.
- Dynamic background opacity bindings retain a patchable group at opacity one, so cached fades update group opacity instead of scaling each overlapping child.
- CSS paint records retain whether their ranges belong to the dynamic motion batch, allowing static-cache patchers to request a full repaint instead of using indices from another batch.

### Added
- `RenderTree::render_snapshot` uses the shared composition walker without an entry/exit animation state or a new animation scheduler.
- **`TextInput::on_submit(callback)`** — called when Enter is pressed in the field, with whether Shift was held. Without one Enter does nothing in a single-line field.
- **`widgets::CODE_EDITOR_STYLES`** — the code editor's find and replace bar is built from classed elements (`.blinc-find`, `.blinc-find-input`, `.blinc-find-toggle--on`, …), and this stylesheet is its designed look. Add it with `ctx.add_css(..)`; the cn theme bundle includes it. The bar's search now runs as the query is typed (Enter and Shift+Enter step through the matches) instead of when the editor next redrew.
- **`widgets::table::TableCell::class(name)`** + **`::id(id)`** — table cells can now carry CSS classes / ids for selector matching. Required by `blinc_cn::table` to layer `.cn-table-head` / `.cn-table-cell` over the layout widget's theme-token defaults.
- **`TextInput::text_align(TextAlign)`** — horizontal alignment of the visible text inside the field. `Left` (default) keeps the previous behaviour. `Center` swaps the text wrapper from absolute-positioned (with `scroll_offset`) to a flex-centred row, suppresses the absolute-positioned cursor (whose math doesn't account for the dynamic centre offset), and triggers a single-click-selects-all gesture so numeric / OTP / code fields behave like `<input type=number>` in browsers.
- **`TextInput::padding_x(px)`** — exposes the previously hard-coded 12 px internal padding so tight cells (number / OTP / code) can drop it to 4–6 px.
- **`TextInput::on_step(callback)`** — stepper hook fired on ↑ / ↓ / `+` / `−` / `=` keys while the field is focused. When set, the keys are consumed (no character insert for `+` / `−`); the callback receives `+1` or `-1`. Off by default so plain text inputs still accept a typed `-` as the leading sign of a negative number.
- **`refresh_text_input(state)`** + promoted-to-public **`stateful::refresh_stateful`** — external mutation of `SharedTextInputData` (value / cursor / selection) can now push the change through the Stateful so the visible field updates without waiting for an unrelated frame trigger.
- **`blinc_layout::widgets::number_input`** — typed numeric field wrapping `text_input` with `InputType::Number`. Bound to a `State<f64>`; builder methods set `min` / `max` / `step` / `precision`. On every keystroke the visible text parses → clamps → writes back to `state` (empty / mid-edit invalid strings like `""` / `"-"` / `"."` leave state untouched so the user can keep typing). `step_up` / `step_down` helpers operate on `NumberInputConfig` for caller-supplied stepper buttons (notably `cn::number_input`'s `+` / `−`).
- **`blinc_layout::widgets::toggle`** — first-class binary on/off control. Renders a button-shaped Stateful with theme-token defaults (colours from `ColorToken::Secondary` / `TextSecondary` / `TextPrimary` / `BorderSecondary`, radius from `RadiusToken::Default`, padding / typography from spacing + typography tokens). Pairs an external `State<bool>` with an internal `Stateful<ButtonState>` for hover / pressed detection; `:checked` pseudo-state covers the on-state for CSS overrides. Builder methods cover height, padding, icon, label, font size, custom on/off bg/fg, border-width, bordered-off (outline mode), and `on_change`. `blinc_cn::toggle` wraps this with shadcn-style `.cn-toggle` classes + variant / size ladders.

### Performance
- **Stateful layout-prop fast path** — `refresh_props_internal` discriminates three rebuild kinds via paired structural / topology hashes: `Visual` (no children change, no taffy `Style` change → visual-only prop update on existing nodes); `LayoutProps` (taffy `Style` changed, no topology change → patch styles in place + re-`compute_layout`); `Structural` (children added / removed / reordered → full subtree rebuild + mint + collect). The middle path is what keeps spring-driven `.w()` / `.h()` / `.left()` / `.top()` cheap — pre-fix every spring tick on a stateful went through the full Structural rebuild path, tearing down + re-collecting handlers / physics / motion bindings each frame.
- **`FontTextMeasurer` remembers measurements**: taffy asks for each text's size several times per layout at the same few widths, and every ask shaped the text afresh. Results are kept by text, face, size and width (up to 16,384 entries, texts over 1 KiB excluded) and dropped when the font registry's new `generation()` changes. A definite width no longer shapes an unused single-line layout.
- **`LayoutTree` node maps hash with FxHash**: the Taffy-to-`LayoutNodeId` reverse map and the baseline and incidental-`align_self` tables were std maps, and `children()` hashed every child through the reverse map, so flushing a wide tree was quadratic in SipHash. The keys are integers the tree makes itself.

### Added
- **Per-corner `CornerShape` on `ClipShape::RoundedRect`** carried end-to-end through the GPU SDF pipeline. `overflow: clip` on a squircle parent now follows the parent's squircle curve at each corner instead of the previous fixed circular arc — fixes the bg-leak between the parent's squircle outline and the clip's circular outline visible on cn::accordion hover.
- **`router.focused()` lookup** in `apply_complex_selector_styles::focused_node` — replaces the previous `element_registry.all_ids()` walk that only saw elements with a string `id`. Class-only widgets (e.g. `cn::input` attaches by class but has no id) now have `.cn-input:focus` rules apply correctly.
- **Outline propagation** through `Div::merge` and `RenderProps::merge_from`. The Stateful state-callback's `queue_prop_update` path uses `merge_from`; without carrying outline, every state transition wiped outline back to the defaults and the focus ring would flash off-then-on.
- `rich_text_editor` return type explicitly opts out of input lifetime capture (`+ use<>`) so callers on edition 2024 can stash it inside a `'static`-bounded `Div::child` without HRTB inference failures.
- `RenderTree::painted_node_ids()` — set of node ids the paint walker actually rendered this frame (clipped against the window viewport even on scrolls without `viewport_cull` opt-in). Exposes the source of truth for visibility-gated redraw decisions.
- `RenderTree::has_active_visible_visual_animations(painted)` and `has_active_visible_flip_animations(painted)` — visibility-gated counterparts of the existing `has_active_*` predicates. Used by the windowed redraw chain so off-screen `animate_bounds` / FLIP entries don't pin the chain alive.
- `RenderTree::css_has_visible_transitions(painted)` — companion to `css_transitions_empty`, returns true when at least one CSS transition has a painted target.
- `CssAnimationStore::has_visible_active(painted)` and `has_visible_vsync_class(painted)` — visibility-gated keyframe/transition checks. The `vsync_class` variant additionally filters on `KeyframeProperties::needs_vsync_for_smoothness` so the windowed app's animation FPS cap can bypass to native vsync when a visible transform / layout / clip-path keyframe is mid-cycle.
- `RenderTree::has_any_cursor_style()` and `HandlerRegistry::has_any_pointer_handler()` — short-circuit gates for the mouse-move pipeline; UIs with no pointer handlers, no `:hover`/`:active` rules, and no `cursor:` styles skip the per-move hit_test entirely.
- `Stylesheet::has_pointer_state_rules()` — detects `:hover` / `:active` selectors so the mouse-move skip path knows when CSS state styling is at risk.
- `stateful::has_visible_animating_statefuls(painted)` — registry of animating Statefuls now carries a `node_id_fn` so the windowed app can intersect with painted nodes. An off-screen spinner whose Stateful is mid-animation no longer pins the redraw chain.

### Changed
- Mouse-move hit-test pipeline short-circuits when nothing in the tree could react. Pure-static views (e.g. `hello_blinc`) no longer hit_test on every cursor move; cursor styling still works via a single one-shot resolve.
- Scroll FSM settles `Scrolling → Idle` after 200ms idle even when not overscrolling. Wheel-scroll inputs without `ScrollPhase::Ended` (mouse wheel, many Linux trackpad drivers) no longer leave `scroll_animating=true` permanently.

## [0.5.1] - 2026-04-13

### Added
- `video_player()` widget with signal-driven media controls (play/pause, seek, time, volume)
- `VideoControlsBuilder` lazy-built Stateful driven by player signals
- `AudioPlayerWidget` with optional waveform visualization

### Fixed
- Foreground motion opacity bindings retain groups at opaque and zero endpoints, allowing full repaint to recover after a settled zero value.
- Video seek bar wrapped in own Stateful with position/duration signal deps (was invisible until volume clicked)
- Video volume slider bar colors swapped (active/inactive were reversed)

## [0.5.0] - 2026-04-10

### Added

- `RenderTree::has_overscrolling_scroll()` for web scroll bounce-back debounce
- `FontRegistry::invalidate_generic_cache()` clears negative lookups after font loads

### Fixed

- Code editor `std::time::SystemTime` → `web_time::SystemTime` for wasm32 compatibility
- SVG color tint applied via post-rasterization `apply_tint()` for non-`currentColor` sources

## [Unreleased]

### Added

#### Lazy Image Loading (Complete)

- Image placeholder rendering for type 2 (image source) — uses cached preloaded thumbnail/blur-hash
- Skeleton shimmer placeholder (type 3) — animated gradient sweep with auto-redraw
- Fade-in animation when image loads (configurable `fade_duration_ms`)
- CSS `loading: lazy | eager` property
- CSS `image-placeholder-color`, `image-placeholder-image`, `image-placeholder-type`, `fade-duration` properties
- ElementStyle/RenderProps fields for full CSS-driven lazy loading
- `placeholder_image` and `fade_duration_ms` now flow through `ImageData` to renderer
- Placeholder images preloaded eagerly so they're ready when type 2 placeholder renders

#### Performance / memory

- `Scroll::viewport_cull(true)` opts the scroll into per-frame paint culling. The renderer skips painting any descendant whose post-scroll absolute bounds (plus a 200 px overscan) don't intersect the visible viewport. Layout still runs for every child — this only affects the paint walker. Saves GPU primitive memory and draw cost on long lists where most children are off-screen.
- `RustHighlighter` and `JsonHighlighter` share their compiled regex tables across all instances via `OnceLock<Arc<[TokenRule]>>`. An app with multiple `code()` blocks no longer pays the regex DFA build cost (and resident size) N times.
- `Text::new` skips `decode_html_entities` entirely when the input contains no `&` — drops one `String` allocation per text element per build for the common no-entity case.
- Motion-binding lookups in `render_layer_with_motion` are batched: one `motion_bindings.get(&node)` per node-pass instead of four. Non-bound nodes (the ~95% case) short-circuit before reaching the mutex-locked field accessors.
- Class-name storage migrated from `Vec<String>` to `Vec<Arc<str>>` end-to-end, interned through `blinc_core::intern`. Affects `Div`, `Text`, `Svg`, `Stateful`, link / list / blockquote / button widgets, and `ElementRegistry::classes`. The `element_classes()` trait method now returns `&[Arc<str>]`. Repeated class names share one allocation across all nodes.

### Changed
- `ElementRegistry::register_element_type` takes `&'static str` (was `String`). Element types come from `ElementBuilder::semantic_type_name`'s compile-time literals, so the per-build `String` allocation per element was unnecessary.
- `Div::class` / `Text::class` / `.class()` builders take `impl AsRef<str>` instead of `impl Into<String>`. Slightly more permissive and avoids the unconditional `String` allocation.

### Fixed
- Several intra-doc links in `selector::registry`, `widgets::scroll`, `widgets::rich_text_editor::render`, and `css_parser` that were rejected by `cargo doc -D rustdoc::broken-intra-doc-links`.

## [0.4.0] - 2026-04-05

### Added

#### Flow Shader 3D Targets
- CSS parser: `target: vertex` and `target: material` in `@flow` blocks
- Output target parsing for 3D: `position`, `world_normal`, `world_position`, `albedo`, `metallic`, `roughness`, `emissive`, `surface_normal`, `alpha_out`
- `parse_output_target()` helper for unified output name resolution

#### Code Editor Enhancements

- VS Code-style search bar via overlay (Cmd+F)
  - Find with case sensitive, whole word, regex toggles
  - Replace with replace current / replace all
  - Match highlighting with active match indicator
  - Signal-driven Stateful overlay (no deadlocks, no infinite rebuilds)
- Code folding with SVG chevron icons in gutter
- Bracket matching highlight (cursor adjacent to brace)
- Indentation guides (vertical lines at indent levels)
- Current line highlight
- Minimap (scaled-down code overview sidebar)
- Gutter: separate fold column, line numbers, separator with proper alignment

### Fixed

- Gutter line numbers vertically centered instead of top-aligned
- Fold toggle click not reaching correct line when lines folded
- Cursor X offset due to gutter width not subtracted from click coordinates
- Editor expanding vertically instead of scrolling on new lines
- Hover artifacts on sidebar, nav, and menubar components
- Checkbox background not updating on check/uncheck state change

## [0.1.15] - 2026-03-22

### Added

#### Code Editor Widget (`code_editor`)

- `code_editor(&state)` — full-featured editable code editor using Stateful incremental updates
- `code_editor_state("content")` — shared state constructor that persists across rebuilds
- Editing: type, Enter (auto-indent), Backspace, Delete, Tab/Shift+Tab (indent/dedent selected lines)
- Cmd+Backspace/Delete: delete word backward/forward
- Cmd+Z / Cmd+Shift+Z: undo/redo (200-entry history stack)
- Cmd+C/X/V: clipboard copy/cut/paste via platform commands
- Cmd+A: select all
- Cmd+Left/Right: word jump navigation
- Smart Home: toggle between first non-whitespace and column 0
- Page Up/Page Down navigation
- Mouse click cursor positioning, mouse drag selection, double-click word select
- Visual selection highlighting with absolutely-positioned divs
- Current line highlight (subtle overlay when focused)
- Cursor with blink animation
- Vertical scrolling via `overflow_y_scroll` on Stateful
- Monospace text measurement cache (`measure_mono()`)
- Per-line syntax highlight cache with dirty tracking
- Escape to blur/unfocus

#### Shared Text Editing Utilities (`text_edit` module)

- `word_boundary_left/right()` for word-jump navigation
- `word_at_position()` for double-click word selection
- `clipboard_read/write()` for OS clipboard integration (macOS pbpaste/pbcopy, Linux xclip)

#### Text Area Improvements

- Visual selection rendering (absolutely-positioned highlight divs)
- Word navigation: Cmd/Ctrl+Left/Right via shared `text_edit` utilities
- Clipboard: Cmd+C/X/V (copy/cut/paste)
- Select all: Cmd+A
- Control character filtering in TEXT_INPUT handler

#### Stateful Enhancements

- `overflow_y_scroll()` builder method on Stateful
- `inner_scroll_physics()` accessor for programmatic scroll control

#### Platform

- Letter key codes (A-Z) mapped to ASCII in windowed.rs KEY_DOWN events for Cmd+key shortcuts

### Other

- `class_name_with_state()` on `ComplexSelector` for extracting class names from state-bearing selectors (e.g. `.cn-sidebar-item:hover`)
- `get_classes()` on `ElementRegistry` for retrieving registered CSS classes by node ID

### Fixed

- Hover transition artifacts on sidebar, nav-link, and menubar components caused by incorrect base_styles caching and transition restart loops on sustained hover
- Checkbox background not updating on check/uncheck: parent node CSS class registrations now updated during full structural rebuilds so `.cn-checkbox--checked` is matched correctly
- Per-side borders with different colors now use SDF-based rendering with proper corner radius instead of rectangular fill strips
- Combobox dropdown scroll by using registered physics directly
- Smooth corner radius artifact on thin borders
- Stale element data (SVG tint, text content, images) during visual-only rebuilds: `element_type` is now updated in `update_subtree_props_from_builder`
- Base styles cache invalidated when CSS classes change during visual-only rebuilds
- Transition detection skipped for nodes already sustaining a hover state to prevent restart loops
- Toast stack container uses absolute positioning so it covers the full viewport independently of other overlay siblings

## [0.1.14] - 2026-02-24

### Added

- `set_auto_id()` and `children_builders_mut()` methods on `ElementBuilder` trait for stable child ID assignment within stateful containers
- `assign_inner_ids_recursive()` in stateful callback wrapper: auto-assigns deterministic inner IDs to children via `derive_child_key()` (e.g. `"button_0:div:0"`) so event handlers and CSS selectors persist across rebuilds

### Fixed

- CSS `color` property now inherits into text elements inside stateful container rebuilds (added text_color propagation to `apply_stylesheet_base_styles_for_subtree()` post-pass)
- Event handlers on inner children are now re-registered during visual-only subtree rebuilds (`update_subtree_props_from_builder`), ensuring closures capturing new state are properly propagated
- CSS text property inheritance (text_color, text_decoration, white_space, text_overflow) added to the full-tree post-pass at initial render time

## [0.1.13] - 2026-02-18

### Added

#### Pointer Query Pressure & Touch Count

- `env(pointer-pressure)` — normalized touch/click pressure (0.0-1.0), smoothed via `pointer-smoothing`
- `env(pointer-touch-count)` — number of active touch points (0 for mouse input)
- `set_pressure()` and `set_touch_count()` on `PointerQueryState` for per-event platform input
- Pressure smoothing using same exponential decay as position smoothing
- Registered `pointer-pressure` and `pointer-touch-count` env var names in renderer

#### Flow Shader Macro & Builders

- `flow!` macro for defining `@flow` shaders using Rust identifiers and primitives (no raw strings)
- `parse_flow_string()` public API for parsing `@flow` block strings into `FlowGraph`
- `FlowRef` enum (`Name`/`Graph`) with `From` impls for `&str`, `String`, and `FlowGraph`
- `.flow()` builder on `Div` accepting both `FlowRef::Name` (CSS reference) and `FlowRef::Graph` (direct)
- `.flow()` builder on `ElementStyle` for stylesheet-based flow references
- `flow:` property in `css!` and `style!` macros
- `flow_graph: Option<Arc<FlowGraph>>` field on `RenderProps` for direct graph attachment
- Flow parser: swizzle tolerance for `stringify!()` spaces (`uv . x` → `uv.x`)
- Flow parser: hex color tolerance for space after `#` (`# ff0000` → `#ff0000`)
- Flow parser: newline normalization in `parse_flow_string()` for `stringify!()` multi-line output

#### SVG Animation Properties

- `fill`, `stroke`, `stroke-width` as animatable CSS properties for SVG elements
- `stroke-dasharray`, `stroke-dashoffset` CSS properties for SVG line-drawing effects
- `d: path("...")` CSS property for SVG path morphing animation
- `svg_path_data` field on ElementStyle and RenderProps for path data propagation
- Decomposed transform fields (`rotate`, `scale_x`, `scale_y`) on ElementStyle to avoid lossy atan2 decomposition
- `parse_scale_values()` helper for extracting original scale factors from CSS

### Fixed

- Border morph/distortion on rounded clipping containers: overflow clip now deferred to after border rendering, preventing double-AA between border SDF and clip SDF at the same boundary
- Borders on elements with `overflow: clip` now render in the foreground layer (after images), matching CSS painting order (content → border → outline)
- `style_to_keyframe_properties()` preserves original rotation angle from CSS (avoids 359deg → -1deg via atan2)
- Clippy: use `Ok(v)` instead of `Some(v) = .ok()` pattern in glass parser

#### CSS Parser & Stylesheet Engine

- Full CSS parser with `#id` selector support and `:hover`, `:focus`, `:active`, `:disabled` pseudo-states
- Global stylesheet injection via `ctx.add_css()` with runtime theme variable support
- Stylesheet runtime integration: base styles, state overrides, and animation overrides (layered application)
- `css!` and `style!` macros for scoped inline styling that returns `ElementStyle`

#### Layout Properties

- `width`, `height` (px, %, auto)
- `min-width`, `max-width`, `min-height`, `max-height`
- `padding`, `margin` (shorthand and per-side)
- `gap` between flex children
- `display: flex | block | none`
- `flex-direction: row | column | row-reverse | column-reverse`
- `flex-wrap: wrap | nowrap`
- `flex-grow`, `flex-shrink`
- `align-items`, `align-self`, `justify-content` (start, center, end, stretch, space-between, space-around, space-evenly)
- `overflow: visible | clip | scroll`
- `border-width`, `border-color`
- `visibility: visible | hidden | collapse | normal` — hides rendering and collapses layout (Display::None in taffy)

#### Visual Properties

- `background` / `background-color` with solid colors, `linear-gradient()`, `radial-gradient()`, `conic-gradient()`
- Color formats: `#hex`, `rgb()`, `rgba()`, named colors, `theme()` tokens
- `border-radius` (uniform and per-corner)
- `opacity`
- `box-shadow` with offset, blur, spread, and color
- `transform: scale() rotate() translate()` (2D transforms)
- `backdrop-filter: glass | blur(Npx) | chrome | gold | metallic | wood`
- `backdrop-filter: liquid-glass(blur() saturate() brightness() border() tint())` variant with configurable border thickness and tint

#### SVG CSS Transform Inheritance

- SVGs now inherit CSS transforms from ancestor elements via `css_affine` propagation
- Affine decomposition into uniform scale (applied to bounds) + rotation angle (sent to shader)

#### Visibility

- `StyleVisibility` enum (`Visible`, `Hidden`) on `ElementStyle`
- CSS parser recognizes `visibility: hidden | visible | collapse | normal`
- `visibility: hidden` both skips rendering and collapses layout (sets `Display::None` in taffy)
- `visibility: visible` restores `Display::Flex` when reversing hidden state
- Visibility applied across all render paths: `render_layer_with_motion`, `render_text_recursive`, `collect_elements_recursive`
- Complex selector state changes (hover/leave) properly reset taffy styles via `base_taffy_styles`
- Layout recomputed after state style changes that affect layout properties

#### 3D CSS Transforms

- `perspective: <px>` for 3D perspective distance
- `rotate-x: <deg>`, `rotate-y: <deg>` for 3D axis rotation
- `translate-z: <px>` for Z-axis translation
- Correct inverse homography unprojection for flat elements with perspective

#### 3D SDF Shapes (Raymarched)

- `shape-3d: box | sphere | cylinder | torus | capsule | group`
- `depth: <px>` for 3D extrusion
- 32-step raymarching with analytical ray-AABB intersection for accurate hit detection
- Edge anti-aliasing via closest-approach distance tracking
- Blinn-Phong lighting with configurable `ambient`, `specular`, `light-direction`, `light-intensity`
- UV mapping: screen-space gradient evaluation for smooth gradients across all 3D faces

#### 3D Boolean Operations

- `3d-op: union | subtract | intersect | smooth-union | smooth-subtract | smooth-intersect`
- `3d-blend: <px>` for smooth blend radius
- `shape-3d: group` for collecting children into compound SDF

#### CSS Animations

- `@keyframes` with named animation definitions
- `animation` shorthand: name, duration, timing-function, delay, iteration-count, direction, fill-mode
- Timing functions: `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`
- `animation-direction: normal | reverse | alternate | alternate-reverse`
- `animation-fill-mode: none | forwards | backwards | both`
- `animation-iteration-count: <number> | infinite`
- Animatable properties: `opacity`, `scale`, `scale-x`, `scale-y`, `translate-x`, `translate-y`, `rotate`, `rotate-x`, `rotate-y`, `perspective`, `depth`, `translate-z`, `blend-3d`, `background`, `border-color`, `border-radius`, `border-width`, `box-shadow`, `clip-path`, `filter` (including `blur`), `width`, `height`, `padding`, `margin`, `gap`

#### CSS Transitions

- `transition` shorthand: property, duration, timing-function, delay
- `transition: all 300ms ease` wildcard for all animatable properties
- Comma-separated multi-property transitions
- Smooth reverse transitions on hover-leave with mid-flight reversal support
- Filter identity defaults for proper interpolation (brightness/contrast/saturate default to 1.0)

#### CSS Filters

- `filter` property: `grayscale()`, `invert()`, `sepia()`, `hue-rotate()`, `brightness()`, `contrast()`, `saturate()`
- `filter: blur(Npx)` with GPU Kawase multi-pass blur via LayerEffect pipeline
- `filter: drop-shadow(x y blur color)` with GPU drop-shadow via LayerEffect pipeline
- Space-separated multi-function syntax: `filter: blur(4px) grayscale(1) brightness(1.5)`
- Supports `N`, `N%`, `Ndeg`, `Npx` argument formats
- Nested parenthesis handling in filter parser (e.g. `drop-shadow(4px 4px 8px rgba(0,0,0,0.5))`)

#### Backdrop-Filter Animation

- `backdrop-filter: blur(Npx)` now extracts actual blur radius (was ignoring value)
- `backdrop-filter: blur(Npx) saturate(N) brightness(N)` multi-function parsing
- Animatable `backdrop_blur`, `backdrop_saturation`, `backdrop_brightness` in `KeyframeProperties`
- Transition support: `transition: backdrop-filter 400ms ease` with smooth interpolation
- `@keyframes` support for backdrop-filter properties

#### Selector Hierarchy

- `.class` selectors via `Div::class("name")`
- Descendant combinator (space): `#parent .child`
- Child combinator (`>`): `#parent > .child`
- Compound selectors: `#id.class:hover`
- Structural pseudo-classes: `:first-child`, `:last-child`, `:nth-child(N)`, `:only-child`
- Complex selector matching engine with ancestor chain walking

#### Advanced Selectors

- Adjacent sibling combinator (`+`): `.a + .b`
- General sibling combinator (`~`): `.a ~ .b`
- `:not()` negation pseudo-class
- `:is()` / `:where()` functional pseudo-classes (matches any of listed selectors)
- `:first-of-type`, `:last-of-type`, `:nth-of-type(N)`, `:nth-last-of-type(N)`, `:only-of-type`
- `:empty`, `:root` pseudo-classes
- `*` universal selector

#### Layout Property Animation

- Animatable layout properties: `width`, `height`, `padding`, `margin`, `gap`
- Per-frame taffy style override with automatic `compute_layout()` when layout properties change
- `base_taffy_styles` snapshot for state reset

#### Theme System

- `theme()` function for accessing design tokens in CSS values
- Color tokens: `primary`, `secondary`, `background`, `surface`, `success`, `warning`, `error`, `info`, `text-primary`, `text-secondary`, etc.
- Radius tokens: `radius-none`, `radius-sm`, `radius-default`, `radius-md`, `radius-lg`, `radius-xl`, `radius-full`
- Shadow tokens: `shadow-none`, `shadow-sm`, `shadow-md`, `shadow-lg`, `shadow-xl`

#### Events

- `PINCH` event support in `EventContext` (center and scale fields)

#### CSS Form Widget Styling

- `caret-color` CSS property for text input cursor color
- `selection-color` CSS property for text selection highlight
- `::placeholder` pseudo-element for placeholder text styling (`color` property)
- `Stateful<S>` now forwards `element_id()` and `element_classes()` to ElementBuilder, enabling CSS matching for all Stateful-based widgets
- `.id()` and `.class()` builder methods on TextInput and TextArea
- CSS-aware `state_callback` in TextInput and TextArea: queries active stylesheet for base, `:hover`, `:focus`, `:disabled`, and `::placeholder` overrides
- `set_active_stylesheet()` / `active_stylesheet()` global for widget access to the current stylesheet
- `get_placeholder_style()` on Stylesheet for `::placeholder` pseudo-element lookup
- `Stateful::inner_layout_style()` method for capturing final taffy Style after all builder methods

### Fixed

- Stateful `base_style` capture timing: `on_state()` captured layout style before `.w()`/`.h()` were applied, causing widgets to revert to constructor defaults (e.g., `w_full()`) on state transitions. Now updated in `build()` with the final layout style
- CSS-parsed `backdrop-filter: blur()` glass now uses subtle white tint (`rgba(1,1,1,0.1)`) and zero border-thickness for clean frosted glass appearance (was fully transparent tint, making glass indistinguishable from backdrop)

- CSS-parsed `backdrop-filter: blur()` glass now uses subtle white tint (`rgba(1,1,1,0.1)`) and zero border-thickness for clean frosted glass appearance (was fully transparent tint, making glass indistinguishable from backdrop)
- CSS timing functions now map to spec-correct cubic-bezier values (`ease` was incorrectly using `ease-in-out` polynomial, causing 6.5x slower initial progress than CSS spec)
- Transform-origin mid-flight reversal jitter: `snapshot_before_keyframe_properties` now overlays `transform_origin` from active transition, preventing snap-back on hover-leave
- Cubic-bezier solver rewritten with f64 internal precision and binary-search fallback for jitter-free interpolation at 120fps
- Hover-leave reverse transitions now properly detected and animated (previously snapped to base state instantly)
- Transition repeat regression: pre-reset snapshots prevent spurious re-transitions when hover state persists after transition completion

## [0.1.12] - 2025-01-19

### Added

- `apply_touch_scroll_delta()` method for touch velocity tracking
- `scroll_time` field in `EventContext` for momentum scrolling
- `dispatch_scroll_chain_with_time()` in RenderTree for mobile scroll dispatch
- Momentum deceleration in scroll physics tick for touch devices

### Changed

- Scroll physics now supports velocity-based momentum scrolling
- `on_scroll_end()` starts momentum if velocity exceeds threshold

## [0.1.1] - Initial Release

- Initial public release with layout engine and scroll widgets
