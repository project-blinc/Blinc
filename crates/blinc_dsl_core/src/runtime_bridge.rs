// =====================================================================
// Runtime-substrate bridge (blinc_runtime::fsm)
// =====================================================================
//
// JIT-side impls of the substrate traits. Both publishers (JIT here, future
// LLVM AOT) write to the same `FsmRegistry` and install their own dispatcher.

use super::*;
use zyntax_embed::FiberToken;

/// JIT `GuardDispatcher` — routes tick-guard calls through `TieredRuntime`.
/// Lifted guards return `i32` (1 = fires, 0 = doesn't).
pub(crate) struct JitGuardDispatcher {
    pub(crate) runtime: Arc<Mutex<TieredRuntime>>,
}

// SAFETY: `TieredRuntime` is `!Send + !Sync` (Cranelift `JITModule`). The
// surrounding `Mutex` serialises access; UI threads run single-threaded anyway.
// The unsafe impl is what lets `Arc<dyn GuardDispatcher>` hold a JIT dispatcher.
unsafe impl Send for JitGuardDispatcher {}
unsafe impl Sync for JitGuardDispatcher {}

impl blinc_runtime::fsm::GuardDispatcher for JitGuardDispatcher {
    fn call_guard(&self, symbol: &str) -> Option<bool> {
        let runtime = self.runtime.lock().ok()?;
        // Direct JIT dispatch: `call_function` routes through the
        // new BC interp tier which fails with `Host("missing block
        // …")` on lifted-guard HIR shapes; `call_raw` routes
        // through `call_dynamic_function` → zrtl TypeMeta lookup
        // which null-derefs for user-compiled fns. Transmute the
        // JIT pointer to `extern "C" fn() -> i32` instead —
        // exactly the shape `populate_fsm_registry_pass` lifts
        // guards to.
        let ptr = runtime.get_function_ptr(symbol)?;
        let guard: extern "C" fn() -> i32 = unsafe { std::mem::transmute(ptr) };
        // Release the runtime mutex BEFORE running the JIT body —
        // see `call_action` for the same rationale. Even read-only
        // guards can fire property-binding side effects via the
        // signals they read (notify_active → wake_callback → frame
        // scheduling), and any reentry into TieredRuntime would
        // deadlock against this lock.
        drop(runtime);
        Some(guard() != 0)
    }

    fn call_action(&self, symbol: &str) -> Option<()> {
        let runtime = self.runtime.lock().ok()?;
        // Same JIT-pointer transmute path as `call_guard`. Action
        // bodies are lifted by [`crate::passes`] to
        // `__fsm_action_<Fsm>_<idx>__` as zero-arg
        // `extern "C" fn()` — no return; side effects (signal
        // writes) happen during the call.
        let Some(ptr) = runtime.get_function_ptr(symbol) else {
            tracing::warn!(symbol = symbol, "JIT: action symbol not registered");
            return None;
        };
        tracing::debug!(symbol = symbol, "JIT: calling lifted action");
        let action: extern "C" fn() = unsafe { std::mem::transmute(ptr) };
        // CRITICAL: release the runtime mutex BEFORE running the JIT
        // body. The action's lifted code calls host externs
        // (`__signal_set_by_id_i32`, etc.); those don't reach back
        // into TieredRuntime, but anything in the framework that
        // does (computed re-evals, view re-renders triggered by
        // signal change) would deadlock against our held lock.
        drop(runtime);
        action();
        Some(())
    }
}

/// JIT `ViewRenderer` — value-returning views call as `() -> i64` (handle);
/// legacy Unit-returning views call as `() -> ()` and drain the scene-op buffer.
pub(crate) struct JitViewRenderer {
    pub(crate) runtime: Arc<Mutex<TieredRuntime>>,
    pub(crate) value_returning_views: Arc<Mutex<std::collections::HashSet<String>>>,
}

