//! Blinc procedural macros
//!
//! Provides derive macros for the Blinc UI framework.
//!
//! # Platform-Agnostic Design
//!
//! The generated code uses traits from `blinc_core` and `blinc_animation`
//! rather than concrete types, enabling components to work across different
//! platforms without depending on `blinc_app`.
//!
//! # Scope and Instance Awareness
//!
//! The `BlincComponent` macro generates unique component keys that include
//! both scope information (module path + struct name) and support for
//! instance differentiation through key suffixes.
//!
//! ## Component Key Composition
//!
//! Keys are composed of three parts:
//! 1. **Scope key**: `module_path!()::StructName` - identifies the component type
//! 2. **Field key**: Field name for struct fields - identifies the field
//! 3. **Instance key**: User-provided or auto-generated suffix - differentiates instances
//!
//! ## Usage Patterns
//!
//! For single-instance components, use the field methods directly:
//! ```ignore
//! let scale = MyComponent::use_scale(ctx, 1.0, SpringConfig::snappy());
//! ```
//!
//! For multiple instances (e.g., in loops), use the `_for` variants with an instance key:
//! ```ignore
//! for i in 0..10 {
//!     let scale = MyComponent::use_scale_for(ctx, i, 1.0, SpringConfig::snappy());
//! }
//! ```
//!
//! For automatic instance keys based on call site (via `#[track_caller]`):
//! ```ignore
//! // Each call site gets a unique key automatically
//! let scale1 = MyComponent::use_scale_auto(ctx, 1.0, SpringConfig::snappy());
//! let scale2 = MyComponent::use_scale_auto(ctx, 1.0, SpringConfig::snappy()); // Different key!
//! ```

mod flow;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Check if a field has the `#[animation]` attribute
fn has_animation_attr(field: &syn::Field) -> bool {
    field
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("animation"))
}

