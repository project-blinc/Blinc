//! List widgets for ordered and unordered lists
//!
//! Provides HTML-like list elements: `ul`, `ol`, `li` for creating structured lists.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! // Unordered list
//! ul()
//!     .child(li().child(p("Item 1")))
//!     .child(li().child(p("Item 2")))
//!     .child(li().child(p("Item 3")))
//!
//! // Ordered list
//! ol()
//!     .child(li().child(p("First")))
//!     .child(li().child(p("Second")))
//!     .child(li().child(p("Third")))
//!
//! // Nested list
//! ul()
//!     .child(li().child(p("Parent item"))
//!         .child(ul()
//!             .child(li().child(p("Nested item")))
//!         )
//!     )
//! ```

use std::ops::{Deref, DerefMut};

use blinc_core::Color;
use blinc_theme::{ColorToken, ThemeState};

use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::svg::svg;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// List marker style
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListMarker {
    /// Filled circle (•)
    #[default]
    Disc,
    /// Empty circle (○)
    Circle,
    /// Filled square (▪)
    Square,
    /// Decimal numbers (1, 2, 3...)
    Decimal,
    /// Lowercase letters (a, b, c...)
    LowerAlpha,
    /// Uppercase letters (A, B, C...)
    UpperAlpha,
    /// Lowercase roman numerals (i, ii, iii...)
    LowerRoman,
    /// Uppercase roman numerals (I, II, III...)
    UpperRoman,
    /// No marker
    None,
}

impl ListMarker {
    /// Get the marker string for a given index
    pub fn marker_for(&self, index: usize) -> String {
        match self {
            ListMarker::Disc => "•".to_string(),
            ListMarker::Circle => "○".to_string(),
            ListMarker::Square => "▪".to_string(),
            ListMarker::Decimal => format!("{}.", index + 1),
            ListMarker::LowerAlpha => {
                if index < 26 {
                    format!("{}.", (b'a' + index as u8) as char)
                } else {
                    format!("{}.", index + 1)
                }
            }
            ListMarker::UpperAlpha => {
                if index < 26 {
                    format!("{}.", (b'A' + index as u8) as char)
                } else {
                    format!("{}.", index + 1)
                }
            }
            ListMarker::LowerRoman => format!("{}.", to_roman(index + 1).to_lowercase()),
            ListMarker::UpperRoman => format!("{}.", to_roman(index + 1)),
            ListMarker::None => String::new(),
        }
    }
}

/// Convert number to roman numeral (basic implementation)
fn to_roman(mut n: usize) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }

    let numerals = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut result = String::new();
    for (value, symbol) in numerals {
        while n >= value {
            result.push_str(symbol);
            n -= value;
        }
    }
    result
}

/// Configuration for list styling
#[derive(Clone, Debug)]
pub struct ListConfig {
    /// Color of list markers
    pub marker_color: Color,
    /// Width reserved for the marker
    pub marker_width: f32,
    /// Gap between marker and content
    pub marker_gap: f32,
    /// Spacing between list items
    pub item_spacing: f32,
    /// Left margin for the entire list (for nesting)
    pub indent: f32,
    /// Font size for markers
    pub marker_font_size: f32,
}

impl Default for ListConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            marker_color: theme.color(ColorToken::TextTertiary),
            marker_width: 24.0,
            marker_gap: 8.0,
            item_spacing: 4.0,
            indent: 0.0,
            marker_font_size: 14.0,
        }
    }
}

// ============================================================================
// Unordered List
// ============================================================================

/// An unordered list container
pub struct UnorderedList {
    inner: Div,
    config: ListConfig,
    marker: ListMarker,
    item_count: usize,
    css_element_id: Option<String>,
    css_classes: Vec<std::sync::Arc<str>>,
}

impl UnorderedList {
    /// Create a new unordered list
    pub fn new() -> Self {
        Self::with_config(ListConfig::default())
    }

    /// Create a new unordered list with custom config
    pub fn with_config(config: ListConfig) -> Self {
        // `gap_px`: `item_spacing` is pixels, `gap` takes 4px units.
        let inner = div()
            .flex_col()
            .gap_px(config.item_spacing)
            .ml(config.indent / 4.0);

        Self {
            inner,
            config,
            marker: ListMarker::Disc,
            item_count: 0,
            css_element_id: None,
            css_classes: Vec::new(),
        }
    }

    /// Add a list item
    pub fn child(mut self, item: ListItem) -> Self {
        let item = item.with_marker_and_config(self.marker, Some(self.item_count), &self.config);
        self.inner = self.inner.child(item);
        self.item_count += 1;
        self
    }

    /// Add any element as a child (for nesting lists)
    pub fn child_element(mut self, element: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(element);
        self
    }

    /// Add a boxed child element.
    pub fn child_box(mut self, element: Box<dyn ElementBuilder>) -> Self {
        self.inner = self.inner.child_box(element);
        self
    }

    /// Set the marker style
    pub fn marker(mut self, marker: ListMarker) -> Self {
        self.marker = marker;
        self
    }

