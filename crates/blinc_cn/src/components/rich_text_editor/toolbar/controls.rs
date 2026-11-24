//! The toolbar's controls, built from cn components: icon buttons named by tooltips, the colour
//! picker and the heading menu.

use std::sync::Arc;

use blinc_core::{Color, Signal, reactive::signal};
use blinc_icons::icons;
use blinc_layout::div::{Div, div};

use super::actions;
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::color_picker::color_picker;
use crate::components::dropdown_menu::DropdownMenuBuilder;
use crate::components::rich_text_editor::cursor::ActiveFormat;
use crate::components::rich_text_editor::document::BlockKind;
use crate::components::rich_text_editor::format::Mark;
use crate::components::rich_text_editor::state::{PickerState, RichTextData, RichTextState};
use crate::components::tooltip::{TooltipSide, tooltip};

/// The side of an icon button.
pub(super) const BUTTON: f32 = 30.0;
/// The gap between controls.
pub(super) const GAP: f32 = 2.0;
/// A divider's width, margins included.
pub(super) const DIVIDER: f32 = 13.0;

/// The editor a toolbar acts on.
#[derive(Clone)]
pub(super) struct Target {
    pub state: RichTextState,
    pub version: Signal<u32>,
}

impl Target {
    /// Change the editor's state and show the change.
    pub fn act(&self, f: impl FnOnce(&mut RichTextData)) {
        if let Ok(mut data) = self.state.lock() {
            f(&mut data);
        }
        self.version.update(|n| n.wrapping_add(1));
    }
}

/// An icon button with a tooltip naming it; `active` shows it pressed.
pub(super) fn icon_button(
    target: &Target,
    icon: &'static str,
    label: &'static str,
    active: bool,
    on_click: impl Fn(&Target) + Send + Sync + 'static,
) -> Div {
    let (target, on_click) = (target.clone(), Arc::new(on_click));
    let trigger = move || {
        let (target, on_click) = (target.clone(), Arc::clone(&on_click));
        let mut b = button("")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Custom(BUTTON, BUTTON))
            .icon(icon)
            .icon_size(16.0)
            .on_click(move |_| on_click(&target))
            .build_component()
            .class("blinc-rte-button");
        if active {
            b = b.class("blinc-rte-button--active");
        }
        div().child(b)
    };
    div().child(tooltip(trigger).text(label).side(TooltipSide::Top))
}

/// The boolean marks, in toolbar order.
pub(super) const MARKS: [(Mark, &str, &str); 5] = [
    (Mark::Bold, icons::BOLD, "Bold"),
    (Mark::Italic, icons::ITALIC, "Italic"),
    (Mark::Underline, icons::UNDERLINE, "Underline"),
    (Mark::Strikethrough, icons::STRIKETHROUGH, "Strikethrough"),
    (Mark::Code, icons::CODE, "Code"),
];

/// One button per boolean mark, pressed where the mark is on.
pub(super) fn marks(target: &Target, format: &ActiveFormat) -> Vec<Div> {
    MARKS
        .iter()
        .map(|(mark, icon, label)| {
            let on = actions::mark_on(format, mark);
            let mark = mark.clone();
            icon_button(target, icon, label, on, move |t| {
                let mark = mark.clone();
                t.act(|d| actions::toggle_mark(d, mark));
            })
        })
        .collect()
}

/// The cn colour picker, shown as a swatch of the colour under the cursor; picking colours the
/// selection.
pub(super) fn color(target: &Target, current: Color) -> Div {
    let target = target.clone();
    // Transparent text takes the theme's text colour, so the swatch shows that.
    let current = if current.a == 0.0 {
        blinc_theme::ThemeState::get().color(blinc_theme::ColorToken::TextPrimary)
    } else {
        current
    };
    div().child(
        color_picker(signal(current))
            .swatches(actions::palette())
            .on_change(move |c| target.act(|d| actions::set_color(d, c)))
            .class("blinc-rte-color"),
    )
}

/// The link button, which opens or closes the link field.
pub(super) fn link(target: &Target, open: bool) -> Div {
    icon_button(target, icons::LINK, "Link", open, |t| {
        let field = t.state.lock().ok().map(|d| Arc::clone(&d.link_input));
        let mut opened = false;
        t.act(|d| {
            if d.picker == PickerState::Link {
                d.picker = PickerState::None;
            } else {
                d.picker = PickerState::Link;
                opened = true;
                // The field takes the keys; the selection stays shown while it is open.
                d.set_focus(false);
                let url = actions::existing_link(d).unwrap_or_default();
                if let Ok(mut f) = d.link_input.lock() {
                    f.value = url;
                    f.cursor = f.value.chars().count();
                }
            }
        });
        if let Some(field) = field.filter(|_| opened) {
            blinc_layout::widgets::text_input::focus_text_input_deferred(&field);
        }
    })
}

/// The close button, which collapses the selection and so closes the floating toolbar.
pub(super) fn close(target: &Target) -> Div {
    icon_button(target, icons::X, "Close", false, |t| t.act(actions::close))
}

/// A thin vertical rule between groups of controls.
pub(super) fn divider() -> Div {
    div()
        .class("blinc-rte-separator")
        .w(1.0)
        .h(18.0)
        // 4px units: 6px each side.
        .mx(1.5)
}

/// The name of a block kind in the heading menu.
fn kind_label(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Heading(1) => "Heading 1",
        BlockKind::Heading(2) => "Heading 2",
        BlockKind::Heading(3) => "Heading 3",
        BlockKind::Heading(4) => "Heading 4",
        BlockKind::Heading(5) => "Heading 5",
        BlockKind::Heading(_) => "Heading 6",
        _ => "Paragraph",
    }
}

/// The heading menu, labelled with the block under the cursor's kind.
pub(super) fn heading(target: &Target, kind: &BlockKind) -> Div {
    let mut menu = DropdownMenuBuilder::new(kind_label(kind)).class("blinc-rte-heading");
    for level in [None, Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)] {
        let label = kind_label(&level.map_or(BlockKind::Paragraph, BlockKind::Heading));
        let target = target.clone();
        menu = menu.item(label, move || {
            target.act(|d| actions::apply_heading_level(d, level));
        });
    }
    div().child(menu)
}
