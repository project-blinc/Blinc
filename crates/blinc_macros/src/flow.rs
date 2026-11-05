//! `flow!`: a flow shader transpiled to WGSL at compile time.

use proc_macro2::{Delimiter, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Token};

use blinc_core::{FlowShader, FlowTarget};

/// The flow as written in the macro call: `name, target, { body }`.
pub(crate) struct FlowInput {
    name: Ident,
    target: Ident,
    body: TokenStream,
}

impl Parse for FlowInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        let target = input.parse()?;
        input.parse::<Token![,]>()?;
        let body: proc_macro2::Group = input.parse()?;
        if body.delimiter() != Delimiter::Brace {
            return Err(syn::Error::new(body.span(), "expected `{ ... }`"));
        }
        input.parse::<Option<Token![,]>>()?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the flow body"));
        }
        Ok(Self {
            name,
            target,
            body: body.stream(),
        })
    }
}

pub(crate) fn expand(input: FlowInput) -> TokenStream {
    let FlowInput { name, target, body } = &input;
    let source = format!("@flow {name} {{\n  target: {target};\n  {body}\n}}");
    let span = name.span();
    match compile(&source) {
        Ok(shader) => emit(&shader),
        Err(message) => syn::Error::new(span, message).to_compile_error(),
    }
}

fn compile(source: &str) -> Result<FlowShader, String> {
    let graph = blinc_flow::parse_flow_string(source)?;
    blinc_flow::compile_flow(&graph).map_err(|e| format!("flow codegen: {e}"))
}

/// The shader as a constant expression, so it works in a `const` and builds nothing at run time.
fn emit(shader: &FlowShader) -> TokenStream {
    let core = crate::paths::resolve("blinc_core", "core");
    let name = &*shader.name;
    let wgsl = &*shader.wgsl;
    let target = match shader.target {
        FlowTarget::Fragment => quote!(Fragment),
        FlowTarget::Compute => quote!(Compute),
        FlowTarget::Vertex => quote!(Vertex),
        FlowTarget::Material => quote!(Material),
    };
    let needs_scene = shader.needs_scene;
    let animated = shader.animated;
    let dynamic_fields = shader.dynamic_fields;
    let buffers = shader.buffers.iter().map(|b| {
        let (name, size, rw) = (&*b.name, b.element_size, b.read_write);
        quote! {
            #core::FlowBuffer {
                name: ::std::borrow::Cow::Borrowed(#name),
                element_size: #size,
                read_write: #rw,
            }
        }
    });
    quote! {
        #core::FlowShader {
            name: ::std::borrow::Cow::Borrowed(#name),
            target: #core::FlowTarget::#target,
            wgsl: ::std::borrow::Cow::Borrowed(#wgsl),
            needs_scene: #needs_scene,
            animated: #animated,
            dynamic_fields: #dynamic_fields,
            buffers: ::std::borrow::Cow::Borrowed(&[#(#buffers),*]),
        }
    }
}
