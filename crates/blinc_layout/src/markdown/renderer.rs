//! Markdown to blinc layout renderer

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::div::{Div, ElementBuilder, div};
use crate::image::img;
use crate::text::text;
use crate::typography::{h1, h2, h3, h4, h5, h6};
use crate::widgets::{
    ListConfig, ListItem, OrderedList, TaskListItem, UnorderedList, code, li, link,
    ol_start_with_config, ol_with_config, striped_tr, table, task_item, task_item_with_config,
    tbody, td, th, thead, tr, ul_with_config,
};

use super::config::MarkdownConfig;

// Re-export for HTML entity decoding
use html_escape::decode_html_entities;

/// Markdown renderer that converts markdown text to blinc layout elements
pub struct MarkdownRenderer {
    config: MarkdownConfig,
}

impl MarkdownRenderer {
    /// Create a new markdown renderer with default configuration
    pub fn new() -> Self {
        Self {
            config: MarkdownConfig::default(),
        }
    }

    /// Create a renderer with custom configuration
    pub fn with_config(config: MarkdownConfig) -> Self {
        Self { config }
    }

    /// Set the configuration
    pub fn config(mut self, config: MarkdownConfig) -> Self {
        self.config = config;
        self
    }

    /// Render markdown text to a Div containing all the elements
    pub fn render(&self, markdown_text: &str) -> Div {
        // Set up parser with GFM extensions and additional features
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = Parser::new_ext(markdown_text, options);
        let events: Vec<Event<'_>> = parser.collect();

        // Build the layout
        let mut renderer = RenderState::new(&self.config);
        renderer.render_events(&events);

        renderer.into_container()
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Render markdown to a Div
///
/// # Example
///
/// ```ignore
/// use blinc_layout::markdown::markdown;
///
/// let content = markdown("# Hello\n\nThis is *italic* and **bold**.");
/// ```
pub fn markdown(text: &str) -> Div {
    MarkdownRenderer::new().render(text)
}

/// Render markdown to a Div with custom configuration
pub fn markdown_with_config(text: &str, config: MarkdownConfig) -> Div {
    MarkdownRenderer::with_config(config).render(text)
}

/// Render markdown with light theme (for white backgrounds)
pub fn markdown_light(text: &str) -> Div {
    MarkdownRenderer::with_config(MarkdownConfig::light()).render(text)
}

// ============================================================================
// Internal render state
// ============================================================================

/// Inline style state for tracking bold/italic/strikethrough
#[derive(Clone, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strikethrough: bool,
    link_url: Option<String>,
}

/// A styled text segment with its content and styling
#[derive(Clone, Debug)]
#[allow(dead_code)] // Fields reserved for future link/decoration support
struct StyledSegment {
    text: String,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    underline: bool,
    color: blinc_core::Color,
    link_url: Option<String>,
}

/// State during rendering
struct RenderState<'a> {
    config: &'a MarkdownConfig,
    /// Root container for all elements
    container: Div,
    /// Stack of elements being built (for nesting)
    stack: Vec<StackItem>,
    /// Current inline text being accumulated (for current style)
    inline_text: String,
    /// Current inline styles
    inline_style: InlineStyle,
    /// Completed styled segments for the current paragraph
    styled_segments: Vec<StyledSegment>,
    /// Buffer of inline elements (for mixed text + inline_code)
    inline_elements: Vec<Box<dyn ElementBuilder>>,
    /// Current code block language
    code_language: Option<String>,
    /// Inside a code block
    in_code_block: bool,
    /// Code block content
    code_content: String,
    /// Current list item index (for ordered lists)
    list_item_index: usize,
    /// Current list start number
    list_start: usize,
    /// Table state
    in_table_head: bool,
    /// Current table body row index (for striped rows)
    table_row_index: usize,
    /// Inside an image tag (to skip alt text)
    in_image: bool,
    /// Footnote definitions collected during parsing (label -> content)
    footnote_defs: Vec<(String, Div)>,
    /// Current footnote being defined
    current_footnote: Option<String>,
    /// Footnote definition counter for numbering
    footnote_counter: usize,
    /// Inside a metadata block (skip content)
    in_metadata_block: bool,
    /// Inside an HTML block
    in_html_block: bool,
    /// HTML block content accumulator
    html_content: String,
}

#[allow(clippy::large_enum_variant)]
enum StackItem {
    Paragraph,
    Heading(u8),
    Blockquote(Div),
    UnorderedList(UnorderedList),
    OrderedList(OrderedList),
    ListItem(ListItem),
    TaskItem(TaskListItem),
    Link(String), // URL
    Table(Div),
    TableHead(Div),
    TableBody(Div),
    TableRow(Div),
    FootnoteDefinition(String, Div), // (label, content container)
}

impl<'a> RenderState<'a> {
    fn new(config: &'a MarkdownConfig) -> Self {
        Self {
            config,
            // `gap_px`: the config's spacings are pixels, while `gap`
            // takes 4px units — passing one straight through spaced
            // every block four times further apart than configured.
            container: div().flex_col().gap_px(config.paragraph_spacing),
            stack: Vec::new(),
            inline_text: String::new(),
            inline_style: InlineStyle::default(),
            styled_segments: Vec::new(),
            inline_elements: Vec::new(),
            code_language: None,
            in_code_block: false,
            code_content: String::new(),
            list_item_index: 0,
            list_start: 1,
            in_table_head: false,
            table_row_index: 0,
            in_image: false,
            footnote_defs: Vec::new(),
            current_footnote: None,
            footnote_counter: 0,
            in_metadata_block: false,
            in_html_block: false,
            html_content: String::new(),
        }
    }

