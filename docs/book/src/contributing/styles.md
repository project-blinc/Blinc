# Writing Component Styles

The default look of the cn components is written in SCSS and compiled when the crate is built. The
result is the `CN_STYLES` constant, which is plain CSS. Nothing is compiled when an application
runs, and an application's own CSS stays plain CSS.

## Where the files are

```text
crates/blinc_cn/styles/
  index.scss              the order of the components
  components/_button.scss one partial for each component
  components/_card.scss
  ...
crates/blinc_sass/scss/
  _mixins.scss            shared by every stylesheet
```

`index.scss` lists the partials with `@use`. The compiled rules come out in that order, so a later
component can override an earlier one when both match. The build wraps the whole result in
`@layer components`, so every rule sits below an application's own rules. You do not write the
layer in a partial.

`crates/blinc_layout/src/ua_styles.rs` is the stylesheet of the layout widgets. It is short, so it
stays a Rust string and is not compiled.

## Add or change a component

1. Add `components/_name.scss` for a new component, and add `@use "components/name";` to
   `index.scss`.
2. Write the rules. A partial is plain SCSS at the top level. Colours, radii, durations and
   easings are `var(--theme-token)` references, as before.
3. Run `cargo build -p blinc_cn`. A mistake stops the build and the message names the file, the
   line and the column.

A name that Windows reserves (`aux`, `con`, `prn`, `nul`, `com1` to `com9`, `lpt1` to `lpt9`)
cannot be a file name, so do not use one for a partial.

## Shared mixins and tokens

A partial starts with the modules it needs:

```scss
@use "blinc/mixins" as *;
```

| Name | What it gives |
| --- | --- |
| `transition($props, $duration, $easing)` | A `transition` over the space-separated properties, at `fast` and `state` unless you name others |
| `focus-ring($offset)` | The focus outline, `2px` from the edge unless you give another offset |
| `tint($percent, $into)` | The text colour mixed into `$into`, as a hover or press tint is |
| `blend($color, $percent, $into)` | Any colour mixed into `$into` |

```scss
.cn-toggle {
  @include transition(background-color color);
  &:hover:not(:disabled) { background: tint(5%); }
  &:focus-visible { @include focus-ring; }
}
```

A rule that does not fit a mixin stays plain CSS. Write the property out where a mixin would hide
what the rule does.
Blinc's own CSS passes through the compiler unchanged: custom properties, `color-mix()`,
`var()` with fallbacks, `overflow-fade`, `@keyframes`, and the pseudo-classes and attribute
selectors the engine reads. The one visible change is that an attribute selector loses the quotes
where the value is a plain word: `[data-state="on"]` is written `[data-state=on]`, which means the
same.

## Check the result

The compiled sheet is in `target/debug/build/blinc_cn/<hash>/out/cn.css`. The tests in
`blinc_cn` and `blinc_recorder` read `CN_STYLES`, so they check the compiled text.

The web component pages cut their code examples from the compiled sheet, and the fixture digest
that decides whether a preview must be recorded again is taken from it too. A change to how a
sheet is written that leaves the CSS alone does not make the previews stale.
