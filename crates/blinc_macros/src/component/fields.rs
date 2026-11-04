use quote::{format_ident, quote};
use syn::ext::IdentExt;

use super::has_animation_attr;

pub(super) fn expand(field: &syn::Field) -> proc_macro2::TokenStream {
    let name = field.ident.as_ref().unwrap();
    let key = name.unraw().to_string();
    let single = format_ident!("use_{}", key);
    let keyed = format_ident!("use_{}_for", key);
    let automatic = format_ident!("use_{}_auto", key);
    let ty = &field.ty;
    let (context, arguments, output, bounds, get) = if has_animation_attr(field) {
        (
            quote!(::blinc_animation::AnimationContext),
            quote!(initial: f32, config: ::blinc_animation::SpringConfig),
            quote!(::blinc_animation::SharedAnimatedValue),
            quote!(),
            quote!(ctx.use_animated_value_for(key, initial, config)),
        )
    } else {
        (
            quote!(::blinc_core::BlincContext),
            quote!(initial: #ty),
            quote!(::blinc_core::Signal<#ty>),
            quote!(where #ty: ::core::clone::Clone + ::core::marker::Send + 'static),
            quote!(ctx.use_signal_keyed(&key, || initial)),
        )
    };

    quote! {
        /// Retrieve this field's persistent hook for a single instance.
        pub fn #single<__BlincContext: #context>(ctx: &__BlincContext, #arguments) -> #output #bounds {
            let key = ::std::format!("{}:{}", ::core::any::type_name::<Self>(), #key);
            #get
        }

        /// Retrieve this field's persistent hook for an explicit instance key.
        pub fn #keyed<__BlincContext: #context, __BlincKey: ::core::fmt::Display>(
            ctx: &__BlincContext, instance_key: __BlincKey, #arguments
        ) -> #output #bounds {
            let key = ::std::format!("{}:{}:{}", ::core::any::type_name::<Self>(), #key, instance_key);
            #get
        }

        /// Retrieve this field's persistent hook keyed by the caller location.
        #[track_caller]
        pub fn #automatic<__BlincContext: #context>(ctx: &__BlincContext, #arguments) -> #output #bounds {
            let loc = ::core::panic::Location::caller();
            let key = ::std::format!("{}:{}:{}:{}:{}", ::core::any::type_name::<Self>(), #key, loc.file(), loc.line(), loc.column());
            #get
        }
    }
}
