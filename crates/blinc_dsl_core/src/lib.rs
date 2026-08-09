//! Blinc DSL core — Zyntax-embedded grammar, runtime engine, and host glue.
//!
//! Pipeline: source → `Grammar2::from_source` → `TypedProgram` →
//! `TieredRuntime::compile_typed_program` → JIT call → scene buffer drained
//! via `take_scene_ops()` → `ElementBuilder` tree for the renderer.
//!
//! Builtins are registered statically — no `.zrtl` plugin discovery.

use std::path::Path;
use std::sync::{Arc, Mutex};

use thiserror::Error;
use zyntax_embed::{
    Grammar2, Grammar2Error, NativeSignature, NativeType, RuntimeError, TieredRuntime, TypeTag,
    ZrtlSigFlags, ZrtlSymbolSig,
};

/// Mirror of `zyntax_compiler::zrtl::MAX_PARAMS` (16). Part of `ZrtlSymbolSig`'s wire ABI.
const ZRTL_MAX_PARAMS: usize = 16;
use zyntax_typed_ast::type_registry::{PrimitiveType, Type};
use zyntax_typed_ast::{Span, TypedProgram, TypedStatement, typed_node};

/// Embedded Blinc DSL grammar source.
pub const BLINC_GRAMMAR: &str = include_str!("../grammar/blinc.zyn");

// The `#[extern_widget]` macro emits absolute `::blinc_dsl_core`
// paths, so the crate has to be able to name itself.
extern crate self as blinc_dsl_core;

mod abi;
pub mod core_widgets;
mod fsm_registry;
mod host;
mod passes;

pub use host::set_pending_fsm_event;
mod read_scope;

/// Region id for the whole-program `@stateful` render's read scope.
/// Reserved: `with` region ids come from the lowering and are positive.
const PROGRAM_READ_SCOPE: i64 = i64::MIN;
pub mod refs;
mod runtime_bridge;
mod widget_ffi;
mod with_regions;
#[doc(hidden)]
pub use with_regions::{
    __clear_mounted_deps, __deps_mentioning, __mounted_deps, __record_program_deps,
};

use abi::{register_builtins, type_to_native, type_to_tag};
pub use fsm_registry::{
    EventTransition, FsmDefinition, FsmId, FsmInstance, FsmRegistry, TickGuard, with_fsm_registry,
    with_fsm_registry_mut,
};
pub use host::{DslOp, take_scene_ops};
use passes::inject_call_site_keys;
use passes::inject_user_view_instance_id_params;
use passes::{
    annotate_computed_lambda_types, apply_module_namespace_prefix, bind_component_props,
    collect_declared, desugar_compound_assigns, detect_and_strip_stateful_views,
    ensure_unit_return, expand_const_groups, expand_exported_signals, expand_map_calls,
    extract_and_strip_exports, extract_and_strip_stylesheets, inject_fsm_context_markers,
    lower_bare_call_named_args, lower_children_arrays_to_blocks, lower_component_calls,
    lower_match_blocks, lower_reactive_args, lower_struct_literals,
    lower_struct_widget_props_to_handles, lower_styling_args_to_overlays,
    lower_view_to_value_returning, lower_with_blocks, materialize_view, module_namespace_from_path,
    populate_fsm_registry_pass, resolve_const_references, resolve_dotted_fsm_field_access,
    resolve_extern_widget_named_args, resolve_fsm_subscribe_calls, resolve_fsm_trigger_calls,
    resolve_signal_calls, resolve_signal_calls_scoped, rewrite_component_calls_in_program,
    synthesize_fsm_context_and_actions, synthesize_fsm_event_enums,
    synthesize_fsm_trait_interfaces, validate_component_calls,
};
use runtime_bridge::{
    JitGuardDispatcher, JitViewRenderer, publish_components_to_runtime_registry,
    publish_fsms_to_runtime_registry, register_blinc_layout_primitives,
};

pub use blinc_dsl_macro::extern_widget;

// `Reactive<T>` — the first typed DSL value at the FFI boundary.
// Re-exported here so widget-pack crates (`blinc_cn_dsl`,
// third-party `*_dsl` packs) get the type from the same crate
// they're already pulling for `#[extern_widget]` + `BlincDsl`. The
// canonical definition lives in `blinc_runtime::reactive_value`
// so the JIT and AOT compile paths share one enum.
pub use blinc_runtime::Reactive;
/// Stable per-call-site id, pushed around every widget call by the
/// lowering. Extern widgets use it to key per-instance state without
/// asking the DSL author for a name.
pub use widget_ffi::current_call_id;
pub use widget_ffi::{
    __extern_widget_internals, BlincStructFieldValue, BlincStructValue, ExternWidget,
    ExternWidgetSpec, RenderPropsOverlay, Styled, WidgetBox, materialize_overlay,
    materialize_widget,
};
pub use zyntax_embed::ZyntaxValue;

// =====================================================================
// Errors
// =====================================================================

