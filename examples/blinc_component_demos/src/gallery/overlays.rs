use crate::demos::DemoState;
use blinc_cn as cn;
use blinc_layout::prelude::*;

pub fn tooltip_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-stack").class("demo-stack--center").child(
        cn::tooltip(|| {
            div().child(cn::button("Hover for a hint").variant(cn::ButtonVariant::Outline))
        })
        .text("Your work is saved automatically.")
        .open_delay_ms(200),
    )
}

pub fn hover_card_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-stack").class("demo-stack--center").child(
        cn::hover_card(|| div().child(cn::button("@blinc").variant(cn::ButtonVariant::Ghost)))
            .content(|| {
                div()
                    .class("demo-profile")
                    .child(cn::avatar().fallback("BL"))
                    .child(
                        div()
                            .class("demo-profile-copy")
                            .child(text("Blinc").class("demo-label"))
                            .child(
                                text("Build something worth interacting with.")
                                    .class("demo-caption"),
                            ),
                    )
            })
            .open_delay_ms(200),
    )
}

pub fn popover_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let notifications = values.confirmed;
    div().class("demo-stack").class("demo-stack--center").child(
        cn::popover(|_| div().child(cn::button("Preferences").variant(cn::ButtonVariant::Outline)))
            .content(move || {
                div()
                    .class("demo-popover-form")
                    .child(cn::label("Notifications"))
                    .child(cn::switch(notifications).label("Email updates"))
            }),
    )
}

pub fn sheet_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let access = values.confirmed;
    div().class("demo-stack").class("demo-stack--center").child(
        cn::button("Share project")
            .variant(cn::ButtonVariant::Outline)
            .on_click(move |_| {
                cn::sheet_bottom()
                    .size(cn::SheetSize::Large)
                    .title("Share project")
                    .description("Manage access to Component library.")
                    .content(move || {
                        let mut members = div().class("demo-share-members");
                        for (initials, name, detail) in [
                            ("AL", "Alex Lee", "Project owner"),
                            ("MJ", "Maya Jones", "Can edit"),
                        ] {
                            members = members.child(
                                div()
                                    .class("demo-share-member")
                                    .child(cn::avatar().fallback(initials))
                                    .child(
                                        div()
                                            .class("demo-member-copy")
                                            .child(text(name).class("demo-label"))
                                            .child(text(detail).class("demo-caption")),
                                    ),
                            );
                        }
                        div()
                            .class("demo-share-content")
                            .child(members)
                            .child(cn::separator())
                            .child(
                                div()
                                    .class("demo-share-permission")
                                    .child(
                                        cn::icon(blinc_icons::icons::GLOBE)
                                            .size(cn::IconSize::Small),
                                    )
                                    .child(
                                        div()
                                            .class("demo-member-copy")
                                            .class("demo-share-permission-copy")
                                            .child(text("Anyone with the link").class("demo-label"))
                                            .child(
                                                text("Allow guests to view this project")
                                                    .class("demo-caption"),
                                            ),
                                    )
                                    .child(cn::switch(access).label("Guests")),
                            )
                    })
                    .footer(|| {
                        div().w_full().flex_row().justify_end().child(
                            cn::button("Done")
                                .on_click(|_| {
                                    let handle = blinc_layout::overlay_state::overlay_stack()
                                        .lock()
                                        .ok()
                                        .and_then(|s| s.iter_top_down().next().map(|e| e.handle));
                                    if let Some(handle) = handle {
                                        handle.close();
                                    }
                                })
                                .build_component()
                                .class("demo-share-done"),
                        )
                    })
                    .show();
            })
            .build_component()
            .id("demo-action"),
    )
}

pub fn drawer_demo(values: &DemoState, _: (f32, f32)) -> Div {
    use blinc_core::reactive::computed;
    let selected = values.choice;
    div().class("demo-stack").class("demo-stack--center").child(
        cn::button("Browse project").variant(cn::ButtonVariant::Outline).on_click(move |_| {
            cn::drawer_right().size(cn::DrawerSize::Wide).title("Project files")
                .child(move || {
                    let file_icon = blinc_icons::to_svg(blinc_icons::icons::FILE, 16.0);
                    let folder_icon = blinc_icons::to_svg(blinc_icons::icons::FOLDER, 16.0);
                    let files = cn::tree_view()
                        .class("demo-project-tree")
                        .with_guides()
                        .node("src", "src", |node| {
                            node.expanded().icon(folder_icon.clone())
                                .child("main.rs", "main.rs", |node| node.icon(file_icon.clone()))
                                .child("ui.rs", "ui.rs", |node| node.icon(file_icon.clone()))
                        })
                        .node("styles", "styles", |node| {
                            node.expanded().icon(folder_icon)
                                .child("theme.css", "theme.css", |node| node.icon(file_icon.clone()))
                        })
                        .node("Cargo.toml", "Cargo.toml", |node| node.icon(file_icon))
                        .on_select(move |key| selected.set(key.to_string()));
                    div().class("demo-drawer-content")
                        .child(div().class("demo-project-summary").child(text("Website").class("demo-label"))
                            .child(cn::badge("Rust project").variant(cn::BadgeVariant::Secondary)))
                        .child(files)
                        .child(cn::separator())
                        .child(crate::demos::bound_text(computed(move |g| {
                            let description = match g.get(selected).as_deref() {
                                Some("main.rs") => "main.rs · Starts the application and creates its window.",
                                Some("ui.rs") => "ui.rs · Composes reusable, reactive components.",
                                Some("theme.css") => "theme.css · Defines the shared visual language.",
                                Some("Cargo.toml") => "Cargo.toml · Declares the package and its dependencies.",
                                Some("src") => "src · Application code and reusable components.",
                                Some("styles") => "styles · Shared styles for the whole interface.",
                                _ => "Choose a file to inspect its role in this project.",
                            };
                            description.to_string()
                        })).class("demo-caption"))
                })
                .footer(|| div().class("demo-drawer-footer").child(text("4 files").class("demo-caption"))
                    .child(cn::badge("Up to date").variant(cn::BadgeVariant::Success)))
                .show();
        }).build_component().id("demo-action")
    )
}

pub fn toast_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-stack").class("demo-stack--center").child(
        cn::button("Save changes")
            .variant(cn::ButtonVariant::Outline)
            .on_click(|_| {
                cn::toast_success("Changes saved")
                    .description("Your workspace is up to date.")
                    .show();
            })
            .build_component()
            .id("demo-action"),
    )
}
