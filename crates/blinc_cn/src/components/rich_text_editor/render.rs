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
//! Sizes are passed in via [`RichTextTheme`]. Colours come from the
//! `.blinc-rich-text` rules of the layout stylesheet unless the theme sets
//! them; [`RichTextTheme::dark`] and [`RichTextTheme::light`] are fixed
//! palettes.

use super::document::{Block, BlockAlign, BlockKind, RichDocument};
use super::line::{LineBox, LineFont};
use crate::components::checkbox::{CheckboxSize, checkbox};
use blinc_core::{Color, reactive::signal};
use blinc_layout::div::{Div, FontWeight, div};
use blinc_layout::rich_text::{RichText, rich_text_styled};
use blinc_layout::styled_text::StyledText;
use blinc_theme::{RadiusToken, ThemeState};

/// Visual configuration for the rich text renderer.
///
/// Sizes are explicit. A colour left `None` comes from the stylesheet's
/// `.blinc-rich-text` rules, so it follows the theme; one set here wins.
#[derive(Clone, Debug)]
pub struct RichTextTheme {
    /// Text without a colour of its own, over the `.blinc-rich-text`
    /// rule's; also the caret and selection colour.
    pub text: Option<Color>,
    /// List markers, over the `.blinc-rich-text-marker` rule's.
    pub muted: Option<Color>,
    /// The blockquote's left border, over the `.blinc-rich-text-quote-bar`
    /// rule's.
    pub quote_border: Option<Color>,
    /// The divider line, over the `.blinc-rich-text-divider` rule's.
    pub divider: Option<Color>,
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
    /// Where the formatting toolbar sits: floating over the selection, or fixed above the text.
    pub toolbar: super::toolbar::ToolbarPlacement,
    /// Space around the text inside the editor, in pixels. The fixed toolbar spans the full width.
    pub padding: f32,
}

/// The space between a quote's bar and its text.
pub(super) const QUOTE_GAP: f32 = 12.0;

impl RichTextTheme {
    /// The width a quote's bar and its gap take.
    pub(super) fn quote_inset(&self) -> f32 {
        self.quote_border_width + QUOTE_GAP
    }

    /// Place the formatting toolbar: over the selection (the default), or above the text.
    pub fn toolbar(mut self, placement: super::toolbar::ToolbarPlacement) -> Self {
        self.toolbar = placement;
        self
    }

    /// Set the space around the text inside the editor, in pixels.
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// A fixed palette for a dark UI, whatever the theme.
    pub fn dark() -> Self {
        Self {
            text: Some(Color::rgba(0.92, 0.92, 0.95, 1.0)),
            muted: Some(Color::rgba(0.62, 0.62, 0.7, 1.0)),
            quote_border: Some(Color::rgba(0.35, 0.35, 0.42, 1.0)),
            divider: Some(Color::rgba(0.25, 0.25, 0.32, 1.0)),
            ..Self::default()
        }
    }

    /// A fixed palette for a light UI, whatever the theme.
    pub fn light() -> Self {
        Self {
            text: Some(Color::rgba(0.10, 0.10, 0.13, 1.0)),
            muted: Some(Color::rgba(0.42, 0.42, 0.5, 1.0)),
            quote_border: Some(Color::rgba(0.78, 0.78, 0.84, 1.0)),
            divider: Some(Color::rgba(0.88, 0.88, 0.92, 1.0)),
            ..Self::default()
        }
    }

    pub(super) fn heading_size(&self, level: u8) -> f32 {
        let i = (level.clamp(1, 6) - 1) as usize;
        self.heading_sizes[i]
    }
}

/// Colours from the stylesheet, so the editor follows the theme.
impl Default for RichTextTheme {
    fn default() -> Self {
        Self {
            text: None,
            muted: None,
            quote_border: None,
            divider: None,
            font_size: 15.0,
            heading_sizes: [30.0, 24.0, 20.0, 18.0, 16.0, 15.0],
            block_gap: 8.0,
            indent_step: 24.0,
            quote_border_width: 3.0,
            bullet_column_width: 18.0,
            line_height: 1.45,
            toolbar: Default::default(),
            padding: 0.0,
        }
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
    let mut root = div()
        .class("blinc-rich-text")
        .w_full()
        .flex_col()
        .gap_px(theme.block_gap);
    for (i, block) in doc.blocks.iter().enumerate() {
        root = root.child(render_block(
            block,
            Marker::of(doc, i),
            theme,
            content_width,
        ));
    }
    root
}

/// What marks a list item.
pub(super) enum Marker {
    Text(String),
    /// A checklist item's box.
    Task(Box<Div>),
}

impl Marker {
    /// The marker of the block at `index`, a task's box being one that cannot be pressed.
    fn of(doc: &RichDocument, index: usize) -> Option<Self> {
        let block = &doc.blocks[index];
        match block.kind {
            BlockKind::TaskItem { checked } => Some(Self::Task(Box::new(
                div().child(
                    checkbox(signal(checked))
                        .size(CheckboxSize::Small)
                        .disabled(true)
                        .build_component(),
                ),
            ))),
            _ => Self::text(doc, index).map(Self::Text),
        }
    }