/// Top-level error type for the embed API.
#[derive(Error)]
pub enum BlincDslError {
    /// `Grammar2::from_source(BLINC_GRAMMAR)` failed (Blinc-internal bug).
    #[error("blinc grammar compile failed: {0}")]
    Grammar(#[from] Grammar2Error),

    /// User's `.blinc` source has a parse / type / lowering error.
    #[error("blinc compile error: {0}")]
    Compile(String),

    /// `runtime.call::<T>(...)` failed at execution time.
    #[error("blinc runtime error: {0}")]
    Runtime(#[from] RuntimeError),

    /// Reading the source file off disk failed.
    #[error("blinc source io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Prints the same text `Display` does, verbatim.
///
/// The compile variant carries a rendered diagnostic — source excerpt,
/// carets, ANSI colour. The derived `Debug` wrapped that in
/// `Compile("…")` and escaped every newline and escape byte, so the one
/// place it matters most (`.expect(..)` on a failed compile, which
/// formats with `Debug`) printed an unreadable single line. A caller
/// should not have to know to use `{}` to read their own error.
impl std::fmt::Debug for BlincDslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

pub type BlincDslResult<T> = std::result::Result<T, BlincDslError>;

// =====================================================================
// Runtime engine
// =====================================================================

/// The Blinc DSL runtime. Owns the compiled grammar, the Zyntax runtime,
/// and the loaded module table.
pub struct BlincDsl {
    grammar: Grammar2,
    runtime: Arc<Mutex<TieredRuntime>>,
    /// JIT symbols `lower_view_to_value_returning` rewrote to the
    /// `i64` widget-handle ABI; consulted to choose `call_function`
    /// vs `call::<()>` at render time.
    value_returning_views: Arc<Mutex<std::collections::HashSet<String>>>,
    /// JIT function names per source path; used by `recompile_file`.
    compiled_modules: Arc<Mutex<std::collections::HashMap<std::path::PathBuf, Vec<String>>>>,
    /// Module-namespace prefix applied to user components in each
    /// compiled file (e.g. `widgets` for `widgets.blinc`, `ui$widgets`
    /// for `ui/widgets.blinc`). Empty string means the file was
    /// compiled without a source-root context — `compile_source` /
    /// `compile_file` / `compile_directory` all leave this empty so
    /// existing single-file tests stay un-mangled.
    /// `compile_project` populates non-empty entries for every file
    /// it compiles. `inject_imported_view_externs` reads this map to
    /// build the cross-file mangled-name reference at the entry site.
    module_namespaces: Arc<Mutex<std::collections::HashMap<std::path::PathBuf, String>>>,
    /// Non-fatal compile-time diagnostics accumulated across every
    /// `compile_*` call (e.g. duplicate-local-name imports from
    /// distinct source files). Compile keeps going — the warning is
    /// surfaced via `tracing::warn!` AND appended here so callers
    /// can present diagnostics in a structured way (IDE squiggles,
    /// test assertions, CI reports). Leverages Zyntax's
    /// `Diagnostic` shape — same annotation / help / note / code
    /// surface its type-checker and other phases emit, so a future
    /// unified renderer (ariadne, console, JSON) can format these
    /// alongside Zyntax-emitted diagnostics. Read via
    /// [`Self::compile_diagnostics`]; cleared via
    /// [`Self::clear_compile_diagnostics`].
    compile_diagnostics: Arc<Mutex<Vec<zyntax_typed_ast::Diagnostic>>>,
    /// CSS from `style { … }` blocks, compile-order.
    compiled_stylesheets: Arc<Mutex<Vec<String>>>,
    /// Cursor into `compiled_stylesheets` of how far the
    /// `BlincContextState` queue flush has reached.
    stylesheets_queued_up_to: Arc<Mutex<usize>>,
    /// Every declared `signal <name>: <T>`, accumulated across compiles.
    declared_signals: Arc<Mutex<Vec<(String, Type)>>>,
    /// Every declared `fsm <Name>`, accumulated across compiles.
    declared_fsms: Arc<Mutex<Vec<String>>>,
    /// Set when any view carries `@stateful`. `view_widget` then
    /// wraps the tree in a reactive `Stateful`.
    has_stateful_view: Arc<Mutex<bool>>,
    /// Signal ids the last whole-program render actually read.
    ///
    /// A bare `@stateful` has no dep list to go on, and the fallback is
    /// every declared signal — so an unrelated write re-renders the
    /// program. The render records what it touched, and the next build
    /// subscribes to that instead. `None` until one render has
    /// completed, which is why the fallback still has to exist.
    observed_view_deps: Arc<Mutex<Option<Vec<u64>>>>,
    /// Signal names this program declares reachable from outside it.
    exported_signals: Arc<Mutex<Vec<String>>>,
    /// Components carrying `@stateful`, by name. These mount their own
    /// `Stateful` at their call site, so a transition re-renders that
    /// component rather than the whole program. The entry `view { }` is
    /// never in here: it has no call site to scope to, so a decoration
    /// there still wraps everything.
    stateful_components: Arc<Mutex<Vec<String>>>,
    /// Signal names listed in `@stateful([…])`. Empty = subscribe
    /// to every declared signal.
    stateful_view_deps: Arc<Mutex<Vec<String>>>,
    /// FSM names listed in `@fsm([…])`. Empty = first declared FSM.
    stateful_view_fsms: Arc<Mutex<Vec<String>>>,
    /// `"<Fsm>.<field>"` for every context field a decorated view reads
    /// as a VALUE. Fields only passed as binding handles are absent, and
    /// must stay absent: they update through the property channel, so a
    /// stateful subscribed to them re-renders for nothing.
    stateful_ctx_value_reads: Arc<Mutex<Vec<String>>>,
}

impl BlincDsl {
    /// Build a fresh runtime with the embedded Blinc grammar and all
    /// host builtins pre-registered.
    pub fn new() -> BlincDslResult<Self> {
        // Parse grammar first so embedded-grammar bugs fail fast.
        let grammar = Grammar2::from_source(BLINC_GRAMMAR)?;

        // Tiered: cold code runs at tier 0 and hot views promote, and
        // the fiber-and-effect host API lives on this runtime only.
        // OSR so a view already running when a function promotes
        // transfers rather than finishing on the cold entry.
        let mut config = zyntax_embed::TieredConfig::development();
        config.enable_osr = true;
        let mut runtime = TieredRuntime::new(config)
            .map_err(|e| BlincDslError::Compile(format!("runtime init: {e}")))?;

        // MUST register builtins BEFORE any module load so the JIT linker can resolve them.
        register_builtins(&mut runtime);

        // MUST finalize after register_function — Cranelift JIT only sees symbols after rebuild.
        runtime
            .finalize_runtime_symbols()
            .map_err(|e| BlincDslError::Compile(format!("finalize symbols: {e}")))?;

        // `TieredRuntime` isn't Send+Sync; the Arc<Mutex<_>> wrapper is the production shape.
        #[allow(clippy::arc_with_non_send_sync)]
        let runtime = Arc::new(Mutex::new(runtime));

        register_blinc_layout_primitives();

        let value_returning_views = Arc::new(Mutex::new(std::collections::HashSet::new()));
        let compiled_modules = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let module_namespaces = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let compile_diagnostics = Arc::new(Mutex::new(Vec::new()));
        let compiled_stylesheets = Arc::new(Mutex::new(Vec::new()));
        let stylesheets_queued_up_to = Arc::new(Mutex::new(0));
        let declared_signals = Arc::new(Mutex::new(Vec::new()));
        let declared_fsms = Arc::new(Mutex::new(Vec::new()));
        let has_stateful_view = Arc::new(Mutex::new(false));
        let observed_view_deps = Arc::new(Mutex::new(None));
        let exported_signals = Arc::new(Mutex::new(Vec::new()));
        let stateful_components = Arc::new(Mutex::new(Vec::new()));
        let stateful_view_deps = Arc::new(Mutex::new(Vec::new()));
        let stateful_view_fsms = Arc::new(Mutex::new(Vec::new()));
        let stateful_ctx_value_reads = Arc::new(Mutex::new(Vec::new()));

        let this = Self {
            grammar,
            runtime,
            value_returning_views,
            compiled_modules,
            module_namespaces,
            compile_diagnostics,
            compiled_stylesheets,
            stylesheets_queued_up_to,
            declared_signals,
            declared_fsms,
            has_stateful_view,
            observed_view_deps,
            exported_signals,
            stateful_components,
            stateful_view_deps,
            stateful_view_fsms,
            stateful_ctx_value_reads,
        };
        // Auto-install the JIT bridge so FSM tick guards + transition
        // action bodies dispatch out of the box. Pre-fix this was an
        // opt-in `install_runtime_bridge()` call, which the common
        // single-DSL case had no reason to make — meaning guards / actions
        // silently no-op'd through the dispatcher's `None` fallback unless
        // the user happened to discover the method. Multi-DSL apps can
        // still call `install_runtime_bridge()` explicitly to swap which
        // instance owns the process-wide slot (last-write-wins).
        this.install_runtime_bridge();
        // Core widgets declared with `#[extern_widget]`. Registered
        // here rather than by the caller, so they are available with no
        // `register_*` call — the same guarantee the hand-written
        // builtins give.
        this.register_extern_widget::<core_widgets::RichTextWidget>()?;
        this.register_extern_widget::<core_widgets::MarkdownWidget>()?;
        Ok(this)
    }

    /// Install this `BlincDsl`'s JIT guard dispatcher as the process-wide
    /// `blinc_runtime::fsm::GuardDispatcher`. `BlincDsl::new()` already
    /// calls this; only needed when multiple `BlincDsl` instances coexist
    /// and you want to choose which one owns the dispatcher slot
    /// (last-write-wins).
    pub fn install_runtime_bridge(&self) {
        blinc_runtime::fsm::set_guard_dispatcher(std::sync::Arc::new(JitGuardDispatcher {
            runtime: self.runtime.clone(),
        }));
    }

    /// Register a Rust widget that implements [`ExternWidget`]. Primary Rust→DSL surface.
    pub fn register_extern_widget<W: ExternWidget>(&self) -> BlincDslResult<()> {
        self.register_extern_widget_spec(W::extern_widget_spec())
    }

    /// Lower-level than [`Self::register_extern_widget`] — register an explicit
    /// [`ExternWidgetSpec`]. MUST be called before `compile_source` for any
    /// source that uses the widget.
    pub fn register_extern_widget_spec(&self, spec: ExternWidgetSpec) -> BlincDslResult<()> {
        if spec.param_types.len() > ZRTL_MAX_PARAMS {
            return Err(BlincDslError::Compile(format!(
                "register_extern_widget({}): parameter count {} exceeds ZRTL_MAX_PARAMS ({})",
                spec.name,
                spec.param_types.len(),
                ZRTL_MAX_PARAMS
            )));
        }
        let mut params = [TypeTag::VOID; ZRTL_MAX_PARAMS];
        for (i, ty) in spec.param_types.iter().enumerate() {
            params[i] = type_to_tag(ty);
        }
        let sig = ZrtlSymbolSig {
            param_count: spec.param_types.len() as u8,
            flags: ZrtlSigFlags::NONE,
            return_type: type_to_tag(&spec.return_type),
            params,
        };

        // Leak the symbol — `register_function_typed` requires `&'static str`.
        // Bounded by widget-type count, not instance count.
        let view_symbol_static: &'static str = Box::leak(spec.view_symbol.into_boxed_str());

        {
            let mut runtime = self
                .runtime
                .lock()
                .expect("BlincDsl runtime mutex poisoned");
            // MUST finalize after register — Cranelift only sees new symbols after rebuild.
            runtime.register_function_typed(view_symbol_static, spec.extern_ptr, sig);
            runtime
                .finalize_runtime_symbols()
                .map_err(|e| BlincDslError::Compile(format!("finalize symbols: {e}")))?;
        }

        blinc_runtime::component::with_component_registry_mut(|r| {
            r.register(blinc_runtime::component::ComponentDefinition {
                name: std::sync::Arc::from(spec.name.as_str()),
                view_symbol: std::sync::Arc::from(view_symbol_static),
                props: spec.props,
            });
        });

        // Widget-handle externs are value-returning — pick the `i64`-return ABI at render time.
        self.value_returning_views
            .lock()
            .expect("value_returning_views mutex poisoned")
            .insert(view_symbol_static.to_string());

        Ok(())
    }

    /// Build a DSL-defined component as a `Box<dyn ElementBuilder>`. DSL→Rust
    /// half of the interop. Props pass through as positional Zyntax values.
    pub fn query(
        &self,
        name: &str,
        props: &[ZyntaxValue],
    ) -> BlincDslResult<Box<dyn blinc_layout::div::ElementBuilder>> {
        let view_symbol = blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(name).map(|def| def.view_symbol.clone())
        })
        .ok_or_else(|| {
            BlincDslError::Compile(format!(
                "query({name}): no component named `{name}` is registered — compile DSL source \
                 that declares it, or call `register_extern_widget` first"
            ))
        })?;

        let is_value_returning = self
            .value_returning_views
            .lock()
            .map(|set| set.contains(view_symbol.as_ref()))
            .unwrap_or(false);
        if !is_value_returning {
            return Err(BlincDslError::Compile(format!(
                "query({name}): component's view symbol `{}` isn't value-returning — only \
                 widget-primitive-rooted views can be queried (legacy `text(...)` bodies \
                 produce Unit)",
                view_symbol
            )));
        }

        // Param-count check happens inside `call_function`. We
        // build a signature whose return is I64 (widget handle)
        // and whose params are derived from the component def's
        // prop list, which (after `publish_components_to_runtime_registry`)
        // mirrors the view method's actual ABI.
        let param_types: Vec<Type> = blinc_runtime::component::with_component_registry(|r| {
            r.get_by_name(name)
                .map(|def| def.props.iter().map(|p| p.ty.clone()).collect())
                .unwrap_or_default()
        });

        if param_types.len() > ZRTL_MAX_PARAMS {
            return Err(BlincDslError::Compile(format!(
                "query({name}): component declares {} props, exceeds ZRTL_MAX_PARAMS ({})",
                param_types.len(),
                ZRTL_MAX_PARAMS
            )));
        }

        // Build a `NativeSignature` for `call_function`. The
        // ZRTL `Type` → native conversion mirrors `type_to_tag`
        // but lives in this caller because `call_function`
        // takes the broader `NativeType` shape, not a `TypeTag`.
        //
        // User-component views (`<X>$view`) now take a leading
        // `__instance_id__: u64` synthetic param injected by
        // [`crate::passes::inject_user_view_instance_id_params`].
        // Substrate widget extern views (`$Blinc$X$view`) similarly
        // take a leading `u64` injected by `descriptor_to_sig`. We
        // detect both cases by the view_symbol shape and synthesise
        // a `0` instance-id at the front of the props list.
        let view_symbol_str: &str = view_symbol.as_ref();
        let takes_instance_id = view_symbol_str.starts_with("$Blinc$")
            || (view_symbol_str.ends_with("$view") && !view_symbol_str.starts_with("$Blinc$"));

        let mut native_params: Vec<NativeType> = Vec::with_capacity(param_types.len() + 1);
        if takes_instance_id {
            native_params.push(NativeType::I64); // u64 maps to I64 in NativeType
        }
        for ty in &param_types {
            let nt = type_to_native(ty).map_err(|ty| {
                BlincDslError::Compile(format!(
                    "query({name}): no NativeType mapping for prop type {ty:?}"
                ))
            })?;
            native_params.push(nt);
        }
        let sig = NativeSignature::new(&native_params, NativeType::I64);

        // Prepend the synthetic instance_id = 0 to the props if needed.
        let mut props_with_id: Vec<ZyntaxValue>;
        let props_ref: &[ZyntaxValue] = if takes_instance_id {
            props_with_id = Vec::with_capacity(props.len() + 1);
            props_with_id.push(ZyntaxValue::Int(0));
            props_with_id.extend_from_slice(props);
            &props_with_id
        } else {
            props
        };

        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        let result = runtime
            .call_function(view_symbol.as_ref(), props_ref, &sig)
            .map_err(BlincDslError::from)?;
        drop(runtime);

        let ZyntaxValue::Int(handle) = result else {
            return Err(BlincDslError::Compile(format!(
                "query({name}): expected ZyntaxValue::Int(handle) from view call, got {result:?}"
            )));
        };

        // SAFETY: handle came from a registered widget-handle extern.
        let widget = unsafe { materialize_widget(handle) }.ok_or_else(|| {
            BlincDslError::Compile(format!(
                "query({name}): view returned the null handle (extern build failed)"
            ))
        })?;
        Ok(widget.into_element_builder())
    }

    /// Compile a `.blinc` source. Returns JIT function names (keyed by name
    /// for hot-reload). Runs the full post-parse pipeline: marker injection,
    /// signal/fsm resolution, component lowering, value-returning view rewrite,
    /// styling overlay collection, extern named-arg resolution, init dispatch.
    pub fn compile_source(&self, source: &str, filename: &str) -> BlincDslResult<Vec<String>> {
        self.compile_source_with_namespace(source, filename, "")
    }

    /// `compile_source` variant that prefixes every user-component
    /// declaration with `module_namespace` so cross-file declarations
    /// don't collide in the JIT symbol table / component registry.
    /// Pass `""` for the single-file unprefixed shape (which is what
    /// the public `compile_source` does). `compile_project` derives
    /// the namespace from each file's path relative to the source
    /// root and calls this with the non-empty form.
    pub fn compile_source_with_namespace(
        &self,
        source: &str,
        filename: &str,
        module_namespace: &str,
    ) -> BlincDslResult<Vec<String>> {
        let mut runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");

        // `parse_with_signatures` runs Zyntax's `inject_builtin_externs`. We
        // populate signatures via `register_function_typed` in `register_builtins`.
        let mut typed_program = self
            .grammar
            .parse_with_signatures(source, filename, runtime.plugin_signatures())
            // A parse failure carries the source it came from, so it
            // renders as a snippet with the offending token underlined
            // rather than a line:column pair the reader has to go and
            // look up. Colour follows the terminal: on for a dev loop,
            // off when the output is piped, captured or read back into
            // a UI label.
            .map_err(|e| {
                BlincDslError::Compile(e.render_auto().unwrap_or_else(|| e.to_string()))
            })?;

        // Names the module for the whole pass pipeline below, so the
        // passes that turn a bare identifier into a signal id resolve it
        // against THIS file's declarations. Dropped at the end of the
        // function, restoring whatever an outer compile had set.
        let module = passes::module_of(filename, module_namespace);
        let _scope = passes::enter_module(&module);

        // Apply the module-namespace prefix to local components
        // BEFORE import-extern injection so cross-module references
        // resolve against the registered mangled name. No-op when
        // `module_namespace` is empty (single-file compiles).
        apply_module_namespace_prefix(&mut typed_program, module_namespace);

        // Pre-Zyntax-212dba3 a call to an imported `<Comp>$view` lowered
        // to `Indirect(Undef)` and slid through; post-bump it surfaces a
        // clean `Lowering("Call to undefined function ...")`. Inject
        // extern decls for already-compiled imports so the entry's
        // lowering sees them as known symbols.
        self.inject_imported_view_externs(&mut typed_program, filename);

        // Lift `with @fsm([…]) { … }` regions into synthetic component
        // views. MUST precede the decorator sweep below: the markers a
        // `with` block carries belong to its region, and left in place
        // the sweep would read them as a decoration on the enclosing
        // view — the whole-program wrap `with` exists to avoid.
        let with_regions = lower_with_blocks(&mut typed_program);

        // Detect and strip `@stateful` / `@fsm` markers. Accumulate explicit deps.
        {
            let (saw_stateful, explicit_deps, explicit_fsms, ctx_value_reads, components) =
                detect_and_strip_stateful_views(&mut typed_program);
            if !components.is_empty() {
                let mut acc = self
                    .stateful_components
                    .lock()
                    .expect("stateful_components mutex poisoned");
                for name in components {
                    if !acc.contains(&name) {
                        acc.push(name);
                    }
                }
            }
            if !ctx_value_reads.is_empty() {
                let mut acc = self
                    .stateful_ctx_value_reads
                    .lock()
                    .expect("stateful_ctx_value_reads mutex poisoned");
                for name in ctx_value_reads {
                    if !acc.contains(&name) {
                        acc.push(name);
                    }
                }
            }
            if saw_stateful {
                *self
                    .has_stateful_view
                    .lock()
                    .expect("has_stateful_view mutex poisoned") = true;
            }
            if !explicit_deps.is_empty() {
                let mut acc = self
                    .stateful_view_deps
                    .lock()
                    .expect("stateful_view_deps mutex poisoned");
                for name in explicit_deps {
                    if !acc.contains(&name) {
                        acc.push(name);
                    }
                }
            }
            if !explicit_fsms.is_empty() {
                let mut acc = self
                    .stateful_view_fsms
                    .lock()
                    .expect("stateful_view_fsms mutex poisoned");
                for name in explicit_fsms {
                    if !acc.contains(&name) {
                        acc.push(name);
                    }
                }
            }
        }

        inject_fsm_context_markers(&mut typed_program);
        synthesize_fsm_event_enums(&mut typed_program);
        synthesize_fsm_trait_interfaces(&mut typed_program);
        // Expand `+=` / `-=` / `*=` / `/=` markers into plain
        // `target = target op value` BEFORE any pass that inspects
        // Binary expressions (resolve_signal_calls, match-lowering, …).
        desugar_compound_assigns(&mut typed_program);
        // Synthesise mangled-signal decls for `context { … }` fields,
        // lift transition-action bodies to top-level fns, rewrite
        // `ctx.<field>` inside lifted bodies + tick-guard expressions.
        // MUST run BEFORE `resolve_signal_calls` so the synthesised
        // signals + lifted-body rewrites flow through the standard
        // signal-resolution path.
        synthesize_fsm_context_and_actions(&mut typed_program);

        // Snapshot signal/fsm decls AFTER `synthesize_fsm_context_and_actions`
        // so the synthesised `__fsm_ctx_<Fsm>_<field>` signals join the
        // declared-signal list. The `@stateful` (no-deps) subscription
        // relies on this snapshot to enumerate which signals trigger
        // refresh — without it, FSM context mutations would write the
        // signal but the Stateful container wouldn't re-render. MUST
        // still run BEFORE `resolve_signal_calls` and friends, which
        // strip / rewrite signal decls.
        {
            let (signals, fsms) = collect_declared(&typed_program);
            self.declared_signals
                .lock()
                .expect("declared_signals mutex poisoned")
                .extend(signals);
            self.declared_fsms
                .lock()
                .expect("declared_fsms mutex poisoned")
                .extend(fsms);
        }
        lower_match_blocks(&mut typed_program);
        // MUST run before `resolve_const_references` so const-group
        // members are hoisted into individual `const` Variable
        // markers that the const-resolution pass can see.
        expand_const_groups(&mut typed_program);
        // MUST run before `resolve_signal_calls` and the FSM passes so
        // any const-substituted literals look identical to author-
        // written ones to downstream symbol-resolution work.
        resolve_const_references(&mut typed_program);
        // `<Fsm>.<field>` dotted access from outside an FSM body
        // (view / init / sibling component) becomes a bare reference
        // to the mangled signal. MUST run BEFORE `resolve_signal_calls`
        // so it then handles `.get()` / `.set()` / direct assignment
        // uniformly.
        resolve_dotted_fsm_field_access(&mut typed_program);
        // BEFORE `resolve_signal_calls`: a ref declaration is the same
        // shape as a signal's, so its uses have to be claimed first.
        crate::passes::resolve_ref_calls(&mut typed_program, filename);
        // MUST run BEFORE `resolve_signal_calls`: the shorthand arrives
        // as a marker decl, and the signals pass would otherwise mint a
        // signal named `__blinc_export_signal__` and never see the real
        // one.
        {
            let mut exports = self
                .exported_signals
                .lock()
                .expect("exported_signals mutex poisoned");
            expand_exported_signals(&mut typed_program, &mut exports);
        }

        // Qualified by the module that declares them, so two files that
        // each declare `page` get two signals. References inside this
        // program keep resolving to the id this pass bakes in; only the
        // registry key is qualified.
        resolve_signal_calls_scoped(&mut typed_program, &module);
        // AFTER the signals pass, not before: a region files the ids of
        // the signals it depends on, and those ids only exist once this
        // module's declarations have minted. Filing names instead would
        // push the lookup to mount time, where the module is no longer
        // known.
        self.register_with_regions(&with_regions, &module);
        // Module hardcoded to "main" here — same key
        // `populate_fsm_registry_pass` uses below. Both FSM-call
        // passes consult the global `FsmRegistry` keyed by this
        // module so cross-file FSMs imported from previously-
        // compiled modules in the same `compile_project` resolve.
        let fsm_lookup_module = zyntax_typed_ast::InternedString::new_global("main");
        resolve_fsm_trigger_calls(&mut typed_program, fsm_lookup_module);
        resolve_fsm_subscribe_calls(&mut typed_program, fsm_lookup_module);

        // Extract `style { … }` blocks and queue through `BlincContextState` for next frame.
        {
            let mut sheets = self
                .compiled_stylesheets
                .lock()
                .expect("compiled_stylesheets mutex poisoned");
            let before = sheets.len();
            extract_and_strip_stylesheets(&mut typed_program, &mut sheets);
            if blinc_core::context_state::BlincContextState::is_initialized() {
                let ctx = blinc_core::context_state::BlincContextState::get();
                for css in &sheets[before..] {
                    ctx.queue_stylesheet(css.clone());
                }
            }
        }

        // `export { … }` — the names a host may reach. Collected before
        // the signals pass so a declaration can be checked against them.
        {
            let mut exports = self
                .exported_signals
                .lock()
                .expect("exported_signals mutex poisoned");
            extract_and_strip_exports(&mut typed_program, &mut exports);
            blinc_runtime::signal::set_exported(&exports);
        }

        lower_struct_literals(&mut typed_program)
            .map_err(|errors| BlincDslError::Compile(errors.join("\n")))?;

        // MUST run BEFORE component-call lowering. That pass does not
        // descend into lambda bodies, so a widget call inside the map
        // closure would keep its `__component_call__` marker and reach
        // the JIT unlowered. Expanding first puts each element's call in
        // an ordinary position where the marker is rewritten normally.
        expand_map_calls(&mut typed_program);

        // MUST validate BEFORE lower_component_calls — validator reads the marker shape.
        validate_component_calls(&typed_program)
            .map_err(|errors| BlincDslError::Compile(errors.join("\n")))?;
        lower_component_calls(&mut typed_program, filename);
        bind_component_props(&mut typed_program);
        // Inject `__instance_id__: u64` as the leading view-method param
        // so each user-component instance receives a distinct id at call
        // time. MUST run AFTER `bind_component_props` so prop params are
        // in place — instance_id goes before them.
        inject_user_view_instance_id_params(&mut typed_program);

        // Module hardcoded to "main" — Zyntax compiles each source into one module.
        let module = zyntax_typed_ast::InternedString::new_global("main");
        populate_fsm_registry_pass(&mut typed_program, module);

        publish_fsms_to_runtime_registry(&typed_program);

        // MUST run after `bind_component_props` so view params reflect the prop list.
        // `__instance_id__` is filtered out at registry-publication time
        // ([runtime_bridge.rs]) so it doesn't leak into the user-visible
        // prop list.
        publish_components_to_runtime_registry(&typed_program);

        // MUST run BEFORE `ensure_unit_return` so its defensive `Return(None)`
        // doesn't override the value-bearing one.
        {
            let mut vrv = self
                .value_returning_views
                .lock()
                .expect("value_returning_views mutex poisoned");
            lower_view_to_value_returning(&mut typed_program, &mut vrv);
        }

        // MUST run AFTER `lower_view_to_value_returning` and BEFORE `ensure_unit_return`.
        {
            let vrv = self
                .value_returning_views
                .lock()
                .expect("value_returning_views mutex poisoned");
            lower_children_arrays_to_blocks(&mut typed_program, &vrv);
        }

        // MUST run BEFORE `resolve_extern_widget_named_args` so it sees uniform `__style`.
        lower_styling_args_to_overlays(&mut typed_program);

        lower_struct_widget_props_to_handles(&mut typed_program)
            .map_err(|errors| BlincDslError::Compile(errors.join("\n")))?;

        // Resolve named args against our component registry — Zyntax's auto-injected
        // extern decls carry synthetic `p0`, `p1`, … param names that can't bind by name.
        resolve_extern_widget_named_args(&mut typed_program);

        // Expand `Reactive<T>` prop slots into (tag, payload) FFI pairs.
        // MUST run after `resolve_extern_widget_named_args` (sees
        // fully-positional args) and before `compile_typed_program`
        // (the macro thunk's signature has two slots per reactive
        // prop). Handles `cn.Progress(value = lit | signal | computed)`
        // and any other `#[reactive] Reactive<T>` prop on any
        // registered extern widget — built-in or namespaced.
        lower_reactive_args(&mut typed_program);

        // Resolve named args + splice defaults on bare calls to
        // user-declared top-level fns (`step(by = 5)` →
        // `step(default_x, 5)`). The `__named__` markers must be
        // lifted into a fully-positional list before
        // `compile_typed_program` — Zyntax doesn't recognise the
        // marker as a host extern.
        lower_bare_call_named_args(&mut typed_program);

        // Inject span-derived `u64` call-site keys as the leading arg to
        // every substrate-primitive widget call. Widget FFIs use it as
        // the state-key seed so dup-labelled widgets at distinct call
        // sites hold distinct state. MUST run AFTER
        // `resolve_extern_widget_named_args` (which rebuilds positional
        // args from the registry's prop list — the registry doesn't
        // know about the auto-injected u64, so prepending earlier would
        // get our literal dropped into the wrong slot).
        inject_call_site_keys(&mut typed_program, filename);

        // Defensive `Return(None)` so the body classifier can't infer a value-bearing return.
        // Last pass before codegen: earlier passes rebuild the computed
        // lambda nodes, so annotate here where the shape is final. Zyntax
        // reads the Lambda node's `Type::Function` to pick the return
        // register; without it the lambda returns I64 in `rax` while the
        // host FFI reads `xmm0` as f64 (silent 0.0).
        annotate_computed_lambda_types(&mut typed_program);

        ensure_unit_return(&mut typed_program);

        let function_names = runtime
            .compile_typed_program(typed_program)
            .map_err(|e| BlincDslError::Compile(e.to_string()))?;

        // Export `<Component>$view` symbols so the next `compile_source`
        // (e.g. the entry in `compile_project`) can link against imports
        // that resolve to them.
        let mut exported = false;
        for name in &function_names {
            if name.ends_with("$view") {
                exported |= runtime.export_function(name).is_ok();
            }
        }
        // An export accumulates a runtime symbol; the JIT resolves it
        // only after a rebuild. Batched: this module's whole surface goes
        // in, then one rebuild makes it linkable by the next compile.
        if exported {
            runtime
                .finalize_runtime_symbols()
                .map_err(|e| BlincDslError::Compile(format!("export views: {e}")))?;
        }

        // Eagerly run each component's `<Component>$__init__` exactly once at compile.
        // Running at compile (not on_mount) avoids accumulating subscribers across rebuilds.
        for name in &function_names {
            if !name.ends_with("$__init__") {
                continue;
            }
            // Best-effort: a typo'd signal/FSM in `init { … }` shouldn't sink the whole compile.
            let _ = runtime.call::<()>(name, &[]);
        }

        Ok(function_names)
    }

    /// For each `import { X } from "./path"` in `program`, resolve the
    /// imported file relative to `entry_filename`'s parent, look it up
    /// in `compiled_modules`, and synthesize `extern fn <X>$view(): i64`
    /// decls so the entry program's lowering recognises imported view
    /// calls as known symbols.
    ///
    /// When the imported file was compiled with a non-empty module
    /// namespace (recorded in `module_namespaces`), the extern's
    /// symbol becomes `<ns>$<X>$view` (matching what
    /// `apply_module_namespace_prefix` produced on the source side),
    /// AND every local `__component_call__("X")` marker in the entry
    /// is rewritten to `__component_call__("<ns>$X")` so the same
    /// mangled name flows through `lower_component_calls`.
    ///
    /// The synthesized extern carries no parameter list. Prop values still
    /// reach an imported component: `lower_component_calls` positionalises
    /// the call's arguments against the component's own signature, which is
    /// resolved from the imported module's compiled form rather than from
    /// this declaration. Covered by `tests/cross_module_props.rs`.
    fn inject_imported_view_externs(&self, program: &mut TypedProgram, entry_filename: &str) {
        use std::collections::HashMap;
        use zyntax_typed_ast::type_registry::{CallingConvention, Visibility};
        use zyntax_typed_ast::typed_ast::{TypedDeclaration, TypedFunction};
        use zyntax_typed_ast::{InternedString, typed_node};

        let entry_path = Path::new(entry_filename);
        let parent = entry_path.parent().unwrap_or_else(|| Path::new("."));
        let arch = zyntax_typed_ast::import_resolver::ModuleArchitecture::NodeStyle {
            extensions: vec![".blinc".to_string()],
            index_name: "index".to_string(),
        };

        let modules = match self.compiled_modules.lock() {
            Ok(m) => m.clone(),
            Err(_) => return,
        };
        let namespaces = match self.module_namespaces.lock() {
            Ok(m) => m.clone(),
            Err(_) => return,
        };

        let mut wanted: Vec<String> = Vec::new();
        // `local_name → mangled_name` rewrites for `__component_call__`
        // markers in the entry. Populated below from each import +
        // its source module's namespace. Keys interned for the shared
        // `rewrite_component_calls_in_program` helper.
        let mut import_rewrites: HashMap<InternedString, InternedString> = HashMap::new();
        // `local_name → (source_file, span)` for the first import that
        // bound each local name. Used to detect a second import from
        // a DIFFERENT file that re-binds the same local — that's the
        // shape `import { Counter } from "./red"` followed by
        // `import { Counter } from "./blue"` takes, where the second
        // import silently shadows the first at the use site.
        let mut local_name_owner: HashMap<String, (std::path::PathBuf, Span)> = HashMap::new();
        for decl in &program.declarations {
            let TypedDeclaration::Import(import) = &decl.node else {
                continue;
            };
            let import_span = decl.span;
            let segments: Vec<String> = import
                .module_path
                .iter()
                .filter_map(|s| s.resolve_global().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('/').to_string())
                .collect();
            if segments.is_empty() {
                continue;
            }
            let candidates = arch.module_to_paths(&segments, &parent.to_path_buf());
            let Some(imported_path) = candidates.into_iter().find(|p| p.exists()) else {
                continue;
            };
            let Some(compiled) = modules.get(&imported_path) else {
                continue;
            };
            let source_ns = namespaces.get(&imported_path).cloned().unwrap_or_default();
            for item in &import.items {
                let zyntax_typed_ast::TypedImportItem::Named { name, .. } = item else {
                    continue;
                };
                let Some(import_name) = name.resolve_global() else {
                    continue;
                };
                let local: &str = import_name.as_ref();

                // Duplicate-import diagnostic: second import binding
                // the same local from a DIFFERENT source file means
                // the later one shadows the earlier at the use site.
                // The namespacing pass keeps the two underlying
                // components distinct in the JIT symbol table, but
                // author-side disambiguation (`as MyCounter` alias,
                // qualified-call syntax) is needed to USE both. Same
                // file re-imports are silently de-duped instead of
                // flagged — they're a stylistic preference, not a
                // semantic collision.
                if let Some((existing_path, existing_span)) = local_name_owner.get(local) {
                    if existing_path != &imported_path {
                        let diag = zyntax_typed_ast::Diagnostic::warning(format!(
                            "duplicate import: `{local}` is imported from \
                             both `{first}` and `{second}` — the later import \
                             shadows the earlier at every reference in this file",
                            local = local,
                            first = existing_path.display(),
                            second = imported_path.display(),
                        ))
                        .with_code(zyntax_typed_ast::DiagnosticCode("BLINC-IMPORT-DUP"))
                        .with_primary(import_span, "duplicate import here")
                        .with_secondary(*existing_span, "first imported here")
                        .with_help(format!(
                            "the namespacing pass keeps both components \
                             distinct in the JIT symbol table (`{first_ns}` \
                             vs `{second_ns}`), but you need an alias or \
                             qualified-call syntax to reference both — \
                             rename one import with `as`, e.g. \
                             `import {{ {local} as {local}Other }} from \"{second_src}\"`",
                            first_ns = namespaces.get(existing_path).cloned().unwrap_or_default(),
                            second_ns = source_ns,
                            local = local,
                            second_src = imported_path.display(),
                        ));
                        self.emit_compile_diagnostic(diag);
                    }
                } else {
                    local_name_owner
                        .insert(local.to_string(), (imported_path.clone(), import_span));
                }

                let mangled_name = if source_ns.is_empty() {
                    local.to_string()
                } else {
                    format!("{source_ns}${local}")
                };
                let view_sym = format!("{mangled_name}$view");
                if compiled.iter().any(|s| s == &view_sym) && !wanted.contains(&view_sym) {
                    wanted.push(view_sym);
                }
                if mangled_name != local {
                    import_rewrites.insert(
                        InternedString::new_global(local),
                        InternedString::new_global(&mangled_name),
                    );
                }
            }
        }

        // Rewrite entry's `__component_call__("X")` markers to point at
        // each import's mangled name. Mirrors the in-pass rewrite that
        // `apply_module_namespace_prefix` does for local components.
        if !import_rewrites.is_empty() {
            rewrite_component_calls_in_program(program, &import_rewrites);
        }

        for sym in wanted {
            // Sanity guard against double-injection if the entry
            // source already declared `<X>$view` for some reason.
            let already_declared = program.declarations.iter().any(|d| {
                if let TypedDeclaration::Function(f) = &d.node {
                    f.name.resolve_global().as_deref() == Some(sym.as_str())
                } else {
                    false
                }
            });
            if already_declared {
                continue;
            }
            let interned = InternedString::new_global(&sym);
            // See `passes::consts` — rest-default is intentional so the
            // literal survives fields added in a local zyntax checkout.
            #[allow(clippy::needless_update)]
            let func = TypedFunction {
                name: interned,
                annotations: vec![],
                effects: vec![],
                with_handlers: vec![],
                type_params: vec![],
                params: vec![],
                return_type: Type::Primitive(PrimitiveType::I64),
                body: None,
                visibility: Visibility::Public,
                is_async: false,
                is_pure: false,
                is_external: true,
                calling_convention: CallingConvention::Default,
                link_name: Some(interned),
                // Rest-default so fields added upstream (e.g. `is_fiber`)
                // don't break this literal. Keeps the source compiling
                // against both the git pin and a local zyntax checkout.
                ..Default::default()
            };
            program.declarations.push(typed_node(
                TypedDeclaration::Function(func),
                Type::Primitive(PrimitiveType::Unit),
                Span::default(),
            ));
        }
    }

    /// Compile a `.blinc` file off disk. Records JIT names per-path for hot reload.
    pub fn compile_file(&self, path: &Path) -> BlincDslResult<Vec<String>> {
        self.compile_file_with_namespace(path, "")
    }

    /// `compile_file` variant that compiles the file with a non-empty
    /// module namespace. Used by `compile_project_inner` to mangle
    /// each project file's components per its path relative to the
    /// source root. Also stamps `module_namespaces` so subsequent
    /// import resolution from other files maps cross-module
    /// references to the right mangled name.
    pub fn compile_file_with_namespace(
        &self,
        path: &Path,
        module_namespace: &str,
    ) -> BlincDslResult<Vec<String>> {
        let source = std::fs::read_to_string(path)?;
        let filename = path.to_string_lossy();
        let names = self.compile_source_with_namespace(&source, &filename, module_namespace)?;
        self.compiled_modules
            .lock()
            .expect("compiled_modules mutex poisoned")
            .insert(path.to_path_buf(), names.clone());
        self.module_namespaces
            .lock()
            .expect("module_namespaces mutex poisoned")
            .insert(path.to_path_buf(), module_namespace.to_string());
        Ok(names)
    }

    /// Compile every `*.blinc` file directly inside `path` (non-recursive).
    /// Names must be unique across the directory (shared global substrate registry).
    pub fn compile_directory(
        &self,
        path: &Path,
    ) -> BlincDslResult<std::collections::HashMap<std::path::PathBuf, Vec<String>>> {
        let mut out = std::collections::HashMap::new();
        let entries = std::fs::read_dir(path)?;
        // Sort by file name for deterministic compile order.
        let mut files: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("blinc"))
            .collect();
        files.sort();
        for file in files {
            let names = self.compile_file(&file)?;
            out.insert(file, names);
        }
        Ok(out)
    }

