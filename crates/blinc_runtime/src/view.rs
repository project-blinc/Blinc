//! Backend-agnostic view rendering.
//!
//! A [`ViewRenderer`] resolves a DSL-emitted view symbol (e.g.
//! `Counter$view` for an inherent-impl `view` method, or
//! `render_view` for a bare-form `view { ... }`) and runs it,
//! returning the typed value the view produced.
//!
//! ## Value-returning views
//!
//! View functions in the Blinc DSL **return a widget tree**.
//! The JIT-compiled (or AOT-linked) body of a `view { ... }`
//! constructs a [`ZyntaxValue`] carrying handles to real
//! `blinc_layout::Div` / `Text` / etc. instances built up via
//! registered widget primitives — `Div(props) { ..children }`
//! in source lowers to `$Blinc$Div(props, children)` extern
//! calls that return widget handles.
//!
//! This is a retained-mode shape (return-a-value, like
//! React / Vue / SwiftUI), not immediate-mode (push-an-op-stream).
//! It exists because both backends compile views through
//! Zyntax's value machinery — and Cranelift / LLVM can inline,
//! fold, DCE across the entire view body when the body is a
//! single value-returning expression. The earlier op-stream
//! design left those optimisations on the table.
//!
//! ## Ownership story
//!
//! Unlike [`super::fsm::GuardDispatcher`] (which lives in a
//! process-wide slot because FSM state names are global), view
//! renderers are typically held directly by the embed code that
//! constructed them — a top-level `App` struct, for example,
//! stores `Arc<dyn ViewRenderer>` alongside its widget tree.
//! There's no global slot here. Apps with two coexisting DSLs
//! (rare but legal) would hold two renderer Arcs without
//! contention.

use std::sync::Arc;

use zyntax_embed::ZyntaxValue;

/// Strategy for resolving a view symbol and running it.
///
/// Implementations:
///
/// - **`JitViewRenderer`** (in `blinc_dsl_core`) — wraps an
///   `Arc<Mutex<ZyntaxRuntime>>`, calls
///   `runtime.call::<ZyntaxValue>(symbol, &[])` so the Cranelift
///   JIT runs the compiled view body and hands back the value
///   it returned (a widget tree).
///
/// - **AOT renderer** (future) — wraps a static lookup table of
///   `extern "C" fn() -> ZyntaxValue` pointers produced at LLVM
///   link time, calls the pointer directly.
///
/// Both produce identical [`ZyntaxValue`] trees from identical
/// `.blinc` sources. Consumer code (the
/// `ZyntaxValue -> blinc_layout::Div` walker) only depends on
/// this trait — neither backend needs to be linked at consumer
/// compile time.
///
/// `Send + Sync` because renderers commonly live behind an
/// `Arc` shared across the UI thread + worker threads.
/// Backends whose underlying runtime is `!Send` (e.g. the JIT
/// path's `ZyntaxRuntime`) carry an `unsafe Send + Sync` impl
/// alongside a `Mutex` that serialises access — see the JIT
/// crate's `JitViewRenderer` for the safety argument.
pub trait ViewRenderer: Send + Sync + 'static {
    /// Run the view registered under `symbol` and return the
    /// widget tree value it produced.
    ///
    /// `symbol` is the JIT-linker-visible name:
    ///
    /// - `"render_view"` for the bare-form `view { ... }` at
    ///   the top of a `.blinc` file.
    /// - `"<ComponentName>$view"` for an inherent-impl `view`
    ///   method inside a `component` block. The
    ///   `<ComponentName>$view` mangling is what Zyntax's
    ///   `lower_impl_block` emits for inherent-impl methods.
    ///
    /// The convenience helpers [`render_main`] and
    /// [`render_component`] construct these names so callers
    /// don't have to remember the mangling.
    fn render_named(&self, symbol: &str) -> Result<ZyntaxValue, ViewRenderError>;
}

/// Errors a view-renderer might return. Kept simple — most
/// backend-specific failure detail folds into the `Backend`
/// variant's stringified payload.
#[derive(Debug, thiserror::Error)]
pub enum ViewRenderError {
    /// The symbol resolved to nothing the backend could call.
    /// Usually means the embedder's caller passed the wrong
    /// component name (typo, removed, never compiled).
    #[error("no view symbol `{0}` is registered")]
    NotFound(String),

    /// The backend ran the view but the call itself failed
    /// (panic in the JIT, link error in AOT, ABI mismatch,
    /// etc.). The string carries whatever diagnostic the
    /// backend produced.
    #[error("view-renderer backend error: {0}")]
    Backend(String),
}