    /// Set the indent (for nested lists)
    pub fn indent(mut self, indent: f32) -> Self {
        self.config.indent = indent;
        self.inner = self.inner.ml(indent / 4.0);
        self
    }

    /// Set item spacing
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.config.item_spacing = spacing;
        self.inner = self.inner.gap_px(spacing);
        self
    }

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.css_element_id = Some(id.to_string());
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: &str) -> Self {
        self.css_classes.push(blinc_core::intern::intern(name));
        self
    }
}

impl Default for UnorderedList {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for UnorderedList {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for UnorderedList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for UnorderedList {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("ul")
    }

    fn element_id(&self) -> Option<&str> {
        self.css_element_id.as_deref()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.css_classes
    }
}

// ============================================================================
// Ordered List
// ============================================================================

/// An ordered list container
pub struct OrderedList {
    inner: Div,
    config: ListConfig,
    marker: ListMarker,
    start: usize,
    item_count: usize,
    css_element_id: Option<String>,
    css_classes: Vec<std::sync::Arc<str>>,
}

impl OrderedList {
    /// Create a new ordered list starting at 1
    pub fn new() -> Self {
        Self::starting_at(1)
    }

    /// Create a new ordered list with custom config
    pub fn with_config(config: ListConfig) -> Self {
        Self::starting_at_with_config(1, config)
    }

    /// Create an ordered list starting at a specific number
    pub fn starting_at(start: usize) -> Self {
        Self::starting_at_with_config(start, ListConfig::default())
    }

    /// Create an ordered list starting at a specific number with custom config
    pub fn starting_at_with_config(start: usize, config: ListConfig) -> Self {
        // `gap_px`: `item_spacing` is pixels, `gap` takes 4px units.
        let inner = div()
            .flex_col()
            .gap_px(config.item_spacing)
            .ml(config.indent / 4.0);

        Self {
            inner,
            config,
            marker: ListMarker::Decimal,
            start,
            item_count: 0,
            css_element_id: None,
            css_classes: Vec::new(),
        }
    }

    /// Add a list item
    pub fn child(mut self, item: ListItem) -> Self {
        let item = item.with_marker_and_config(
            self.marker,
            Some(self.start + self.item_count - 1),
            &self.config,
        );
        self.inner = self.inner.child(item);
        self.item_count += 1;
        self
    }

    /// Add any element as a child (for nesting lists)
    pub fn child_element(mut self, element: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(element);
        self
    }

    /// Add a boxed child element.
    pub fn child_box(mut self, element: Box<dyn ElementBuilder>) -> Self {
        self.inner = self.inner.child_box(element);
        self
    }

    /// Set the marker style (decimal, roman, alpha, etc.)
    pub fn marker(mut self, marker: ListMarker) -> Self {
        self.marker = marker;
        self
    }

    /// Set the starting number
    pub fn start(mut self, start: usize) -> Self {
        self.start = start;
        self
    }

    /// Set the indent (for nested lists)
    pub fn indent(mut self, indent: f32) -> Self {
        self.config.indent = indent;
        self.inner = self.inner.ml(indent / 4.0);
        self
    }

    /// Set item spacing
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.config.item_spacing = spacing;
        self.inner = self.inner.gap_px(spacing);
        self
    }

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.css_element_id = Some(id.to_string());
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: &str) -> Self {
        self.css_classes.push(blinc_core::intern::intern(name));
        self
    }
}

impl Default for OrderedList {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for OrderedList {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for OrderedList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for OrderedList {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("ol")
    }

    fn element_id(&self) -> Option<&str> {
        self.css_element_id.as_deref()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.css_classes
    }
}

// ============================================================================
// List Item
// ============================================================================

/// A list item
pub struct ListItem {
    inner: Div,
    content: Div,
    marker: Option<ListMarker>,
    index: Option<usize>,
    config: ListConfig,
}

impl ListItem {
    /// Create a new list item
    pub fn new() -> Self {
        let config = ListConfig::default();
        let inner = div().flex_row().items_start().gap_px(config.marker_gap);
        // Content has a small gap for spacing between text and nested lists
        let content = div().flex_col().flex_1().gap_px(4.0);

        Self {
            inner,
            content,
            marker: None,
            index: None,
            config,
        }
    }

    /// Add content to the list item
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.content = self.content.child(child);
        self
    }

    /// Add a boxed child element (for dynamic element types)
    pub fn child_box(mut self, child: Box<dyn crate::div::ElementBuilder>) -> Self {
        self.content = self.content.child_box(child);
        self
    }

    /// Set marker and index (called by parent list) - uses default config
    fn with_marker(self, marker: ListMarker, index: Option<usize>) -> Self {
        self.with_marker_and_config(marker, index, &ListConfig::default())
    }

    /// Set marker, index and config (called by parent list)
    fn with_marker_and_config(
        mut self,
        marker: ListMarker,
        index: Option<usize>,
        config: &ListConfig,
    ) -> Self {
        self.marker = Some(marker);
        self.index = index;

        // Build marker element with provided config
        let marker_str = marker.marker_for(index.unwrap_or(0));
        let marker_element = text(&marker_str)
            .size(config.marker_font_size)
            .color(config.marker_color);

        let marker_div = div()
            .w(config.marker_width)
            .flex_shrink_0()
            .child(marker_element);

        // Rebuild inner with marker + content
        self.inner = div()
            .flex_row()
            .items_start()
            .gap_px(config.marker_gap)
            .child(marker_div)
            .child(std::mem::replace(&mut self.content, div()));

        self
    }
}