    /// Re-run compilation for a single file. Replaces the per-
    /// path entry in the compiled-modules map; the JIT-side
    /// symbol table picks up the new function pointers via
    /// beadie's atomic `swap_compiled` (FSM / signal / component
    /// registry state survives — see `blinc_runtime::reload`).
    ///
    /// Returns the file's freshly-emitted function names.
    pub fn recompile_file(&self, path: &Path) -> BlincDslResult<Vec<String>> {
        self.compile_file(path)
    }

    /// Compile an entry `.blinc` file with cross-module
    /// resolution rooted at `source_root`. ES6-style imports
    /// (`import { X } from "./widgets"`) in the entry source
    /// pull dependent files via a registered callback resolver;
    /// Zyntax's import chain parses each, flat-merges its
    /// declarations into the entry program, and JIT-compiles
    /// the whole thing as one unit.
    ///
    /// Imports are de-duplicated per process (thread-local in
    /// Zyntax's `process_imports_for_traits`), so the same
    /// module pulled by multiple files only compiles once.
    pub fn compile_project(&self, entry: &Path, source_root: &Path) -> BlincDslResult<Vec<String>> {
        let mut aggregated: Vec<String> = Vec::new();
        self.compile_project_inner(entry, source_root, &mut aggregated, true)?;
        Ok(aggregated)
    }

