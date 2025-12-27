//! Table element builder for structured data display
//!
//! This module provides HTML-like table building helpers that use flexbox layout
//! to create structured table layouts.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! // Create a simple table
//! table()
//!     .w_full()
//!     .child(
//!         thead()
//!             .child(tr()
//!                 .child(th("Name"))
//!                 .child(th("Age"))
//!                 .child(th("City")))
//!     )
//!     .child(
//!         tbody()
//!             .child(tr()
//!                 .child(td("Alice"))
//!                 .child(td("30"))
//!                 .child(td("NYC")))
//!             .child(tr()
//!                 .child(td("Bob"))
//!                 .child(td("25"))
//!                 .child(td("LA")))
//!     )
//! ```
//!
//! # Table Structure
//!
//! Tables follow HTML conventions:
//! - `table()` - The outer container
//! - `thead()` - Table header section
//! - `tbody()` - Table body section
//! - `tfoot()` - Table footer section
//! - `tr()` - Table row
//! - `th(content)` - Header cell (bold, centered)
//! - `td(content)` - Data cell
//!
//! # Styling
//!
//! All table elements return `Div` and support the full fluent API:
//!
//! ```ignore
//! table()
//!     .bg(Color::from_hex(0x1a1a1a))
//!     .rounded(8.0)
//!     .child(
//!         tr()
//!             .bg(Color::from_hex(0x2a2a2a))
//!             .child(td("Styled cell").p(16.0))
//!     )
//! ```

use blinc_core::Color;

use crate::div::{div, Div};
use crate::text::{text, Text};

// ============================================================================
// Default Table Styling
// ============================================================================

/// Default background color for table headers
const HEADER_BG: Color = Color::rgba(0.15, 0.15, 0.18, 1.0);

/// Default border color
const BORDER_COLOR: Color = Color::rgba(0.3, 0.3, 0.35, 1.0);

/// Default text color for header cells
const HEADER_TEXT_COLOR: Color = Color::rgba(0.9, 0.9, 0.95, 1.0);

/// Default text color for data cells
const CELL_TEXT_COLOR: Color = Color::rgba(0.8, 0.8, 0.85, 1.0);

/// Default cell padding (in pixels)
const CELL_PADDING: f32 = 12.0;

/// Default font size
const DEFAULT_FONT_SIZE: f32 = 14.0;

// ============================================================================
// Table Container
// ============================================================================

/// Create a table container
///
/// The table is a flex-column container that holds thead, tbody, and tfoot sections.
///
/// # Example
///
/// ```ignore
/// table()
///     .w_full()
///     .rounded(8.0)
///     .bg(Color::from_hex(0x1a1a1a))
///     .child(thead().child(tr().child(th("Column"))))
///     .child(tbody().child(tr().child(td("Data"))))
/// ```
pub fn table() -> Div {
    div().flex_col().overflow_clip()
}

// ============================================================================
// Table Sections
// ============================================================================

/// Create a table header section
///
/// The thead is a flex-column container for header rows.
/// By default, it has a slightly darker background.
///
/// # Example
///
/// ```ignore
/// thead()
///     .bg(Color::from_hex(0x2a2a2a))
///     .child(tr()
///         .child(th("Name"))
///         .child(th("Value")))
/// ```
pub fn thead() -> Div {
    div().flex_col().bg(HEADER_BG)
}

/// Create a table body section
///
/// The tbody is a flex-column container for data rows.
///
/// # Example
///
/// ```ignore
/// tbody()
///     .child(tr().child(td("Row 1")))
///     .child(tr().child(td("Row 2")))
/// ```
pub fn tbody() -> Div {
    div().flex_col()
}

/// Create a table footer section
///
/// The tfoot is a flex-column container for footer rows.
///
/// # Example
///
/// ```ignore
/// tfoot()
///     .bg(Color::from_hex(0x1a1a1a))
///     .child(tr().child(td("Total: 100")))
/// ```
pub fn tfoot() -> Div {
    div().flex_col().bg(HEADER_BG)
}

// ============================================================================
// Table Row
// ============================================================================

/// Create a table row
///
/// A row is a flex-row container that holds cells (th or td).
/// Cells in a row will share space equally by default.
///
/// # Example
///
/// ```ignore
/// tr()
///     .child(td("Cell 1"))
///     .child(td("Cell 2"))
///     .child(td("Cell 3"))
/// ```
pub fn tr() -> Div {
    // Use a bottom separator line via a child div
    div().flex_row().w_full()
}

