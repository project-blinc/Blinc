#![allow(dead_code)]

use ui::{core::FlowShader, prelude::*};

#[derive(BlincComponent)]
pub struct Project {
    pub name: String,
    #[animation]
    progress: f32,
}

pub fn project(props: ProjectProps) -> Div {
    div()
        .class("project")
        .child(Text::bound(props.name).class("project-name"))
}

pub const SOLID: FlowShader = ui::macros::flow!(solid, fragment, {
    output color = vec4(0.2, 0.4, 0.8, 1.0);
});

#[cfg(feature = "components")]
pub fn project_actions() -> Div {
    let icon = ui::icons::to_svg(ui::icons::icons::FOLDER, 16.0);
    let _svg_module = ui::svg::SvgDocument::from_str(&icon);
    div().child(ui::cn::button("Open project").icon(&icon))
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub fn browser_app(_: &ui::app::web::WebApp) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_props_keep_their_signal_and_computed_sources() {
        let name = signal("Website".to_string());
        let title = computed(move |g| format!("Project: {}", g.get(name).unwrap_or_default()));
        let props = Project::props(name).name(title);
        assert_eq!(props.name.now().as_deref(), Some("Project: Website"));
        name.set("Documentation".into());
        assert_eq!(props.name.now().as_deref(), Some("Project: Documentation"));
        let _view = project(props);
    }

    #[test]
    fn flow_macro_emits_a_shader_through_the_renamed_facade() {
        assert_eq!(SOLID.name, "solid");
        assert_eq!(SOLID.target, ui::core::FlowTarget::Fragment);
        assert!(SOLID.wgsl.contains("fn fs_main"));
    }
}