    /// Build a FRESH instance from the same sources, for a hot reload.
    ///
    /// Not a recompile of this instance. Compiling into a live runtime
    /// re-runs the changed module, but a call from the entry to
    /// `<Module>$view` keeps binding to the symbol registered the first
    /// time, so editing an imported module changed nothing on screen
    /// while the entry alone appeared to reload. A new runtime resolves
    /// every symbol against the new code.
    ///
    /// `setup` runs on the new instance before its sources compile, and
    /// is where a host re-registers whatever it registered originally
    /// (extern widgets, host functions). A fresh runtime knows none of
    /// it.
    ///
    /// State is not lost: signal values live in a process-global
    /// registry keyed by name, and a declared default applies only when
    /// a signal is first minted, so the new instance adopts the values
    /// the old one was showing.
    ///
    /// On failure the caller simply keeps using the instance it has --
    /// source is unparseable for most of the time it is being edited.
    pub fn reload_project<F>(entry: &Path, source_root: &Path, setup: F) -> BlincDslResult<BlincDsl>
    where
        F: FnOnce(&BlincDsl) -> BlincDslResult<()>,
    {
        let fresh = BlincDsl::new()?;
        setup(&fresh)?;
        fresh.compile_project(entry, source_root)?;
        Ok(fresh)
    }

