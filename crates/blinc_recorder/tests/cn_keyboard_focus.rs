#![cfg(feature = "headless")]

use blinc_cn::prelude::*;
use blinc_layout::{div::div, tree::LayoutNodeId};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn session() -> Headless {
    let mut ui = Headless::new(640, 480).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui
}

fn items(ui: &Headless) -> Vec<LayoutNodeId> {
    fn walk(ui: &Headless, node: LayoutNodeId, out: &mut Vec<LayoutNodeId>) {
        if ui
            .tree()
            .element_registry()
            .attribute(node, "role")
            .as_deref()
            == Some("menuitem")
        {
            out.push(node);
        }
        for child in ui.tree().layout_tree.children(node) {
            walk(ui, child, out);
        }
    }
    let mut result = Vec::new();
    walk(ui, ui.tree().root().unwrap(), &mut result);
    result
}

fn control(label: &str) -> blinc_layout::div::Div {
    div()
        .class("cn-button")
        .attr("tabindex", "0")
        .w(120.0)
        .h(32.0)
        .child(blinc_layout::text::text(label).pointer_events_none())
}

#[test]
fn pointer_focus_hides_ring_then_keyboard_reveals_it_and_activates() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    let count = Arc::new(AtomicUsize::new(0));
    let fired = count.clone();
    ui.mount(move |_| {
        let fired = fired.clone();
        div().p(8.0).child(button("Action").on_click(move |_| {
            fired.fetch_add(1, Ordering::SeqCst);
        }))
    })
    .settle();
    ui.click(".cn-button").leave().settle();
    let node = ui.find(".cn-button").unwrap();
    assert_eq!(ui.focused(), Some(node));
    assert_eq!(
        ui.tree().get_render_node(node).unwrap().props.outline_width,
        0.0
    );
    ui.key(13).settle();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(ui.tree().get_render_node(node).unwrap().props.outline_width > 0.0);
    ui.click(".cn-button").leave().settle();
    assert_eq!(
        ui.tree().get_render_node(node).unwrap().props.outline_width,
        0.0
    );
}

#[test]
fn dropdown_roves_skips_disabled_activates_once_and_restores_trigger() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    let count = Arc::new(AtomicUsize::new(0));
    let fired = count.clone();
    ui.mount(move |_| {
        let fired = fired.clone();
        div().p(8.0).child(
            dropdown_menu("Actions")
                .item("First", || {})
                .separator()
                .item_disabled("Unavailable")
                .item("Last", move || {
                    fired.fetch_add(1, Ordering::SeqCst);
                }),
        )
    })
    .settle();
    let trigger = ui.tree().tab_stops()[0];
    ui.key(9).key(40).settle();
    let rows = items(&ui);
    assert_eq!(rows.len(), 3);
    assert_eq!(ui.focused(), Some(rows[0]));
    ui.key(40).settle();
    assert_eq!(ui.focused(), Some(rows[2]));
    ui.leave()
        .settle()
        .assert_snapshot("cn_menu_keyboard_focus");
    ui.key(40).key(38).settle();
    assert_eq!(ui.focused(), Some(rows[2]));
    ui.key(13).settle();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(ui.focused(), Some(trigger));
}

#[test]
fn submenu_right_opens_left_returns_then_escape_returns_to_trigger() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div().p(8.0).child(
            dropdown_menu("Actions")
                .submenu("More", |sub| sub.item("Child", || {}))
                .item("Other", || {}),
        )
    })
    .settle();
    let trigger = ui.tree().tab_stops()[0];
    ui.key(9).key(40).settle();
    let parent = ui.tree().stable_id(ui.focused().unwrap()).unwrap();
    ui.key(39).settle();
    assert_ne!(
        ui.focused().and_then(|n| ui.tree().stable_id(n)),
        Some(parent)
    );
    assert_eq!(
        ui.tree().tab_stops().len(),
        1,
        "only the child panel is a Tab stop"
    );
    ui.key(37).settle();
    assert_eq!(
        ui.focused().and_then(|n| ui.tree().stable_id(n)),
        Some(parent)
    );
    ui.key(27).settle();
    assert_eq!(ui.focused(), Some(trigger));
}

#[test]
fn pointer_menu_starts_on_panel_up_starts_on_last_row() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div().p(8.0).child(
            dropdown_menu("Actions")
                .item("First", || {})
                .item("Last", || {}),
        )
    })
    .settle();
    ui.click(".cn-dropdown-trigger").settle();
    assert_eq!(ui.focused(), ui.find(".cn-dropdown-menu"));
    ui.key(38).settle();
    assert_eq!(ui.focused(), items(&ui).last().copied());
}