// ============================================================================
// Table Cells
// ============================================================================

/// A table cell wrapper that can hold any content
pub struct TableCell {
    inner: Div,
}

impl TableCell {
    fn new() -> Self {
        Self {
            inner: div()
                .flex_row()
                .items_center()
                .flex_1() // flex: 1 1 0% - grow equally with zero basis
                .padding_x_px(CELL_PADDING)
                .padding_y_px(CELL_PADDING),
        }
    }

    /// Add a child element to this cell
    pub fn child(mut self, child: impl crate::div::ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(child);
        self
    }

    /// Set cell width in pixels
    pub fn w(mut self, px: f32) -> Self {
        self.inner = self.inner.w(px).flex_shrink_0();
        self
    }

    /// Set cell to flex-grow with a specific weight
    pub fn flex_weight(mut self, weight: f32) -> Self {
        self.inner = self.inner.flex_grow();
        // Note: can't set specific flex-grow weight in current API
        // Use w() for fixed widths or flex_grow() for equal distribution
        let _ = weight; // Suppress unused warning
        self
    }

    /// Set cell to not grow (fixed width based on content)
    pub fn w_fit(mut self) -> Self {
        self.inner = self.inner.w_fit();
        self
    }

    /// Set cell padding
    pub fn p(mut self, units: f32) -> Self {
        self.inner = self.inner.p(units);
        self
    }

    /// Set cell padding in pixels
    pub fn padding_px(mut self, px: f32) -> Self {
        self.inner = self.inner.padding_x_px(px).padding_y_px(px);
        self
    }

    /// Set horizontal padding
    pub fn px(mut self, units: f32) -> Self {
        self.inner = self.inner.px(units);
        self
    }

    /// Set vertical padding
    pub fn py(mut self, units: f32) -> Self {
        self.inner = self.inner.py(units);
        self
    }

    /// Set cell background color
    pub fn bg(mut self, color: Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }

    /// Center content horizontally
    pub fn justify_center(mut self) -> Self {
        self.inner = self.inner.justify_center();
        self
    }

    /// Align content to end (right)
    pub fn justify_end(mut self) -> Self {
        self.inner = self.inner.justify_end();
        self
    }

    /// Center content vertically
    pub fn items_center(mut self) -> Self {
        self.inner = self.inner.items_center();
        self
    }

    /// Add a separator (vertical line) after this cell
    ///
    /// Creates a visual divider by adding a narrow colored div as the last child
    pub fn with_separator(self) -> Div {
        div()
            .flex_row()
            .child(self)
            .child(div().w(1.0).h_full().bg(BORDER_COLOR))
    }

    /// Convert to the underlying Div (for advanced customization)
    pub fn into_div(self) -> Div {
        self.inner
    }
}

impl crate::div::ElementBuilder for TableCell {
    fn build(&self, tree: &mut crate::tree::LayoutTree) -> crate::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> crate::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn crate::div::ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }
}

/// Create a table header cell (th)
///
/// Header cells are bold and centered by default.
///
/// # Example
///
/// ```ignore
/// th("Column Name")
/// th("Right Aligned").justify_end()
/// ```
pub fn th(content: impl Into<String>) -> TableCell {
    let txt = text(content)
        .size(DEFAULT_FONT_SIZE)
        .color(HEADER_TEXT_COLOR)
        .bold();

    TableCell::new().child(txt)
}

/// Create a table data cell (td)
///
/// Data cells contain regular text and are left-aligned by default.
///
/// # Example
///
/// ```ignore
/// td("Cell content")
/// td("123.45").justify_end()  // Right-align numbers
/// ```
pub fn td(content: impl Into<String>) -> TableCell {
    let txt = text(content).size(DEFAULT_FONT_SIZE).color(CELL_TEXT_COLOR);

    TableCell::new().child(txt)
}

/// Create an empty table cell
///
/// Useful for placeholder cells or cells with custom content.
///
/// # Example
///
/// ```ignore
/// // Empty cell
/// cell()
///
/// // Cell with custom content
/// cell().child(button("Edit"))
/// ```
pub fn cell() -> TableCell {
    TableCell::new()
}