// SAFETY: same as `JitGuardDispatcher` — Mutex serialises access to `!Send` runtime.
unsafe impl Send for JitViewRenderer {}
unsafe impl Sync for JitViewRenderer {}

impl blinc_runtime::view::ViewRenderer for JitViewRenderer {
    fn render_named(
        &self,
        symbol: &str,
    ) -> Result<ZyntaxValue, blinc_runtime::view::ViewRenderError> {
        let is_value_returning = self
            .value_returning_views
            .lock()
            .map(|set| set.contains(symbol))
            .unwrap_or(false);

        let runtime = self.runtime.lock().map_err(|_| {
            blinc_runtime::view::ViewRenderError::Backend(
                "BlincDsl runtime mutex poisoned".to_string(),
            )
        })?;
        if is_value_returning {
            // Direct JIT dispatch — see [`JitGuardDispatcher::call_guard`].
            let ptr = runtime.get_function_ptr(symbol).ok_or_else(|| {
                blinc_runtime::view::ViewRenderError::Backend(format!(
                    "view symbol '{symbol}' not registered in runtime"
                ))
            })?;
            // User-component views (`<X>$view`) take a leading u64
            // `__instance_id__` synthetic param. `render_view` is the
            // top-level function exception — no `$view` suffix matches,
            // so it stays in the zero-arg ABI path.
            let user_view_takes_instance_id = symbol != "render_view"
                && !symbol.starts_with("$Blinc$")
                && symbol.ends_with("$view");
            // Release the runtime BEFORE entering JIT code. The lock
            // guards the runtime's own tables, which the lookup above
            // has finished with; the compiled function needs none of
            // them. Holding it across the call made every nested render
            // a deadlock -- `std::sync::Mutex` is not reentrant -- which
            // is what stopped a component from mounting its own
            // `Stateful` whose callback renders that component.
            //
            // Safe for the same reason the lock exists at all: the
            // runtime is `!Send` and only ever driven from the main
            // thread, so dropping the guard early widens no window that
            // was closed before.
            drop(runtime);
            if user_view_takes_instance_id {
                let view: extern "C" fn(u64) -> i64 = unsafe { std::mem::transmute(ptr) };
                Ok(ZyntaxValue::Int(view(0)))
            } else {
                let view: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
                Ok(ZyntaxValue::Int(view()))
            }
        } else {
            runtime
                .call::<()>(symbol, &[])
                .map_err(|e| blinc_runtime::view::ViewRenderError::Backend(e.to_string()))?;
            Ok(ZyntaxValue::Void)
        }
    }
}