    fn render_events(&mut self, events: &[Event<'_>]) {
        for event in events {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: &Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.handle_text(text),
            Event::Code(code) => self.handle_inline_code(code),
            Event::SoftBreak => self.handle_soft_break(),
            Event::HardBreak => self.handle_hard_break(),
            Event::Rule => self.handle_rule(),
            Event::Html(html) => self.handle_html(html),
            Event::FootnoteReference(label) => self.handle_footnote_reference(label),
            Event::TaskListMarker(checked) => self.handle_task_marker(*checked),
            Event::InlineHtml(html) => self.handle_inline_html(html),
        }
    }

    fn start_tag(&mut self, tag: &Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                self.stack.push(StackItem::Paragraph);
            }
            Tag::Heading { level, .. } => {
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.stack.push(StackItem::Heading(level));
            }
            Tag::BlockQuote => {
                // Note: blockquote() already has proper styling, just wrap in a div for flexibility
                self.stack.push(StackItem::Blockquote(
                    div().flex_col().gap_px(self.config.paragraph_spacing / 2.0),
                ));
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                self.code_content.clear();
                self.code_language = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
            }
            Tag::List(start) => {
                // Flush any pending inline text before starting a nested list
                // This ensures "Parent item" text appears before the nested list
                if let Some(content) = self.build_inline_content() {
                    self.add_to_current_context(content);
                }

                let list_config = ListConfig {
                    marker_width: self.config.list_marker_width,
                    marker_gap: self.config.list_marker_gap,
                    item_spacing: self.config.list_item_spacing,
                    indent: self.config.list_indent,
                    ..ListConfig::default()
                };
                if let Some(start_num) = start {
                    self.list_start = *start_num as usize;
                    self.list_item_index = 0;
                    self.stack.push(StackItem::OrderedList(ol_start_with_config(
                        self.list_start,
                        list_config,
                    )));
                } else {
                    self.list_item_index = 0;
                    self.stack
                        .push(StackItem::UnorderedList(ul_with_config(list_config)));
                }
            }
            Tag::Item => {
                self.stack.push(StackItem::ListItem(li()));
            }
            Tag::Emphasis => {
                self.flush_inline_text();
                self.inline_style.italic = true;
            }
            Tag::Strong => {
                self.flush_inline_text();
                self.inline_style.bold = true;
            }
            Tag::Strikethrough => {
                self.flush_inline_text();
                self.inline_style.strikethrough = true;
            }
            Tag::Link { dest_url, .. } => {
                self.flush_inline_text();
                self.inline_style.link_url = Some(dest_url.to_string());
                self.stack.push(StackItem::Link(dest_url.to_string()));
            }
            Tag::Image { dest_url, .. } => {
                // Handle images inline - mark that we're in an image to skip alt text
                self.in_image = true;
                let img_elem = img(dest_url.to_string());
                self.add_to_current_context(img_elem);
            }
            Tag::Table(_) => {
                // Create table with styling - use code_bg for background
                let tbl = table()
                    .w_full()
                    .bg(self.config.code_bg)
                    .rounded(4.0)
                    .overflow_clip();
                self.table_row_index = 0; // Reset row index for new table
                self.stack.push(StackItem::Table(tbl));
            }
            Tag::TableHead => {
                self.in_table_head = true;
                // Header section - use thead() which applies header_bg internally
                let head = thead();
                self.stack.push(StackItem::TableHead(head));
                // pulldown-cmark doesn't emit TableRow for header rows, only cells directly
                // So we need to push a row here to collect the header cells
                self.stack.push(StackItem::TableRow(tr()));
            }
            Tag::TableRow => {
                // Use striped_tr for body rows, regular tr for header rows
                let row = if self.in_table_head {
                    tr()
                } else {
                    let row = striped_tr(self.table_row_index);
                    self.table_row_index += 1;
                    row
                };
                self.stack.push(StackItem::TableRow(row));
            }
            Tag::TableCell => {
                // Cell content will be accumulated in inline_text
            }
            Tag::FootnoteDefinition(label) => {
                // Start collecting content for this footnote definition
                self.current_footnote = Some(label.to_string());
                self.footnote_counter += 1;
                // Create a container for the footnote content
                let footnote_content = div().flex_col().gap_px(self.config.paragraph_spacing / 2.0);
                self.stack.push(StackItem::FootnoteDefinition(
                    label.to_string(),
                    footnote_content,
                ));
            }
            Tag::MetadataBlock(_kind) => {
                // Skip metadata blocks (YAML frontmatter) - just mark that we're in one
                self.in_metadata_block = true;
            }
            Tag::HtmlBlock => {
                // Start collecting HTML content
                self.in_html_block = true;
                self.html_content.clear();
            }
        }
    }

    fn end_tag(&mut self, tag: &TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_paragraph();
                self.stack.pop();
            }
            TagEnd::Heading(level) => {
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.flush_heading(level);
                self.stack.pop();
            }
            TagEnd::BlockQuote => {
                if let Some(StackItem::Blockquote(bq_content)) = self.stack.pop() {
                    // Wrap the accumulated content in a blockquote widget with config colors
                    let bq_config = crate::widgets::BlockquoteConfig {
                        border_color: self.config.blockquote_border,
                        bg_color: self.config.blockquote_bg,
                        padding: self.config.blockquote_padding,
                        margin_y: self.config.paragraph_spacing / 2.0,
                        ..Default::default()
                    };
                    let bq = crate::widgets::blockquote_with_config(bq_config).child(bq_content);
                    self.add_to_current_context(bq);
                }
            }
            TagEnd::CodeBlock => {
                self.flush_code_block();
                self.in_code_block = false;
            }
            TagEnd::List(_) => match self.stack.pop() {
                Some(StackItem::UnorderedList(list)) => {
                    self.add_to_current_context(list);
                }
                Some(StackItem::OrderedList(list)) => {
                    self.add_to_current_context(list);
                }
                _ => {}
            },
            TagEnd::Item => {
                // Build inline content from accumulated elements
                let content = self.build_inline_content();

                // Create list config for placeholder lists
                let list_config = ListConfig {
                    marker_width: self.config.list_marker_width,
                    marker_gap: self.config.list_marker_gap,
                    item_spacing: self.config.list_item_spacing,
                    indent: self.config.list_indent,
                    ..ListConfig::default()
                };

                // Peek at the stack to determine what kind of item we have
                let is_task_item = matches!(self.stack.last(), Some(StackItem::TaskItem(_)));
                let is_list_item = matches!(self.stack.last(), Some(StackItem::ListItem(_)));

                if is_list_item {
                    if let Some(StackItem::ListItem(item)) = self.stack.pop() {
                        // Add content to item
                        let item = if let Some(content) = content {
                            item.child_box(Box::new(content))
                        } else {
                            item
                        };

                        // Add to parent list
                        match self.stack.last_mut() {
                            Some(StackItem::UnorderedList(list)) => {
                                let new_list =
                                    std::mem::replace(list, ul_with_config(list_config.clone()));
                                *list = new_list.child(item);
                                self.list_item_index += 1;
                            }
                            Some(StackItem::OrderedList(list)) => {
                                let new_list =
                                    std::mem::replace(list, ol_with_config(list_config.clone()));
                                *list = new_list.child(item);
                                self.list_item_index += 1;
                            }
                            _ => {}
                        }
                    }
                } else if is_task_item {
                    if let Some(StackItem::TaskItem(item)) = self.stack.pop() {
                        // Add content to task item
                        let item = if let Some(content) = content {
                            item.child_box(Box::new(content))
                        } else {
                            item
                        };

                        // Add task item to parent list
                        if let Some(StackItem::UnorderedList(list)) = self.stack.last_mut() {
                            let new_list = std::mem::replace(list, ul_with_config(list_config));
                            *list = new_list.child_element(item);
                            self.list_item_index += 1;
                        }
                    }
                }
            }
            TagEnd::Emphasis => {
                self.flush_inline_text();
                self.inline_style.italic = false;
            }
            TagEnd::Strong => {
                self.flush_inline_text();
                self.inline_style.bold = false;
            }
            TagEnd::Strikethrough => {
                self.flush_inline_text();
                self.inline_style.strikethrough = false;
            }
            TagEnd::Link => {
                self.flush_inline_text();
                self.inline_style.link_url = None;
                self.stack.pop(); // Pop the Link stack item
            }
            TagEnd::Image => {
                // Done with image, stop skipping alt text
                self.in_image = false;
            }
            TagEnd::Table => {
                // Close tbody if it's on the stack
                if let Some(StackItem::TableBody(body)) = self.stack.pop() {
                    if let Some(StackItem::Table(tbl)) = self.stack.last_mut() {
                        *tbl = std::mem::replace(tbl, div()).child(body);
                    }
                }
                // Now pop the table
                if let Some(StackItem::Table(tbl)) = self.stack.pop() {
                    self.add_to_current_context(tbl);
                }
            }
            TagEnd::TableHead => {
                self.in_table_head = false;
                // First pop the header row we added in Tag::TableHead
                if let Some(StackItem::TableRow(row)) = self.stack.pop() {
                    // Add row to thead
                    if let Some(StackItem::TableHead(head)) = self.stack.last_mut() {
                        *head = std::mem::replace(head, div()).child(row);
                    }
                }
                // Now pop the thead and add to table
                if let Some(StackItem::TableHead(head)) = self.stack.pop() {
                    if let Some(StackItem::Table(tbl)) = self.stack.last_mut() {
                        *tbl = std::mem::replace(tbl, div()).child(head);
                    }
                }
                // Start tbody for remaining rows
                self.stack.push(StackItem::TableBody(tbody()));
            }
            TagEnd::TableRow => {
                if let Some(StackItem::TableRow(row)) = self.stack.pop() {
                    match self.stack.last_mut() {
                        Some(StackItem::TableHead(head)) => {
                            *head = std::mem::replace(head, div()).child(row);
                        }
                        Some(StackItem::TableBody(body)) => {
                            *body = std::mem::replace(body, div()).child(row);
                        }
                        Some(StackItem::Table(tbl)) => {
                            // Direct child of table (no thead/tbody)
                            *tbl = std::mem::replace(tbl, div()).child(row);
                        }
                        _ => {}
                    }
                }
            }
            TagEnd::TableCell => {
                let cell_text = std::mem::take(&mut self.inline_text);
                // Use th() for header cells and td() for body cells
                let table_cell = if self.in_table_head {
                    th(&cell_text)
                } else {
                    td(&cell_text)
                };

                if let Some(StackItem::TableRow(row)) = self.stack.last_mut() {
                    *row = std::mem::replace(row, tr()).child(table_cell);
                }
            }
            TagEnd::FootnoteDefinition => {
                // Finish the footnote definition and store it
                if let Some(StackItem::FootnoteDefinition(label, content)) = self.stack.pop() {
                    self.footnote_defs.push((label, content));
                }
                self.current_footnote = None;
            }
            TagEnd::MetadataBlock(_) => {
                // Done skipping metadata block
                self.in_metadata_block = false;
            }
            TagEnd::HtmlBlock => {
                // Render accumulated HTML as preformatted text (basic support)
                self.in_html_block = false;
                let html = std::mem::take(&mut self.html_content);
                if !html.trim().is_empty() {
                    // Render HTML blocks as preformatted code-like text
                    let html_block = div().bg(self.config.code_bg).rounded(4.0).p(2.0).child(
                        text(&html)
                            .size(self.config.code_size)
                            .color(self.config.text_secondary)
                            .monospace(),
                    );
                    self.add_to_current_context(html_block);
                }
            }
        }
    }

    fn handle_text(&mut self, text: &str) {
        // Skip alt text inside images - it's only for accessibility/fallback
        if self.in_image {
            return;
        }

        // Skip text inside metadata blocks (YAML frontmatter)
        if self.in_metadata_block {
            return;
        }

        // Collect HTML block content
        if self.in_html_block {
            self.html_content.push_str(text);
            return;
        }

        if self.in_code_block {
            // Don't decode entities in code blocks - preserve literal text
            self.code_content.push_str(text);
        } else {
            // Decode HTML entities (e.g., &amp; -> &, &nbsp; -> non-breaking space)
            let decoded = decode_html_entities(text);
            self.inline_text.push_str(&decoded);
        }
    }

    fn handle_inline_code(&mut self, code_text: &str) {
        // Flush any accumulated styled segments to elements first
        self.flush_segments_to_elements();

        // Build inline code manually with matching size to body text for proper alignment
        // We need to set size BEFORE no_wrap() to ensure correct measurement
        let code_elem = text(code_text)
            .size(self.config.body_size + 2.0) // Match body text size for baseline alignment
            .monospace()
            .color(self.config.code_text)
            .line_height(1.0)
            .v_baseline()
            .no_wrap(); // Measurement happens here with correct size

        self.inline_elements.push(Box::new(code_elem));
    }

    fn handle_soft_break(&mut self) {
        // Soft break is rendered as a single space
        self.inline_text.push(' ');
    }

    fn handle_hard_break(&mut self) {
        // Hard break creates an actual line break
        // Flush current content, add a line break element
        self.flush_segments_to_elements();

        // Add a line break (full-width zero-height div forces next content to new line)
        let line_break = div().w_full().h(0.0);
        self.inline_elements.push(Box::new(line_break));
    }

    fn handle_rule(&mut self) {
        // Use config color and minimal margin
        let rule = crate::widgets::hr_with_config(crate::widgets::HrConfig {
            color: self.config.hr_color,
            thickness: 1.0,
            margin_y: 4.0,
        });
        self.add_to_current_context(rule);
    }

    fn handle_task_marker(&mut self, checked: bool) {
        // Convert the current ListItem to a TaskItem with config
        if let Some(StackItem::ListItem(_)) = self.stack.pop() {
            let list_config = ListConfig {
                marker_width: self.config.list_marker_width,
                marker_gap: self.config.list_marker_gap,
                item_spacing: self.config.list_item_spacing,
                indent: self.config.list_indent,
                ..ListConfig::default()
            };
            self.stack.push(StackItem::TaskItem(task_item_with_config(
                checked,
                list_config,
            )));
        }
    }

    fn flush_inline_text(&mut self) {
        if self.inline_text.is_empty() {
            return;
        }

        // Determine color and underline based on whether this is a link
        let (color, underline) = if self.inline_style.link_url.is_some() {
            (self.config.link_color, true)
        } else {
            (self.config.text_color, false)
        };

        // Create a styled segment with the current text and style
        let segment = StyledSegment {
            text: std::mem::take(&mut self.inline_text),
            bold: self.inline_style.bold,
            italic: self.inline_style.italic,
            strikethrough: self.inline_style.strikethrough,
            underline,
            color,
            link_url: self.inline_style.link_url.clone(),
        };

        self.styled_segments.push(segment);
    }

    /// Flush styled segments into the element buffer
    fn flush_segments_to_elements(&mut self) {
        // First flush any remaining inline text to segments
        self.flush_inline_text();

        // Convert segments to text elements and add to buffer
        // Use span() for inline text which preserves natural spacing
        let segments = std::mem::take(&mut self.styled_segments);
        for segment in segments {
            if segment.text.is_empty() {
                continue;
            }

            // If this is a link, use the link widget for proper cursor support
            if let Some(url) = &segment.link_url {
                let link_elem = link(&segment.text, url)
                    .font_size(self.config.body_size)
                    .text_color(segment.color);
                self.inline_elements.push(Box::new(link_elem));
                continue;
            }

            // Build text with styles set BEFORE no_wrap() so measurement includes correct weight/style
            let mut txt = text(&segment.text)
                .size(self.config.body_size)
                .color(segment.color)
                .line_height(1.5);

            if segment.bold {
                txt = txt.bold();
            }
            if segment.italic {
                txt = txt.italic();
            }
            if segment.strikethrough {
                txt = txt.strikethrough();
            }
            if segment.underline {
                txt = txt.underline();
            }

            // Call no_wrap() last to trigger final measurement with correct styles
            txt = txt.v_baseline().no_wrap();

            self.inline_elements.push(Box::new(txt));
        }
    }

    /// Build a row div from the element buffer, consuming it
    fn build_inline_content(&mut self) -> Option<Div> {
        // Flush any remaining segments to elements
        self.flush_segments_to_elements();

        let elements = std::mem::take(&mut self.inline_elements);

        if elements.is_empty() {
            return None;
        }

        // Build a flex row with baseline alignment
        // No gap needed - text elements include their natural spacing
        let mut row = div().flex_row().flex_wrap().items_baseline();
        for elem in elements {
            row = row.child_box(elem);
        }

        Some(row)
    }

    fn flush_paragraph(&mut self) {
        // Build inline content from segments and elements
        if let Some(content) = self.build_inline_content() {
            self.add_to_current_context(content);
        }
    }

    fn flush_heading(&mut self, level: u8) {
        // Flush any remaining inline text first
        self.flush_inline_text();

        // Clear any inline elements (headings don't support inline code etc.)
        self.inline_elements.clear();

        let segments = std::mem::take(&mut self.styled_segments);

        if segments.is_empty() {
            return;
        }

        // Combine all segment text for the heading
        let text_content: String = segments.iter().map(|s| s.text.as_str()).collect();

        // Use config font sizes and apply text color
        let (heading, size) = match level {
            1 => (h1(&text_content), self.config.h1_size),
            2 => (h2(&text_content), self.config.h2_size),
            3 => (h3(&text_content), self.config.h3_size),
            4 => (h4(&text_content), self.config.h4_size),
            5 => (h5(&text_content), self.config.h5_size),
            _ => (h6(&text_content), self.config.h6_size),
        };

        let heading = heading.size(size).color(self.config.text_color);
        self.add_to_current_context(heading);
    }

    fn flush_code_block(&mut self) {
        let content = std::mem::take(&mut self.code_content);
        let _lang = self.code_language.take();

        // Create code block (without syntax highlighting for now)
        // Note: code() returns a Code struct that derefs to Div
        // We can't chain Div methods after Code methods due to Deref ownership rules
        let code_block = code(&content)
            .line_numbers(true)
            .font_size(self.config.code_size);

        self.add_to_current_context(code_block);
    }

    fn add_to_current_context(&mut self, element: impl ElementBuilder + 'static) {
        // Find the appropriate parent to add to
        for item in self.stack.iter_mut().rev() {
            match item {
                StackItem::Blockquote(bq) => {
                    *bq = std::mem::replace(bq, div()).child(element);
                    return;
                }
                StackItem::ListItem(list_item) => {
                    let new_item = std::mem::replace(list_item, li());
                    *list_item = new_item.child(element);
                    return;
                }
                StackItem::TaskItem(ti) => {
                    *ti = std::mem::replace(ti, task_item(false)).child(element);
                    return;
                }
                StackItem::FootnoteDefinition(_, content) => {
                    *content = std::mem::replace(content, div()).child(element);
                    return;
                }
                _ => continue,
            }
        }

        // No special context, add to root container
        self.container = std::mem::replace(&mut self.container, div()).child(element);
    }

    fn handle_footnote_reference(&mut self, label: &str) {
        // Flush any pending text first
        self.flush_segments_to_elements();

        // Render as a link-styled element with pointer cursor
        // TODO: In the future, this could scroll to the footnote definition
        let footnote_ref = link(format!("[{}]", label), format!("#footnote-{}", label))
            .font_size(self.config.body_size * 0.75) // Smaller, superscript-like
            .text_color(self.config.link_color);

        self.inline_elements.push(Box::new(footnote_ref));
    }

    fn handle_html(&mut self, html: &str) {
        // Block-level HTML - try to parse known tags, otherwise render as preformatted
        let trimmed = html.trim();
        if trimmed.is_empty() {
            return;
        }

        // Try to parse as a known block-level HTML tag
        if let Some(element) = self.parse_html_block(trimmed) {
            self.container = std::mem::replace(&mut self.container, div()).child_box(element);
        } else {
            // Fallback: render as preformatted code-like text
            let html_block = div().bg(self.config.code_bg).rounded(4.0).p(2.0).child(
                text(trimmed)
                    .size(self.config.code_size)
                    .color(self.config.text_secondary)
                    .monospace(),
            );
            self.add_to_current_context(html_block);
        }
    }

    fn handle_inline_html(&mut self, html: &str) {
        // Inline HTML - try to parse known inline tags
        let trimmed = html.trim();
        if trimmed.is_empty() {
            return;
        }

        // Handle common inline HTML tags by modifying inline style
        if let Some(tag_name) = Self::parse_html_tag_name(trimmed) {
            let is_closing = trimmed.starts_with("</");

            match tag_name.to_lowercase().as_str() {
                "strong" | "b" => {
                    self.flush_inline_text();
                    self.inline_style.bold = !is_closing;
                    return;
                }
                "em" | "i" => {
                    self.flush_inline_text();
                    self.inline_style.italic = !is_closing;
                    return;
                }
                "s" | "del" | "strike" => {
                    self.flush_inline_text();
                    self.inline_style.strikethrough = !is_closing;
                    return;
                }
                "u" | "ins" => {
                    // Underline - we don't have a direct flag but can use link style
                    self.flush_inline_text();
                    // No direct underline support in inline_style, skip for now
                    return;
                }
                "code" => {
                    // Inline code handled separately
                    return;
                }
                "br" => {
                    // Line break
                    self.handle_hard_break();
                    return;
                }
                "sup" | "sub" => {
                    // Super/subscript - we don't have native support, skip
                    return;
                }
                _ => {}
            }
        }

        // Fallback: render as inline code-like text for unknown tags
        self.flush_segments_to_elements();
        let html_span = text(trimmed)
            .size(self.config.code_size)
            .color(self.config.text_secondary)
            .monospace()
            .v_baseline()
            .no_wrap();
        self.inline_elements.push(Box::new(html_span));
    }

    /// Parse an HTML tag name from an HTML string like "<strong>text</strong>" or "<br>"
    fn parse_html_tag_name(html: &str) -> Option<&str> {
        let html = html.trim();
        if !html.starts_with('<') {
            return None;
        }

        // Find the end of the opening tag
        let tag_end = html.find('>')?;

        // Get the opening tag content (between < and >)
        let tag_content = &html[1..tag_end];

        // Handle closing tags (skip the /)
        let tag_content = tag_content.strip_prefix('/').unwrap_or(tag_content);

        // Get tag name (until space, /, or end)
        let end = tag_content
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(tag_content.len());
        let tag_name = &tag_content[..end];

        if tag_name.is_empty() {
            None
        } else {
            Some(tag_name)
        }
    }

    /// Try to parse a block-level HTML element and return a corresponding layout element
    fn parse_html_block(&self, html: &str) -> Option<Box<dyn ElementBuilder>> {
        // Extract the outer tag name
        let tag_name = Self::parse_html_tag_name(html)?;

        match tag_name.to_lowercase().as_str() {
            "hr" => Some(Box::new(crate::widgets::hr_with_config(
                crate::widgets::HrConfig {
                    color: self.config.hr_color,
                    thickness: 1.0,
                    margin_y: 4.0,
                },
            ))),
            "br" => {
                // Block-level br is just empty space
                Some(Box::new(div().h(self.config.paragraph_spacing)))
            }
            "div" | "section" | "article" | "aside" | "header" | "footer" | "main" | "nav" => {
                // Container elements - extract inner content and render as a div
                if let Some(inner) = Self::extract_tag_content(html, tag_name) {
                    // For now, just render inner content as text
                    // A full implementation would recursively parse
                    Some(Box::new(
                        div().p(2.0).child(
                            text(inner)
                                .size(self.config.body_size)
                                .color(self.config.text_color),
                        ),
                    ))
                } else {
                    None
                }
            }
            "p" => {
                if let Some(inner) = Self::extract_tag_content(html, tag_name) {
                    Some(Box::new(
                        text(inner)
                            .size(self.config.body_size)
                            .color(self.config.text_color),
                    ))
                } else {
                    None
                }
            }
            "pre" => {
                if let Some(inner) = Self::extract_tag_content(html, tag_name) {
                    Some(Box::new(code(inner).font_size(self.config.code_size)))
                } else {
                    None
                }
            }
            "blockquote" => {
                if let Some(inner) = Self::extract_tag_content(html, tag_name) {
                    let bq_config = crate::widgets::BlockquoteConfig {
                        border_color: self.config.blockquote_border,
                        bg_color: self.config.blockquote_bg,
                        padding: self.config.blockquote_padding,
                        margin_y: self.config.paragraph_spacing / 2.0,
                        ..Default::default()
                    };
                    Some(Box::new(
                        crate::widgets::blockquote_with_config(bq_config).child(
                            text(inner)
                                .size(self.config.body_size)
                                .color(self.config.text_secondary),
                        ),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract content between opening and closing tags
    fn extract_tag_content<'b>(html: &'b str, tag_name: &str) -> Option<&'b str> {
        // Simple extraction - find content between <tag...> and </tag>
        let lower = html.to_lowercase();
        let open_end = html.find('>')?;
        let close_start = lower.rfind(&format!("</{}", tag_name.to_lowercase()))?;

        if open_end + 1 < close_start {
            Some(html[open_end + 1..close_start].trim())
        } else {
            None
        }
    }

    fn into_container(mut self) -> Div {
        // Append footnote definitions at the end if any exist
        if !self.footnote_defs.is_empty() {
            // Add a separator
            let separator = crate::widgets::hr_with_config(crate::widgets::HrConfig {
                color: self.config.hr_color,
                thickness: 1.0,
                margin_y: 8.0,
            });
            self.container = std::mem::replace(&mut self.container, div()).child(separator);

            // Add footnotes section
            let mut footnotes_section =
                div().flex_col().gap_px(self.config.paragraph_spacing / 2.0);

            for (label, content) in self.footnote_defs {
                // Create footnote row: number + content
                let footnote_row = div()
                    .flex_row()
                    .gap_px(8.0)
                    .items_start()
                    .child(
                        text(format!("[{}]", label))
                            .size(self.config.body_size * 0.85)
                            .color(self.config.text_secondary),
                    )
                    .child(content);

                footnotes_section = footnotes_section.child(footnote_row);
            }

            self.container = std::mem::replace(&mut self.container, div()).child(footnotes_section);
        }

        self.container.w_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::LayoutTree;
    use blinc_theme::ThemeState;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_simple_paragraph() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("Hello world");
        content.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_bold_spacing_events() {
        // Test that pulldown-cmark preserves spaces around styled text
        let md = "This is **bold** and text";
        let parser = Parser::new_ext(md, Options::empty());
        let events: Vec<_> = parser.collect();

        // Print events for debugging
        for (i, event) in events.iter().enumerate() {
            println!("{}: {:?}", i, event);
        }

        // Verify we get the space before "and"
        let has_space_after_bold = events.iter().any(|e| {
            if let Event::Text(t) = e {
                t.starts_with(" and") || t.as_ref() == " and text"
            } else {
                false
            }
        });
        assert!(has_space_after_bold, "Expected space after bold text");
    }

    #[test]
    fn test_bold_spacing_segments() {
        init_theme();
        // Test that our renderer preserves spaces in segments
        let md = "This is **bold** and text";

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(md, options);
        let events: Vec<Event<'_>> = parser.collect();

        // Print all events
        println!("Events:");
        for (i, event) in events.iter().enumerate() {
            println!("  {}: {:?}", i, event);
        }

        let config = super::super::config::MarkdownConfig::default();
        let mut renderer = RenderState::new(&config);

        // Process events one by one and trace what happens
        for (i, event) in events.iter().enumerate() {
            println!("\nProcessing event {}: {:?}", i, event);
            renderer.handle_event(event);
            println!("  inline_text: '{}'", renderer.inline_text);
            println!("  styled_segments: {}", renderer.styled_segments.len());
            for (j, seg) in renderer.styled_segments.iter().enumerate() {
                println!(
                    "    {}: '{}' (bold={}, italic={})",
                    j, seg.text, seg.bold, seg.italic
                );
            }
            println!("  inline_elements: {}", renderer.inline_elements.len());
        }
    }

    #[test]
    fn test_italic_spacing_segments() {
        init_theme();
        // Test italic spacing vs bold
        let md = "This is *italic* and text";

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(md, options);
        let events: Vec<Event<'_>> = parser.collect();

        // Print all events
        println!("Events:");
        for (i, event) in events.iter().enumerate() {
            println!("  {}: {:?}", i, event);
        }

        let config = super::super::config::MarkdownConfig::default();
        let mut renderer = RenderState::new(&config);

        // Process events one by one and trace what happens
        for (i, event) in events.iter().enumerate() {
            println!("\nProcessing event {}: {:?}", i, event);
            renderer.handle_event(event);
            println!("  inline_text: '{}'", renderer.inline_text);
            println!("  styled_segments: {}", renderer.styled_segments.len());
            for (j, seg) in renderer.styled_segments.iter().enumerate() {
                println!(
                    "    {}: '{}' (bold={}, italic={})",
                    j, seg.text, seg.bold, seg.italic
                );
            }
            println!("  inline_elements: {}", renderer.inline_elements.len());
        }
    }

    #[test]
    fn test_heading() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("# Hello");
        content.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("- Item 1\n- Item 2");
        content.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_code_block() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("```rust\nfn main() {}\n```");
        content.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_blockquote() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("> A quote");
        content.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_horizontal_rule() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("---");
        content.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_table_parsing_events() {
        let md = r#"| Feature | Status |
|---------|--------|
| Headings | Done |"#;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);

        let parser = Parser::new_ext(md, options);
        let events: Vec<_> = parser.collect();

        // Print events for debugging
        for (i, event) in events.iter().enumerate() {
            println!("{}: {:?}", i, event);
        }

        // Verify we get TableHead events
        let has_table_head = events
            .iter()
            .any(|e| matches!(e, Event::Start(Tag::TableHead)));
        assert!(has_table_head, "Expected TableHead event");

        // Verify we get header text
        let has_feature_text = events.iter().any(|e| {
            if let Event::Text(t) = e {
                t.as_ref() == "Feature"
            } else {
                false
            }
        });
        assert!(has_feature_text, "Expected 'Feature' text");
    }

    #[test]
    fn test_table_builds_with_headers() {
        init_theme();
        let mut tree = LayoutTree::new();
        let md = r#"| Feature | Status |
|---------|--------|
| Headings | Done |"#;
        let content = markdown(md);
        content.build(&mut tree);

        // Should have multiple nodes (table, thead, tbody, rows, cells)
        println!("Tree has {} nodes", tree.len());
        assert!(tree.len() > 5, "Table should have multiple nodes");
    }

    #[test]
    fn test_nested_list_events() {
        // Test that pulldown-cmark generates nested list events correctly
        let md = r#"- Item 1
  - Nested item
- Item 2"#;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(md, options);
        let events: Vec<_> = parser.collect();

        // Print events for debugging
        println!("Nested list events:");
        for (i, event) in events.iter().enumerate() {
            println!("{}: {:?}", i, event);
        }

        // Should have two List starts (outer and nested)
        let list_starts = events
            .iter()
            .filter(|e| matches!(e, Event::Start(Tag::List(_))))
            .count();
        assert_eq!(list_starts, 2, "Expected 2 list starts (outer + nested)");
    }

    #[test]
    fn test_task_list_events() {
        // Test that pulldown-cmark generates task list events correctly
        let md = r#"- [x] Done task
- [ ] Pending task"#;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(md, options);
        let events: Vec<_> = parser.collect();

        // Print events for debugging
        println!("Task list events:");
        for (i, event) in events.iter().enumerate() {
            println!("{}: {:?}", i, event);
        }

        // Should have TaskListMarker events
        let task_markers = events
            .iter()
            .filter(|e| matches!(e, Event::TaskListMarker(_)))
            .count();
        assert_eq!(task_markers, 2, "Expected 2 task list markers");
    }

    #[test]
    fn test_nested_list_renders() {
        init_theme();
        let mut tree = LayoutTree::new();
        let md = r#"- Item 1
  - Nested item
- Item 2"#;
        let content = markdown(md);
        content.build(&mut tree);

        println!("Nested list tree has {} nodes", tree.len());
        // Should have nodes for: container, outer list, 2 outer items, nested list, nested item
        assert!(tree.len() > 5, "Nested list should have multiple nodes");
    }

    #[test]
    fn test_task_list_renders() {
        init_theme();
        let mut tree = LayoutTree::new();
        let md = r#"- [x] Done task
- [ ] Pending task"#;
        let content = markdown(md);
        content.build(&mut tree);

        println!("Task list tree has {} nodes", tree.len());
        // Should have nodes for: container, list, 2 task items with checkboxes and text
        assert!(tree.len() > 3, "Task list should have multiple nodes");
    }

    #[test]
    fn test_image_alt_text_not_rendered() {
        // Verify that alt text is not rendered as visible text
        let md = r#"![Alt text should not appear](image.png)"#;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(md, options);
        let events: Vec<_> = parser.collect();

        // Print events for debugging
        println!("Image events:");
        for (i, event) in events.iter().enumerate() {
            println!("{}: {:?}", i, event);
        }

        // Verify the text event exists (pulldown-cmark does emit it)
        let text_events = events
            .iter()
            .filter(|e| matches!(e, Event::Text(_)))
            .count();
        assert_eq!(text_events, 1, "Alt text should be emitted as Text event");

        // But our renderer should skip it - verify by building
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown(md);
        content.build(&mut tree);

        // Should have minimal nodes: container + image (no text nodes for alt)
        println!("Image tree has {} nodes", tree.len());
        assert!(tree.len() >= 2, "Should have container and image");
    }

    #[test]
    fn test_footnote_events() {
        // Test that pulldown-cmark generates footnote events correctly
        let md = r#"This has a footnote[^1].

[^1]: This is the footnote content."#;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_FOOTNOTES);

        let parser = Parser::new_ext(md, options);
        let events: Vec<_> = parser.collect();

        // Print events for debugging
        println!("Footnote events:");
        for (i, event) in events.iter().enumerate() {
            println!("{}: {:?}", i, event);
        }

        // Should have FootnoteReference and FootnoteDefinition events
        let footnote_refs = events
            .iter()
            .filter(|e| matches!(e, Event::FootnoteReference(_)))
            .count();
        assert!(footnote_refs >= 1, "Expected at least 1 footnote reference");

        let footnote_defs = events
            .iter()
            .filter(|e| matches!(e, Event::Start(Tag::FootnoteDefinition(_))))
            .count();
        assert!(
            footnote_defs >= 1,
            "Expected at least 1 footnote definition"
        );
    }

    #[test]
    fn test_footnote_renders() {
        init_theme();
        let mut tree = LayoutTree::new();
        let md = r#"This has a footnote[^1].

[^1]: This is the footnote content."#;
        let content = markdown(md);
        content.build(&mut tree);

        println!("Footnote tree has {} nodes", tree.len());
        // Should have nodes for: container, paragraph, footnote ref, separator, footnote section
        assert!(
            tree.len() > 3,
            "Footnote content should have multiple nodes"
        );
    }

    #[test]
    fn test_yaml_metadata_block_skipped() {
        init_theme();
        let mut tree = LayoutTree::new();
        // YAML frontmatter should be parsed but not rendered
        let md = r#"---
title: My Document
author: Test Author
---

# Actual Content

This is the body."#;
        let content = markdown(md);
        content.build(&mut tree);

        println!("Metadata block tree has {} nodes", tree.len());
        // Should have nodes for heading and paragraph, but not the metadata
        assert!(tree.len() > 2, "Should have heading and paragraph nodes");
    }

    #[test]
    fn test_html_block_renders() {
        init_theme();
        let mut tree = LayoutTree::new();
        let md = r#"Regular paragraph.

<div class="custom">
  <p>HTML content</p>
</div>

Another paragraph."#;
        let content = markdown(md);
        content.build(&mut tree);

        println!("HTML block tree has {} nodes", tree.len());
        // Should have nodes for paragraphs and HTML block
        assert!(tree.len() > 2, "Should have paragraph and HTML nodes");
    }

    #[test]
    fn test_inline_html_renders() {
        init_theme();
        let mut tree = LayoutTree::new();
        let md = r#"This is <em>emphasized</em> text."#;
        let content = markdown(md);
        content.build(&mut tree);

        println!("Inline HTML tree has {} nodes", tree.len());
        // Should have nodes for the paragraph with inline elements
        assert!(tree.len() > 1, "Should have paragraph with inline HTML");
    }

    #[test]
    fn test_html_parsing_functions() {
        // Test the HTML parsing functions directly
        let html_p = "<p>Block-level HTML paragraphs.</p>";
        let html_bq = "<blockquote>HTML blockquote content.</blockquote>";

        // Test tag name extraction
        let tag_p = RenderState::parse_html_tag_name(html_p);
        let tag_bq = RenderState::parse_html_tag_name(html_bq);

        assert_eq!(tag_p, Some("p"));
        assert_eq!(tag_bq, Some("blockquote"));

        // Test content extraction
        let content_p = RenderState::extract_tag_content(html_p, "p");
        let content_bq = RenderState::extract_tag_content(html_bq, "blockquote");

        assert_eq!(content_p, Some("Block-level HTML paragraphs."));
        assert_eq!(content_bq, Some("HTML blockquote content."));
    }
}
