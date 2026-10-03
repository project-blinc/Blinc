//! Floating context toolbar for the selection.
//!
//! When the editor has a non-empty selection, this widget renders a
//! small absolute-positioned toolbar just above the selection's
//! top-left corner. The toolbar is composed of:
//!
//! - **Mark buttons** — Bold, Italic, Underline, Strikethrough, Code.
//!   Clicking applies the mark to the current selection via
//!   `apply_mark_to_selection`.
//! - **Color button** — opens an inline color picker (a small palette
//!   of preset swatches) anchored next to the toolbar.
//! - **Link button** — opens an inline URL prompt that captures
//!   keystrokes until Enter (commit) or Esc (cancel).
//!
//! The pickers are intentionally *editor-only* widgets — not
//! standalone main widgets — so they live and die with the selection.
//! They're rendered as siblings of the toolbar inside the same
//! absolute-positioned overlay container.
//!
//! All buttons use direct closures (no Stateful wrappers) — clicking a
//! button mutates the editor state through the shared `RichTextState`
//! and bumps the editor's version signal.

use std::sync::Arc;

use blinc_core::{Color, State};

use crate::div::{Div, FontWeight, div};
use crate::widgets::rich_text_editor::cursor::ActiveFormat;

use super::format::{Mark, apply_mark_to_selection};
use super::render::RichTextTheme;
use super::state::{PickerState, RichTextState};

/// Build the floating context toolbar (and any open inline picker)
/// anchored to the current selection. Returns `None` when there's no
/// selection or it's collapsed.
pub fn selection_toolbar(
    state: &RichTextState,
    version: &State<u32>,
    theme: &RichTextTheme,
) -> Option<Div> {
    let mut data = state.lock().ok()?;
    let bounds = data.selection_bounds()?;
    let picker = data.picker.clone();

    let (x, y, w, _h) = bounds;
    // Toolbar is a fixed-width column. The mark row defines the
    // baseline width and the picker (if open) stacks below it on a
    // second row. Some pickers (notably the link form) need more
    // horizontal room than the mark row, so we widen the toolbar
    // accordingly when one of those is open.
    let toolbar_w: f32 = match picker {
        PickerState::None => 280.0,
        PickerState::Color => 280.0,
        // Heading row: 7 buttons * (34 + 4) + padding ≈ 280px.
        PickerState::Heading => 300.0,
        // Wide enough for the URL field (160px min) + three buttons
        // (50/60/56px) + gaps + padding without flex shrinking the
        // children. Leave a little headroom so the input can grow.
        PickerState::Link { .. } => 420.0,
    };
    let mark_row_h = 36.0_f32;
    let picker_row_h = match picker {
        PickerState::None => 0.0,
        PickerState::Color => 36.0,
        PickerState::Heading => 32.0,
        PickerState::Link { .. } => 38.0,
    };
    let toolbar_h = mark_row_h + picker_row_h;

    let center_x = x + w * 0.5;
    let tx = (center_x - toolbar_w * 0.5).max(0.0);
    let mut ty = y - toolbar_h - 6.0;
    if ty < 0.0 {
        // Selection is at the very top — drop the toolbar below the
        // selection instead.
        ty = y + bounds.3 + 6.0;
    }

    // Cache rect for diagnostics; the click-swallow path uses the
    // suppress flag set by individual button handlers.
    data.toolbar_rect = Some((tx, ty, toolbar_w, toolbar_h));
    drop(data);

    let mark_row = mark_row(state, version, theme);
    let mut toolbar = div()
        .absolute()
        .left(tx)
        .top(ty)
        .w(toolbar_w)
        .padding_x_px(6.0)
        .padding_y_px(4.0)
        .flex_col()
        .gap_px(4.0)
        // Render on the foreground layer so the bg fill draws ON TOP
        // of the document text underneath, rather than as part of the
        // background layer where text glyphs are composited last and
        // can show through.
        .foreground()
        .bg(Color::rgba(0.10, 0.10, 0.13, 1.0))
        .rounded(8.0)
        .border(1.0, Color::rgba(0.30, 0.30, 0.36, 1.0))
        .shadow(blinc_core::Shadow {
            offset_x: 0.0,
            offset_y: 4.0,
            blur: 12.0,
            spread: 0.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.45),
        })
        .child(mark_row);

    // Inline picker, rendered as a second row below the mark row.
    match picker {
        PickerState::None => {}
        PickerState::Color => {
            toolbar = toolbar
                .child(row_separator())
                .child(color_picker_row(state, version));
        }
        PickerState::Heading => {
            toolbar = toolbar
                .child(row_separator())
                .child(heading_picker_row(state, version));
        }
        PickerState::Link { draft } => {
            toolbar = toolbar
                .child(row_separator())
                .child(link_prompt_row(state, version, theme, &draft));
        }
    }

    Some(toolbar)
}

