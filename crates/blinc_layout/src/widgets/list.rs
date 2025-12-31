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

use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
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
}

impl UnorderedList {
    /// Create a new unordered list
    pub fn new() -> Self {
        Self::with_config(ListConfig::default())
    }

    /// Create a new unordered list with custom config
    pub fn with_config(config: ListConfig) -> Self {
        let inner = div().flex_col().gap(config.item_spacing).ml(config.indent);

        Self {
            inner,
            config,
            marker: ListMarker::Disc,
            item_count: 0,
        }
    }

    /// Add a list item
    pub fn child(mut self, item: ListItem) -> Self {
        // Set the marker and index on the item with our config
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

    /// Set the marker style
    pub fn marker(mut self, marker: ListMarker) -> Self {
        self.marker = marker;
        self
    }

    /// Set the indent (for nested lists)
    pub fn indent(mut self, indent: f32) -> Self {
        self.config.indent = indent;
        self.inner = self.inner.ml(indent);
        self
    }

    /// Set item spacing
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.config.item_spacing = spacing;
        self.inner = self.inner.gap(spacing);
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
        let inner = div().flex_col().gap(config.item_spacing).ml(config.indent);

        Self {
            inner,
            config,
            marker: ListMarker::Decimal,
            start,
            item_count: 0,
        }
    }

    /// Add a list item
    pub fn child(mut self, item: ListItem) -> Self {
        // Set the marker and index on the item with our config
        let item = item.with_marker_and_config(self.marker, Some(self.start + self.item_count - 1), &self.config);
        self.inner = self.inner.child(item);
        self.item_count += 1;
        self
    }

    /// Add any element as a child (for nesting lists)
    pub fn child_element(mut self, element: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(element);
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
        self.inner = self.inner.ml(indent);
        self
    }

    /// Set item spacing
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.config.item_spacing = spacing;
        self.inner = self.inner.gap(spacing);
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
        let inner = div().flex_row().items_start().gap(config.marker_gap);
        let content = div().flex_col().flex_1();

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
    fn with_marker_and_config(mut self, marker: ListMarker, index: Option<usize>, config: &ListConfig) -> Self {
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
            .gap(config.marker_gap)
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
        // Build checkbox element
        let checkbox_str = if checked { "☑" } else { "☐" };
        let checkbox_element = text(checkbox_str)
            .size(config.marker_font_size)
            .color(config.marker_color);

        let checkbox_div = div()
            .w(config.marker_width)
            .flex_shrink_0()
            .child(checkbox_element);

        // Start with just the checkbox - content will be added via child()
        let inner = div()
            .flex_row()
            .items_start()
            .gap(config.marker_gap)
            .child(checkbox_div);

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
        let list = ul()
            .child(li().child(div()))
            .child(li().child(div()));
        list.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_ordered_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let list = ol()
            .child(li().child(div()))
            .child(li().child(div()));
        list.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_task_list() {
        init_theme();
        let mut tree = LayoutTree::new();
        let item = task_item(true).child(div());
        item.build(&mut tree);
        assert!(tree.len() > 0);
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