    /// The text that marks the block at `index`, if it is a list item marked by text.
    pub(super) fn text(doc: &RichDocument, index: usize) -> Option<String> {
        let block = &doc.blocks[index];
        match block.kind {
            BlockKind::BulletItem => Some(["•", "◦", "▪"][block.indent as usize % 3].to_string()),
            BlockKind::NumberedItem => {
                Some(format!("{}.", doc.numbered_ordinal(index).unwrap_or(1)))
            }
            _ => None,
        }
    }
}

/// Build the element tree for a single block, with the `marker` of a list item.
///
/// `content_width` is the available pixel width *outside*
/// any indent: each block subtracts its own indent before passing the
/// remainder down to its inline content.
pub(super) fn render_block(
    block: &Block,
    marker: Option<Marker>,
    theme: &RichTextTheme,
    content_width: f32,
) -> Div {
    let indent_px = (block.indent as f32) * theme.indent_step;
    let inner_width = (content_width - indent_px).max(0.0);

    match &block.kind {
        BlockKind::Paragraph => text_block(
            block,
            theme,
            theme.font_size,
            FontWeight::Normal,
            indent_px,
            inner_width,
        ),
        BlockKind::Heading(level) => text_block(
            block,
            theme,
            theme.heading_size(*level),
            FontWeight::Bold,
            indent_px,
            inner_width,
        ),
        BlockKind::BulletItem | BlockKind::NumberedItem | BlockKind::TaskItem { .. } => {
            list_item_div(block, theme, marker, indent_px, inner_width)
        }
        BlockKind::Table { .. } => indented_block(table_div(block, theme, inner_width), indent_px),
        BlockKind::Divider => {
            let line = div().class("blinc-rich-text-divider").w_full().h(1.0);
            let line = match theme.divider {
                Some(c) => line.bg(c),
                None => line,
            };
            indented_block(line, indent_px)
        }
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
fn paragraph_div(
    block: &Block,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    line_width: f32,
) -> Div {
    let line_height_px = font_size * theme.line_height;
    let font = LineFont {
        size: font_size,
        weight,
        italic,
    };
    let mut col = div().w_full().flex_col();
    for source_line in &block.lines {
        let visual_lines = super::geometry::wrap_source_line(source_line, line_width, &font);
        let count = visual_lines.len();
        for (i, visual) in visual_lines.iter().enumerate() {
            let line_box = LineBox {
                width: line_width,
                align: block.align,
                last: i + 1 == count,
            };
            col = col.child(absolute_positioned_line(
                visual,
                theme,
                &font,
                &line_box,
                line_height_px,
            ));
        }
    }
    col
}

/// Build a single visual line, its text where [`super::line::runs`] places it, so the drawn glyphs
/// sit where the cursor and the selection measure them.
fn absolute_positioned_line(
    visual: &super::wrap::WrappedLine,
    theme: &RichTextTheme,
    font: &LineFont,
    line_box: &LineBox,
    line_height_px: f32,
) -> Div {
    let visual_line = &visual.line;
    let runs = super::line::runs(visual, font, line_box);
    let has_code = visual_line.spans.iter().any(|s| s.code);
    let justified = line_box.align == BlockAlign::Justify && !line_box.last;
    let (font_size, weight, italic) = (font.size, font.weight, font.italic);
    let line_width = line_box.width;

    // Everything on the line is put on the block's baseline, whatever its face.
    let baseline = run_baseline(font_size, theme.line_height, weight, italic);

    // One RichText draws a line that has no code and keeps its own spacing.
    if !has_code && !justified {
        // The child needs the width as well as the container: a
        // content-sized child is handed min-content and wraps inside a
        // box that is already the right size, so a heading broke across
        // two lines inside a one-line box and overlapped what followed.
        let x = runs.first().map_or(0.0, |(_, run)| run.x_in_line);
        let text = make_rich_text(visual_line, theme, font_size, weight, italic).baseline(baseline);
        if x <= 0.0 {
            return div()
                .w(line_width)
                .h(line_height_px)
                .child(text.w(line_width));
        }
        // A pixel spare, so rounding does not wrap the last word.
        let width = (line_width - x).ceil() + 1.0;
        return div().w(line_width).h(line_height_px).relative().child(
            div()
                .absolute()
                .left(x)
                .top(0.0)
                .w(width)
                .h(line_height_px)
                .child(text.w(width)),
        );
    }

    let code_font = blinc_layout::div::FontFamily::monospace();
    let mut container = div().w(line_width).h(line_height_px).relative();
    for (bytes, run) in runs {
        let is_code = run.font_family.generic == code_font.generic;
        // Draw the run without its edge spaces, and shift it right by
        // the leading space's width. Text rendering drops a leading or
        // trailing space, so a run whose box merely reserved room for
        // one came out butted against its neighbour: "a`code`via". The
        // space is now spent on position rather than left to the
        // renderer to honour.
        let lead = run.text.len() - run.text.trim_start().len();
        let drawn = run.text.trim();
        if drawn.is_empty() {
            continue;
        }
        let measure = |text: &str| {
            super::state::measure_width(
                text,
                run.font_size,
                run.weight,
                run.italic,
                Some(&run.font_family),
            )
        };
        let lead_w = if lead > 0 {
            measure(&run.text[..lead])
        } else {
            0.0
        };
        let start = bytes.start + lead;
        let run_line = slice_styled_line(visual_line, start, start + drawn.len());
        let draw_w = measure(drawn).ceil();
        let mut positioned = div();
        if is_code {
            positioned = positioned.class("blinc-rich-text-code");
        }
        container = container.child(
            positioned
                .absolute()
                .left(run.x_in_line + lead_w)
                .top(0.0)
                .w(draw_w)
                .h(line_height_px)
                .child(
                    if is_code {
                        make_code_run(&run_line, theme, font_size, weight, italic)
                    } else {
                        make_rich_text(&run_line, theme, font_size, weight, italic)
                    }
                    .w(draw_w)
                    .baseline(baseline),
                ),
        );
    }
    container
}

/// Build a `StyledLine` containing just the text and rebased spans for
/// byte range `[start, end)` of `source`.
fn slice_styled_line(
    source: &blinc_layout::styled_text::StyledLine,
    start: usize,
    end: usize,
) -> blinc_layout::styled_text::StyledLine {
    use blinc_layout::styled_text::{StyledLine, TextSpan};
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
    marker: Option<Marker>,
    indent_px: f32,
    inner_width: f32,
) -> Div {
    // Transparent: the marker takes the `.blinc-rich-text-marker` colour.
    let ink = theme.muted.unwrap_or(Color::TRANSPARENT);
    let column = div()
        .class("blinc-rich-text-marker")
        .flex_shrink_0()
        .w(theme.bullet_column_width);
    let column = match marker {
        Some(Marker::Text(text)) => {
            column.child(rich_text_styled(StyledText::plain(&text, ink)).size(theme.font_size))
        }
        // In the middle of the first line.
        Some(Marker::Task(task)) => column
            .h(theme.font_size * theme.line_height)
            .flex_row()
            .items_center()
            .child(*task),
        None => column,
    };
    // The marker column eats `bullet_column_width + 8` (gap) of the inner width.
    let line_width = (inner_width - theme.bullet_column_width - 8.0).max(0.0);

    let row = div()
        .w_full()
        .flex_row()
        .gap_px(8.0)
        .child(column)
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

/// Build a table: its cells where [`super::grid::layout`] puts them, and the lines between them.
fn table_div(block: &Block, theme: &RichTextTheme, width: f32) -> Div {
    let (cells, height) = super::grid::layout(block, theme, width);
    let line_height = theme.font_size * theme.line_height;
    let rule = || {
        let rule = div().class("blinc-rich-text-table-rule").absolute();
        match theme.divider {
            Some(c) => rule.bg(c),
            None => rule,
        }
    };
    let mut table = div()
        .class("blinc-rich-text-table")
        .relative()
        .w(width)
        .h(height)
        .border_width(1.0)
        // The header's fill follows the table's corners.
        .rounded(ThemeState::get().radius(RadiusToken::Md))
        .overflow_clip();
    let header = cells
        .iter()
        .filter(|c| c.header)
        .map(|c| c.height)
        .next()
        .unwrap_or_default();
    table = table.child(
        div()
            .class("blinc-rich-text-table-header")
            .absolute()
            .left(0.0)
            .top(0.0)
            .w(width)
            .h(header),
    );
    for cell in &cells {
        if cell.x > 0.0 && cell.header {
            table = table.child(rule().left(cell.x).top(0.0).w(1.0).h(height));
        }
        if cell.y > 0.0 && cell.x == 0.0 {
            table = table.child(rule().left(0.0).top(cell.y).w(width).h(1.0));
        }
        let (x, y) = cell.text_origin();
        for (i, line) in cell.lines.iter().enumerate() {
            let line_box = cell.line_box(i);
            table = table.child(
                div()
                    .absolute()
                    .left(x)
                    .top(y + i as f32 * line_height)
                    .w(line_box.width)
                    .h(line_height)
                    .child(absolute_positioned_line(
                        line,
                        theme,
                        &cell.font,
                        &line_box,
                        line_height,
                    )),
            );
        }
    }
    table
}

/// A paragraph or a heading: its lines, and beside them a bar where it is a quote, whose text is
/// in italics.
fn text_block(
    block: &Block,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    indent_px: f32,
    inner_width: f32,
) -> Div {
    if !block.quote {
        let lines = paragraph_div(block, theme, font_size, weight, false, inner_width);
        return indented_block(lines, indent_px);
    }
    // The bar and its gap take `quote_inset` of the width.
    let line_width = (inner_width - theme.quote_inset()).max(0.0);
    let inner = paragraph_div(block, theme, font_size, weight, true, line_width);
    let bar = div()
        .class("blinc-rich-text-quote-bar")
        .w(theme.quote_border_width);
    let bar = match theme.quote_border {
        Some(c) => bar.bg(c),
        None => bar,
    };
    let row = div()
        .w_full()
        .flex_row()
        .gap_px(QUOTE_GAP)
        .child(bar)
        .child(inner);
    indented_block(row, indent_px)
}

/// The baseline of a line of the document's text, from the top of its line box: half the leading,
/// then the face's ascender.
fn run_baseline(font_size: f32, line_height: f32, weight: FontWeight, italic: bool) -> f32 {
    let family = blinc_layout::div::FontFamily::default();
    let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
    options.font_weight = weight.weight();
    options.italic = italic;
    options.line_height = line_height;
    options.font_name = family.name.clone();
    options.generic_font = family.generic;
    // The string only selects the face; the metrics are the face's.
    let m = blinc_layout::text_measure::measure_text_with_options("Hxq", font_size, &options);
    let line_box = m.height / m.line_count.max(1) as f32;
    let half_leading = (line_box - (m.ascender - m.descender)) / 2.0;
    half_leading + m.ascender
}

/// Build a monospace `RichText` for a code run. No chip background or
/// internal padding — purely a font + color change — so the rendered
/// pixel width matches the geometry's `RunGeometry.width` exactly. Its
/// colour is the `.blinc-rich-text-code` rule's, from the run's wrapper.
fn make_code_run(
    line: &blinc_layout::styled_text::StyledLine,
    theme: &RichTextTheme,
    font_size: f32,
    _weight: FontWeight,
    _italic: bool,
) -> RichText {
    let mut chip_line = line.clone();
    for span in chip_line.spans.iter_mut() {
        // Drop the `code` flag so the inner `RichText` doesn't recurse,
        // and clear the colour so the code rule's applies.
        span.code = false;
        span.color = Color::TRANSPARENT;
    }
    let styled = StyledText::from_lines(vec![chip_line]);
    rich_text_styled(styled)
        .size(font_size)
        .weight(FontWeight::Normal)
        .line_height(theme.line_height)
        .monospace()
}

/// Build a `RichText` for a wrapped line. Each line goes into its own
/// `RichText`, so wrapping inside a single line never happens at the
/// shaper level — we already pre-wrap in `paragraph_div`.
///
/// Spans without a colour of their own take the inherited
/// `.blinc-rich-text` colour, or the theme's text colour when it sets one:
/// a span's colour is the only one that wins over an inherited rule.
fn make_rich_text(
    line: &blinc_layout::styled_text::StyledLine,
    theme: &RichTextTheme,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
) -> RichText {
    let mut line = line.clone();
    if let Some(c) = theme.text {
        for span in line.spans.iter_mut().filter(|s| s.color.a <= 0.0) {
            span.color = c;
        }
    }
    let styled = StyledText::from_lines(vec![line]);
    let mut rt = rich_text_styled(styled)
        .size(font_size)
        .weight(weight)
        .line_height(theme.line_height);
    if italic {
        rt = rt.italic();
    }
    rt
}

pub use super::geometry::compute_line_geometry;

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_layout::styled_text::TextSpan;

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
