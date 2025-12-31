//! Markdown to blinc layout renderer

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::div::{div, Div, ElementBuilder};
use crate::image::img;
use crate::text::text;
use crate::typography::{h1, h2, h3, h4, h5, h6};
use crate::widgets::{
    code, li, ol_start_with_config, ol_with_config, striped_tr, table, task_item,
    task_item_with_config, tbody, td, th, thead, tr, ul_with_config, ListConfig, ListItem,
    OrderedList, TaskListItem, UnorderedList,
};

use super::config::MarkdownConfig;


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
        // Set up parser with GFM extensions
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

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
}

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
}

impl<'a> RenderState<'a> {
    fn new(config: &'a MarkdownConfig) -> Self {
        Self {
            config,
            container: div().flex_col().gap(config.paragraph_spacing),
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
            Event::Html(_) => {} // Skip HTML for now
            Event::FootnoteReference(_) => {}
            Event::TaskListMarker(checked) => self.handle_task_marker(*checked),
            Event::InlineHtml(_) => {}
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
                    div().flex_col().gap(self.config.paragraph_spacing / 2.0),
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
                    self.stack.push(StackItem::OrderedList(
                        ol_start_with_config(self.list_start, list_config),
                    ));
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
                // Handle images inline
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
            Tag::FootnoteDefinition(_) => {}
            Tag::MetadataBlock(_) => {}
            Tag::HtmlBlock => {}
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
                } else if let Some(StackItem::TaskItem(item)) = self.stack.pop() {
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
            TagEnd::Image => {}
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
            TagEnd::FootnoteDefinition => {}
            TagEnd::MetadataBlock(_) => {}
            TagEnd::HtmlBlock => {}
        }
    }

    fn handle_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_content.push_str(text);
        } else {
            self.inline_text.push_str(text);
        }
    }

    fn handle_inline_code(&mut self, code_text: &str) {
        // Flush any accumulated styled segments to elements first
        self.flush_segments_to_elements();

        // Build inline code manually with matching size to body text for proper alignment
        // We need to set size BEFORE no_wrap() to ensure correct measurement
        let code_elem = text(code_text)
            .size(self.config.body_size) // Match body text size for baseline alignment
            .monospace()
            .color(self.config.code_text)
            .line_height(1.5)
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
            self.stack
                .push(StackItem::TaskItem(task_item_with_config(checked, list_config)));
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

            // Call no_wrap() last to trigger final measurement with correct styles
            txt = txt.no_wrap();

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
                _ => continue,
            }
        }

        // No special context, add to root container
        self.container = std::mem::replace(&mut self.container, div()).child(element);
    }

    fn into_container(self) -> Div {
        self.container
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::ThemeState;
    use crate::tree::LayoutTree;

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
        assert!(tree.len() > 0);
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
                println!("    {}: '{}' (bold={}, italic={})", j, seg.text, seg.bold, seg.italic);
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
                println!("    {}: '{}' (bold={}, italic={})", j, seg.text, seg.bold, seg.italic);
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
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("- Item 1\n- Item 2");
        content.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_code_block() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("```rust\nfn main() {}\n```");
        content.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_blockquote() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("> A quote");
        content.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_horizontal_rule() {
        init_theme();
        let mut tree = LayoutTree::new();
        let content = markdown("---");
        content.build(&mut tree);
        assert!(tree.len() > 0);
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
        let has_table_head = events.iter().any(|e| matches!(e, Event::Start(Tag::TableHead)));
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
}
