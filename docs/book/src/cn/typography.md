# Typography

Typography helpers provide defaults for headings, body copy and short inline
fragments. They return `Text`, except `chained_text`, which groups fragments in a
`Div`. Import them through `blinc_cn::cn` or the component prelude. Install
`cn_bundle()` as described in [Getting Started](overview.md).

## Headings

Use a level that reflects the content hierarchy. The named helpers and
`heading(level, content)` share these defaults; application CSS can override them.

| Helper | Default size / weight | CSS class |
|--------|-----------------------|-----------|
| `cn::h1` | 32px / bold | `.cn-h1` |
| `cn::h2` | 24px / bold | `.cn-h2` |
| `cn::h3` | 20px / semibold | `.cn-h3` |
| `cn::h4` | 18px / semibold | `.cn-h4` |
| `cn::h5` | 16px / medium | `.cn-h5` |
| `cn::h6` | 14px / medium | `.cn-h6` |

```rust
use blinc_cn::cn;
use blinc_layout::prelude::*;

div().flex_col().gap(3.0)
    .child(cn::h1("Project overview"))
    .child(cn::h2("Recent activity"))
    .child(cn::h3("Design reviews"))
    .child(cn::h4("Comments"))
    .child(cn::h5("Resolved"))
    .child(cn::h6("Earlier updates"))
```

`cn::heading(2, content)` chooses the same heading level and `.cn-h2` class as
`cn::h2(content)`. Its level is a construction option from 1 to 6; its content
also accepts a Signal or Computed.

## Body and supporting text

- `cn::p(content)`: wrapping body copy; 16px, line-height 1.5. Class: `.cn-p`.
- `cn::muted(content)`: short secondary information; secondary text colour,
  no wrapping. Class: `.cn-muted`.
- `cn::caption(content)`: captions and footnotes; 12px, tertiary text colour,
  no wrapping. Class: `.cn-caption`.
- `cn::small(content)`: short fine print; 12px, no wrapping. No automatic class.

```rust
use blinc_cn::cn;
use blinc_layout::prelude::*;

div().flex_col().gap(2.0)
    .child(cn::p("Keep project notes, decisions and next steps together."))
    .child(cn::muted("Updated a few minutes ago"))
    .child(cn::caption("Figure 1: Workspace layout"))
    .child(cn::small("Changes are saved automatically."))
```

All of these return `Text` and support its fluent methods. For supporting copy
that needs multiple lines, enable wrapping with `.wrap(true)`.

## Inline helpers and composition

- `cn::span(content)`: neutral inline text fragment.
- `cn::b(content)`: bold fragment.
- `cn::strong(content)`: alias for `b`.
- `cn::inline_code(content)`: monospace fragment, such as a filename or identifier.
- `cn::chained_text([fragments])`: a `Div` containing a baseline-aligned row of
  `Text` fragments.

```rust
use blinc_cn::cn;

cn::chained_text([
    cn::span("Saved to "),
    cn::strong("Design system"),
    cn::span(" as "),
    cn::inline_code("theme.css"),
])
```

Include spaces in the fragments where needed. Inline helpers do not wrap by
default, and `chained_text` lays out separate elements in a row. For a paragraph
that wraps mixed styles as flowing text, use
[`rich_text()`](../widgets/text.md#rich-text). For syntax-highlighted code blocks,
use [`code()`](../widgets/code-editor.md#read-only-code-block).

The plain `blinc_layout::typography::label(content)` helper returns 14px,
medium-weight inline `Text`; it is also re-exported from
`blinc_cn::components::typography`. `cn::label(content)` instead creates the
themed [form Label component](form.md#label), with required and disabled options.

## Reactive text

`heading` accepts `impl IntoReactive<String>`. The named `h1`–`h6`, `p`, `muted`,
`caption` and inline helpers take construction-time strings. For other live
text, bind a Signal or Computed with `Text::bound` and give it a CSS class:

```rust
use blinc_cn::cn;
use blinc_core::{computed, signal};
use blinc_layout::{binding::IntoReactive, prelude::*};

let title = signal("Project overview".to_string());
let pending = signal(3usize);
let summary = computed(move |g| {
    format!("{} files awaiting review", g.get(pending).unwrap_or_default())
});

let header = div().class("project-copy")
    .child(cn::heading(2, title))
    .child(Text::bound(summary.into_reactive()).class("project-summary"));

pending.set(5);
```

Create these bindings once, or use `ctx.use_signal(...)` when retaining them
across UI builds. Updating either source replaces the bound text in place.

## Restyling

The heading, paragraph, muted and caption classes listed above are styling
hooks. `span`, `b`, `strong`, `small`, `inline_code` and `chained_text` do not add
`cn-*` classes; attach your own with `.class(...)`. The equivalent helpers in
`blinc_layout::typography` have the same text defaults without the CN classes.

Add overrides after the component stylesheet, using the theme's root variables:

```css
.cn-h2 {
  font-size: 28px;
  font-weight: 700;
  color: var(--text-primary);
}

.cn-p {
  font-size: var(--text-base);
  line-height: 1.6;
}

.project-copy {
  flex-direction: column;
  gap: var(--space-2);
}

.project-summary {
  font-size: var(--text-sm);
  color: var(--text-secondary);
}
```

Scope shared classes to an application container when only one section should
change, for example `.project-copy .cn-h2`.
