use crate::demos::DemoState;
use blinc_layout::prelude::Div;

mod builtin;
mod canvas;
mod display;
mod forms;
mod media;
mod navigation;
mod notch;
mod overlays;

pub fn build(slug: &str, values: &DemoState, viewport: (f32, f32)) -> Div {
    match slug {
        "image" => media::image_demo(values, viewport),
        "waveform" => media::waveform_demo(values, viewport),
        "audio-player" => media::audio_player_demo(values, viewport),
        "video-player" => media::video_player_demo(values, viewport),
        "card" => display::card_demo(values, viewport),
        "badge" => display::badge_demo(values, viewport),
        "alert" => display::alert_demo(values, viewport),
        "avatar" => display::avatar_demo(values, viewport),
        "separator" => display::separator_demo(values, viewport),
        "aspect-ratio" => display::aspect_ratio_demo(values, viewport),
        "kbd" => display::kbd_demo(values, viewport),
        "icons" => display::icon_demo(values, viewport),
        "typography" => display::typography_demo(values, viewport),
        "table" => display::table_demo(values, viewport),
        "tree" => display::tree_demo(values, viewport),
        "skeleton" => display::skeleton_demo(values, viewport),
        "spinner" => display::spinner_demo(values, viewport),
        "scroll-area" => display::scroll_area_demo(values, viewport),
        "resizable" => display::resizable_demo(values, viewport),
        "collapsible" => display::collapsible_demo(values, viewport),
        "accordion" => display::accordion_demo(values, viewport),
        "tabs" => navigation::tabs_demo(values, viewport),
        "breadcrumb" => navigation::breadcrumb_demo(values, viewport),
        "pagination" => navigation::pagination_demo(values, viewport),
        "dropdown-menu" => navigation::dropdown_menu_demo(values, viewport),
        "context-menu" => navigation::context_menu_demo(values, viewport),
        "menubar" => navigation::menubar_demo(values, viewport),
        "navigation-menu" => navigation::navigation_menu_demo(values, viewport),
        "sidebar" => navigation::sidebar_demo(values, viewport),
        "code-block" => builtin::code_block_demo(values, viewport),
        "code-editor" => builtin::code_editor_demo(values, viewport),
        "rich-text-editor" => builtin::rich_text_editor_demo(values, viewport),
        "markdown" => builtin::markdown_demo(values, viewport),
        "virtual-list" => builtin::virtual_list_demo(values, viewport),
        "canvas" => canvas::canvas_demo(values, viewport),
        "gestures" => builtin::gestures_demo(values, viewport),
        "motion" => builtin::motion_demo(values, viewport),
        "lists" => builtin::lists_demo(values, viewport),
        "blockquote" => builtin::blockquote_demo(values, viewport),
        "link" => builtin::link_demo(values, viewport),
        "svg" => builtin::svg_demo(values, viewport),
        "notch" => notch::notch_demo(values, viewport),
        "checkbox" => forms::checkbox_demo(values, viewport),
        "switch" => forms::switch_demo(values, viewport),
        "radio-group" => forms::radio_demo(values, viewport),
        "toggle" => forms::toggle_demo(values, viewport),
        "toggle-group" => forms::toggle_group_demo(values, viewport),
        "number-input" => forms::number_input_demo(values, viewport),
        "number-gauge" => forms::number_gauge_demo(values, viewport),
        "number-range-gauge" => forms::number_range_gauge_demo(values, viewport),
        "slider" => forms::slider_demo(values, viewport),
        "textarea" => forms::textarea_demo(values, viewport),
        "input-otp" => forms::input_otp_demo(values, viewport),
        "combobox" => forms::combobox_demo(values, viewport),
        "calendar" => forms::calendar_demo(values, viewport),
        "date-picker" => forms::date_picker_demo(values, viewport),
        "time-picker" => forms::time_picker_demo(values, viewport),
        "color-picker" => forms::color_picker_demo(values, viewport),
        "label" => forms::label_demo(values, viewport),
        "tooltip" => overlays::tooltip_demo(values, viewport),
        "hover-card" => overlays::hover_card_demo(values, viewport),
        "popover" => overlays::popover_demo(values, viewport),
        "sheet" => overlays::sheet_demo(values, viewport),
        "drawer" => overlays::drawer_demo(values, viewport),
        "toast" => overlays::toast_demo(values, viewport),
        _ => panic!("unknown catalog demo: {slug}"),
    }
}