    fn compile_project_inner(
        &self,
        entry: &Path,
        source_root: &Path,
        out: &mut Vec<String>,
        is_entry: bool,
    ) -> BlincDslResult<()> {
        // ES6 / Node-style resolution: tries `<root>/<segs>.blinc`
        // then `<root>/<segs>/index.blinc`. Drives the dotted
        // `["", "/widgets"]` shape Zyntax's auto-split produces
        // for `./widgets` AND plain `["widgets"]` for bare names.
        let arch = zyntax_typed_ast::import_resolver::ModuleArchitecture::NodeStyle {
            extensions: vec![".blinc".to_string()],
            index_name: "index".to_string(),
        };

        let source = std::fs::read_to_string(entry)?;
        // Parse only. The full pipeline would mint this file's signals
        // as a side effect, and it runs here with no namespace — so a
        // scan whose only job is reading the import list would claim the
        // bare key first and shadow the qualified one the real compile
        // is about to mint.
        let program = self.parse_only(&source, entry.to_string_lossy().as_ref())?;

        for decl in &program.declarations {
            let zyntax_typed_ast::TypedDeclaration::Import(import) = &decl.node else {
                continue;
            };
            let segments: Vec<String> = import
                .module_path
                .iter()
                .filter_map(|s| s.resolve_global().map(|s| s.to_string()))
                // Action language's `.`-split on `./widgets` → `["", "/widgets"]`.
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('/').to_string())
                .collect();
            if segments.is_empty() {
                continue;
            }
            let candidates = arch.module_to_paths(&segments, &source_root.to_path_buf());
            let Some(imported) = candidates.into_iter().find(|p| p.exists()) else {
                continue;
            };
            let already = self
                .compiled_modules
                .lock()
                .map(|m| m.contains_key(&imported))
                .unwrap_or(false);
            if already {
                continue;
            }
            self.compile_project_inner(&imported, source_root, out, false)?;
        }