/// Horizontal hairline between the mark row and an open picker row.
fn row_separator() -> Div {
    div().h(1.0).w_full().bg(Color::rgba(0.34, 0.34, 0.40, 1.0))
}

/// Build the row of mark buttons (B, I, U, S, code) plus the color
/// and link triggers.
fn mark_row(state: &RichTextState, version: &State<u32>, theme: &RichTextTheme) -> Div {
    div()
        .flex_row()
        .items_center()
        .gap_px(2.0)
        .child(mark_button(
            "B",
            "Bold",
            LabelStyle::BOLD,
            state,
            version,
            theme,
            |d| apply_mark_via(d, Mark::Bold),
        ))
        .child(mark_button(
            "I",
            "Italic",
            LabelStyle::ITALIC,
            state,
            version,
            theme,
            |d| apply_mark_via(d, Mark::Italic),
        ))
        .child(mark_button(
            "U",
            "Underline",
            LabelStyle::UNDERLINE,
            state,
            version,
            theme,
            |d| apply_mark_via(d, Mark::Underline),
        ))
        .child(mark_button(
            "S",
            "Strikethrough",
            LabelStyle::STRIKE,
            state,
            version,
            theme,
            |d| apply_mark_via(d, Mark::Strikethrough),
        ))
        .child(mark_button(
            "<>",
            "Inline code",
            LabelStyle::CODE,
            state,
            version,
            theme,
            |d| apply_mark_via(d, Mark::Code),
        ))
        .child(divider())
        // Color swatch button — single-click toggles the accent color
        // on the current selection. The button face IS a colored
        // circle so users immediately see what it does.
        .child(color_swatch_button(state, version))
        .child(picker_button("H", "Heading", state, version, theme, |d| {
            d.picker = if matches!(d.picker, PickerState::Heading) {
                PickerState::None
            } else {
                PickerState::Heading
            };
        }))
        .child(picker_button("@", "Link", state, version, theme, |d| {
            d.picker = if matches!(d.picker, PickerState::Link { .. }) {
                PickerState::None
            } else {
                PickerState::Link {
                    draft: existing_link(d).unwrap_or_default(),
                }
            };
        }))
}

/// Color swatch button — same footprint as a mark button, but the
/// content is a colored circle so users immediately recognise it as
/// a color picker trigger. Clicking opens (or closes) the inline
/// color picker row beneath the toolbar. The swatch shows the
/// currently active color (defaults to white), so you can tell at a
/// glance what the next typed character will look like.
fn color_swatch_button(state: &RichTextState, version: &State<u32>) -> Div {
    let state_for_click = Arc::clone(state);
    let version_for_click = version.clone();
    let current_color = state
        .lock()
        .ok()
        .and_then(|d| d.active_format.color)
        .unwrap_or(Color::WHITE);
    div()
        .w(28.0)
        .h(24.0)
        .items_center()
        .justify_center()
        .rounded(4.0)
        .cursor_pointer()
        .child(
            div()
                .w(14.0)
                .h(14.0)
                .rounded(7.0)
                .bg(current_color)
                .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.32)),
        )
        .on_mouse_down(move |_| {
            if let Ok(mut data) = state_for_click.lock() {
                data.suppress_next_outer_click = true;
                data.picker = if matches!(data.picker, PickerState::Color) {
                    PickerState::None
                } else {
                    PickerState::Color
                };
                drop(data);
                version_for_click.set(version_for_click.get().wrapping_add(1));
            }
        })
}

