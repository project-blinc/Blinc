//! The table controls: outside a table, a button that opens a grid to pick a new table's size from;
//! in one, a menu for its rows and columns.

use std::sync::Arc;

use blinc_core::Signal;
use blinc_core::reactive::{computed, signal};
use blinc_icons::icons;
use blinc_layout::binding::Reactive;
use blinc_layout::div::{Div, div};
use blinc_layout::widgets::overlay_stack::OverlayHandle;

use super::controls::{self, Target};
use crate::components::dropdown_menu::DropdownMenuBuilder;
use crate::components::label::{LabelSize, label};
use crate::components::picker_popover::picker_popover;
use crate::components::rich_text_editor::cursor::DocPosition;
use crate::components::rich_text_editor::document::RichDocument;
use crate::components::rich_text_editor::table::{self, Cell};

/// The largest table the grid offers.
const ROWS: usize = 6;
const COLUMNS: usize = 8;
/// A square of the grid, and the gap between two.
const SQUARE: f32 = 20.0;
const SQUARE_GAP: f32 = 3.0;
/// The grid's size, and the popover's with the label under it.
const WIDTH: f32 = COLUMNS as f32 * (SQUARE + SQUARE_GAP) - SQUARE_GAP;
const HEIGHT: f32 = ROWS as f32 * (SQUARE + SQUARE_GAP) - SQUARE_GAP;
const LABEL: f32 = 28.0;
const PAD: f32 = 16.0;

/// The squares of a new table's size. `active` is the popover's overlay, closed by a pick.
fn grid(target: &Target, active: Signal<Option<u64>>) -> Div {
    let size = signal((0usize, 0usize));
    let mut rows = div()
        .class("cn-rich-text-editor-grid")
        .flex_col()
        .gap_px(SQUARE_GAP)
        .w(WIDTH)
        .h(HEIGHT)
        .on_hover_leave(move |_| size.set((0, 0)));
    for row in 1..=ROWS {
        let mut line = div().flex_row().gap_px(SQUARE_GAP);
        for column in 1..=COLUMNS {
            let target = target.clone();
            let within = computed(move |g| {
                g.get(size)
                    .is_some_and(|(rows, columns)| row <= rows && column <= columns)
            });
            line = line.child(
                div()
                    .class("cn-rich-text-editor-grid-square")
                    .attr("role", "button")
                    .attr("aria-label", format!("{row} by {column} table"))
                    .attr_when("data-within", "", within)
                    .w(SQUARE)
                    .h(SQUARE)
                    .on_hover_enter(move |_| size.set((row, column)))
                    .on_click(move |_| {
                        target.act(|d| d.insert_table(row, column));
                        if let Some(raw) = active.get() {
                            OverlayHandle::from_raw(raw).close();
                        }
                    }),
            );
        }
        rows = rows.child(line);
    }
    let named = computed(move |g| match g.get(size) {
        Some((rows, columns)) if rows > 0 => format!("{rows} \u{00d7} {columns}"),
        _ => "Insert table".to_string(),
    });
    div()
        .class("cn-rich-text-editor-grid-panel")
        .flex_col()
        .child(rows)
        .child(
            div()
                .w_full()
                .h(LABEL)
                .flex_row()
                .items_end()
                .justify_center()
                .child(label(named).size(LabelSize::Small)),
        )
}

/// The button that opens the grid.
pub(super) fn insert(target: &Target) -> Div {
    let id = format!(
        "cn-rich-text-editor-table-menu-{:p}",
        Arc::as_ptr(&target.state)
    );
    let trigger = controls::icon_button(target, icons::TABLE, "Insert table", false, |_| {});
    let target = target.clone();
    picker_popover(
        trigger,
        id,
        Reactive::Const(false),
        (WIDTH + 2.0 * PAD, HEIGHT + LABEL + 2.0 * PAD),
        move |active| grid(&target, active),
    )
}

/// A change to the table the cursor is in, which says where the cursor goes.
type Change = fn(&mut RichDocument, DocPosition, &Cell) -> Option<DocPosition>;

/// The table menu: a cn dropdown of what is done to the rows and columns at the cursor.
pub(super) fn menu(target: &Target) -> Div {
    let groups: [&[(&str, &str, Change)]; 3] = [
        &[
            ("Row above", icons::BETWEEN_HORIZONTAL_END, |d, p, c| {
                table::insert_row(d, p, c.row)
            }),
            ("Row below", icons::BETWEEN_HORIZONTAL_START, |d, p, c| {
                table::insert_row(d, p, c.row + 1)
            }),
            ("Column before", icons::BETWEEN_VERTICAL_END, |d, p, c| {
                table::insert_column(d, p, c.column)
            }),
            ("Column after", icons::BETWEEN_VERTICAL_START, |d, p, c| {
                table::insert_column(d, p, c.column + 1)
            }),
        ],
        &[
            ("Delete row", icons::ROWS_2, |d, p, _| {
                table::remove_row(d, p)
            }),
            ("Delete column", icons::COLUMNS_2, |d, p, _| {
                table::remove_column(d, p)
            }),
        ],
        &[("Delete table", icons::TRASH_2, |d, p, _| {
            table::remove_table(d, p)
        })],
    ];
    let svg = |icon| blinc_icons::to_svg(icon, 16.0);
    let mut menu = DropdownMenuBuilder::new("Table").icon(svg(icons::TABLE));
    for (i, group) in groups.into_iter().enumerate() {
        if i > 0 {
            menu = menu.separator();
        }
        for &(name, icon, change) in group {
            let target = target.clone();
            menu = menu.item_with_icon(name, svg(icon), move || {
                target.act(|d| d.change_table(change));
            });
        }
    }
    div()
        .class("cn-rich-text-editor-table-menu")
        .flex_shrink_0()
        .w(controls::MENU)
        .child(menu)
}