/// Run the bare-form `view { ... }` at the top of the DSL
/// source. Equivalent to `renderer.render_named("render_view")`.
pub fn render_main(renderer: &Arc<dyn ViewRenderer>) -> Result<ZyntaxValue, ViewRenderError> {
    renderer.render_named("render_view")
}

/// Run the view method of a named component. Constructs the
/// `<ComponentName>$view` mangling internally; callers pass
/// the user-visible name (e.g. `"Counter"`).
pub fn render_component(
    renderer: &Arc<dyn ViewRenderer>,
    name: &str,
) -> Result<ZyntaxValue, ViewRenderError> {
    let symbol = format!("{name}$view");
    renderer.render_named(&symbol)
}

/// Process-wide renderer, for callers that have no `BlincDsl` in hand.
///
/// A `with` region's `Stateful` re-renders from inside a layout build,
/// long after the host that compiled the source has gone out of scope,
/// so it resolves the renderer here rather than capturing one. Same
/// trade the FSM `GuardDispatcher` makes: two coexisting DSL instances
/// would share this slot, and the last one installed wins.
static GLOBAL_RENDERER: std::sync::Mutex<Option<Arc<dyn ViewRenderer>>> =
    std::sync::Mutex::new(None);

/// Install the process-wide renderer, replacing any previous one.
pub fn set_global_renderer(renderer: Arc<dyn ViewRenderer>) {
    *GLOBAL_RENDERER.lock().unwrap_or_else(|e| e.into_inner()) = Some(renderer);
}

/// The process-wide renderer, if one was installed.
pub fn global_renderer() -> Option<Arc<dyn ViewRenderer>> {
    GLOBAL_RENDERER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// In-process test renderer that records every symbol
    /// called and emits a programmed `ZyntaxValue` per call.
    /// Mirrors the shape of a real backend without dragging in
    /// any actual compile / JIT machinery.
    struct ScriptedRenderer {
        calls: Mutex<Vec<String>>,
        results: Mutex<std::collections::HashMap<String, ZyntaxValue>>,
    }

    impl ScriptedRenderer {
        fn new(scripts: &[(&str, ZyntaxValue)]) -> Self {
            let mut map = std::collections::HashMap::new();
            for (sym, value) in scripts {
                map.insert((*sym).to_string(), value.clone());
            }
            Self {
                calls: Mutex::new(Vec::new()),
                results: Mutex::new(map),
            }
        }
    }

    impl ViewRenderer for ScriptedRenderer {
        fn render_named(&self, symbol: &str) -> Result<ZyntaxValue, ViewRenderError> {
            self.calls.lock().unwrap().push(symbol.to_string());
            self.results
                .lock()
                .unwrap()
                .get(symbol)
                .cloned()
                .ok_or_else(|| ViewRenderError::NotFound(symbol.to_string()))
        }
    }

    /// `render_main` resolves to the `"render_view"` symbol.
    /// Returns whatever ZyntaxValue the view produced — for
    /// this test, a simple `String` standing in for a widget
    /// handle.
    #[test]
    fn render_main_uses_render_view_symbol() {
        let renderer: Arc<dyn ViewRenderer> = Arc::new(ScriptedRenderer::new(&[(
            "render_view",
            ZyntaxValue::String("bare-view-result".into()),
        )]));
        let value = render_main(&renderer).unwrap();
        assert_eq!(value, ZyntaxValue::String("bare-view-result".into()));
    }

    /// `render_component(name)` mangles to `<name>$view`.
    #[test]
    fn render_component_mangles_view_symbol() {
        let renderer: Arc<dyn ViewRenderer> = Arc::new(ScriptedRenderer::new(&[(
            "Counter$view",
            ZyntaxValue::Int(42),
        )]));
        let value = render_component(&renderer, "Counter").unwrap();
        assert_eq!(value, ZyntaxValue::Int(42));
    }

    /// Unknown symbol surfaces as `ViewRenderError::NotFound`.
    #[test]
    fn unknown_symbol_returns_not_found() {
        let renderer: Arc<dyn ViewRenderer> = Arc::new(ScriptedRenderer::new(&[]));
        let err = render_component(&renderer, "Missing").unwrap_err();
        assert!(
            matches!(&err, ViewRenderError::NotFound(s) if s == "Missing$view"),
            "expected NotFound for `Missing$view`, got {err:?}"
        );
    }
}
