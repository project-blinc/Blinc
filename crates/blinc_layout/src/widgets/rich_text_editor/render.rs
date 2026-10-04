//! Read-only renderer for `RichDocument`.
//!
//! This phase produces a static element tree from a document — paragraphs,
//! headings, list items, quotes, dividers — using the existing `RichText`
//! element for inline span rendering. No cursor, no selection, no editing.
//!
//! Editing-aware rendering (cursor overlay, selection rects, click → cursor)
//! is added in Phase 3.
//!
//! # Theme
//!
//! Colors and font sizes are passed in via [`RichTextTheme`] so the renderer
//! stays decoupled from the application theme system. A reasonable default
//! is provided ([`RichTextTheme::dark`] / [`RichTextTheme::light`]).

use super::document::{Block, BlockKind, RichDocument};
use crate::div::{Div, FontWeight, div};
use crate::rich_text::{RichText, rich_text_styled};
use crate::styled_text::StyledText;
use blinc_core::Color;

/// Visual configuration for the rich text renderer.
///
/// All colors and sizes are explicit so the editor never reaches into a
/// global theme — pass a different `RichTextTheme` per editor instance to
/// theme it however you like.
#[derive(Clone, Debug)]
pub struct RichTextTheme {
    /// Default text color (paragraphs, list items).
    pub text: Color,
    /// Muted color used for blockquote text and list bullets.
    pub muted: Color,
    /// Color of the blockquote left border.
    pub quote_border: Color,
    /// Color of the divider line.
    pub divider: Color,
    /// Base font size for paragraph text (px).
    pub font_size: f32,
    /// Heading sizes for levels 1..=6 in px.
    pub heading_sizes: [f32; 6],
    /// Vertical spacing between blocks (px).
    pub block_gap: f32,
    /// Horizontal indent step for nested list items (px).
    pub indent_step: f32,
    /// Width of the blockquote left border (px).
    pub quote_border_width: f32,
    /// Width of the marker column for bullet/numbered list items (px).
    pub bullet_column_width: f32,
    /// Line height multiplier (1.0 = font_size, 1.5 = font_size * 1.5).
    /// Used both for spacing between visual lines and for cursor height.
    pub line_height: f32,
}

impl RichTextTheme {
    /// Reasonable defaults for a dark UI.
    pub fn dark() -> Self {
        Self {
            text: Color::rgba(0.92, 0.92, 0.95, 1.0),
            muted: Color::rgba(0.62, 0.62, 0.7, 1.0),
            quote_border: Color::rgba(0.35, 0.35, 0.42, 1.0),
            divider: Color::rgba(0.25, 0.25, 0.32, 1.0),
            font_size: 15.0,
            heading_sizes: [30.0, 24.0, 20.0, 18.0, 16.0, 15.0],
            block_gap: 8.0,
            indent_step: 24.0,
            quote_border_width: 3.0,
            bullet_column_width: 18.0,
            line_height: 1.45,
        }
    }

    /// Reasonable defaults for a light UI.
    pub fn light() -> Self {
        Self {
            text: Color::rgba(0.10, 0.10, 0.13, 1.0),
            muted: Color::rgba(0.42, 0.42, 0.5, 1.0),
            quote_border: Color::rgba(0.78, 0.78, 0.84, 1.0),
            divider: Color::rgba(0.88, 0.88, 0.92, 1.0),
            font_size: 15.0,
            heading_sizes: [30.0, 24.0, 20.0, 18.0, 16.0, 15.0],
            block_gap: 8.0,
            indent_step: 24.0,
            quote_border_width: 3.0,
            bullet_column_width: 18.0,
            line_height: 1.45,
        }
    }

    fn heading_size(&self, level: u8) -> f32 {
        let i = (level.clamp(1, 6) - 1) as usize;
        self.heading_sizes[i]
    }
}

impl Default for RichTextTheme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Build a flat element tree representing `doc` using `theme`.
///
/// `content_width` is the pixel width of the column the document will be
/// rendered into. It's required because [`RichText`] needs an explicit
/// `wrap_to_width` to compute multi-line layouts — without it, long lines
/// silently overflow to the right of the container. Pass the inner width
/// of whatever wrapping div hosts the editor (i.e. width minus padding).
///
/// All spacing in the returned tree is expressed in raw pixels via `_px`
/// builders so that the renderer is independent of the global 4-unit
/// spacing scale used by the rest of the layout API.
pub fn render_document(doc: &RichDocument, theme: &RichTextTheme, content_width: f32) -> Div {
    let mut root = div().w_full().flex_col().gap_px(theme.block_gap);
    for (i, block) in doc.blocks.iter().enumerate() {
        root = root.child(render_block(doc, i, block, theme, content_width));
    }
    root
}