// ============================================================================
// Striped Rows Helper
// ============================================================================

/// Create a striped table row
///
/// Alternates background color based on index for zebra striping.
///
/// # Example
///
/// ```ignore
/// tbody()
///     .child(striped_tr(0).child(td("Row 0")))
///     .child(striped_tr(1).child(td("Row 1")))
///     .child(striped_tr(2).child(td("Row 2")))
/// ```
pub fn striped_tr(index: usize) -> Div {
    let bg = if index.is_multiple_of(2) {
        Color::TRANSPARENT
    } else {
        Color::rgba(0.1, 0.1, 0.12, 1.0)
    };

    tr().bg(bg)
}

// ============================================================================
// Table Builder (Alternative API)
// ============================================================================

/// A builder for creating tables with headers and data
///
/// This provides a more declarative way to create tables from data.
///
/// # Example
///
/// ```ignore
/// TableBuilder::new()
///     .headers(&["Name", "Age", "City"])
///     .row(&["Alice", "30", "NYC"])
///     .row(&["Bob", "25", "LA"])
///     .striped(true)
///     .build()
/// ```
pub struct TableBuilder {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    striped: bool,
    header_bg: Color,
    border_color: Color,
}

impl TableBuilder {
    /// Create a new table builder
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            striped: false,
            header_bg: HEADER_BG,
            border_color: BORDER_COLOR,
        }
    }

    /// Set table headers
    pub fn headers(mut self, headers: &[&str]) -> Self {
        self.headers = headers.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Add a data row
    pub fn row(mut self, cells: &[&str]) -> Self {
        self.rows
            .push(cells.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Enable zebra striping
    pub fn striped(mut self, enabled: bool) -> Self {
        self.striped = enabled;
        self
    }

    /// Set header background color
    pub fn header_bg(mut self, color: Color) -> Self {
        self.header_bg = color;
        self
    }

    /// Set border color
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    /// Build the table
    pub fn build(self) -> Div {
        let mut tbl = table();

        // Build header
        if !self.headers.is_empty() {
            let mut header_row = tr();
            for h in &self.headers {
                header_row = header_row.child(th(h.as_str()));
            }
            tbl = tbl.child(thead().bg(self.header_bg).child(header_row));
        }

        // Build body
        if !self.rows.is_empty() {
            let mut body = tbody();
            for (i, row_data) in self.rows.iter().enumerate() {
                let mut row = if self.striped { striped_tr(i) } else { tr() };

                for cell_data in row_data {
                    row = row.child(td(cell_data.as_str()));
                }

                body = body.child(row);
            }
            tbl = tbl.child(body);
        }

        tbl
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Convenience: Text-based cells
// ============================================================================

/// Create a header cell with custom text styling
///
/// Returns a Text element that you can further style.
/// Use `th()` if you need cell-level styling (padding, background).
pub fn th_text(content: impl Into<String>) -> Text {
    text(content)
        .size(DEFAULT_FONT_SIZE)
        .color(HEADER_TEXT_COLOR)
        .bold()
}

/// Create a data cell with custom text styling
///
/// Returns a Text element that you can further style.
/// Use `td()` if you need cell-level styling (padding, background).
pub fn td_text(content: impl Into<String>) -> Text {
    text(content).size(DEFAULT_FONT_SIZE).color(CELL_TEXT_COLOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::ElementBuilder;
    use crate::tree::LayoutTree;

    #[test]
    fn test_simple_table() {
        let mut tree = LayoutTree::new();

        let tbl = table()
            .child(thead().child(tr().child(th("Header"))))
            .child(tbody().child(tr().child(td("Data"))));

        tbl.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_table_builder() {
        let mut tree = LayoutTree::new();

        let tbl = TableBuilder::new()
            .headers(&["A", "B", "C"])
            .row(&["1", "2", "3"])
            .row(&["4", "5", "6"])
            .striped(true)
            .build();

        tbl.build(&mut tree);
        assert!(tree.len() > 0);
    }

    #[test]
    fn test_cell_methods() {
        let mut tree = LayoutTree::new();

        let cell = td("Test")
            .w(100.0)
            .justify_end()
            .bg(Color::from_hex(0x333333));

        cell.build(&mut tree);
        assert!(tree.len() > 0);
    }
}
