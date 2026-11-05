use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

fn dependency(name: &str) -> Option<TokenStream> {
    let found = crate_name(name).ok()?;
    let name = match found {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => name.to_owned(),
    };
    let ident = format_ident!("{}", name.replace('-', "_"));
    Some(quote!(::#ident))
}

/// Resolve a direct dependency or its module in the facade.
pub(crate) fn resolve(package: &str, module: &str) -> TokenStream {
    if let Some(path) = dependency(package) {
        return path;
    }
    if let Some(facade) = dependency("blinc") {
        let module = format_ident!("{module}");
        return quote!(#facade::#module);
    }
    let package = format_ident!("{package}");
    quote!(::#package)
}
