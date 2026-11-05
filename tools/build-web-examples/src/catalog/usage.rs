//! Present each compiled fixture with only its component's inputs.
use super::{Component, Path, Result, pages};
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::{Command, Stdio},
};
use syn::{Expr, FnArg, Item, Pat, Stmt, Type, UseTree, visit_mut::VisitMut};

#[derive(Default)]
struct Inputs {
    session: BTreeSet<String>,
    fields: Vec<syn::Ident>,
}
fn binding_name(name: &syn::Ident) -> syn::Ident {
    if name == "input" {
        syn::Ident::new("input_data", name.span())
    } else {
        name.clone()
    }
}
impl VisitMut for Inputs {
    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        if path
            .path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .eq(["crate", "demos", "bound_text"].map(str::to_owned))
        {
            path.path = syn::parse_quote!(bound_text);
        }
        syn::visit_mut::visit_expr_path_mut(self, path);
    }
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Field(field) = expression {
            if let Expr::Path(base) = &*field.base {
                if base
                    .path
                    .get_ident()
                    .is_some_and(|id| self.session.contains(&id.to_string()))
                {
                    if let syn::Member::Named(name) = &field.member {
                        let name = name.clone();
                        if !self.fields.contains(&name) {
                            self.fields.push(name.clone());
                        }
                        let binding = binding_name(&name);
                        *expression = syn::parse_quote!(#binding);
                        return;
                    }
                }
            }
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }
}

#[derive(Default)]
struct Names(BTreeSet<String>);
impl VisitMut for Names {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        self.0.extend(
            path.segments
                .first()
                .into_iter()
                .map(|part| part.ident.to_string()),
        );
        syn::visit_mut::visit_path_mut(self, path);
    }
    fn visit_macro_mut(&mut self, value: &mut syn::Macro) {
        self.0.extend(
            value
                .tokens
                .to_string()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
        );
        syn::visit_mut::visit_macro_mut(self, value);
    }
}

fn session_type(ty: &Type) -> bool {
    let ty = if let Type::Reference(reference) = ty {
        &*reference.elem
    } else {
        ty
    };
    matches!(ty, Type::Path(path) if path.path.is_ident("DemoState"))
}

fn trim_import(tree: &mut UseTree, used: &BTreeSet<String>) -> bool {
    match tree {
        UseTree::Name(name) => used.contains(&name.ident.to_string()),
        UseTree::Rename(rename) => used.contains(&rename.rename.to_string()),
        UseTree::Glob(_) => true,
        UseTree::Path(path) => trim_import(&mut path.tree, used),
        UseTree::Group(group) => {
            group.items = std::mem::take(&mut group.items)
                .into_iter()
                .filter_map(|mut item| trim_import(&mut item, used).then_some(item))
                .collect();
            !group.items.is_empty()
        }
    }
}

pub(super) fn source(root: &Path, component: &Component) -> Result<String> {
    let mut file = syn::parse_file(&pages::fixture_source(root, component)?)?;
    let fields: BTreeMap<_, _> = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(item) if item.ident == "DemoState" => Some(
                item.fields
                    .iter()
                    .map(|field| (field.ident.as_ref().unwrap().to_string(), field.ty.clone()))
                    .collect(),
            ),
            _ => None,
        })
        .ok_or("missing fixture inputs")?;
    file.items.retain(|item| match item {
        Item::Struct(item) => item.ident != "DemoState",
        Item::Impl(item) => !session_type(&item.self_ty),
        _ => true,
    });
    for item in &mut file.items {
        let Item::Fn(function) = item else { continue };
        if function.sig.ident != component.entry {
            continue;
        }
        let mut inputs = Inputs::default();
        for argument in &function.sig.inputs {
            if let FnArg::Typed(argument) = argument {
                if session_type(&argument.ty) {
                    if let Pat::Ident(name) = &*argument.pat {
                        inputs.session.insert(name.ident.to_string());
                    }
                }
            }
        }
        inputs.visit_block_mut(&mut function.block);
        function.block.stmts.retain(|statement| {
            !matches!(statement, Stmt::Local(local)
                if matches!((&local.pat, &local.init), (Pat::Ident(name), Some(value))
                    if matches!(&*value.expr, Expr::Path(path) if path.path.is_ident(&name.ident))))
        });
        let mut used = Names::default();
        used.visit_block_mut(&mut function.block);
        let mut arguments = syn::punctuated::Punctuated::new();
        for name in inputs.fields {
            let ty = fields
                .get(&name.to_string())
                .ok_or("unknown fixture input")?;
            let binding = binding_name(&name);
            arguments.push(syn::parse_quote!(#binding: #ty));
        }
        arguments.extend(std::mem::take(&mut function.sig.inputs).into_iter().filter(|argument| {
            matches!(argument, FnArg::Typed(argument) if !session_type(&argument.ty)
                && !matches!(&*argument.pat, Pat::Wild(_))
                && !matches!(&*argument.pat, Pat::Ident(name) if !used.0.contains(&name.ident.to_string())))
        }));
        function.sig.inputs = arguments;
    }
    let mut used = Names::default();
    for item in &mut file.items {
        if matches!(item, Item::Fn(function) if function.sig.ident == "bound_text") {
            continue;
        }
        if !matches!(item, Item::Use(_)) {
            used.visit_item_mut(item);
        }
    }
    if !used.0.contains("bound_text") {
        file.items.retain(
            |item| !matches!(item, Item::Fn(function) if function.sig.ident == "bound_text"),
        );
    }
    let mut used = Names::default();
    for item in &mut file.items {
        if !matches!(item, Item::Use(_)) {
            used.visit_item_mut(item);
        }
    }
    file.items.retain_mut(|item| match item {
        Item::Use(import) => trim_import(&mut import.tree, &used.0),
        _ => true,
    });
    let mut formatter = Command::new("rustfmt")
        .args([
            "--edition",
            "2024",
            "--emit",
            "stdout",
            "--config",
            "max_width=120",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut source = String::new();
    let mut previous_import = false;
    for item in &file.items {
        let import = matches!(item, Item::Use(_));
        if !source.is_empty() {
            source.push_str(if import && previous_import {
                "\n"
            } else {
                "\n\n"
            });
        }
        source.push_str(&item.to_token_stream().to_string());
        previous_import = import;
    }
    formatter
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())?;
    let output = formatter.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "{} usage formatting: {}",
            component.slug,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let source = String::from_utf8(output.stdout)?;
    if source.contains("DemoState") {
        return Err("session plumbing leaked into usage".into());
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_examples_expose_only_their_own_inputs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let catalog: super::super::Catalog =
            serde_json::from_str(include_str!("../../../../web/components/catalog.json")).unwrap();
        let destination = root.join("target/catalog-usage");
        std::fs::create_dir_all(&destination).unwrap();
        for component in catalog.fixtures() {
            let source = source(root, &component).unwrap();
            syn::parse_file(&source).unwrap();
            assert!(!source.contains("DemoState"), "{}", component.slug);
            if component.slug == "button" {
                assert!(source.contains("clicks: Signal<u32>"));
                assert!(source.contains("disabled: Signal<bool>"));
                assert!(!source.contains("SharedTextInputData"));
                assert!(!source.contains("let clicks = clicks"));
            }
            if component.slug == "badge" {
                assert!(source.contains("fn badge_demo()"));
            }
            std::fs::write(destination.join(format!("{}.rs", component.slug)), source).unwrap();
        }
    }
}