/// Heading picker row — applies the chosen heading level to the
/// current block, or reverts to plain paragraph via the "P" button.
fn heading_picker_row(state: &RichTextState, version: &State<u32>) -> Div {
    let mut row = div().flex_row().items_center().gap_px(4.0);
    let levels: &[(Option<u8>, &str, f32, FontWeight)] = &[
        (None, "P", 12.0, FontWeight::Normal),
        (Some(1), "H1", 13.0, FontWeight::Bold),
        (Some(2), "H2", 12.5, FontWeight::Bold),
        (Some(3), "H3", 12.0, FontWeight::Bold),
        (Some(4), "H4", 11.5, FontWeight::Bold),
        (Some(5), "H5", 11.0, FontWeight::Bold),
        (Some(6), "H6", 10.5, FontWeight::Bold),
    ];
    for (level, label, font_size, weight) in levels {
        let level = *level;
        let label = *label;
        let font_size = *font_size;
        let weight = *weight;
        let state_for_click = Arc::clone(state);
        let version_for_click = version.clone();
        row = row.child(
            div()
                .w(34.0)
                .h(24.0)
                .items_center()
                .justify_center()
                .rounded(4.0)
                .bg(Color::rgba(0.20, 0.20, 0.26, 1.0))
                .cursor_pointer()
                .child(
                    crate::text::text(label)
                        .size(font_size)
                        .weight(weight)
                        .color(Color::WHITE)
                        .no_cursor(),
                )
                .on_mouse_down(move |_| {
                    if let Ok(mut data) = state_for_click.lock() {
                        data.suppress_next_outer_click = true;
                        apply_heading_level(&mut data, level);
                        data.picker = PickerState::None;
                        drop(data);
                        version_for_click.set(version_for_click.get().wrapping_add(1));
                    }
                }),
        );
    }
    row
}

/// Apply a heading level to the current selection. If the selection
/// is partial-within-a-line, the containing block is split into up to
/// three blocks (prefix paragraph, middle heading, suffix paragraph)
/// so that only the *selected* run becomes the heading. With a
/// collapsed cursor, a full-line selection, or a multi-block
/// selection, every block in the range is converted in place.
///
/// `None` reverts to plain paragraph.
fn apply_heading_level(data: &mut super::state::RichTextData, level: Option<u8>) {
    use super::block_ops::convert_selection_to_block;
    use super::document::BlockKind;

    let range = data.selection.unwrap_or(super::cursor::Selection {
        anchor: data.cursor,
        head: data.cursor,
    });
    let kind = match level {
        Some(n) => BlockKind::Heading(n),
        None => BlockKind::Paragraph,
    };

    let before = data.document.clone();
    data.push_undo();
    let middle_block_idx = convert_selection_to_block(&mut data.document, range, kind);
    if data.document == before {
        // Nothing actually changed — drop the undo entry we just pushed.
        data.undo_stack.pop();
        return;
    }

    // Re-anchor the cursor and selection inside the converted block.
    // When the block was split, `middle_block_idx` is the index of the
    // new heading block; otherwise the cursor's existing block is fine
    // (its index is unchanged for in-place conversions).
    if let Some(idx) = middle_block_idx {
        let middle_chars = data
            .document
            .blocks
            .get(idx)
            .map(|b| b.char_len())
            .unwrap_or(0);
        let new_start = super::cursor::DocPosition::new(idx, 0, 0);
        let new_end = super::cursor::DocPosition::new(idx, 0, middle_chars);
        data.selection = Some(super::cursor::Selection {
            anchor: new_start,
            head: new_end,
        });
        data.set_cursor(new_end);
    } else {
        let cursor = data.cursor;
        data.set_cursor(cursor);
    }
    data.reset_cursor_blink();
}

