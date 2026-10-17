//! The integration tests, as one binary: each file is a module, so the
//! crate and its dependencies are linked once rather than once per file.

mod array_literals;
mod bare_lambdas;
mod comment_in_view;
mod component_decorators;
mod control_flow_in_view_bodies;
mod cross_module_props;
mod example_reactive_dsl;
mod exports;
mod extern_widget_macro;
mod fsm_fiber;
mod hot_reload_contract;
mod lambda_return_abi;
mod let_in_widget_body;
mod list_reactivity;
mod list_writes;
mod map_over_list;
mod map_over_signal_list;
mod module_signal_collision;
mod multi_module;
mod nested_render;
mod observed_deps;
mod parse_diagnostics;
mod reload_module;
mod rich_text_markdown;
mod signal_defaults;
mod string_literals;
mod stylesheet;
mod text_style_props;
mod vec_props;
mod view_returning_fns;
mod with_blocks;
mod zyntax_fiber_reach;