#[test]
fn context_menu_opens_from_shift_f10_and_receives_roving_focus() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div().p(8.0).child(
            div()
                .id("context-target")
                .w(120.0)
                .h(40.0)
                .on_context_menu(|ctx| {
                    context_menu()
                        .at(ctx.bounds_x, ctx.bounds_y + ctx.bounds_height)
                        .item("First", || {})
                        .item_disabled("Disabled")
                        .item("Last", || {})
                        .show();
                }),
        )
    })
    .settle();
    ui.key(9).key_with_shift(121, true).settle();
    assert_eq!(ui.focused(), items(&ui).first().copied());
    ui.key(35).settle();
    assert_eq!(ui.focused(), items(&ui).last().copied());
    ui.key(27).settle();
    assert_eq!(ui.focused(), ui.find("#context-target"));
    ui.click("#context-target").key(93).settle();
    assert_eq!(
        ui.focused(),
        items(&ui).first().copied(),
        "the Context Menu key uses keyboard focus after a pointer press"
    );
}

#[test]
fn menubar_is_one_tab_stop_and_switches_open_menus_with_arrows() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div()
            .p(8.0)
            .child(
                menubar()
                    .menu("File", |m| m.item("New", || {}))
                    .menu("Edit", |m| m.item("Copy", || {})),
            )
            .child(button("After"))
    })
    .settle();
    assert_eq!(ui.tree().tab_stops().len(), 2);
    ui.key(9).settle();
    let file = ui.focused();
    ui.key(39).settle();
    let edit = ui.focused();
    assert_ne!(file, edit);
    ui.key(40).settle();
    assert_ne!(ui.focused(), edit);
    ui.key(37).settle();
    assert_eq!(
        ui.tree()
            .element_registry()
            .attribute(ui.focused().unwrap(), "role")
            .as_deref(),
        Some("menuitem")
    );
    ui.key(27).settle();
    assert_eq!(ui.focused(), file);
}

#[test]
fn tab_dismisses_menu_and_continues_after_its_trigger() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div()
            .p(8.0)
            .child(control("Before").class("before"))
            .child(dropdown_menu("Actions").item("Item", || {}))
            .child(control("After").class("after"))
    })
    .settle();
    ui.key(9).key(9).key(40).key(9).settle();
    assert_eq!(ui.focused(), ui.find(".after"));
    ui.key_with_shift(9, true)
        .key(38)
        .key_with_shift(9, true)
        .settle();
    assert_eq!(ui.focused(), ui.find(".before"));
}

#[test]
fn modal_restores_trigger_before_exit_finishes_and_next_tab_continues() {
    if !Headless::available() {
        return;
    }
    use blinc_layout::widgets::overlay_stack::OverlayBuilder;
    let mut ui = session();
    ui.mount(|_| {
        div()
            .p(8.0)
            .child(control("Open").class("open").on_click(|_| {
                OverlayBuilder::dialog()
                    .content(|| {
                        div()
                            .w(200.0)
                            .h(100.0)
                            .child(control("Inside").class("inside"))
                    })
                    .motion_class("cn-dialog-motion")
                    .show();
            }))
            .child(control("After").class("after"))
    })
    .settle();
    ui.key(9).key(13).settle();
    assert_eq!(ui.focused(), ui.find(".inside"));
    ui.key(27);
    assert_eq!(ui.focused(), ui.find(".open"));
    assert!(
        ui.find(".inside").is_some(),
        "exit animation should still be mounted"
    );
    ui.key(9);
    assert_eq!(ui.focused(), ui.find(".after"));
}

#[test]
fn outside_click_keeps_the_new_control_focused() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div()
            .p(8.0)
            .child(dropdown_menu("Actions").item("Item", || {}))
            .child(control("Other").class("other"))
    })
    .settle();
    ui.key(9).key(40).settle();
    ui.click(".other").settle();
    assert_eq!(ui.focused(), ui.find(".other"));
    assert_eq!(
        ui.tree()
            .get_render_node(ui.focused().unwrap())
            .unwrap()
            .props
            .outline_width,
        0.0
    );
}

#[test]
fn menu_action_can_open_dialog_and_return_to_original_trigger() {
    if !Headless::available() {
        return;
    }
    use blinc_layout::widgets::overlay_stack::OverlayBuilder;
    let mut ui = session();
    ui.mount(|_| {
        div()
            .p(8.0)
            .child(dropdown_menu("Actions").item("Dialog", || {
                OverlayBuilder::dialog()
                    .content(|| {
                        div()
                            .w(200.0)
                            .h(100.0)
                            .child(control("Inside").class("inside"))
                    })
                    .show();
            }))
    })
    .settle();
    ui.key(9);
    let trigger = ui.focused();
    ui.key(40).key(13).settle();
    assert_eq!(ui.focused(), ui.find(".inside"));
    ui.key(27).settle();
    assert_eq!(ui.focused(), trigger);
}