/// Pre-register `blinc_layout` widget primitives (`Div`, `Text`, …) in the
/// substrate's `ComponentRegistry`. Idempotent; called once at `BlincDsl::new()`.
pub(crate) fn register_blinc_layout_primitives() {
    use blinc_runtime::component::{ComponentDefinition, PropDef, Type};
    use zyntax_typed_ast::InternedString;
    use zyntax_typed_ast::type_registry::PrimitiveType;

    let string_ty = Type::Primitive(PrimitiveType::String);
    let i64_ty = Type::Primitive(PrimitiveType::I64);
    let bool_ty = Type::Primitive(PrimitiveType::Bool);
    let prop = |name: &'static str, ty: Type| PropDef {
        name: std::sync::Arc::from(name),
        ty,
        reactive_inner: None,
    };
    let style_prop = || prop("__style", i64_ty.clone());
    let class_prop = || prop("class", string_ty.clone());
    let text_props = || {
        vec![
            prop("content", string_ty.clone()),
            style_prop(),
            class_prop(),
        ]
    };
    let child_props = || vec![prop("children", i64_ty.clone()), style_prop(), class_prop()];

    // `Div { ..children }` — container. `children` and `__style` cross as `i64` payloads.
    let div = ComponentDefinition {
        name: std::sync::Arc::from("Div"),
        view_symbol: std::sync::Arc::from("$Blinc$Div$view"),
        props: vec![
            prop("children", i64_ty.clone()),
            style_prop(),
            class_prop(),
            // `on_click = || { … }` — Zyntax closure value as `i64`.
            prop("on_click", i64_ty.clone()),
            // `overflow_scroll = true` — use Div's built-in scroll physics.
            prop("overflow_scroll", bool_ty.clone()),
            // `ref = card` — a `ref x: Div` / `ref x: Scroll` handle.
            prop("ref", i64_ty.clone()),
        ],
    };

    // `Text("hi", italic = true)` — text leaf.
    //
    // The four style flags mirror the Rust builder. `font-weight`,
    // `font-style` and `text-decoration` reach the same places from CSS,
    // so a class can carry them instead; these exist for the one-off that
    // does not deserve a rule.
    let text_widget = ComponentDefinition {
        name: std::sync::Arc::from("Text"),
        view_symbol: std::sync::Arc::from("$Blinc$Text$view"),
        props: vec![
            prop("content", string_ty.clone()),
            style_prop(),
            class_prop(),
            prop("italic", bool_ty.clone()),
            prop("bold", bool_ty.clone()),
            prop("strikethrough", bool_ty.clone()),
            prop("underline", bool_ty.clone()),
        ],
    };

    let stack = ComponentDefinition {
        name: std::sync::Arc::from("Stack"),
        view_symbol: std::sync::Arc::from("$Blinc$Stack$view"),
        props: vec![prop("children", i64_ty.clone()), style_prop()],
    };

    let image = ComponentDefinition {
        name: std::sync::Arc::from("Image"),
        view_symbol: std::sync::Arc::from("$Blinc$Image$view"),
        props: vec![prop("source", string_ty.clone()), style_prop()],
    };

    let svg = ComponentDefinition {
        name: std::sync::Arc::from("Svg"),
        view_symbol: std::sync::Arc::from("$Blinc$Svg$view"),
        props: vec![
            prop("source", string_ty.clone()),
            style_prop(),
            class_prop(),
        ],
    };

    let canvas = ComponentDefinition {
        name: std::sync::Arc::from("Canvas"),
        view_symbol: std::sync::Arc::from("$Blinc$Canvas$view"),
        props: vec![style_prop()],
    };

    let motion = ComponentDefinition {
        name: std::sync::Arc::from("Motion"),
        view_symbol: std::sync::Arc::from("$Blinc$Motion$view"),
        props: vec![prop("children", i64_ty.clone()), style_prop()],
    };

    let notch = ComponentDefinition {
        name: std::sync::Arc::from("Notch"),
        view_symbol: std::sync::Arc::from("$Blinc$Notch$view"),
        props: vec![prop("children", i64_ty.clone()), style_prop()],
    };

    blinc_runtime::component::with_component_registry_mut(|r| {
        r.register(div);
        r.register(text_widget);
        for name in [
            "H1",
            "H2",
            "H3",
            "H4",
            "H5",
            "H6",
            "P",
            "Span",
            "Small",
            "Label",
            "Muted",
            "Strong",
            "B",
            "Caption",
            "InlineCode",
        ] {
            r.register(ComponentDefinition {
                name: std::sync::Arc::from(name),
                view_symbol: std::sync::Arc::from(format!("$Blinc${name}$view").as_str()),
                props: text_props(),
            });
        }
        r.register(stack);
        r.register(image);
        r.register(svg);
        r.register(canvas);
        r.register(motion);
        r.register(notch);
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Hr"),
            view_symbol: std::sync::Arc::from("$Blinc$Hr$view"),
            props: vec![style_prop(), class_prop()],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Blockquote"),
            view_symbol: std::sync::Arc::from("$Blinc$Blockquote$view"),
            props: child_props(),
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Link"),
            view_symbol: std::sync::Arc::from("$Blinc$Link$view"),
            props: vec![
                prop("label", string_ty.clone()),
                prop("url", string_ty.clone()),
                style_prop(),
                class_prop(),
            ],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Ul"),
            view_symbol: std::sync::Arc::from("$Blinc$Ul$view"),
            props: child_props(),
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Ol"),
            view_symbol: std::sync::Arc::from("$Blinc$Ol$view"),
            props: vec![
                prop("children", i64_ty.clone()),
                prop("start", Type::Primitive(PrimitiveType::I32)),
                style_prop(),
                class_prop(),
            ],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Li"),
            view_symbol: std::sync::Arc::from("$Blinc$Li$view"),
            props: vec![prop("children", i64_ty.clone()), style_prop()],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("TaskItem"),
            view_symbol: std::sync::Arc::from("$Blinc$TaskItem$view"),
            props: vec![
                prop("children", i64_ty.clone()),
                prop("checked", bool_ty.clone()),
                style_prop(),
            ],
        });
        for name in ["Table", "Thead", "Tbody", "Tfoot", "Tr"] {
            r.register(ComponentDefinition {
                name: std::sync::Arc::from(name),
                view_symbol: std::sync::Arc::from(format!("$Blinc${name}$view").as_str()),
                props: child_props(),
            });
        }
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Th"),
            view_symbol: std::sync::Arc::from("$Blinc$Th$view"),
            props: vec![prop("content", string_ty.clone()), style_prop()],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Td"),
            view_symbol: std::sync::Arc::from("$Blinc$Td$view"),
            props: vec![prop("content", string_ty.clone()), style_prop()],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Cell"),
            view_symbol: std::sync::Arc::from("$Blinc$Cell$view"),
            props: vec![prop("children", i64_ty.clone()), style_prop()],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Button"),
            view_symbol: std::sync::Arc::from("$Blinc$Button$view"),
            props: vec![prop("label", string_ty.clone()), style_prop(), class_prop()],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Checkbox"),
            view_symbol: std::sync::Arc::from("$Blinc$Checkbox$view"),
            props: vec![
                prop("label", string_ty.clone()),
                prop("checked", bool_ty.clone()),
                style_prop(),
            ],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("TextInput"),
            view_symbol: std::sync::Arc::from("$Blinc$TextInput$view"),
            props: vec![
                prop("placeholder", string_ty.clone()),
                style_prop(),
                class_prop(),
            ],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("TextArea"),
            view_symbol: std::sync::Arc::from("$Blinc$TextArea$view"),
            props: vec![
                prop("placeholder", string_ty.clone()),
                prop("rows", Type::Primitive(PrimitiveType::I32)),
                style_prop(),
                class_prop(),
            ],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Code"),
            view_symbol: std::sync::Arc::from("$Blinc$Code$view"),
            props: vec![
                prop("content", string_ty.clone()),
                prop("line_numbers", bool_ty.clone()),
                style_prop(),
            ],
        });
        r.register(ComponentDefinition {
            name: std::sync::Arc::from("Pre"),
            view_symbol: std::sync::Arc::from("$Blinc$Pre$view"),
            props: vec![prop("content", string_ty.clone()), style_prop()],
        });
    });

    let _ = InternedString::new_global("__blinc_layout_primitives_marker__");
}