/// Derive macro that generates a unique compile-time key for a component
/// and generates field accessors based on field attributes.
///
/// This enables type-safe access to animation and state hooks without manual string keys.
/// The key is generated from the full module path and struct name, ensuring
/// uniqueness across the codebase.
///
/// # Platform-Agnostic
///
/// The generated methods are generic over `BlincContext` and `AnimationContext` traits,
/// allowing components to work with any platform implementation (desktop, Android, web, etc.)
/// without depending on `blinc_app`.
///
/// # Scope and Instance Awareness
///
/// The macro generates both single-instance and keyed (`_for`) variants for each field:
/// - Single-instance: `use_<field>(ctx, ...)` - for components used once
/// - Keyed: `use_<field>_for(ctx, key, ...)` - for components used in loops/lists
///
/// # Field Attributes
///
/// - `#[animation]` - Field generates animation methods returning `SharedAnimatedValue`
/// - No attribute - Field generates state methods returning `State<FieldType>`
///
/// # Example - Unit Struct (simple component key)
///
/// ```ignore
/// use blinc_macros::BlincComponent;
///
/// #[derive(BlincComponent)]
/// pub struct AnimatedDemoCard;
///
/// // Works with any context implementing the required traits
/// fn build_ui<C: BlincContext + AnimationContext>(ctx: &C) -> impl ElementBuilder {
///     let ball_x = AnimatedDemoCard::use_animated_value(ctx, 20.0, SpringConfig::wobbly());
///     // ...
/// }
/// ```
///
/// # Example - Struct with Mixed Fields
///
/// ```ignore
/// use blinc_macros::BlincComponent;
///
/// #[derive(BlincComponent)]
/// pub struct PullToRefresh {
///     #[animation]
///     content_offset: f32,  // -> use_content_offset(ctx, initial, config)
///     #[animation]
///     icon_scale: f32,      // -> use_icon_scale(ctx, initial, config)
///     #[animation]
///     icon_opacity: f32,    // -> use_icon_opacity(ctx, initial, config)
/// }
///
/// fn build_ui<C: AnimationContext>(ctx: &C) -> impl ElementBuilder {
///     let offset = PullToRefresh::use_content_offset(ctx, 0.0, SpringConfig::wobbly());
///     let scale = PullToRefresh::use_icon_scale(ctx, 0.5, SpringConfig::snappy());
///     // ...
/// }
/// ```
///
/// # Example - Multiple Instances (Loop/List)
///
/// ```ignore
/// #[derive(BlincComponent)]
/// pub struct ListItem {
///     #[animation]
///     scale: f32,
///     selected: bool,
/// }
///
/// fn build_list<C: BlincContext + AnimationContext>(ctx: &C, items: &[Item]) -> impl ElementBuilder {
///     div().children(items.iter().enumerate().map(|(i, item)| {
///         // Use _for variants with instance key for uniqueness
///         let scale = ListItem::use_scale_for(ctx, i, 1.0, SpringConfig::snappy());
///         let selected = ListItem::use_selected_for(ctx, i, false);
///         // ...
///     }))
/// }
/// ```
///
/// # Example - State Values
///
/// ```ignore
/// #[derive(BlincComponent)]
/// pub struct Counter {
///     count: i32,           // -> use_count(ctx, initial) -> State<i32>
///     step: i32,            // -> use_step(ctx, initial) -> State<i32>
///     #[animation]
///     scale: f32,           // -> use_scale(ctx, initial, config) -> SharedAnimatedValue
/// }
/// ```
///
/// # Generated Code
///
/// For all structs, the macro generates:
/// - A `COMPONENT_KEY` constant containing the unique key
/// - `use_animated_value` / `use_animated_value_with` for ad-hoc spring animations
/// - `use_animated_timeline` / `use_animated_timeline_with` for timeline animations
///
/// For structs with named fields:
/// - Fields with `#[animation]`:
///   - `use_<field>(ctx, initial, config)` -> `SharedAnimatedValue`
///   - `use_<field>_for(ctx, key, initial, config)` -> `SharedAnimatedValue`
/// - Fields without attribute:
///   - `use_<field>(ctx, initial)` -> `State<FieldType>`
///   - `use_<field>_for(ctx, key, initial)` -> `State<FieldType>`
#[proc_macro_derive(BlincComponent, attributes(animation))]
pub fn derive_blinc_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Extract named fields if present and generate appropriate methods
    let field_methods = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                fields
                    .named
                    .iter()
                    .map(|field| {
                        let field_name = field.ident.as_ref().unwrap();
                        let field_type = &field.ty;
                        let method_name =
                            syn::Ident::new(&format!("use_{}", field_name), field_name.span());
                        let field_key = format!("{}", field_name);

                        // Generate _for method name for instance-aware variant
                        let method_name_for =
                            syn::Ident::new(&format!("use_{}_for", field_name), field_name.span());

                        // Generate _auto method name for caller-location-aware variant
                        let method_name_auto =
                            syn::Ident::new(&format!("use_{}_auto", field_name), field_name.span());

                        if has_animation_attr(field) {
                            // #[animation] attribute -> SharedAnimatedValue
                            quote! {
                                /// Get a persisted animated value for this field (single instance).
                                ///
                                /// Returns a `SharedAnimatedValue` that is persisted across UI rebuilds.
                                /// Use `use_<field>_for` when you need multiple instances.
                                pub fn #method_name<C: blinc_animation::AnimationContext>(
                                    ctx: &C,
                                    initial: f32,
                                    config: blinc_animation::SpringConfig,
                                ) -> blinc_animation::SharedAnimatedValue {
                                    let key = format!("{}:{}", Self::COMPONENT_KEY, #field_key);
                                    ctx.use_animated_value_for(key, initial, config)
                                }

                                /// Get a persisted animated value for this field with instance key.
                                ///
                                /// Use this when you have multiple instances of the same component
                                /// (e.g., in a loop or list). The `instance_key` differentiates
                                /// between instances.
                                ///
                                /// # Example
                                ///
                                /// ```ignore
                                /// for i in 0..10 {
                                ///     let scale = MyComponent::use_scale_for(ctx, i, 1.0, config);
                                /// }
                                /// ```
                                pub fn #method_name_for<C: blinc_animation::AnimationContext, K: std::fmt::Display>(
                                    ctx: &C,
                                    instance_key: K,
                                    initial: f32,
                                    config: blinc_animation::SpringConfig,
                                ) -> blinc_animation::SharedAnimatedValue {
                                    let key = format!("{}:{}:{}", Self::COMPONENT_KEY, #field_key, instance_key);
                                    ctx.use_animated_value_for(key, initial, config)
                                }

                                /// Get a persisted animated value with auto-generated instance key.
                                ///
                                /// Uses `#[track_caller]` to generate a unique key based on the
                                /// call site location. Each unique call site gets its own instance.
                                ///
                                /// Prefer `use_<field>_for` in loops where you control the key.
                                #[track_caller]
                                pub fn #method_name_auto<C: blinc_animation::AnimationContext>(
                                    ctx: &C,
                                    initial: f32,
                                    config: blinc_animation::SpringConfig,
                                ) -> blinc_animation::SharedAnimatedValue {
                                    let loc = std::panic::Location::caller();
                                    let key = format!("{}:{}:{}:{}:{}",
                                        Self::COMPONENT_KEY, #field_key,
                                        loc.file(), loc.line(), loc.column());
                                    ctx.use_animated_value_for(key, initial, config)
                                }
                            }
                        } else {
                            // No attribute -> State<T>
                            quote! {
                                /// Get a persisted state value for this field (single instance).
                                ///
                                /// Returns a `State<T>` that is persisted across UI rebuilds.
                                /// Use `use_<field>_for` when you need multiple instances.
                                pub fn #method_name<C: blinc_core::BlincContext>(
                                    ctx: &C,
                                    initial: #field_type,
                                ) -> blinc_core::State<#field_type>
                                where
                                    #field_type: Clone + Send + 'static,
                                {
                                    let key = format!("{}:{}", Self::COMPONENT_KEY, #field_key);
                                    ctx.use_state_keyed(&key, || initial)
                                }

                                /// Get a persisted state value for this field with instance key.
                                ///
                                /// Use this when you have multiple instances of the same component
                                /// (e.g., in a loop or list). The `instance_key` differentiates
                                /// between instances.
                                ///
                                /// # Example
                                ///
                                /// ```ignore
                                /// for i in 0..10 {
                                ///     let selected = MyComponent::use_selected_for(ctx, i, false);
                                /// }
                                /// ```
                                pub fn #method_name_for<C: blinc_core::BlincContext, K: std::fmt::Display>(
                                    ctx: &C,
                                    instance_key: K,
                                    initial: #field_type,
                                ) -> blinc_core::State<#field_type>
                                where
                                    #field_type: Clone + Send + 'static,
                                {
                                    let key = format!("{}:{}:{}", Self::COMPONENT_KEY, #field_key, instance_key);
                                    ctx.use_state_keyed(&key, || initial)
                                }

                                /// Get a persisted state value with auto-generated instance key.
                                ///
                                /// Uses `#[track_caller]` to generate a unique key based on the
                                /// call site location. Each unique call site gets its own instance.
                                ///
                                /// Prefer `use_<field>_for` in loops where you control the key.
                                #[track_caller]
                                pub fn #method_name_auto<C: blinc_core::BlincContext>(
                                    ctx: &C,
                                    initial: #field_type,
                                ) -> blinc_core::State<#field_type>
                                where
                                    #field_type: Clone + Send + 'static,
                                {
                                    let loc = std::panic::Location::caller();
                                    let key = format!("{}:{}:{}:{}:{}",
                                        Self::COMPONENT_KEY, #field_key,
                                        loc.file(), loc.line(), loc.column());
                                    ctx.use_state_keyed(&key, || initial)
                                }
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            }
            Fields::Unnamed(_) => Vec::new(),
            Fields::Unit => Vec::new(),
        },
        _ => Vec::new(),
    };

    // We use module_path!() + stringify!() in the generated code for a unique key
    let expanded = quote! {
        impl #name {
            /// Unique compile-time key for this component type.
            ///
            /// This is the base key derived from the module path and struct name.
            /// For instance-specific keys, use `instance_key()` or `instance_key_for()`.
            pub const COMPONENT_KEY: &'static str = concat!(module_path!(), "::", stringify!(#name));

            /// Generate an instance key based on the call site location.
            ///
            /// Uses `#[track_caller]` to capture file:line:column, creating a unique
            /// key for each instantiation point. This is useful for `blinc_cn` components
            /// that need automatic instance differentiation.
            ///
            /// # Example
            ///
            /// ```ignore
            /// struct MyComponent;
            /// impl MyComponent {
            ///     fn new() -> Self {
            ///         let key = Self::instance_key(); // Unique per call site
            ///         // Use key with state/animation...
            ///         Self
            ///     }
            /// }
            /// ```
            #[track_caller]
            pub fn instance_key() -> String {
                let loc = std::panic::Location::caller();
                format!("{}:{}:{}:{}",
                    Self::COMPONENT_KEY,
                    loc.file(), loc.line(), loc.column())
            }

            /// Generate an instance key with a user-provided suffix.
            ///
            /// Use this in loops or when you need to combine automatic location-based
            /// keys with custom identifiers (like array indices).
            ///
            /// # Example
            ///
            /// ```ignore
            /// for i in 0..10 {
            ///     let key = ListItem::instance_key_for(i);
            ///     // Use key with state/animation...
            /// }
            /// ```
            #[track_caller]
            pub fn instance_key_for<K: std::fmt::Display>(suffix: K) -> String {
                let loc = std::panic::Location::caller();
                format!("{}:{}:{}:{}:{}",
                    Self::COMPONENT_KEY,
                    loc.file(), loc.line(), loc.column(),
                    suffix)
            }

            /// Get a persisted animated value for this component.
            ///
            /// The value is uniquely identified by the component type.
            /// Multiple calls with the same component return the same animation.
            /// Generic over any context implementing `AnimationContext`.
            pub fn use_animated_value<C: blinc_animation::AnimationContext>(
                ctx: &C,
                initial: f32,
                config: blinc_animation::SpringConfig,
            ) -> blinc_animation::SharedAnimatedValue {
                ctx.use_animated_value_for(Self::COMPONENT_KEY, initial, config)
            }

            /// Get a persisted animated value with a suffix for multiple values per component.
            ///
            /// Use this when a component needs multiple independent animated values.
            /// Generic over any context implementing `AnimationContext`.
            ///
            /// # Example
            ///
            /// ```ignore
            /// let x = MyComponent::use_animated_value_with(ctx, "x", 0.0, SpringConfig::default());
            /// let y = MyComponent::use_animated_value_with(ctx, "y", 0.0, SpringConfig::default());
            /// ```
            pub fn use_animated_value_with<C: blinc_animation::AnimationContext>(
                ctx: &C,
                suffix: &str,
                initial: f32,
                config: blinc_animation::SpringConfig,
            ) -> blinc_animation::SharedAnimatedValue {
                let key = format!("{}:{}", Self::COMPONENT_KEY, suffix);
                ctx.use_animated_value_for(key, initial, config)
            }

            /// Get a persisted animated timeline for this component.
            ///
            /// The timeline is uniquely identified by the component type.
            /// Multiple calls with the same component return the same timeline.
            /// Generic over any context implementing `AnimationContext`.
            pub fn use_animated_timeline<C: blinc_animation::AnimationContext>(
                ctx: &C,
            ) -> blinc_animation::SharedAnimatedTimeline {
                ctx.use_animated_timeline_for(Self::COMPONENT_KEY)
            }

            /// Get a persisted animated timeline with a suffix for multiple timelines per component.
            ///
            /// Use this when a component needs multiple independent timelines.
            /// Generic over any context implementing `AnimationContext`.
            pub fn use_animated_timeline_with<C: blinc_animation::AnimationContext>(
                ctx: &C,
                suffix: &str,
            ) -> blinc_animation::SharedAnimatedTimeline {
                let key = format!("{}:{}", Self::COMPONENT_KEY, suffix);
                ctx.use_animated_timeline_for(key)
            }

            #(#field_methods)*
        }
    };

    TokenStream::from(expanded)
}

