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
use crate::components::rich_text_editor::document::{BlockAlign, BlockKind};
use crate::components::rich_text_editor::format::Mark;
use crate::components::rich_text_editor::state::{RichTextData, RichTextState, report_change};
use crate::components::select::{SelectSize, select};
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
        report_change(&self.state);
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
            .class("blinc-rte-button")
            .attr("aria-label", label);
        if active {
            b = b.class("blinc-rte-button--active");
        }
        div().child(b)
    };
    div()
        .flex_shrink_0()
        .child(tooltip(trigger).text(label).side(TooltipSide::Top))
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

/// The alignments, in toolbar order.
pub(super) const ALIGNS: [(BlockAlign, &str, &str); 4] = [
    (BlockAlign::Start, icons::TEXT_ALIGN_START, "Align left"),
    (BlockAlign::Center, icons::TEXT_ALIGN_CENTER, "Align centre"),
    (BlockAlign::End, icons::TEXT_ALIGN_END, "Align right"),
    (BlockAlign::Justify, icons::TEXT_ALIGN_JUSTIFY, "Justify"),
];

/// One button per alignment, pressed for the one of the block under the cursor.
pub(super) fn aligns(target: &Target, current: BlockAlign) -> Vec<Div> {
    ALIGNS
        .iter()
        .map(|&(align, icon, label)| {
            icon_button(target, icon, label, align == current, move |t| {
                t.act(|d| {
                    d.set_align(align);
                });
            })
        })
        .collect()
}

/// The list menu's width.
pub(super) const LIST: f32 = 48.0;

/// The kinds of list: the icon, the name and the block kind of each.
const LISTS: [(&str, &str, BlockKind); 3] = [
    (icons::LIST, "Bulleted list", BlockKind::BulletItem),
    (
        icons::LIST_ORDERED,
        "Numbered list",
        BlockKind::NumberedItem,
    ),
    (
        icons::LIST_TODO,
        "Checklist",
        BlockKind::TaskItem { checked: false },
    ),
];

/// The list menu: a cn dropdown whose icon is the list the block under the cursor is in.
pub(super) fn lists(target: &Target, current: &BlockKind) -> Div {
    let svg = |icon| blinc_icons::to_svg(icon, 16.0);
    let shown = LISTS
        .iter()
        .find(|(_, _, kind)| kind.is_same_kind(current))
        .map_or(icons::LIST, |(icon, ..)| *icon);
    let mut menu = DropdownMenuBuilder::new("List").icon(svg(shown));
    for (icon, label, kind) in LISTS {
        let (target, kind) = (target.clone(), kind.clone());
        menu = menu.item_with_icon(label, svg(icon), move || {
            let kind = kind.clone();
            target.act(|d| {
                d.toggle_kind(kind);
            });
        });
    }
    div()
        .class("blinc-rte-list")
        .flex_shrink_0()
        .w(LIST)
        .child(menu)
}

/// The cn colour picker, shown as a swatch of the colour under the cursor; picking colours the
/// selection.
pub(super) fn color(target: &Target, swatch: Signal<Color>) -> Div {
    let target = target.clone();
    div().flex_shrink_0().child(
        color_picker(swatch)
            .swatches(actions::palette())
            .on_change(move |c| target.act(|d| actions::set_color(d, c)))
            .class("blinc-rte-color"),
    )
}

/// The close button, which collapses the selection and so closes the floating toolbar.
pub(super) fn close(target: &Target) -> Div {
    icon_button(target, icons::X, "Close", false, |t| t.act(actions::close))
}

/// A thin vertical rule between groups of controls.
pub(super) fn divider() -> Div {
    div()
        .class("blinc-rte-separator")
        .flex_shrink_0()
        .w(1.0)
        .h(18.0)
        // 4px units: 6px each side.
        .mx(1.5)
}

/// The name of a block kind in the heading menu: in full, or as P and H with the level.
fn kind_label(kind: &BlockKind, compact: bool) -> String {
    match (kind, compact) {
        (BlockKind::Heading(level), true) => format!("H{}", level.clamp(&1, &6)),
        (BlockKind::Heading(level), false) => format!("Heading {}", level.clamp(&1, &6)),
        (_, true) => "P".to_string(),
        (_, false) => "Paragraph".to_string(),
    }
}

/// The heading menu's width, fixed so the toolbar knows its own width: in full, and compact.
pub(super) const HEADING: f32 = 120.0;
pub(super) const HEADING_COMPACT: f32 = 64.0;

/// The heading menu: a cn select of the block under the cursor's kind, `compact` with short
/// names, centred.
pub(super) fn heading(target: &Target, kind: &BlockKind, compact: bool) -> Div {
    let current = match kind {
        BlockKind::Heading(level) => level.to_string(),
        _ => "p".to_string(),
    };
    let target = target.clone();
    let mut menu = select(signal(current))
        .size(SelectSize::Small)
        .w(if compact { HEADING_COMPACT } else { HEADING })
        .option("p", kind_label(&BlockKind::Paragraph, compact))
        .on_change(move |value| {
            let level = value.parse::<u8>().ok();
            target.act(|d| actions::apply_heading_level(d, level));
        });
    for level in 1..=6u8 {
        menu = menu.option(
            level.to_string(),
            kind_label(&BlockKind::Heading(level), compact),
        );
    }
    let mut wrapper = div().class("blinc-rte-heading").flex_shrink_0();
    if compact {
        wrapper = wrapper.class("blinc-rte-heading--compact");
    }
    wrapper.child(menu)
}
