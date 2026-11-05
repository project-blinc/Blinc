use crate::demos::{DemoState, bound_text};
use blinc_cn as cn;
use blinc_core::reactive::computed;
use blinc_layout::prelude::*;

pub fn tabs_demo(values: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        cn::tabs(values.choice)
            .tab("rust", "Overview", || {
                div()
                    .class("demo-panel")
                    .child(text("Everything you need, in one place."))
            })
            .tab("typescript", "Activity", || {
                div()
                    .class("demo-panel")
                    .child(text("Your latest changes are ready to review."))
            })
            .transition(cn::TabsTransition::Fade),
    )
}

pub fn breadcrumb_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-wide")
        .child(
            cn::breadcrumb()
                .item("Workspace", move || choice.set("workspace".into()))
                .item("Projects", move || choice.set("projects".into()))
                .current("Website"),
        )
        .child(
            bound_text(computed(move |g| {
                format!(
                    "Page: {}",
                    g.get(choice)
                        .filter(|v| v != "rust")
                        .unwrap_or_else(|| "Website".into())
                )
            }))
            .class("demo-caption"),
        )
}

pub fn pagination_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let page = values.page;
    div()
        .class("demo-wide")
        .child(cn::pagination(10, page).visible_pages(5))
        .child(
            bound_text(computed(move |g| {
                format!("Page {} of 10", g.get(page).unwrap_or(1))
            }))
            .class("demo-caption"),
        )
}

pub fn dropdown_menu_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(
            cn::dropdown_menu("Project actions")
                .item("Rename", move || choice.set("Rename selected".into()))
                .item_with_shortcut("Save", "⌘ S", move || choice.set("Saved".into()))
                .separator()
                .item_disabled("Archive"),
        )
        .child(
            bound_text(computed(move |g| {
                let choice = g.get(choice).unwrap_or_default();
                if choice == "rust" {
                    "Choose an action".into()
                } else {
                    choice
                }
            }))
            .class("demo-caption"),
        )
}

pub fn context_menu_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-stack")
        .child(
            div()
                .class("demo-context-target")
                .id("demo-action")
                .child(text("Right-click this project"))
                .on_context_menu(move |event| {
                    cn::context_menu()
                        .at(event.mouse_x, event.mouse_y)
                        .item("Duplicate", move || choice.set("Duplicated".into()))
                        .item("Share", move || choice.set("Share selected".into()))
                        .separator()
                        .item("Rename", move || choice.set("Rename selected".into()))
                        .show();
                }),
        )
        .child(
            bound_text(computed(move |g| {
                let choice = g.get(choice).unwrap_or_default();
                if choice == "rust" {
                    "Choose an action".into()
                } else {
                    choice
                }
            }))
            .class("demo-caption"),
        )
}

pub fn menubar_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-wide")
        .child(
            cn::menubar()
                .menu("File", move |menu| {
                    menu.item("New project", move || choice.set("New project".into()))
                        .item("Save", move || choice.set("Saved".into()))
                })
                .menu("Edit", move |menu| {
                    menu.item("Undo", move || choice.set("Undo".into()))
                        .item("Redo", move || choice.set("Redo".into()))
                }),
        )
        .child(
            bound_text(computed(move |g| {
                let choice = g.get(choice).unwrap_or_default();
                if choice == "rust" {
                    "Choose an action".into()
                } else {
                    choice
                }
            }))
            .class("demo-caption"),
        )
}

pub fn navigation_menu_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-wide")
        .child(
            cn::navigation_menu()
                .item("Overview", move || choice.set("overview".into()))
                .trigger("Resources", move || {
                    div()
                        .class("demo-popover-form")
                        .child(
                            cn::navigation_link("Documentation")
                                .description("Build your first interface")
                                .on_click(move || choice.set("documentation".into())),
                        )
                        .child(
                            cn::navigation_link("Examples")
                                .description("Explore what is possible")
                                .on_click(move || choice.set("examples".into())),
                        )
                }),
        )
        .child(
            bound_text(computed(move |g| {
                format!(
                    "Page: {}",
                    g.get(choice)
                        .filter(|v| v != "rust")
                        .unwrap_or_else(|| "Website".into())
                )
            }))
            .class("demo-caption"),
        )
}

pub fn sidebar_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div().class("demo-wide").h(240.0).child(
        cn::sidebar(values.confirmed)
            .section("Workspace")
            .item_active(
                "Projects",
                blinc_icons::to_svg(blinc_icons::icons::FOLDER, 18.0),
                move || choice.set("projects".into()),
            )
            .item(
                "Settings",
                blinc_icons::to_svg(blinc_icons::icons::SETTINGS, 18.0),
                move || choice.set("settings".into()),
            )
            .content(move |_| {
                div()
                    .class("demo-panel")
                    .child(bound_text(computed(move |g| {
                        format!(
                            "{} workspace",
                            g.get(choice)
                                .filter(|v| v != "rust")
                                .unwrap_or_else(|| "Your".into())
                        )
                    })))
                    .child(text("Recent projects").class("demo-caption"))
                    .child(
                        div()
                            .class("demo-project-list")
                            .child(text("Component library").class("demo-project-row"))
                            .child(text("Website").class("demo-project-row"))
                            .child(text("Design system").class("demo-project-row")),
                    )
            }),
    )
}
