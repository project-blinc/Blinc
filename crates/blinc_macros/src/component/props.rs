use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

use super::has_animation_attr;

pub(super) fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "BlincComponent requires a struct",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Ok(quote! {});
    };
    let name = &input.ident;
    let props_name = format_ident!("{}Props", name);
    let visibility = &input.vis;
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let fields: Vec<_> = fields
        .named
        .iter()
        .filter(|field| !has_animation_attr(field))
        .collect();
    let names: Vec<_> = fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect();
    let types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let mut clone_generics = generics.clone();
    for ty in &types {
        clone_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ty: ::core::clone::Clone));
    }
    let (clone_impl_generics, _, clone_where_clause) = clone_generics.split_for_impl();
    let field_visibility = fields.iter().map(|field| &field.vis);
    let docs: Vec<Vec<_>> = fields
        .iter()
        .map(|field| {
            field
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("doc"))
                .collect()
        })
        .collect();
    let mut marker = format_ident!("__blinc_component_marker");
    while names.iter().any(|name| **name == marker) {
        marker = format_ident!("{}__", marker);
    }
    let setters = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let docs = field.attrs.iter().filter(|attr| attr.path().is_ident("doc"));
        quote! {
            #(#docs)*
            #[must_use]
            pub fn #field_name(mut self, value: impl ::blinc_layout::binding::IntoReactive<#ty>) -> Self {
                self.#field_name = ::blinc_layout::binding::IntoReactive::into_reactive(value);
                self
            }
        }
    });
    let props_doc = format!(
        "Reactive props for [`{name}`]. Forward fields to UI bindings without reading them during construction."
    );

    Ok(quote! {
        #[doc = #props_doc]
        #visibility struct #props_name #generics #where_clause {
            #(
                #(#docs)*
                #field_visibility #names: ::blinc_layout::binding::Reactive<#types>,
            )*
            #marker: ::core::marker::PhantomData<fn() -> #name #type_generics>,
        }

        impl #clone_impl_generics ::core::clone::Clone for #props_name #type_generics #clone_where_clause {
            fn clone(&self) -> Self {
                Self {
                    #(#names: ::core::clone::Clone::clone(&self.#names),)*
                    #marker: ::core::marker::PhantomData,
                }
            }
        }

        impl #impl_generics #name #type_generics #where_clause {
            /// Construct reactive props from values, signals or computeds.
            pub fn props(#(#names: impl ::blinc_layout::binding::IntoReactive<#types>),*) -> #props_name #type_generics {
                #props_name {
                    #(#names: ::blinc_layout::binding::IntoReactive::into_reactive(#names),)*
                    #marker: ::core::marker::PhantomData,
                }
            }
        }

        impl #impl_generics #props_name #type_generics #where_clause {
            #(#setters)*
        }
    })
}