/// Visual style applied to a mark button's label so the glyph
/// previews the mark itself (B is bold, I is italic, U is underlined,
/// S has a strikethrough, `<>` is monospace).
#[derive(Clone, Copy)]
struct LabelStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    monospace: bool,
}

impl LabelStyle {
    const PLAIN: Self = Self {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        monospace: false,
    };
    const BOLD: Self = Self {
        bold: true,
        ..Self::PLAIN
    };
    const ITALIC: Self = Self {
        italic: true,
        ..Self::PLAIN
    };
    const UNDERLINE: Self = Self {
        underline: true,
        ..Self::PLAIN
    };
    const STRIKE: Self = Self {
        strikethrough: true,
        ..Self::PLAIN
    };
    const CODE: Self = Self {
        monospace: true,
        ..Self::PLAIN
    };
}

/// One mark button. The label is rendered with the mark applied (e.g.
/// the "B" button is bold) so users can tell what each button does at
/// a glance.
fn mark_button(
    label: &str,
    _tooltip: &str,
    style: LabelStyle,
    state: &RichTextState,
    version: &State<u32>,
    _theme: &RichTextTheme,
    on_click: impl Fn(&mut super::state::RichTextData) -> bool + Send + Sync + 'static,
) -> Div {
    let state_for_click = Arc::clone(state);
    let version_for_click = version.clone();
    // Keep the label passive so the parent button's pointer cursor
    // wins on hover. This only affects cursor style; hit-testing and
    // rendering are unchanged.
    let mut t = crate::text::text(label)
        .size(13.0)
        .color(Color::WHITE)
        .no_cursor();
    if style.bold {
        t = t.weight(crate::div::FontWeight::Bold);
    }
    if style.italic {
        t = t.italic();
    }
    if style.underline {
        t = t.underline();
    }
    if style.strikethrough {
        t = t.strikethrough();
    }
    if style.monospace {
        t = t.monospace();
    }
    div()
        .w(28.0)
        .h(24.0)
        .items_center()
        .justify_center()
        .rounded(4.0)
        .cursor_pointer()
        .child(t)
        .on_mouse_down(move |_| {
            if let Ok(mut data) = state_for_click.lock() {
                // Suppress the editor's outer mouse_down so it doesn't
                // collapse the selection during this click. Events
                // dispatch deepest-first then bubble up, so the editor
                // handler will see this flag set when it runs.
                data.suppress_next_outer_click = true;
                if on_click(&mut data) {
                    drop(data);
                    version_for_click.set(version_for_click.get().wrapping_add(1));
                }
            }
        })
}

/// Picker trigger button — same shape as a mark button but always
/// applies the closure regardless of return value (no undo push, just
/// state-flag change).
fn picker_button(
    label: &str,
    _tooltip: &str,
    state: &RichTextState,
    version: &State<u32>,
    _theme: &RichTextTheme,
    on_click: impl Fn(&mut super::state::RichTextData) + Send + Sync + 'static,
) -> Div {
    let state_for_click = Arc::clone(state);
    let version_for_click = version.clone();
    div()
        .w(28.0)
        .h(24.0)
        .items_center()
        .justify_center()
        .rounded(4.0)
        .cursor_pointer()
        .child(
            crate::text::text(label)
                .size(13.0)
                .color(Color::WHITE)
                .no_cursor(),
        )
        .on_mouse_down(move |_| {
            if let Ok(mut data) = state_for_click.lock() {
                // See mark_button for the rationale.
                data.suppress_next_outer_click = true;
                on_click(&mut data);
                drop(data);
                version_for_click.set(version_for_click.get().wrapping_add(1));
            }
        })
}

/// Vertical separator between groups in the toolbar.
fn divider() -> Div {
    div()
        .w(1.0)
        .h(20.0)
        .bg(Color::rgba(0.34, 0.34, 0.40, 1.0))
        .ml(1.0)
        .mr(1.0)
}

// =====================================================================
// Color picker
// =====================================================================