/// Build the element tree for a single block.
///
/// `index` is the block's position in `doc` — used for computing numbered
/// list ordinals. `content_width` is the available pixel width *outside*
/// any indent: each block subtracts its own indent before passing the
/// remainder down to its inline content.
fn render_block(
    doc: &RichDocument,
    index: usize,
    block: &Block,
    theme: &RichTextTheme,
    content_width: f32,
) -> Div {
    let indent_px = (block.indent as f32) * theme.indent_step;
    let inner_width = (content_width - indent_px).max(0.0);

    match &block.kind {
        BlockKind::Paragraph => indented_block(
            paragraph_div(
                block,
                theme,
                theme.font_size,
                FontWeight::Normal,
                false,
                inner_width,
            ),
            indent_px,
        ),
        BlockKind::Heading(level) => indented_block(
            paragraph_div(
                block,
                theme,
                theme.heading_size(*level),
                FontWeight::Bold,
                false,
                inner_width,
            ),
            indent_px,
        ),
        BlockKind::BulletItem => {
            list_item_div(block, theme, "•".to_string(), indent_px, inner_width)
        }
        BlockKind::NumberedItem => {
            let ordinal = doc.numbered_ordinal(index).unwrap_or(1);
            list_item_div(
                block,
                theme,
                format!("{}.", ordinal),
                indent_px,
                inner_width,
            )
        }
        BlockKind::Quote => quote_div(block, theme, indent_px, inner_width),
        BlockKind::Divider => indented_block(div().w_full().h(1.0).bg(theme.divider), indent_px),
    }
}

/// Wrap `inner` in a parent div whose left padding equals `indent_px`. Done
/// via a wrapper because there's no per-side `_px` padding builder; using
/// `pl(units)` would re-introduce the 4× scale bug.
fn indented_block(inner: Div, indent_px: f32) -> Div {
    if indent_px <= 0.5 {
        return inner.w_full();
    }
    div()
        .w_full()
        .flex_row()
        .child(div().w(indent_px))
        .child(inner.w_full())
}

/// Build a paragraph-style div containing one row per visual line.
///
/// Each source `StyledLine` is pre-wrapped to `line_width` via
/// [`super::wrap::wrap_styled_line_with_offsets`] and the resulting
/// chunks become rows. Each row is a fixed-height `relative()`
/// container that hosts its runs as **absolutely-positioned children**
/// at the exact pixel `x_in_line` computed by
/// [`build_runs_for_visual_line`]. This bypasses flex layout entirely
/// for the runs themselves so the rendered glyphs align with the
/// geometry the cursor uses for hit-testing.
///
/// NOTE: there's a known interaction with `wrap_styled_line` where the
/// chip on a wrapped paragraph appears visually adjacent to neighbour
/// text without a space. The bug is in the pre-wrap step (or in how
/// `RichText` measures the wrapped chunk's text), not in the run
/// positioning itself — putting the chip line on its own paragraph
/// renders correctly. Tracking under "DEFERRED: monospace inline-code
/// chip rendering".
fn paragraph_div(
    block: &Block,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    line_width: f32,
) -> Div {
    let line_height_px = font_size * theme.line_height;
    let mut col = div().w_full().flex_col();
    for source_line in &block.lines {
        let visual_lines = if line_width > 0.0 {
            super::wrap::wrap_styled_line_with_offsets(
                source_line,
                line_width,
                font_size,
                weight,
                italic,
            )
        } else {
            vec![super::wrap::WrappedLine {
                line: source_line.clone(),
                source_start_col: 0,
                source_end_col: source_line.text.chars().count(),
            }]
        };
        for visual in &visual_lines {
            col = col.child(absolute_positioned_line(
                visual,
                theme,
                font_size,
                weight,
                italic,
                line_width,
                line_height_px,
            ));
        }
    }
    col
}