impl Default for ListItem {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for ListItem {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ListItem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for ListItem {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Build the inner div which already has the marker prepended
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("li")
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

// ============================================================================
// Task List Item
// ============================================================================

/// A task list item with a checkbox
pub struct TaskListItem {
    inner: Div,
    checked: bool,
    config: ListConfig,
}

impl TaskListItem {
    /// Create a new task list item
    pub fn new(checked: bool) -> Self {
        Self::with_config(checked, ListConfig::default())
    }

    /// Create a new task list item with custom config
    pub fn with_config(checked: bool, config: ListConfig) -> Self {
        // Checkbox size based on font size (slightly smaller than font for visual balance)
        let checkbox_size = config.marker_font_size;
        let border_width = 1.5;

        // Build checkbox using div for the box and SVG for the checkmark
        let checkbox_box = if checked {
            // Checkmark SVG path (simple checkmark that fits in a square viewBox)
            let checkmark_svg = r#"<svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
                <path d="M3 8L6.5 11.5L13 4.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>"#;

            div()
                .w(checkbox_size)
                .h(checkbox_size)
                .flex_shrink_0()
                .bg(config.marker_color)
                .rounded(2.0)
                .items_center()
                .justify_center()
                .child(
                    svg(checkmark_svg)
                        .size(checkbox_size - 4.0, checkbox_size - 4.0)
                        .tint(Color::WHITE),
                )
        } else {
            // Empty checkbox - just a bordered div
            div()
                .w(checkbox_size)
                .h(checkbox_size)
                .flex_shrink_0()
                .rounded(2.0)
                .border(border_width, config.marker_color)
        };

        // Container with consistent width for alignment
        let checkbox_container = div()
            .w(config.marker_width)
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(checkbox_box);

        // Start with just the checkbox - content will be added via child()
        let inner = div()
            .flex_row()
            .items_start()
            .gap_px(config.marker_gap)
            .child(checkbox_container);

        Self {
            inner,
            checked,
            config,
        }
    }

    /// Add content to the task item
    ///
    /// Content is added after the checkbox in a flex row layout.
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(child);
        self
    }

    /// Add a boxed child element (for dynamic element types)
    pub fn child_box(mut self, child: Box<dyn crate::div::ElementBuilder>) -> Self {
        self.inner = self.inner.child_box(child);
        self
    }

    /// Check if this task item is checked
    pub fn is_checked(&self) -> bool {
        self.checked
    }
}

impl Default for TaskListItem {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Deref for TaskListItem {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TaskListItem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for TaskListItem {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Build inner (which has the checkbox + content structure)
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("li")
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Create an unordered list
pub fn ul() -> UnorderedList {
    UnorderedList::new()
}

/// Create an unordered list with custom config
pub fn ul_with_config(config: ListConfig) -> UnorderedList {
    UnorderedList::with_config(config)
}

/// Create an ordered list
pub fn ol() -> OrderedList {
    OrderedList::new()
}

/// Create an ordered list with custom config
pub fn ol_with_config(config: ListConfig) -> OrderedList {
    OrderedList::with_config(config)
}

/// Create an ordered list starting at a specific number
pub fn ol_start(start: usize) -> OrderedList {
    OrderedList::starting_at(start)
}

/// Create an ordered list starting at a specific number with config
pub fn ol_start_with_config(start: usize, config: ListConfig) -> OrderedList {
    OrderedList::starting_at_with_config(start, config)
}

/// Create a list item
pub fn li() -> ListItem {
    ListItem::new()
}

/// Create a task list item
pub fn task_item(checked: bool) -> TaskListItem {
    TaskListItem::new(checked)
}

/// Create a task list item with custom config
pub fn task_item_with_config(checked: bool, config: ListConfig) -> TaskListItem {
    TaskListItem::with_config(checked, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_unordered_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let list = ul().child(li().child(div())).child(li().child(div()));
        list.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_ordered_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let list = ol().child(li().child(div())).child(li().child(div()));
        list.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_task_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let item = task_item(true).child(div());
        item.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_roman_numerals() {
        assert_eq!(to_roman(1), "I");
        assert_eq!(to_roman(4), "IV");
        assert_eq!(to_roman(9), "IX");
        assert_eq!(to_roman(42), "XLII");
        assert_eq!(to_roman(99), "XCIX");
    }

    #[test]
    fn test_markers() {
        assert_eq!(ListMarker::Disc.marker_for(0), "•");
        assert_eq!(ListMarker::Decimal.marker_for(0), "1.");
        assert_eq!(ListMarker::Decimal.marker_for(9), "10.");
        assert_eq!(ListMarker::LowerAlpha.marker_for(0), "a.");
        assert_eq!(ListMarker::LowerAlpha.marker_for(25), "z.");
    }
}