/// Preset palette used by the inline color picker. Order matches a
/// typical text-color popover: a default white plus a small set of
/// useful highlight colors.
fn color_palette() -> &'static [(Color, &'static str)] {
    const PALETTE: &[(Color, &str)] = &[
        (
            Color {
                r: 0.92,
                g: 0.92,
                b: 0.95,
                a: 1.0,
            },
            "Default",
        ),
        (
            Color {
                r: 0.94,
                g: 0.39,
                b: 0.39,
                a: 1.0,
            },
            "Red",
        ),
        (
            Color {
                r: 0.97,
                g: 0.59,
                b: 0.20,
                a: 1.0,
            },
            "Orange",
        ),
        (
            Color {
                r: 0.96,
                g: 0.85,
                b: 0.40,
                a: 1.0,
            },
            "Yellow",
        ),
        (
            Color {
                r: 0.50,
                g: 0.85,
                b: 0.50,
                a: 1.0,
            },
            "Green",
        ),
        (
            Color {
                r: 0.40,
                g: 0.78,
                b: 1.00,
                a: 1.0,
            },
            "Blue",
        ),
        (
            Color {
                r: 0.66,
                g: 0.55,
                b: 1.00,
                a: 1.0,
            },
            "Purple",
        ),
        (
            Color {
                r: 0.55,
                g: 0.55,
                b: 0.65,
                a: 1.0,
            },
            "Gray",
        ),
    ];
    PALETTE
}

fn color_picker_row(state: &RichTextState, version: &State<u32>) -> Div {
    let mut row = div().flex_row().items_center().gap_px(4.0);
    for (color, _label) in color_palette() {
        let state_for_click = Arc::clone(state);
        let version_for_click = version.clone();
        let c = *color;
        row = row.child(
            div()
                .w(18.0)
                .h(18.0)
                .rounded(9.0)
                .bg(c)
                .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.18))
                .cursor_pointer()
                .on_mouse_down(move |_| {
                    if let Ok(mut data) = state_for_click.lock() {
                        data.suppress_next_outer_click = true;
                        if let Some(sel) = data.selection {
                            if !sel.is_empty() {
                                data.push_undo();
                                apply_mark_to_selection(&mut data.document, sel, Mark::Color(c));
                            }
                        }
                        // Always set the active format so subsequent
                        // typing carries the new color.
                        data.active_format.color = Some(c);
                        data.picker = PickerState::None;
                        drop(data);
                        version_for_click.set(version_for_click.get().wrapping_add(1));
                    }
                }),
        );
    }
    row
}

// =====================================================================
// Link prompt
// =====================================================================