#[test]
fn disabled_trigger_is_not_refocused_on_close() {
    if !Headless::available() {
        return;
    }
    use blinc_core::reactive::signal;
    use blinc_layout::widgets::overlay_stack::OverlayBuilder;
    let mut ui = session();
    let disabled = signal(false);
    ui.mount(move |_| {
        div()
            .p(8.0)
            .child(
                control("Open")
                    .class("open")
                    .attr_when("disabled", "", disabled)
                    .on_click(|_| {
                        OverlayBuilder::dialog()
                            .content(|| div().w(200.0).h(100.0).child(control("Inside")))
                            .show();
                    }),
            )
            .child(control("After").class("after"))
    })
    .settle();
    ui.key(9).key(13).settle();
    disabled.set(true);
    ui.settle().key(27).settle();
    assert_eq!(ui.focused(), None);
    ui.key(9).settle();
    assert_eq!(ui.focused(), ui.find(".after"));
}

#[test]
fn nested_dialog_traps_tab_then_returns_to_parent_control() {
    if !Headless::available() {
        return;
    }
    use blinc_layout::widgets::overlay_stack::OverlayBuilder;
    let mut ui = session();
    ui.mount(|_| {
        div().p(8.0).child(control("Open").on_click(|_| {
            OverlayBuilder::dialog()
                .content(|| {
                    div()
                        .w(240.0)
                        .h(160.0)
                        .child(control("Child").class("child-open").on_click(|_| {
                            OverlayBuilder::dialog()
                                .content(|| {
                                    div().w(180.0).h(80.0).child(control("Leaf").class("leaf"))
                                })
                                .show();
                        }))
                        .child(control("Parent"))
                })
                .show();
        }))
    })
    .settle();
    ui.key(9).key(13).settle().key(13).settle();
    assert_eq!(ui.focused(), ui.find(".leaf"));
    ui.key(9).settle();
    assert_eq!(ui.focused(), ui.find(".leaf"));
    ui.key(27).settle();
    assert_eq!(ui.focused(), ui.find(".child-open"));
}

#[test]
fn id_focus_visible_rules_preserve_pointer_focus_styles() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.theme(HybridTheme::bundle().with_css("#focus-target:focus { opacity: 0.8; } #focus-target:focus-visible { outline: 3px solid red; }"), ColorScheme::Light);
    ui.mount(|_| div().p(8.0).child(control("Action").id("focus-target")))
        .settle();
    ui.click("#focus-target").leave().settle();
    let node = ui.focused().unwrap();
    let props = &ui.tree().get_render_node(node).unwrap().props;
    assert!((props.opacity - 0.8).abs() < 0.001);
    assert_eq!(props.outline_width, 0.0);
    ui.key(39).settle();
    assert_eq!(
        ui.tree().get_render_node(node).unwrap().props.outline_width,
        3.0
    );
}

#[test]
fn custom_dropdown_trigger_has_one_tab_stop() {
    if !Headless::available() {
        return;
    }
    let mut ui = session();
    ui.mount(|_| {
        div().p(8.0).child(
            blinc_cn::dropdown_menu_custom(|_| control("Custom").id("custom-inner"))
                .item("First", || {}),
        )
    })
    .settle();
    assert_eq!(ui.tree().tab_stops().len(), 1);
    ui.click("#custom-inner").settle().key(40).settle();
    assert_eq!(ui.focused(), items(&ui).first().copied());
}

#[test]
fn legacy_modal_restores_keyboard_focus() {
    if !Headless::available() {
        return;
    }
    use blinc_layout::widgets::overlay::{OverlayAnimation, OverlayManagerExt};
    let mut ui = session();
    ui.mount(|ctx| {
        let manager = ctx.overlay_manager();
        div()
            .p(8.0)
            .child(control("Open").id("legacy-open").on_click(move |_| {
                manager
                    .modal()
                    .animation(OverlayAnimation::none())
                    .content(|| {
                        div()
                            .w(200.0)
                            .h(100.0)
                            .child(control("Inside").id("legacy-inside"))
                    })
                    .show();
            }))
    })
    .settle();
    ui.key(9).key(13).settle();
    assert_eq!(ui.focused(), ui.find("#legacy-inside"));
    ui.key(27).settle();
    assert_eq!(ui.focused(), ui.find("#legacy-open"));
}