/// Transpile a flow shader to WGSL at compile time.
///
/// Evaluates to a `blinc_core::FlowShader`: the WGSL and what a pipeline binds for it. It is a
/// constant expression, so a `const` can hold one, and nothing is parsed or generated at run
/// time. Draw it with `Canvas::flow`.
///
/// ```ignore
/// let ripple = flow!(ripple, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     node d = distance(uv, vec2(0.5, 0.5));
///     node wave = sin(d * 20.0 - time * 4.0);
///     output color = vec4(wave, wave, wave, 1.0);
/// });
/// ```
///
/// The body is the flow language written as Rust tokens: `input`, `node`, `step`, `chain`,
/// `use` and `output` statements. Step types take their underscore names (`pattern_noise`,
/// `color_ramp`, `adjust_falloff`) and colours are hex integers (`0x1a3a5a`, or
/// `0xrrggbbaa` with alpha).
///
/// ```ignore
/// let terrain = flow!(terrain, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     step noise: pattern_noise { scale: 4.0; detail: 6; animation: time * 0.3; }
///     step palette: color_ramp {
///         source: noise;
///         stops: 0x1a3a5a 0.0, 0x88cc66 0.55, 0xffffff 1.0;
///     }
///     output color = palette;
/// });
/// ```
///
/// A flow that does not parse or generate is a compile error at the macro call. The expansion
/// names `::blinc_core`, so the calling crate depends on it.
#[proc_macro]
pub fn flow(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as flow::FlowInput);
    TokenStream::from(flow::expand(input))
}