        // The entry compiles unnamespaced. Namespaces exist to keep
        // imported modules' components from colliding, but the entry has
        // no importer to disambiguate it from -- and mangling it breaks
        // identity that the host and the source both address by name:
        // `dispatch_default("Play", ...)` and a bare `view { App() }`
        // calling a component declared in the same file both look up the
        // unmangled name.
        let namespace = if is_entry {
            String::new()
        } else {
            module_namespace_from_path(entry, source_root)
        };
        let names = self.compile_file_with_namespace(entry, &namespace)?;
        out.extend(names);
        Ok(())
    }

    /// JIT function names last emitted for `path`, or `None` if never compiled.
    pub fn compiled_function_names(&self, path: &Path) -> Option<Vec<String>> {
        self.compiled_modules
            .lock()
            .ok()
            .and_then(|m| m.get(path).cloned())
    }

    /// CSS strings from every `style { ... }` block, in compile order.
    pub fn compiled_stylesheets(&self) -> Vec<String> {
        self.compiled_stylesheets
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Non-fatal diagnostics accumulated across every compile call,
    /// in the order they were emitted. Uses Zyntax's structured
    /// `Diagnostic` shape (level / code / message / annotations /
    /// help / notes / suggestions) so callers can render them
    /// through any of Zyntax's existing diagnostic surfaces or a
    /// future unified renderer.
    pub fn compile_diagnostics(&self) -> Vec<zyntax_typed_ast::Diagnostic> {
        self.compile_diagnostics
            .lock()
            .map(|d| d.clone())
            .unwrap_or_default()
    }

    /// Drop every accumulated diagnostic. Useful for incremental
    /// recompile flows that want fresh diagnostics per cycle (the
    /// accumulator otherwise grows across `compile_*` calls so
    /// long-lived sessions can stream the history).
    pub fn clear_compile_diagnostics(&self) {
        if let Ok(mut d) = self.compile_diagnostics.lock() {
            d.clear();
        }
    }

    /// Append a single diagnostic + mirror it through `tracing` so
    /// it surfaces in user-facing logs even if the caller never
    /// reads [`Self::compile_diagnostics`]. Internal helper —
    /// individual compile passes call this with a Zyntax-shaped
    /// `Diagnostic` they construct directly.
    fn emit_compile_diagnostic(&self, diag: zyntax_typed_ast::Diagnostic) {
        use zyntax_typed_ast::DiagnosticLevel;
        let level = diag.level;
        let code = diag.code.map(|c| c.to_string()).unwrap_or_default();
        let message = diag.message.clone();
        match level {
            DiagnosticLevel::Ice | DiagnosticLevel::Error => {
                tracing::error!(code = %code, "{message}");
            }
            DiagnosticLevel::Warning => {
                tracing::warn!(code = %code, "{message}");
            }
            DiagnosticLevel::Note | DiagnosticLevel::Help => {
                tracing::info!(code = %code, "{message}");
            }
        }
        if let Ok(mut acc) = self.compile_diagnostics.lock() {
            acc.push(diag);
        }
    }

    /// Parse `.blinc` source to TypedAST without compiling. Runs the same
    /// post-parse pipeline as `compile_source` so AST tests see the lowered shape.
    /// Parse to a typed AST and run NO passes.
    ///
    /// For callers that want the shape of a file rather than a compiled
    /// program — the import scan being the one that matters, since the
    /// passes have registry side effects it must not cause.
    fn parse_only(&self, source: &str, filename: &str) -> BlincDslResult<TypedProgram> {
        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        self.grammar
            .parse_with_signatures(source, filename, runtime.plugin_signatures())
            .map_err(|e| BlincDslError::Compile(e.render_auto().unwrap_or_else(|| e.to_string())))
    }

    pub fn parse_to_typed_ast(&self, source: &str, filename: &str) -> BlincDslResult<TypedProgram> {
        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");

        let mut program = self
            .grammar
            .parse_with_signatures(source, filename, runtime.plugin_signatures())
            // A parse failure carries the source it came from, so it
            // renders as a snippet with the offending token underlined
            // rather than a line:column pair the reader has to go and
            // look up. Colour follows the terminal: on for a dev loop,
            // off when the output is piped, captured or read back into
            // a UI label.
            .map_err(|e| {
                BlincDslError::Compile(e.render_auto().unwrap_or_else(|| e.to_string()))
            })?;

        // No module: this entry point compiles a program on its own,
        // with no file position to resolve against. Set explicitly so a
        // previous compile on this thread cannot lend it one.
        let _scope = passes::enter_module("");

        // Post-parse passes — no-op on programs without the matching shapes.
        inject_fsm_context_markers(&mut program);
        synthesize_fsm_event_enums(&mut program);
        synthesize_fsm_trait_interfaces(&mut program);
        desugar_compound_assigns(&mut program);
        synthesize_fsm_context_and_actions(&mut program);
        lower_match_blocks(&mut program);
        expand_const_groups(&mut program);
        resolve_const_references(&mut program);
        resolve_dotted_fsm_field_access(&mut program);
        crate::passes::resolve_ref_calls(&mut program, filename);
        resolve_signal_calls(&mut program);
        let fsm_lookup_module = zyntax_typed_ast::InternedString::new_global("main");
        resolve_fsm_trigger_calls(&mut program, fsm_lookup_module);
        resolve_fsm_subscribe_calls(&mut program, fsm_lookup_module);
        let _ = detect_and_strip_stateful_views(&mut program);

        lower_struct_literals(&mut program)
            .map_err(|errors| BlincDslError::Compile(errors.join("\n")))?;

        validate_component_calls(&program)
            .map_err(|errors| BlincDslError::Compile(errors.join("\n")))?;

        // MUST run after validation — validator reads the marker shape.
        expand_map_calls(&mut program);

        lower_component_calls(&mut program, filename);

        bind_component_props(&mut program);
        inject_user_view_instance_id_params(&mut program);

        // Local set; `parse_to_typed_ast` doesn't touch the JIT renderer.
        let mut local_vrv = std::collections::HashSet::new();
        lower_view_to_value_returning(&mut program, &mut local_vrv);

        lower_children_arrays_to_blocks(&mut program, &local_vrv);
        annotate_computed_lambda_types(&mut program);
        lower_styling_args_to_overlays(&mut program);
        lower_struct_widget_props_to_handles(&mut program)
            .map_err(|errors| BlincDslError::Compile(errors.join("\n")))?;
        resolve_extern_widget_named_args(&mut program);
        lower_reactive_args(&mut program);
        lower_bare_call_named_args(&mut program);

        Ok(program)
    }

    /// Invoke bare-form `render_view` and drain the scene buffer.
    /// For `view { ... }` programs (no enclosing `component`).
    pub fn render_view(&self) -> BlincDslResult<Vec<DslOp>> {
        self.render_named("render_view")
    }

    /// Invoke `<Name>$view` (inherent-impl mangling) and drain the scene buffer.
    pub fn render_component(&self, name: &str) -> BlincDslResult<Vec<DslOp>> {
        let symbol = format!("{name}$view");
        self.render_named(&symbol)
    }

    fn render_named(&self, fn_name: &str) -> BlincDslResult<Vec<DslOp>> {
        let is_value_returning = self
            .value_returning_views
            .lock()
            .map(|set| set.contains(fn_name))
            .unwrap_or(false);

        // User-component views (`<X>$view`) now take a leading
        // `__instance_id__: u64` synthetic param. When called from
        // the host (render_component, JitViewRenderer) outside any
        // DSL call-site lowering pass, we pass `0` as the synthetic
        // id — that's the empty-stack sentinel and is fine for
        // top-level ad-hoc rendering. Substrate-style internal symbols
        // (top-level `render_view`, etc.) still use the zero-arg ABI.
        let user_view_takes_instance_id = fn_name != "render_view"
            && !fn_name.starts_with("$Blinc$")
            && fn_name.ends_with("$view");

        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");

        if is_value_returning {
            // Handle discarded — substrate `ViewRenderer` flows it through to consumers.
            // Direct JIT dispatch — see [`JitGuardDispatcher::call_guard`].
            let ptr = runtime.get_function_ptr(fn_name).ok_or_else(|| {
                BlincDslError::Compile(format!("view symbol '{fn_name}' not registered in runtime"))
            })?;
            if user_view_takes_instance_id {
                let view: extern "C" fn(u64) -> i64 = unsafe { std::mem::transmute(ptr) };
                let _ = view(0);
            } else {
                let view: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
                let _ = view();
            }
        } else {
            runtime.call::<()>(fn_name, &[])?;
        }
        Ok(take_scene_ops())
    }

    /// Backend-agnostic view renderer backed by this `BlincDsl`'s Cranelift runtime.
    ///
    /// Also installs the renderer process-wide. A `with` region
    /// re-renders from inside a layout rebuild, with no host in scope to
    /// hand it one.
    pub fn view_renderer(&self) -> std::sync::Arc<dyn blinc_runtime::view::ViewRenderer> {
        let renderer: std::sync::Arc<dyn blinc_runtime::view::ViewRenderer> =
            std::sync::Arc::new(JitViewRenderer {
                runtime: self.runtime.clone(),
                value_returning_views: self.value_returning_views.clone(),
            });
        blinc_runtime::view::set_global_renderer(renderer.clone());
        renderer
    }

    /// Resolve each lifted `with` region's declared dependencies to
    /// signal IDS and file them for [`crate::with_regions::mount`].
    ///
    /// The three cases mirror the whole-program `@stateful` path:
    /// explicit signals win; an `@fsm` list narrows to that FSM's own
    /// context fields the body reads as VALUES; a bare region falls back
    /// to every declared signal.
    ///
    /// `module` is the file being compiled, which is what makes the
    /// name → id step answerable: a region in `pages/navigation.blinc`
    /// that depends on `page` means THAT module's `page`.
    fn register_with_regions(&self, regions: &[passes::WithRegion], module: &str) {
        if regions.is_empty() {
            return;
        }
        let declared: Vec<String> = self
            .declared_signals()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let declared_fsms = self.declared_fsms();

        for region in regions {
            // The bare `with count, Play { … }` form names its
            // dependencies without saying what they are. Sort them here,
            // where both declaration lists exist, rather than guessing
            // from capitalisation at parse time.
            let mut signal_deps = region.signal_deps.clone();
            let mut fsms = region.fsms.clone();
            for name in &region.named_deps {
                if declared_fsms.contains(name) {
                    if !fsms.contains(name) {
                        fsms.push(name.clone());
                    }
                } else if declared.contains(name) {
                    if !signal_deps.contains(name) {
                        signal_deps.push(name.clone());
                    }
                } else {
                    tracing::warn!(
                        dep = %name,
                        region = %region.name,
                        "`with` names a dependency that is neither a declared \
                         signal nor a declared FSM — the region will not \
                         re-render for it"
                    );
                }
            }

            let signal_names: Vec<String> = if !signal_deps.is_empty() {
                signal_deps
                    .iter()
                    .filter(|name| declared.contains(name))
                    .cloned()
                    .collect()
            } else if !fsms.is_empty() {
                // A self transition leaves the state value unchanged, so
                // the context writes are what actually signal a change.
                // Only the fields read as values: a field passed as a
                // binding handle updates through the property channel,
                // and subscribing to it would buy a full re-render for a
                // frame that was already correct.
                region
                    .ctx_value_reads
                    .iter()
                    .filter_map(|dotted| {
                        let (fsm, field) = dotted.split_once('.')?;
                        fsms.iter()
                            .any(|f| f == fsm)
                            .then(|| crate::fsm_registry::mangle_ctx_signal(fsm, field))
                    })
                    .filter(|name| declared.contains(name))
                    .collect()
            } else {
                declared.clone()
            };

            // Names become ids HERE, where `namespace` says which
            // module they belong to. A name that does not resolve is
            // dropped rather than carried: mount could only re-ask the
            // same question with less information.
            let signal_ids: Vec<u64> = signal_names
                .iter()
                .filter_map(|name| {
                    let key = blinc_runtime::signal::qualify(module, name);
                    blinc_runtime::signal::lookup_exact(&key)
                        .or_else(|| blinc_runtime::signal::lookup(name))
                        .map(|(raw, _ty)| raw)
                })
                .collect();

            with_regions::register(
                region.id,
                with_regions::MountedRegion {
                    name: region.name.clone(),
                    signal_ids,
                    // First listed wins: a Stateful exposes one shared
                    // state. No FSM named means no shared state to bind.
                    fsm: fsms.first().cloned(),
                },
            );
        }
    }

    /// Every declared `signal <name>: <T>` across all compiled sources.
    pub fn declared_signals(&self) -> Vec<(String, Type)> {
        self.declared_signals
            .lock()
            .expect("declared_signals mutex poisoned")
            .clone()
    }

    /// Whether any compiled view carried `@stateful` / `@fsm`. True
    /// means [`Self::view_widget`] wraps the whole program in one
    /// `Stateful`; a `with` region does not set it.
    pub fn has_stateful_view(&self) -> bool {
        *self
            .has_stateful_view
            .lock()
            .expect("has_stateful_view mutex poisoned")
    }

    /// View symbols promoted to return a widget handle. Includes the
    /// synthetic `__blinc_with_<n>$view` a `with` region lifts to.
    pub fn value_returning_views(&self) -> Vec<String> {
        self.value_returning_views
            .lock()
            .expect("value_returning_views mutex poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Components carrying `@stateful`, by name. These are the ones a
    /// scoped `Stateful` can be mounted for.
    pub fn stateful_components(&self) -> Vec<String> {
        self.stateful_components
            .lock()
            .expect("stateful_components mutex poisoned")
            .clone()
    }

    /// Every declared `fsm <Name> { ... }` across all compiled sources.
    pub fn declared_fsms(&self) -> Vec<String> {
        self.declared_fsms
            .lock()
            .expect("declared_fsms mutex poisoned")
            .clone()
    }

    /// Materialise the compiled `view { ... }` as a top-level `ElementBuilder`.
    ///
    /// Opt-in reactivity: returns a bare widget by default. If any view carries
    /// `@stateful`, the result is wrapped in a `Stateful<FsmStateId>` that
    /// re-renders on signal/FSM changes.
    ///
    /// ```text
    /// @stateful view {
    ///     Div { Text(f"Count: {count.get()}") }
    /// }
    /// ```
    pub fn view_widget(&self) -> Box<dyn blinc_layout::div::ElementBuilder> {
        use blinc_core::reactive::SignalId;

        // Every call re-runs the whole JIT program and mounts fresh
        // `Stateful`s. Their registry keys are `Arc::as_ptr`, so each
        // call registers under a NEW key while the previous entries
        // stay — eight per call on the playground, every one of which
        // still fires on every future signal write.
        //
        // A host that calls this once at startup pays nothing. One that
        // calls it per frame, or per resize event, compounds. Logged so
        // "the builder only runs once" is checkable rather than assumed.
        {
            static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            tracing::debug!(
                call = n,
                stateful_deps_registered = blinc_layout::stateful::stateful_deps_registered(),
                "view_widget: rebuilding the whole DSL tree"
            );
        }
        use blinc_runtime::fsm::FsmStateId;
        use zyntax_embed::ZyntaxValue;

        // Flush pending stylesheets into `BlincContextState` — `compile_source`
        // typically runs before the context is live. Cursor keeps it idempotent.
        if blinc_core::context_state::BlincContextState::is_initialized() {
            let sheets = self
                .compiled_stylesheets
                .lock()
                .expect("compiled_stylesheets mutex poisoned");
            let mut cursor = self
                .stylesheets_queued_up_to
                .lock()
                .expect("stylesheets_queued_up_to mutex poisoned");
            if *cursor < sheets.len() {
                let ctx = blinc_core::context_state::BlincContextState::get();
                for css in &sheets[*cursor..] {
                    ctx.queue_stylesheet(css.clone());
                }
                *cursor = sheets.len();
            }
        }

        let renderer = self.view_renderer();
        let stateful = *self
            .has_stateful_view
            .lock()
            .expect("has_stateful_view mutex poisoned");

        // No `@stateful` → render once, return bare tree.
        if !stateful {
            return materialize_view(&renderer);
        }

        let signals = self.declared_signals();
        let fsms = self.declared_fsms();

        // Empty explicit lists → bare `@stateful` / `@fsm`: use all declared / first.
        let explicit_deps = self
            .stateful_view_deps
            .lock()
            .expect("stateful_view_deps mutex poisoned")
            .clone();
        let explicit_fsms = self
            .stateful_view_fsms
            .lock()
            .expect("stateful_view_fsms mutex poisoned")
            .clone();

        // Skip signals whose type isn't bridged (only i32/f64/string today).
        // A previous render's observed set beats the blanket fallback:
        // it is what the program actually read, so anything else in
        // `signals` would only wake it for a write it ignores.
        let observed_now = self
            .observed_view_deps
            .lock()
            .expect("observed_view_deps mutex poisoned")
            .clone();

        let dep_pool: Vec<(String, Type)> = if explicit_deps.is_empty() {
            if explicit_fsms.is_empty() {
                match &observed_now {
                    Some(ids) if !ids.is_empty() => signals
                        .iter()
                        .filter(|(name, _)| {
                            blinc_runtime::signal::lookup(name)
                                .is_some_and(|(raw, _)| ids.contains(&raw))
                        })
                        .cloned()
                        .collect(),
                    _ => signals.clone(),
                }
            } else {
                // Narrow to the bound FSM's OWN context fields.
                //
                // The shared state alone is not enough: a self
                // transition (`Idle -> Idle`, the common shape for an
                // action that only mutates context) leaves the state
                // value unchanged, so a stateful bound to it never
                // fires. The context writes are what actually signal
                // the change, so they have to be in the deps.
                //
                // Subscribing to EVERY declared signal was the other
                // extreme: unrelated signals re-rendered this stateful,
                // and the FSM's own fields notified alongside the shared
                // state, so one transition rendered twice.
                //
                // Not every context field of the bound FSM, either --
                // only the ones the body reads as a VALUE. A field
                // passed to a prop as a binding handle updates through
                // the property channel, so subscribing to it buys a full
                // re-render for a frame that was already correct: one
                // `Grow` writing five bound fields re-rendered the whole
                // program five times.
                let value_reads = self
                    .stateful_ctx_value_reads
                    .lock()
                    .expect("stateful_ctx_value_reads mutex poisoned");
                let wanted: Vec<String> = value_reads
                    .iter()
                    .filter_map(|dotted| {
                        let (fsm, field) = dotted.split_once('.')?;
                        explicit_fsms
                            .iter()
                            .any(|f| f == fsm)
                            .then(|| crate::fsm_registry::mangle_ctx_signal(fsm, field))
                    })
                    .collect();
                signals
                    .iter()
                    .filter(|(n, _)| wanted.iter().any(|w| w == n))
                    .cloned()
                    .collect()
            }
        } else {
            explicit_deps
                .iter()
                .filter_map(|name| {
                    signals.iter().find_map(|(n, ty)| {
                        if n == name {
                            Some((n.clone(), ty.clone()))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };

        let mut signal_ids: Vec<SignalId> = Vec::new();
        for (name, _ty) in &dep_pool {
            // Look up the SignalId registered at DSL compile time. The
            // registry stores the raw `SignalId.to_raw()` — reconstruct
            // the strongly-typed handle's id for the Stateful dep list.
            if let Some((id_raw, _sig_ty)) = blinc_runtime::signal::lookup(name) {
                signal_ids.push(blinc_core::reactive::SignalId::from_raw(id_raw));
            }
        }

        let mut builder = blinc_layout::stateful::stateful::<FsmStateId>();
        let fsm_for_binding = if let Some(name) = explicit_fsms.first() {
            // First-listed FSM wins; substrate exposes a single `SharedState` per stateful.
            Some(name.as_str())
        } else {
            fsms.first().map(|s| s.as_str())
        };
        if let Some(fsm_name) = fsm_for_binding
            && let Some(shared) = blinc_runtime::fsm::default_state(fsm_name)
        {
            builder = builder.with_shared_state(shared);
        }
        crate::with_regions::__record_program_deps(&signal_ids);
        builder = builder.deps(signal_ids);

        let renderer_for_callback = renderer.clone();
        let observed_sink = Arc::clone(&self.observed_view_deps);
        Box::new(builder.on_state(move |_sctx| {
            tracing::debug!("stateful container: on_state re-render");
            // Observe this render so the NEXT build can subscribe to what
            // the program reads rather than to everything declared. The
            // id is reserved: a `with` region's ids come from the
            // lowering and never reach it.
            crate::read_scope::enter(PROGRAM_READ_SCOPE);
            let value =
                blinc_runtime::view::render_main(&renderer_for_callback).expect("render_main");
            if let Some(reads) = crate::read_scope::exit(PROGRAM_READ_SCOPE)
                && !reads.is_empty()
                && let Ok(mut sink) = observed_sink.lock()
            {
                *sink = Some(reads);
            }
            let ZyntaxValue::Int(handle) = value else {
                return blinc_layout::div::Div::new();
            };
            let inner = unsafe { materialize_widget(handle) }
                .map(|w| w.into_element_builder())
                .unwrap_or_else(|| Box::new(blinc_layout::div::Div::new()));
            crate::passes::view_root(inner)
        }))
    }

    /// Resolve a tick-driven transition. First-matching guard wins. Returns
    /// `None` when no guard fires.
    /// Construct a machine for `fsm`. The token is this instance's
    /// identity: two calls give two machines with separate state, which
    /// is what a name-keyed state cell cannot express.
    pub fn fsm_machine(&self, fsm: &str) -> BlincDslResult<zyntax_embed::FiberToken> {
        let mut runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        runtime
            .get_fiber(&passes::machine_fn_name(fsm))
            .map_err(BlincDslError::from)
    }

    /// Deliver `event_code` to `machine` and run it to its next
    /// suspension. `Some(state_code)` is where it settled; `None` means
    /// it finished or its code is gone, which is the caller's cue to
    /// drop and remount.
    pub fn step_fsm_machine(
        &self,
        fsm: &str,
        machine: zyntax_embed::FiberToken,
        event_code: u32,
    ) -> BlincDslResult<Option<u32>> {
        let mut runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        // Armed under the same lock that drives the step, so another
        // machine cannot arm over this one between the two.
        host::set_pending_fsm_event(event_code as i64);
        let step = runtime
            .resume_fiber_within(machine, &[&passes::host_events_handler_name(fsm)])
            .map_err(BlincDslError::from)?;
        Ok(match step {
            zyntax_embed::HostFiberStep::Yielded(zyntax_embed::ZyntaxValue::Int(code)) => {
                Some(code as u32)
            }
            _ => None,
        })
    }

    /// Whether the running program has a resolvable handler by this
    /// name. Diagnostic: a synthesized handler that never reaches the
    /// module is indistinguishable at the call site from one that does.
    pub fn effect_handler_present(&self, name: &str) -> bool {
        let mut runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        runtime.get_effect_handler(name).is_ok()
    }

    /// Run `f` with `fsm`'s handler installed, so a perform inside it
    /// resolves to that FSM's context rather than to nothing.
    ///
    /// The read path a component uses: the FSM owns its context, and
    /// asking for a field means asking the owner while its handler is
    /// in scope.
    pub fn with_fsm_context<R>(&self, fsm: &str, f: impl FnOnce() -> R) -> BlincDslResult<R> {
        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        let token = {
            let mut rt = runtime;
            let t = rt
                .get_effect_handler(&passes::host_events_handler_name(fsm))
                .map_err(BlincDslError::from)?;
            drop(rt);
            t
        };
        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        let frame = runtime
            .push_effect_handler(token)
            .map_err(BlincDslError::from)?;
        let out = f();
        runtime.pop_effect_handler(frame);
        Ok(out)
    }

    /// Free a machine. What a component does when it unmounts.
    pub fn drop_fsm_machine(&self, machine: zyntax_embed::FiberToken) -> BlincDslResult<()> {
        let mut runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");
        runtime.drop_fiber(machine).map_err(BlincDslError::from)
    }

    pub fn step_tick(
        &self,
        id: &FsmId,
        current: &str,
    ) -> BlincDslResult<Option<zyntax_typed_ast::InternedString>> {
        // Snapshot candidates and drop the registry lock before taking the runtime lock.
        let candidates: Vec<(
            zyntax_typed_ast::InternedString,
            zyntax_typed_ast::InternedString,
        )> = with_fsm_registry(|r| {
            r.get(id)
                .map(|def| {
                    def.tick_guards
                        .iter()
                        .filter(|g| g.from.resolve_global().as_deref() == Some(current))
                        .filter_map(|g| g.guard_fn.map(|fn_name| (fn_name, g.to)))
                        .collect()
                })
                .unwrap_or_default()
        });

        if candidates.is_empty() {
            return Ok(None);
        }

        let runtime = self
            .runtime
            .lock()
            .expect("BlincDsl runtime mutex poisoned");

        for (guard_fn, to) in candidates {
            let Some(name) = guard_fn.resolve_global() else {
                continue;
            };
            // Direct JIT dispatch — see [`JitGuardDispatcher::call_guard`]
            // for why we transmute the pointer rather than going through
            // `call_function` or `call_raw`.
            let Some(ptr) = runtime.get_function_ptr(&name) else {
                continue;
            };
            let guard: extern "C" fn() -> i32 = unsafe { std::mem::transmute(ptr) };
            if guard() != 0 {
                return Ok(Some(to));
            }
        }

        Ok(None)
    }

    /// Set an i32-typed signal by its DSL-declared name.
    ///
    /// Look up the `SignalId` in [`blinc_runtime::signal`]
    /// (auto-minting if absent — supports
    /// hosts that seed initial values BEFORE compiling DSL source),
    /// then call `blinc_core::reactive::Signal::<i32>::from_id(id).set(value)`.
    /// That fires the property-binding registry the same way native
    /// Rust `.set()` does, so any `Div::bg(&signal_handle)` repaints.
    pub fn set_signal_i32(&self, name: &str, value: i32) {
        let id_raw =
            blinc_runtime::signal::mint_or_get(name, blinc_runtime::signal::SignalType::I32);
        blinc_core::reactive::Signal::<i32>::from_id(blinc_core::reactive::SignalId::from_raw(
            id_raw,
        ))
        .set(value);
    }

    /// Read an i32-typed signal. `None` if undeclared, the id no longer
    /// resolves, or the wrong type was declared.
    pub fn get_signal_i32(&self, name: &str) -> Option<i32> {
        let (id_raw, blinc_runtime::signal::SignalType::I32) = blinc_runtime::signal::lookup(name)?
        else {
            return None;
        };
        blinc_core::reactive::Signal::<i32>::from_id(blinc_core::reactive::SignalId::from_raw(
            id_raw,
        ))
        .try_get()
    }

    /// Set an f64-typed signal. Auto-mints on first call.
    pub fn set_signal_f64(&self, name: &str, value: f64) {
        let id_raw =
            blinc_runtime::signal::mint_or_get(name, blinc_runtime::signal::SignalType::F64);
        blinc_core::reactive::Signal::<f64>::from_id(blinc_core::reactive::SignalId::from_raw(
            id_raw,
        ))
        .set(value);
    }

    /// Read an f64-typed signal. `None` if undeclared or wrong type.
    pub fn get_signal_f64(&self, name: &str) -> Option<f64> {
        let (id_raw, blinc_runtime::signal::SignalType::F64) = blinc_runtime::signal::lookup(name)?
        else {
            return None;
        };
        blinc_core::reactive::Signal::<f64>::from_id(blinc_core::reactive::SignalId::from_raw(
            id_raw,
        ))
        .try_get()
    }

    /// Set a string-typed signal. Auto-mints on first call.
    pub fn set_signal_string(&self, name: &str, value: impl Into<String>) {
        let id_raw =
            blinc_runtime::signal::mint_or_get(name, blinc_runtime::signal::SignalType::String);
        blinc_core::reactive::Signal::<String>::from_id(blinc_core::reactive::SignalId::from_raw(
            id_raw,
        ))
        .set(value.into());
    }

    /// Read a string-typed signal. `None` if undeclared or wrong type.
    pub fn get_signal_string(&self, name: &str) -> Option<String> {
        let (id_raw, blinc_runtime::signal::SignalType::String) =
            blinc_runtime::signal::lookup(name)?
        else {
            return None;
        };
        blinc_core::reactive::Signal::<String>::from_id(blinc_core::reactive::SignalId::from_raw(
            id_raw,
        ))
        .try_get()
    }

    /// Set a bool-typed signal. Auto-mints on first call.
    ///
    /// Widgets that own a toggle (`cn.Switch`, `cn.Checkbox`) write
    /// through the same signal, so this is both how a host drives them
    /// and how it observes what the user did.
    pub fn set_signal_bool(&self, name: &str, value: bool) {
        blinc_runtime::signal::set_bool(name, value);
    }

    /// Read a bool-typed signal. `None` if undeclared or wrong type.
    pub fn get_signal_bool(&self, name: &str) -> Option<bool> {
        blinc_runtime::signal::get_bool(name)
    }
}

#[cfg(test)]
mod tests;