/// Inline URL prompt. Shows the current draft as plain text plus three
/// buttons: Apply, Remove (clears any existing link), Cancel.
///
/// Real text input is captured by the editor's `on_text_input` handler
/// when the link prompt is open — see `editor::handle_link_prompt_*` —
/// so this widget is purely visual feedback for the in-flight URL.
fn link_prompt_row(
    state: &RichTextState,
    version: &State<u32>,
    _theme: &RichTextTheme,
    draft: &str,
) -> Div {
    let placeholder = if draft.is_empty() {
        "https://…"
    } else {
        draft
    };
    let placeholder_color = if draft.is_empty() {
        Color::rgba(0.55, 0.55, 0.65, 1.0)
    } else {
        Color::WHITE
    };

    let state_for_apply = Arc::clone(state);
    let version_for_apply = version.clone();
    let state_for_remove = Arc::clone(state);
    let version_for_remove = version.clone();
    let state_for_cancel = Arc::clone(state);
    let version_for_cancel = version.clone();

    // Even the input display needs to swallow clicks so the editor's
    // outer mouse_down handler doesn't collapse the underlying
    // selection (which would close the prompt mid-edit).
    let state_for_input = Arc::clone(state);
    let input_click = move |_: &crate::event_handler::EventContext| {
        if let Ok(mut data) = state_for_input.lock() {
            data.suppress_next_outer_click = true;
        }
    };

    div()
        .flex_row()
        .items_center()
        .gap_px(6.0)
        .child(
            div()
                .h(22.0)
                .padding_x_px(8.0)
                .min_w(160.0)
                .flex_grow()
                .items_center()
                .rounded(4.0)
                .bg(Color::rgba(0.10, 0.10, 0.13, 1.0))
                .border(1.0, Color::rgba(0.34, 0.34, 0.40, 1.0))
                .cursor_text()
                .child(
                    crate::text::text(placeholder)
                        .size(12.0)
                        .color(placeholder_color)
                        .no_cursor()
                        .no_wrap(),
                )
                .on_mouse_down(input_click),
        )
        .child(prompt_button(
            "Apply",
            state_for_apply,
            version_for_apply,
            |d| {
                if let PickerState::Link { draft } = d.picker.clone() {
                    if !draft.is_empty() {
                        if let Some(sel) = d.selection {
                            if !sel.is_empty() {
                                d.push_undo();
                                apply_mark_to_selection(
                                    &mut d.document,
                                    sel,
                                    Mark::Link(Some(draft)),
                                );
                            }
                        }
                    }
                    d.picker = PickerState::None;
                }
            },
        ))
        .child(prompt_button(
            "Remove",
            state_for_remove,
            version_for_remove,
            |d| {
                if let Some(sel) = d.selection {
                    if !sel.is_empty() {
                        d.push_undo();
                        apply_mark_to_selection(&mut d.document, sel, Mark::Link(None));
                    }
                }
                d.picker = PickerState::None;
            },
        ))
        .child(prompt_button(
            "Cancel",
            state_for_cancel,
            version_for_cancel,
            |d| {
                d.picker = PickerState::None;
            },
        ))
}

fn prompt_button(
    label: &str,
    state: RichTextState,
    version: State<u32>,
    on_click: impl Fn(&mut super::state::RichTextData) + Send + Sync + 'static,
) -> Div {
    // Pick a fixed width per button so flex doesn't shrink them to 0
    // and force the inner text to wrap one character per line.
    let w = match label {
        "Apply" => 50.0,
        "Remove" => 60.0,
        "Cancel" => 56.0,
        _ => 60.0,
    };
    div()
        .w(w)
        .h(22.0)
        .items_center()
        .justify_center()
        .rounded(4.0)
        .bg(Color::rgba(0.20, 0.20, 0.26, 1.0))
        .cursor_pointer()
        .child(
            crate::text::text(label)
                .size(11.0)
                .color(Color::WHITE)
                .no_cursor()
                .no_wrap(),
        )
        .on_mouse_down(move |_| {
            if let Ok(mut data) = state.lock() {
                data.suppress_next_outer_click = true;
                on_click(&mut data);
                drop(data);
                version.set(version.get().wrapping_add(1));
            }
        })
}

// =====================================================================
// Helpers
// =====================================================================

/// Helper for mark buttons — pushes undo and applies the mark to the
/// selection. Returns `true` if anything changed so the caller knows
/// whether to bump the rebuild signal.
fn apply_mark_via(data: &mut super::state::RichTextData, mark: Mark) -> bool {
    if let Some(sel) = data.selection {
        if !sel.is_empty() {
            data.push_undo();
            let changed = apply_mark_to_selection(&mut data.document, sel, mark);
            data.active_format = ActiveFormat::from_position(&data.document, data.cursor);
            return changed;
        }
    }
    false
}

/// Find an existing link URL inside the current selection (if any). The
/// link button uses this to seed the URL prompt's draft so editing an
/// existing link is a no-retype affair.
fn existing_link(data: &super::state::RichTextData) -> Option<String> {
    let sel = data.selection?;
    let (start, _end) = sel.ordered();
    let block = data.document.blocks.get(start.block)?;
    let line = block.lines.get(start.line)?;
    let byte = super::document::char_to_byte(&line.text, start.col);
    line.spans
        .iter()
        .find(|s| s.start <= byte && byte < s.end)
        .and_then(|s| s.link_url.clone())
}
