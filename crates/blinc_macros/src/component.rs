mod fields;
mod props;

use quote::quote;
use syn::{Data, DeriveInput, Fields};

fn has_animation_attr(field: &syn::Field) -> bool {
    field
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("animation"))
}

pub(crate) fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let props = props::expand(&input)?;

    let field_methods = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => fields.named.iter().map(fields::expand).collect::<Vec<_>>(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };

    let expanded = quote! {
        #props

        impl #impl_generics #name #type_generics #where_clause {
            /// Unique compile-time key for this component type.
            pub const COMPONENT_KEY: &'static str = ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#name));

            /// Generate an instance key based on the call site location.
            #[track_caller]
            pub fn instance_key() -> ::std::string::String {
                let loc = ::std::panic::Location::caller();
                ::std::format!("{}:{}:{}:{}",
                    ::core::any::type_name::<Self>(),
                    loc.file(), loc.line(), loc.column())
            }

            /// Generate an instance key with a user-provided suffix.
            #[track_caller]
            pub fn instance_key_for<__BlincKey: ::std::fmt::Display>(suffix: __BlincKey) -> ::std::string::String {
                let loc = ::std::panic::Location::caller();
                ::std::format!("{}:{}:{}:{}:{}",
                    ::core::any::type_name::<Self>(),
                    loc.file(), loc.line(), loc.column(),
                    suffix)
            }

            /// Get a persisted animated value for this component.
            pub fn use_animated_value<__BlincContext: ::blinc_animation::AnimationContext>(
                ctx: &__BlincContext,
                initial: f32,
                config: ::blinc_animation::SpringConfig,
            ) -> ::blinc_animation::SharedAnimatedValue {
                ctx.use_animated_value_for(::core::any::type_name::<Self>(), initial, config)
            }

            /// Get a persisted animated value with a suffix for multiple values per component.
            pub fn use_animated_value_with<__BlincContext: ::blinc_animation::AnimationContext>(
                ctx: &__BlincContext,
                suffix: &str,
                initial: f32,
                config: ::blinc_animation::SpringConfig,
            ) -> ::blinc_animation::SharedAnimatedValue {
                let key = ::std::format!("{}:{}", ::core::any::type_name::<Self>(), suffix);
                ctx.use_animated_value_for(key, initial, config)
            }

            /// Get a persisted animated timeline for this component.
            pub fn use_animated_timeline<__BlincContext: ::blinc_animation::AnimationContext>(
                ctx: &__BlincContext,
            ) -> ::blinc_animation::SharedAnimatedTimeline {
                ctx.use_animated_timeline_for(::core::any::type_name::<Self>())
            }

            /// Get a persisted animated timeline with a suffix for multiple timelines per component.
            pub fn use_animated_timeline_with<__BlincContext: ::blinc_animation::AnimationContext>(
                ctx: &__BlincContext,
                suffix: &str,
            ) -> ::blinc_animation::SharedAnimatedTimeline {
                let key = ::std::format!("{}:{}", ::core::any::type_name::<Self>(), suffix);
                ctx.use_animated_timeline_for(key)
            }

            #(#field_methods)*
        }
    };

    Ok(expanded)
}