/// Build a single visual line as an absolute-positioned container.
/// Each run's `RichText` sits at the geometry's exact `x_in_line`, so
/// flex layout never gets a chance to shrink the body run away from
/// its measured width.
fn absolute_positioned_line(
    visual: &super::wrap::WrappedLine,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    line_width: f32,
    line_height_px: f32,
) -> Div {
    let visual_line = &visual.line;
    let len = visual_line.text.len();

    // Per-byte boolean: which bytes are code-flagged?
    let mut code_at: Vec<bool> = vec![false; len];
    for span in &visual_line.spans {
        if !span.code {
            continue;
        }
        let s = span.start.min(len);
        let e = span.end.min(len);
        for b in code_at.iter_mut().take(e).skip(s) {
            *b = true;
        }
    }

    // Walk byte-by-byte and emit (byte_start, byte_end, is_code) ranges
    // at every transition. Mirrors `build_runs_for_visual_line` so the
    // renderer's runs align 1:1 with the geometry's runs.
    let mut byte_ranges: Vec<(usize, usize, bool)> = Vec::new();
    if len > 0 {
        let mut seg_start = 0usize;
        let mut current_is_code = code_at[0];
        let mut byte = 0usize;
        while byte < len {
            let here = code_at[byte];
            if here != current_is_code {
                byte_ranges.push((seg_start, byte, current_is_code));
                seg_start = byte;
                current_is_code = here;
            }
            let ch_len = visual_line.text[byte..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            byte += ch_len;
        }
        byte_ranges.push((seg_start, len, current_is_code));
    }

    // Hot path: single non-code run — emit a single RichText without
    // any absolute positioning. The line container still gets an
    // explicit width.
    if byte_ranges.len() == 1 && !byte_ranges[0].2 {
        // The child needs the width as well as the container: a
        // content-sized child is handed min-content and wraps inside a
        // box that is already the right size, so a heading broke across
        // two lines inside a one-line box and overlapped what followed.
        return div()
            .w(line_width)
            .h(line_height_px)
            .child(make_rich_text(visual_line, theme, font_size, weight, italic).w(line_width));
    }

    // Multiple runs OR a code-only run — use a relative container with
    // absolute children at the precomputed x positions.
    let body_font = crate::div::FontFamily::default();
    let code_font = crate::div::FontFamily::monospace();
    let mut x_cursor = 0.0_f32;
    let mut container = div().w(line_width).h(line_height_px).relative();
    for (bstart, bend, is_code) in byte_ranges {
        if bstart >= bend {
            continue;
        }
        let run_line = slice_styled_line(visual_line, bstart, bend);
        let font_family = if is_code { &code_font } else { &body_font };
        let width = super::state::measure_width(
            &run_line.text,
            font_size,
            weight,
            italic,
            Some(font_family),
        );
        let positioned = div()
            .absolute()
            .left(x_cursor)
            .top(0.0)
            .h(line_height_px)
            .child(if is_code {
                make_code_run(&run_line, theme, font_size, weight, italic)
            } else {
                make_rich_text(&run_line, theme, font_size, weight, italic)
            });
        container = container.child(positioned);
        x_cursor += width;
    }
    container
}

/// Build a `StyledLine` containing just the text and rebased spans for
/// byte range `[start, end)` of `source`.
fn slice_styled_line(
    source: &crate::styled_text::StyledLine,
    start: usize,
    end: usize,
) -> crate::styled_text::StyledLine {
    use crate::styled_text::{StyledLine, TextSpan};
    let text = source.text[start..end].to_string();
    let mut spans = Vec::new();
    for span in &source.spans {
        let s = span.start.max(start);
        let e = span.end.min(end);
        if s >= e {
            continue;
        }
        spans.push(TextSpan {
            start: s - start,
            end: e - start,
            color: span.color,
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strikethrough: span.strikethrough,
            code: span.code,
            link_url: span.link_url.clone(),
            token_type: span.token_type.clone(),
        });
    }
    StyledLine { text, spans }
}

/// Build a list-item row: marker on the left, content on the right.
fn list_item_div(
    block: &Block,
    theme: &RichTextTheme,
    marker: String,
    indent_px: f32,
    inner_width: f32,
) -> Div {
    let marker_text = rich_text_styled(StyledText::plain(&marker, theme.muted))
        .size(theme.font_size)
        .default_color(theme.muted);
    // The marker column eats `bullet_column_width + 8` (gap) of the inner width.
    let line_width = (inner_width - theme.bullet_column_width - 8.0).max(0.0);

    let row = div()
        .w_full()
        .flex_row()
        .gap_px(8.0)
        .child(div().w(theme.bullet_column_width).child(marker_text))
        .child(paragraph_div(
            block,
            theme,
            theme.font_size,
            FontWeight::Normal,
            false,
            line_width,
        ));
    indented_block(row, indent_px)
}

/// Build a blockquote div with a left border and muted italic text.
fn quote_div(block: &Block, theme: &RichTextTheme, indent_px: f32, inner_width: f32) -> Div {
    // Border + gap consume `quote_border_width + 12` of the inner width.
    let line_width = (inner_width - theme.quote_border_width - 12.0).max(0.0);
    let inner = paragraph_div(
        block,
        theme,
        theme.font_size,
        FontWeight::Normal,
        true,
        line_width,
    );
    let row = div()
        .w_full()
        .flex_row()
        .gap_px(12.0)
        .child(div().w(theme.quote_border_width).bg(theme.quote_border))
        .child(inner);
    indented_block(row, indent_px)
}

/// Render a single (already pre-wrapped) styled line as a `flex_row`
/// of `RichText` siblings, one per run.
///
/// A "run" is a contiguous slice of the line where every character
/// shares the same font (family + weight + italic + size). Currently
/// we only split on inline code (body text vs monospace), but the
/// machinery generalizes to any per-span font override.
///
/// The cursor / hit-test path in [`super::state`] uses the matching
/// `RunGeometry` produced by [`build_runs_for_visual_line`] for pixel
/// measurement, so the runs rendered here and the runs measured there
/// stay in lockstep — every run's pixel left edge in the rendered
/// flex row equals its `RunGeometry.x_in_line`, and every run's pixel
/// width matches the geometry's `width`. That's why the cursor lands
/// on the right character even when a line mixes proportional and
/// monospace text.
fn render_line_row(
    line: &crate::styled_text::StyledLine,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> Div {
    let segments = split_line_by_code(line);
    if segments.len() == 1 && !segments[0].is_code {
        // Hot path: no inline code at all — emit a single RichText so
        // the row layout cost matches the pre-Phase-5 baseline.
        return div().w_full().child(make_rich_text(
            &segments[0].line,
            theme,
            font_size,
            weight,
            italic,
        ));
    }

    let mut row = div().w_full().flex_row().items_start();
    for seg in segments {
        if seg.is_code {
            row = row.child(make_code_run(&seg.line, theme, font_size, weight, italic));
        } else {
            row = row.child(make_rich_text(&seg.line, theme, font_size, weight, italic));
        }
    }
    row
}

/// Build a monospace `RichText` for a code run. No chip background or
/// internal padding — purely a font + color change — so the rendered
/// pixel width matches the geometry's `RunGeometry.width` exactly.
/// (The chip styling needs sub-pixel control of where the chip starts
/// and ends, which we can revisit once the basic per-run measurement
/// path is stable.)
fn make_code_run(
    line: &crate::styled_text::StyledLine,
    theme: &RichTextTheme,
    font_size: f32,
    _weight: FontWeight,
    _italic: bool,
) -> RichText {
    let mut chip_line = line.clone();
    let code_color = Color::rgba(0.93, 0.85, 0.55, 1.0);
    for span in chip_line.spans.iter_mut() {
        // Drop the `code` flag so the inner `RichText` doesn't recurse,
        // and pin the color to our code tint.
        span.code = false;
        span.color = code_color;
    }
    let styled = StyledText::from_lines(vec![chip_line]);
    rich_text_styled(styled)
        .size(font_size)
        .weight(FontWeight::Normal)
        .default_color(code_color)
        .line_height(theme.line_height)
        .monospace()
}

/// One slice of a line, either a normal styled run or a code run.
struct LineSegment {
    line: crate::styled_text::StyledLine,
    is_code: bool,
}

/// Walk a `StyledLine`'s spans and split its text at every transition
/// between "any code" and "no code" coverage. Each segment's spans are
/// rebased so `start` is `0`-relative.
fn split_line_by_code(line: &crate::styled_text::StyledLine) -> Vec<LineSegment> {
    use crate::styled_text::{StyledLine, TextSpan};

    let len = line.text.len();
    if len == 0 {
        return vec![LineSegment {
            line: line.clone(),
            is_code: false,
        }];
    }

    // Build a per-byte boolean: is this byte covered by a `code` span?
    let mut code_at: Vec<bool> = vec![false; len];
    for span in &line.spans {
        if !span.code {
            continue;
        }
        let s = span.start.min(len);
        let e = span.end.min(len);
        for b in code_at.iter_mut().take(e).skip(s) {
            *b = true;
        }
    }

    // Walk byte-by-byte and emit segments at every transition. Use
    // char boundaries — a transition mid-codepoint is impossible
    // because spans are stored on byte boundaries that match char
    // boundaries (the editor only inserts at char boundaries).
    let mut segments = Vec::new();
    let mut seg_start = 0usize;
    let mut current_is_code = code_at[0];
    let mut byte = 0usize;
    while byte < len {
        let here = code_at[byte];
        if here != current_is_code {
            segments.push(slice_segment(line, seg_start, byte, current_is_code));
            seg_start = byte;
            current_is_code = here;
        }
        // Advance by one char to stay on UTF-8 boundaries.
        let ch_len = line.text[byte..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        byte += ch_len;
    }
    segments.push(slice_segment(line, seg_start, len, current_is_code));
    segments.retain(|s| !s.line.text.is_empty());
    if segments.is_empty() {
        return vec![LineSegment {
            line: line.clone(),
            is_code: false,
        }];
    }
    segments
}

/// Build a `LineSegment` covering byte range `[start, end)` of `source`
/// with the given code flag.
fn slice_segment(
    source: &crate::styled_text::StyledLine,
    start: usize,
    end: usize,
    is_code: bool,
) -> LineSegment {
    use crate::styled_text::{StyledLine, TextSpan};

    let text = source.text[start..end].to_string();
    let mut spans = Vec::new();
    for span in &source.spans {
        let s = span.start.max(start);
        let e = span.end.min(end);
        if s >= e {
            continue;
        }
        spans.push(TextSpan {
            start: s - start,
            end: e - start,
            color: span.color,
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strikethrough: span.strikethrough,
            code: span.code,
            link_url: span.link_url.clone(),
            token_type: span.token_type.clone(),
        });
    }
    LineSegment {
        line: StyledLine { text, spans },
        is_code,
    }
}

/// Build a `RichText` for a wrapped line. Each line goes into its own
/// `RichText`, so wrapping inside a single line never happens at the
/// shaper level — we already pre-wrap in `paragraph_div`.
fn make_rich_text(
    line: &crate::styled_text::StyledLine,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> RichText {
    let styled = StyledText::from_lines(vec![line.clone()]);
    let mut rt = rich_text_styled(styled)
        .size(font_size)
        .weight(weight)
        .default_color(theme.text)
        .line_height(theme.line_height);
    if italic {
        rt = rt.italic();
    }
    rt
}

/// Reserved for the future "real chip" rendering — a code run with a
/// rounded background, internal padding, and a 1px border. Currently
/// unused; the active path is [`make_code_run`] which does the
/// font/color switch only so the rendered pixel widths match the
/// `RunGeometry` widths exactly.
#[allow(dead_code)]
fn make_code_chip(
    line: &crate::styled_text::StyledLine,
    theme: &RichTextTheme,
    font_size: f32,
) -> Div {
    // Force the spans to drop the `code` flag so the inner RichText
    // doesn't recurse, and reset color so the chip's tint wins.
    let mut chip_line = line.clone();
    let chip_color = Color::rgba(0.93, 0.85, 0.55, 1.0);
    for span in chip_line.spans.iter_mut() {
        span.code = false;
        span.color = chip_color;
    }
    let styled = StyledText::from_lines(vec![chip_line]);
    let rt = rich_text_styled(styled)
        .size(font_size * 0.92)
        .weight(FontWeight::Normal)
        .default_color(chip_color)
        .line_height(theme.line_height)
        .monospace();
    let mut chip = div()
        .padding_x_px(5.0)
        .rounded(3.0)
        .bg(Color::rgba(0.18, 0.18, 0.22, 1.0))
        .border(1.0, Color::rgba(0.30, 0.30, 0.36, 1.0))
        .child(rt);
    // Raw-pixel horizontal margin so the chip has visible breathing
    // room from neighbouring text. `mx()` would multiply by 4 (Blinc's
    // 4-unit spacing scale), so go through `style_mut` directly.
    {
        use taffy::LengthPercentageAuto;
        let s = chip.style_mut();
        s.margin.left = LengthPercentageAuto::Length(3.0);
        s.margin.right = LengthPercentageAuto::Length(3.0);
    }
    chip
}

// =============================================================================
// Geometry walker — produces the LineGeometry index used by hit-testing
// =============================================================================

use super::cursor::DocPosition;
use super::state::LineGeometry;

/// Compute the line-geometry index for `doc` at the given `content_width`,
/// matching the layout that [`render_document`] produces. Walks every
/// block, applies the same indent / wrap / list-marker accounting, and
/// emits one `LineGeometry` per visual line.
///
/// The returned y-coordinates are relative to the editor's content rect
/// (the same rect whose origin matches the renderer tree). The editor's
/// click handler can use them directly with `local_x` / `local_y` from
/// the mouse-event context.
pub fn compute_line_geometry(
    doc: &RichDocument,
    theme: &RichTextTheme,
    content_width: f32,
) -> Vec<LineGeometry> {
    let mut out = Vec::new();
    let mut y = 0.0f32;
    for (block_idx, block) in doc.blocks.iter().enumerate() {
        let indent_px = (block.indent as f32) * theme.indent_step;
        let inner_width = (content_width - indent_px).max(0.0);

        match &block.kind {
            BlockKind::Divider => {
                // Divider has no inline content; it occupies 1px + the
                // implicit block_gap below. Skip it for cursor purposes.
                y += 1.0;
            }
            BlockKind::Paragraph => {
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    indent_px,
                    inner_width,
                    theme.font_size,
                    FontWeight::Normal,
                    false,
                    theme,
                    &mut y,
                );
            }
            BlockKind::Heading(level) => {
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    indent_px,
                    inner_width,
                    theme.heading_size(*level),
                    FontWeight::Bold,
                    false,
                    theme,
                    &mut y,
                );
            }
            BlockKind::BulletItem | BlockKind::NumberedItem => {
                let marker_total = theme.bullet_column_width + 8.0;
                let line_x = indent_px + marker_total;
                let line_w = (inner_width - marker_total).max(0.0);
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    line_x,
                    line_w,
                    theme.font_size,
                    FontWeight::Normal,
                    false,
                    theme,
                    &mut y,
                );
            }
            BlockKind::Quote => {
                let border_total = theme.quote_border_width + 12.0;
                let line_x = indent_px + border_total;
                let line_w = (inner_width - border_total).max(0.0);
                emit_paragraph_geometry(
                    &mut out,
                    block_idx,
                    block,
                    line_x,
                    line_w,
                    theme.font_size,
                    FontWeight::Normal,
                    true,
                    theme,
                    &mut y,
                );
            }
        }
        // Block gap (matches gap_px on the root flex_col).
        y += theme.block_gap;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn emit_paragraph_geometry(
    out: &mut Vec<LineGeometry>,
    block_idx: usize,
    block: &Block,
    line_x: f32,
    line_width: f32,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    theme: &RichTextTheme,
    y: &mut f32,
) {
    let line_height_px = font_size * theme.line_height;
    for (source_line_idx, source_line) in block.lines.iter().enumerate() {
        let visual_lines = if line_width > 0.0 {
            super::wrap::wrap_styled_line_with_offsets(
                source_line,
                line_width,
                font_size,
                weight,
                italic,
            )
        } else {
            vec![super::wrap::WrappedLine {
                line: source_line.clone(),
                source_start_col: 0,
                source_end_col: source_line.text.chars().count(),
            }]
        };
        let to_emit: Vec<_> = if visual_lines.is_empty() {
            vec![super::wrap::WrappedLine {
                line: source_line.clone(),
                source_start_col: 0,
                source_end_col: 0,
            }]
        } else {
            visual_lines
        };
        for visual in to_emit {
            let runs = build_runs_for_visual_line(&visual, font_size, weight, italic);
            out.push(LineGeometry {
                start: DocPosition::new(block_idx, source_line_idx, visual.source_start_col),
                x: line_x,
                y: *y,
                width: line_width,
                height: line_height_px,
                runs,
            });
            *y += line_height_px;
        }
    }
}

/// Decompose a wrapped visual line into `RunGeometry` runs, splitting
/// at every transition between code and non-code coverage. Each run is
/// measured with its own font so the cursor x stays correct on
/// mixed-font lines.
fn build_runs_for_visual_line(
    visual: &super::wrap::WrappedLine,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> Vec<super::state::RunGeometry> {
    use super::state::{RunGeometry, measure_width};

    let line = &visual.line;
    let text = &line.text;
    let len = text.len();
    let body_font = crate::div::FontFamily::default();
    let code_font = crate::div::FontFamily::monospace();

    // Empty visual line — emit one zero-width run so the cursor has a
    // valid landing spot.
    if len == 0 {
        return vec![RunGeometry {
            source_col: visual.source_start_col,
            text: String::new(),
            x_in_line: 0.0,
            width: 0.0,
            font_family: body_font,
            font_size,
            weight,
            italic,
        }];
    }

    // Build a per-byte boolean: is this byte covered by a `code` span?
    let mut code_at: Vec<bool> = vec![false; len];
    for span in &line.spans {
        if !span.code {
            continue;
        }
        let s = span.start.min(len);
        let e = span.end.min(len);
        for b in code_at.iter_mut().take(e).skip(s) {
            *b = true;
        }
    }

    // Walk byte-by-byte and emit one run per code/non-code transition.
    let mut runs: Vec<RunGeometry> = Vec::new();
    let mut x_cursor = 0.0_f32;
    let mut seg_start_byte = 0usize;
    let mut seg_start_char_offset = 0usize;
    let mut current_is_code = code_at.first().copied().unwrap_or(false);

    let mut byte = 0usize;
    let mut char_offset = 0usize;
    while byte < len {
        let here = code_at[byte];
        if here != current_is_code {
            push_run(
                &mut runs,
                &mut x_cursor,
                visual.source_start_col + seg_start_char_offset,
                &text[seg_start_byte..byte],
                current_is_code,
                font_size,
                weight,
                italic,
                &body_font,
                &code_font,
            );
            seg_start_byte = byte;
            seg_start_char_offset = char_offset;
            current_is_code = here;
        }
        let ch_len = text[byte..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        byte += ch_len;
        char_offset += 1;
    }
    push_run(
        &mut runs,
        &mut x_cursor,
        visual.source_start_col + seg_start_char_offset,
        &text[seg_start_byte..len],
        current_is_code,
        font_size,
        weight,
        italic,
        &body_font,
        &code_font,
    );

    let _ = measure_width; // silence unused-import warning if there are no runs
    runs
}

#[allow(clippy::too_many_arguments)]
fn push_run(
    runs: &mut Vec<super::state::RunGeometry>,
    x_cursor: &mut f32,
    source_col: usize,
    text: &str,
    is_code: bool,
    body_font_size: f32,
    weight: FontWeight,
    italic: bool,
    body_font: &crate::div::FontFamily,
    code_font: &crate::div::FontFamily,
) {
    if text.is_empty() {
        return;
    }
    let font_family = if is_code {
        code_font.clone()
    } else {
        body_font.clone()
    };
    let font_size = body_font_size;
    let width = super::state::measure_width(text, font_size, weight, italic, Some(&font_family));
    runs.push(super::state::RunGeometry {
        source_col,
        text: text.to_string(),
        x_in_line: *x_cursor,
        width,
        font_family,
        font_size,
        weight,
        italic,
    });
    *x_cursor += width;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::styled_text::TextSpan;

    fn sample_doc() -> RichDocument {
        let mut p = Block::paragraph("Hello, world", Color::WHITE);
        // Make "Hello" bold via a span override
        p.lines[0].spans = vec![
            TextSpan::new(0, 5, Color::WHITE, true),
            TextSpan::colored(5, 12, Color::WHITE),
        ];
        RichDocument::from_blocks(vec![
            Block::heading(1, "Demo", Color::WHITE),
            p,
            Block::bullet("first", Color::WHITE),
            Block::bullet("second", Color::WHITE),
            Block::numbered("alpha", Color::WHITE),
            Block::numbered("beta", Color::WHITE),
            Block::quote("a wise quote", Color::WHITE),
            Block::divider(),
            Block::paragraph("after the divider", Color::WHITE),
        ])
    }

    #[test]
    fn renders_without_panicking() {
        let doc = sample_doc();
        let theme = RichTextTheme::dark();
        let _root = render_document(&doc, &theme, 720.0);
    }

    #[test]
    fn empty_document_renders() {
        let doc = RichDocument::new();
        let theme = RichTextTheme::dark();
        let _root = render_document(&doc, &theme, 720.0);
    }
}