/// Mirror DSL component decls (impl + matching Class) into the runtime's
/// `ComponentRegistry`. View symbol is `<Name>$view`.
pub(crate) fn publish_components_to_runtime_registry(program: &TypedProgram) {
    use zyntax_typed_ast::typed_ast::TypedDeclaration;

    for decl in &program.declarations {
        let TypedDeclaration::Impl(imp) = &decl.node else {
            continue;
        };

        // `for_type` is usually `Type::Unresolved(name)` mid-pipeline;
        // post-resolution it can be `Type::Named { id, ... }`.
        let component_name_intern = match &imp.for_type {
            Type::Unresolved(name) => *name,
            Type::Named { id, .. } => {
                if let Some(type_def) = program.type_registry.get_type_by_id(*id) {
                    type_def.name
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        let component_name_string = match component_name_intern.resolve_global() {
            Some(s) => s,
            None => continue,
        };
        let component_name: &str = component_name_string.as_ref();

        // Only register impls with a sibling Class — skips FSM impls and orphan impls.
        let class_match = program.declarations.iter().any(|d| match &d.node {
            TypedDeclaration::Class(c) => c.name == component_name_intern,
            _ => false,
        });
        if !class_match {
            continue;
        }

        // Find the view method. Bodyless components are skipped defensively.
        let Some(view_method) = imp
            .methods
            .iter()
            .find(|m| m.name.resolve_global().as_deref() == Some("view"))
        else {
            continue;
        };

        // Each view param becomes a `PropDef`; `ty` passes through unchanged.
        // `__instance_id__` is auto-injected by
        // [`crate::passes::inject_user_view_instance_id_params`] as the leading
        // synthetic param; it's an implementation detail that downstream
        // call-site lowering threads through, NOT a user-facing prop, so it
        // must NOT appear in the registry's prop list (otherwise
        // `resolve_extern_widget_named_args` treats it as match-able and the
        // user could write `Counter(instance_id=42)`).
        let props: Vec<blinc_runtime::component::PropDef> = view_method
            .params
            .iter()
            .filter(|p| !p.is_self)
            .filter(|p| p.name.resolve_global().as_deref() != Some("__instance_id__"))
            .filter_map(|p| {
                let name_str = p.name.resolve_global()?;
                Some(blinc_runtime::component::PropDef {
                    name: std::sync::Arc::from(name_str.as_ref()),
                    ty: p.ty.clone(),
                    // User-component props don't carry `Reactive<T>`
                    // typing yet — the lowering only learns about
                    // reactive props from extern widgets via the
                    // `#[extern_widget]` macro today.
                    reactive_inner: None,
                })
            })
            .collect();

        let runtime_def = blinc_runtime::component::ComponentDefinition {
            name: std::sync::Arc::from(component_name),
            view_symbol: std::sync::Arc::from(format!("{component_name}$view").as_str()),
            props,
        };

        blinc_runtime::component::with_component_registry_mut(|r| {
            r.register(runtime_def);
        });
    }
}

/// Publish the local FSM registry into `blinc_runtime::fsm::FsmRegistry`.
/// State codes = enum decl order. Event codes = first-appearance order,
/// offset by `FSM_EVENT_CODE_OFFSET` to avoid colliding with pointer event codes.
pub(crate) fn publish_fsms_to_runtime_registry(program: &TypedProgram) {
    use zyntax_typed_ast::typed_ast::TypedDeclaration;

    for decl in &program.declarations {
        let TypedDeclaration::Impl(imp) = &decl.node else {
            continue;
        };
        let fsm_name_intern = imp.trait_name;
        let Some(fsm_name) = fsm_name_intern.resolve_global() else {
            continue;
        };

        // Find the matching state-enum. Mid-pipeline FSMs always have one.
        let state_enum = program.declarations.iter().find_map(|d| match &d.node {
            TypedDeclaration::Enum(e) if e.name == fsm_name_intern => Some(e),
            _ => None,
        });
        let Some(state_enum) = state_enum else {
            continue;
        };

        // Read local registry. Missing entry → bail (idempotent).
        let local_def = with_fsm_registry(|r| {
            r.iter()
                .map(|(_, d)| d)
                .find(|d| d.name.map(|n| n == fsm_name_intern).unwrap_or(false))
                .cloned()
        });
        let Some(local_def) = local_def else {
            continue;
        };

        // State codes = indices into declaration order.
        let state_names: Vec<std::sync::Arc<str>> = state_enum
            .variants
            .iter()
            .map(|v| std::sync::Arc::from(v.name.resolve_global().unwrap_or_default().as_ref()))
            .collect();
        let state_code = |name: zyntax_typed_ast::InternedString| -> Option<u32> {
            let s = name.resolve_global()?;
            let needle: &str = s.as_ref();
            state_names
                .iter()
                .position(|n| {
                    let n_ref: &str = n.as_ref();
                    n_ref == needle
                })
                .map(|i| i as u32)
        };

        // Event codes — first-appearance order, offset to avoid POINTER_* collisions.
        let mut event_names: Vec<std::sync::Arc<str>> = Vec::new();
        let mut event_code_of = |name: zyntax_typed_ast::InternedString| -> u32 {
            let resolved = name.resolve_global().unwrap_or_default();
            let needle: &str = resolved.as_ref();
            if let Some(i) = event_names.iter().position(|n| {
                let n_ref: &str = n.as_ref();
                n_ref == needle
            }) {
                return i as u32 + blinc_runtime::fsm::FSM_EVENT_CODE_OFFSET;
            }
            event_names.push(std::sync::Arc::from(needle));
            (event_names.len() - 1) as u32 + blinc_runtime::fsm::FSM_EVENT_CODE_OFFSET
        };

        let transitions: Vec<blinc_runtime::fsm::EventTransition> = local_def
            .transitions
            .iter()
            .filter_map(|t| {
                Some(blinc_runtime::fsm::EventTransition {
                    from_code: state_code(t.from)?,
                    event_code: event_code_of(t.event),
                    to_code: state_code(t.to)?,
                    actions: t.actions.clone(),
                })
            })
            .collect();

        let tick_guards: Vec<blinc_runtime::fsm::TickGuard> = local_def
            .tick_guards
            .iter()
            .filter_map(|g| {
                let symbol_intern = g.guard_fn?;
                let symbol = symbol_intern.resolve_global()?;
                Some(blinc_runtime::fsm::TickGuard {
                    from_code: state_code(g.from)?,
                    to_code: state_code(g.to)?,
                    guard_symbol: std::sync::Arc::from(symbol.as_ref()),
                })
            })
            .collect();

        let initial_code = local_def.initial.and_then(state_code).unwrap_or(0);

        let runtime_def = blinc_runtime::fsm::FsmDefinition {
            name: std::sync::Arc::from(fsm_name.as_ref()),
            initial_code,
            state_names,
            event_names,
            transitions,
            tick_guards,
        };

        blinc_runtime::fsm::with_fsm_registry_mut(|r| {
            r.register(runtime_def);
        });

        // Seed context-field signals with their declared defaults.
        // The synthesised `__fsm_ctx_<Fsm>_<field>: <ty>` signal decl
        // already exists by this point, and the runtime signal store
        // defaults to zero — but explicit non-zero literals need to be
        // written. We do the write unconditionally for clarity; the
        // cost is one HashMap insert per field at startup.
        for field in &local_def.context_fields {
            let Some(field_name) = field.name.resolve_global() else {
                continue;
            };
            let mangled = crate::fsm_registry::mangle_ctx_signal(&fsm_name, &field_name);
            match &field.default {
                crate::fsm_registry::ContextDefault::I32(v) => {
                    blinc_runtime::signal::set_i32(&mangled, *v);
                }
                crate::fsm_registry::ContextDefault::F64(v) => {
                    blinc_runtime::signal::set_f64(&mangled, *v);
                }
                crate::fsm_registry::ContextDefault::Bool(v) => {
                    // `set_bool`, not `set_i32` with 0/1: the synthesised
                    // signal is declared Bool, and minting it again as
                    // I32 leaves a type-mismatch warning and a signal
                    // whose stored type disagrees with every reader.
                    blinc_runtime::signal::set_bool(&mangled, *v);
                }
                crate::fsm_registry::ContextDefault::String(s) => {
                    if let Some(s_str) = s.resolve_global() {
                        blinc_runtime::signal::set_str(&mangled, s_str.to_string());
                    }
                }
            }
        }
    }
}

/// JIT `MachineDriver` — the fiber-backed FSM path.
///
/// Holds the two pieces of state a machine needs rather than the whole
/// `BlincDsl`: the runtime it lives in, and the handler instance that
/// carries its context.
///
/// The `u64` crossing the seam is this driver's own handle, not a
/// `FiberToken`: `blinc_runtime` needs no Zyntax types, the same reason
/// `GuardDispatcher` passes symbol names rather than function pointers.
/// A handle means nothing outside the map that issued it.
pub(crate) struct JitMachineDriver {
    pub(crate) runtime: crate::machines::Runtime,
    pub(crate) instances: crate::machines::Instances,
    handles: Mutex<std::collections::HashMap<u64, FiberToken>>,
    /// `(scope, fsm)` to the handle already issued for it. What makes
    /// `machine_for` idempotent across rebuilds: a view body asks on
    /// every render, and without this each render would build a machine
    /// whose context starts back at its declared defaults.
    by_key: Mutex<std::collections::HashMap<(u64, String), u64>>,
    next: std::sync::atomic::AtomicU64,
}

impl JitMachineDriver {
    pub(crate) fn new(
        runtime: crate::machines::Runtime,
        instances: crate::machines::Instances,
    ) -> Self {
        Self {
            runtime,
            instances,
            handles: Mutex::new(std::collections::HashMap::new()),
            by_key: Mutex::new(std::collections::HashMap::new()),
            next: std::sync::atomic::AtomicU64::new(1),
        }
    }

    fn issue(&self, token: FiberToken) -> u64 {
        let h = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(h, token);
        h
    }

    fn token(&self, handle: u64) -> Option<FiberToken> {
        self.handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&handle)
            .copied()
    }
}

// SAFETY: same reasoning as `JitGuardDispatcher` — the runtime behind
// this is `!Send + !Sync` and serialised by its own mutex.
unsafe impl Send for JitMachineDriver {}
unsafe impl Sync for JitMachineDriver {}

impl blinc_runtime::fsm::MachineDriver for JitMachineDriver {
    fn machine_for(&self, fsm: &str, scope: u64) -> Option<u64> {
        let key = (scope, fsm.to_string());
        if let Some(existing) = self
            .by_key
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return Some(*existing);
        }
        // Built outside the `by_key` lock: construction reaches into the
        // runtime, which a handler op can re-enter.
        let handle = crate::machines::create(&self.runtime, &self.instances, fsm)
            .ok()
            .map(|t| self.issue(t))?;
        let mut by_key = self.by_key.lock().unwrap_or_else(|e| e.into_inner());
        // A racing caller may have installed one while we built; keep
        // theirs so a scope never has two machines, and let ours fall to
        // `drop_machine` below.
        if let Some(existing) = by_key.get(&key) {
            let winner = *existing;
            drop(by_key);
            self.drop_machine(handle);
            return Some(winner);
        }
        by_key.insert(key, handle);
        Some(handle)
    }

    fn step(&self, _fsm: &str, machine: u64, event_code: u32) -> Option<u32> {
        crate::machines::step(&self.runtime, self.token(machine)?, event_code)
            .ok()
            .flatten()
    }

    fn context_signal_id(&self, fsm: &str, machine: u64, field: &str) -> Option<u64> {
        crate::machines::with_handler(&self.runtime, &self.instances, self.token(machine)?, |rt| {
            rt.call::<i64>(&crate::passes::ctx_id_reader_fn_name(fsm, field), &[])
        })
        .ok()
        .map(|v| v as u64)
    }

    fn drop_machine(&self, machine: u64) {
        let Some(token) = self
            .handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&machine)
        else {
            return;
        };
        self.by_key
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, h| *h != machine);
        let _ = crate::machines::release(&self.runtime, &self.instances, token);
    }
}
