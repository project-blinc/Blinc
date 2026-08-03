use super::*;
use crate::host::{
    blinc_dsl_computed_bool, blinc_dsl_computed_f64, blinc_dsl_computed_i32,
    blinc_dsl_computed_string, blinc_dsl_effect, blinc_dsl_with_region, blinc_format_int,
    blinc_fsm_runtime_trigger, blinc_fsm_subscribe, blinc_fsm_subscribe_all, blinc_input_blur,
    blinc_input_clear, blinc_input_focus, blinc_input_select_all, blinc_ref_blur, blinc_ref_focus,
    blinc_ref_scroll_into_view, blinc_scope_enter, blinc_scroll_by, blinc_scroll_to_bottom,
    blinc_scroll_to_top, blinc_signal_get_by_id_bool, blinc_signal_get_by_id_f64,
    blinc_signal_get_by_id_i32, blinc_signal_get_by_id_i64, blinc_signal_get_by_id_string,
    blinc_signal_set_by_id_bool, blinc_signal_set_by_id_f64, blinc_signal_set_by_id_i32,
    blinc_signal_set_by_id_i64, blinc_signal_set_by_id_string, blinc_string_concat, blinc_text,
    blinc_text_int, blinc_textarea_blur, blinc_textarea_clear, blinc_textarea_focus,
    blinc_textarea_select_all,
};
use crate::widget_ffi::{
    blinc_b_view, blinc_blockquote_view, blinc_button_view, blinc_canvas_view, blinc_caption_view,
    blinc_cell_view, blinc_checkbox_view, blinc_code_view, blinc_div_view, blinc_h1_view,
    blinc_h2_view, blinc_h3_view, blinc_h4_view, blinc_h5_view, blinc_h6_view, blinc_hr_view,
    blinc_image_view, blinc_inline_code_view, blinc_label_view, blinc_li_view, blinc_link_view,
    blinc_motion_view, blinc_muted_view, blinc_new_child_list, blinc_new_struct_value,
    blinc_new_style_overlay, blinc_notch_view, blinc_ol_view, blinc_p_view, blinc_pre_view,
    blinc_push_child, blinc_set_overlay_bg, blinc_set_overlay_bg_computed,
    blinc_set_overlay_bg_signal, blinc_set_overlay_border_color,
    blinc_set_overlay_border_color_computed, blinc_set_overlay_border_color_signal,
    blinc_set_overlay_border_width, blinc_set_overlay_border_width_computed,
    blinc_set_overlay_border_width_signal, blinc_set_overlay_corner_radius,
    blinc_set_overlay_corner_radius_computed, blinc_set_overlay_corner_radius_signal,
    blinc_set_overlay_opacity, blinc_set_overlay_opacity_computed,
    blinc_set_overlay_opacity_signal, blinc_set_struct_bool, blinc_set_struct_f64,
    blinc_set_struct_handle, blinc_set_struct_i32, blinc_set_struct_i64, blinc_set_struct_string,
    blinc_small_view, blinc_span_view, blinc_stack_view, blinc_strong_view, blinc_svg_view,
    blinc_table_view, blinc_task_item_view, blinc_tbody_view, blinc_td_view, blinc_text_area_view,
    blinc_text_input_view, blinc_text_view, blinc_tfoot_view, blinc_th_view, blinc_thead_view,
    blinc_tr_view, blinc_ul_view,
};

/// Pairs a DSL-visible symbol name with an `extern "C"` fn pointer and signature.
/// Used for runtime registration AND type-system injection (spliced as an extern
/// fn decl into each parsed `TypedProgram` before `compile_typed_program`).
struct BuiltinDescriptor {
    /// Mangled symbol the grammar lowers to (no `@builtin` alias indirection).
    name: &'static str,
    param_types: &'static [Type],
    return_type: Type,
    /// `extern "C"` fn cast to `*const u8` for `register_function`.
    ptr: *const u8,
}

// SAFETY: Only fn pointers and `'static` references inside.
unsafe impl Sync for BuiltinDescriptor {}

/// All host builtins. Ordering irrelevant — registration walks the full table.
fn builtins() -> Vec<BuiltinDescriptor> {
    vec![
        BuiltinDescriptor {
            name: "$Blinc$text",
            param_types: &[Type::Primitive(PrimitiveType::String)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_text as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$text_int",
            param_types: &[Type::Primitive(PrimitiveType::I32)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_text_int as *const u8,
        },
        BuiltinDescriptor {
            // Id-keyed signal externs. The DSL lowering pass
            // bakes the `SignalId.to_raw()` as an i64 literal at compile
            // time; these externs reconstruct `Signal::<T>::from_id` and
            // delegate to `blinc_core::reactive::Signal::get`/`set`.
            // No name lookup at runtime, no parallel storage facade —
            // the DSL signal IS the reactive primitive.
            name: "__signal_get_by_id_i32",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I32),
            ptr: blinc_signal_get_by_id_i32 as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_get_by_id_i64",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_signal_get_by_id_i64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_get_by_id_f64",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::F64),
            ptr: blinc_signal_get_by_id_f64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_get_by_id_string",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::String),
            ptr: blinc_signal_get_by_id_string as *const u8,
        },
        BuiltinDescriptor {
            name: "__scroll_to_top_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_scroll_to_top as *const u8,
        },
        BuiltinDescriptor {
            name: "__scroll_to_bottom_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_scroll_to_bottom as *const u8,
        },
        BuiltinDescriptor {
            name: "__scroll_by_id",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::F64),
                Type::Primitive(PrimitiveType::F64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_scroll_by as *const u8,
        },
        BuiltinDescriptor {
            name: "__ref_focus_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_ref_focus as *const u8,
        },
        BuiltinDescriptor {
            name: "__ref_blur_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_ref_blur as *const u8,
        },
        BuiltinDescriptor {
            name: "__ref_scroll_into_view_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_ref_scroll_into_view as *const u8,
        },
        BuiltinDescriptor {
            name: "__input_focus_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_input_focus as *const u8,
        },
        BuiltinDescriptor {
            name: "__input_blur_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_input_blur as *const u8,
        },
        BuiltinDescriptor {
            name: "__input_clear_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_input_clear as *const u8,
        },
        BuiltinDescriptor {
            name: "__input_select_all_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_input_select_all as *const u8,
        },
        BuiltinDescriptor {
            name: "__textarea_focus_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_textarea_focus as *const u8,
        },
        BuiltinDescriptor {
            name: "__textarea_blur_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_textarea_blur as *const u8,
        },
        BuiltinDescriptor {
            name: "__textarea_clear_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_textarea_clear as *const u8,
        },
        BuiltinDescriptor {
            name: "__textarea_select_all_by_id",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_textarea_select_all as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_set_by_id_i32",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I32),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_signal_set_by_id_i32 as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_set_by_id_i64",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_signal_set_by_id_i64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_set_by_id_f64",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::F64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_signal_set_by_id_f64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_set_by_id_string",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_signal_set_by_id_string as *const u8,
        },
        BuiltinDescriptor {
            // bool getter / setter — wire type is i32 because
            // Zyntax's Cranelift backend lowers DSL `bool` to `i32`
            // across the FFI. The host extern coerces 0/1 ↔ false/true.
            name: "__signal_get_by_id_bool",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Bool),
            ptr: blinc_signal_get_by_id_bool as *const u8,
        },
        BuiltinDescriptor {
            name: "__signal_set_by_id_bool",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::Bool),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_signal_set_by_id_bool as *const u8,
        },
        BuiltinDescriptor {
            // `<FsmName>.trigger("State.Event")` lowered by `resolve_fsm_trigger_calls`.
            name: "__fsm_runtime_trigger__",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_fsm_runtime_trigger as *const u8,
        },
        BuiltinDescriptor {
            // `<FsmName>.subscribe("From.Event", closure)` — third arg is the
            // closure's raw fn ptr smuggled as i64.
            name: "__fsm_subscribe__",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_fsm_subscribe as *const u8,
        },
        BuiltinDescriptor {
            // `<FsmName>.subscribe(|state| { … })` — single-arg
            // form, all-paths subscriber. The closure is a one-arg
            // lambda whose body receives the matched
            // `"From.Event"` path as a ZRTL-string ptr; the host
            // extern wraps the i64 closure ptr in a Rust callback
            // that allocates a ZRTL string for the path each
            // dispatch and frees it after the closure returns.
            name: "__fsm_subscribe_all__",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_fsm_subscribe_all as *const u8,
        },
        BuiltinDescriptor {
            // `effect { ... }` block — the DSL grammar's `effect_stmt`
            // wraps the body in a zero-arg lambda; Zyntax lowers that
            // lambda to an `extern "C" fn()` ptr that arrives here as
            // an i64. We hand it to `blinc_core::reactive::effect(...)`
            // — auto-tracking falls out because the closure body reads
            // signals via the same path the native Rust effect uses.
            name: "__blinc_effect__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_dsl_effect as *const u8,
        },
        BuiltinDescriptor {
            // Append to a list signal. A `set([...])` expands into a
            // clear plus one of these per element, so a list never
            // crosses the FFI.
            name: "__blinc_list_push__",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: crate::host::blinc_dsl_list_push as *const u8,
        },
        BuiltinDescriptor {
            // Empty a list signal.
            name: "__blinc_list_clear__",
            param_types: &[Type::Primitive(PrimitiveType::String)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: crate::host::blinc_dsl_list_clear as *const u8,
        },
        BuiltinDescriptor {
            // `items.map(|x| Row(x))` over a list signal. Takes the
            // child list, the signal id, and the lambda as a fn ptr;
            // the host walks the list because a `Vec<String>` has no
            // JIT-side representation.
            name: "__blinc_map_children__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: crate::host::blinc_dsl_map_children as *const u8,
        },
        BuiltinDescriptor {
            // Opens the read scope a `with` region renders under. Sits
            // in argument position so it runs before the region's view.
            name: "__blinc_scope_enter__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_scope_enter as *const u8,
        },
        BuiltinDescriptor {
            // `with @fsm([…]) { … }` — `(region_id, child_handle)` in,
            // the mounted `Stateful`'s handle out. The child is the
            // region's view already rendered: argument order runs it
            // before this call, so nothing re-enters the JIT here.
            name: "__blinc_with__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_dsl_with_region as *const u8,
        },
        BuiltinDescriptor {
            // `computed { expr } : i32` — closure ptr in, DerivedId.to_raw()
            // out (cast to i64 over the wire because Cranelift's
            // value-map population doesn't handle `HirConstant::U64`).
            name: "__blinc_computed_i32__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_dsl_computed_i32 as *const u8,
        },
        BuiltinDescriptor {
            name: "__blinc_computed_f64__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_dsl_computed_f64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__blinc_computed_string__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_dsl_computed_string as *const u8,
        },
        BuiltinDescriptor {
            name: "__blinc_computed_bool__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_dsl_computed_bool as *const u8,
        },
        BuiltinDescriptor {
            // Push a stable instance-ID derived from the call site's
            // (filename, byte_offset) — emitted by `lower_component_calls`
            // around every rewritten component / widget call. Widget FFI
            // reads the top of this stack to key per-instance state.
            name: "__push_call_id__",
            param_types: &[Type::Primitive(PrimitiveType::U64)],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: crate::widget_ffi::blinc_dsl_push_call_id as *const u8,
        },
        BuiltinDescriptor {
            // Pop the most-recent call-ID. Paired with `__push_call_id__`.
            name: "__pop_call_id__",
            param_types: &[],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: crate::widget_ffi::blinc_dsl_pop_call_id as *const u8,
        },
        BuiltinDescriptor {
            // Pop + pass-through. `lower_component_calls` wraps a
            // rewritten call as `__pop_call_id_and_return__(Counter$view(...))`
            // so the call evaluates (widget FFI seeing the pushed id during
            // arg eval), then the pop runs, then the i64 widget handle
            // bubbles up as the bracket-expression's value.
            name: "__pop_call_id_and_return__",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: crate::widget_ffi::blinc_dsl_pop_call_id_and_return as *const u8,
        },
        BuiltinDescriptor {
            // Read the current top of the call-ID stack. Returns 0 when
            // outside any component-call bracket. Used by tests + DSL
            // bodies that want a stable identity to thread to runtime
            // APIs.
            name: "__current_call_id__",
            param_types: &[],
            return_type: Type::Primitive(PrimitiveType::U64),
            ptr: crate::widget_ffi::blinc_dsl_current_call_id as *const u8,
        },
        BuiltinDescriptor {
            // `__fstring_format__` (i32 only — f64 needs a separate `__fstring_format_f64__`).
            name: "$Blinc$format_int",
            param_types: &[Type::Primitive(PrimitiveType::I32)],
            return_type: Type::Primitive(PrimitiveType::String),
            ptr: blinc_format_int as *const u8,
        },
        BuiltinDescriptor {
            // `string_concat` — chains f-string parts.
            name: "$Blinc$string_concat",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::String),
            ptr: blinc_string_concat as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$H1$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_h1_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$H2$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_h2_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$H3$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_h3_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$H4$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_h4_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$H5$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_h5_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$H6$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_h6_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$P$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_p_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Span$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_span_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Small$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_small_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Label$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_label_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Muted$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_muted_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Strong$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_strong_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$B$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_b_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Caption$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_caption_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$InlineCode$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_inline_code_view as *const u8,
        },
        BuiltinDescriptor {
            // `Text("hi")` → leaked `WidgetBox::Text(...)` as i64.
            //
            // Trailing i32s are italic, bold, strikethrough, underline.
            // This list, the `ComponentDefinition` props in
            // `runtime_bridge`, and `blinc_text_view`'s signature are one
            // ABI: change one without the others and every `Text` in the
            // program calls through a mismatched signature.
            name: "$Blinc$Text$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I32),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_text_view as *const u8,
        },
        BuiltinDescriptor {
            // `Div(children, style, class, on_click, overflow_scroll, ref)`.
            // `class` = whitespace-sep names, `on_click` = raw fn ptr as
            // i64 (0 = none), `ref` = a ref handle's id (0 = none).
            //
            // This list, the `ComponentDefinition` props in
            // `runtime_bridge`, and `blinc_div_view`'s signature are one
            // ABI: change one without the others and every `Div` in the
            // program calls through a mismatched signature.
            name: "$Blinc$Div$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_div_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Stack$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_stack_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Image$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_image_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Svg$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_svg_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Canvas$view",
            param_types: &[Type::Primitive(PrimitiveType::I64)],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_canvas_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Motion$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_motion_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Notch$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_notch_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Hr$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_hr_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Blockquote$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_blockquote_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Link$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_link_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Ul$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_ul_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Ol$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_ol_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Li$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_li_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$TaskItem$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_task_item_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Table$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_table_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Thead$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_thead_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Tbody$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_tbody_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Tfoot$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_tfoot_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Tr$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_tr_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Th$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_th_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Td$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_td_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Cell$view",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_cell_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Button$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_button_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Checkbox$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_checkbox_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$TextInput$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_text_input_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$TextArea$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_text_area_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Code$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I32),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_code_view as *const u8,
        },
        BuiltinDescriptor {
            name: "$Blinc$Pre$view",
            param_types: &[
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_pre_view as *const u8,
        },
        BuiltinDescriptor {
            // `__new_child_list__()` — mint `Vec<WidgetHandle>`, populated by `__push_child__`.
            name: "__new_child_list__",
            param_types: &[],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_new_child_list as *const u8,
        },
        BuiltinDescriptor {
            // `__push_child__(list, child)` — append. List pointer stays live for container.
            name: "__push_child__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_push_child as *const u8,
        },
        // Struct-value builders — complex widget props cross the extern ABI as i64 handles.
        BuiltinDescriptor {
            name: "__new_struct_value__",
            param_types: &[],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_new_struct_value as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_struct_i32__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I32),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_struct_i32 as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_struct_bool__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I32),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_struct_bool as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_struct_i64__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_struct_i64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_struct_f64__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::F64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_struct_f64 as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_struct_string__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::String),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_struct_string as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_struct_handle__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::String),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_struct_handle as *const u8,
        },
        // Style-overlay builders — mirror the child-list pattern.
        BuiltinDescriptor {
            name: "__new_style_overlay__",
            param_types: &[],
            return_type: Type::Primitive(PrimitiveType::I64),
            ptr: blinc_new_style_overlay as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_bg__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_bg as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_opacity__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::F64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_opacity as *const u8,
        },
        BuiltinDescriptor {
            // Signal-bound variant of `__set_overlay_opacity__` — second
            // arg is a `SignalId.to_raw()` cast to i64. The lowering
            // pass `lower_styling_args_to_overlays` emits this extern
            // when the `opacity = …` named-arg's value is a Variable
            // referring to a DSL-declared f64 signal.
            name: "__set_overlay_opacity_signal__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_opacity_signal as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_corner_radius__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::F64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_corner_radius as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_border_width__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::F64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_border_width as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_border_color__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_border_color as *const u8,
        },
        // Signal-bound variants — emitted by `lower_styling_args_to_overlays`
        // when the corresponding styling arg's value is a Variable
        // referring to a DSL-declared signal of the matching type
        // (f64 for Float props, i32 for IntColor props). Second arg is
        // a `SignalId.to_raw()` cast to i64.
        BuiltinDescriptor {
            name: "__set_overlay_bg_signal__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_bg_signal as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_corner_radius_signal__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_corner_radius_signal as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_border_width_signal__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_border_width_signal as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_border_color_signal__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_border_color_signal as *const u8,
        },
        // Computed-binding variants — emitted by
        // `lower_styling_args_to_overlays` when the styling arg's
        // value is a `__blinc_computed_<T>__(closure)` call (the
        // desugaring of `computed { … } : T`). Second arg is a
        // `DerivedId.to_raw()` cast to i64; `apply_to` rehydrates a
        // `Computed<T>` and reads the live value each paint.
        BuiltinDescriptor {
            name: "__set_overlay_opacity_computed__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_opacity_computed as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_bg_computed__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_bg_computed as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_corner_radius_computed__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_corner_radius_computed as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_border_width_computed__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_border_width_computed as *const u8,
        },
        BuiltinDescriptor {
            name: "__set_overlay_border_color_computed__",
            param_types: &[
                Type::Primitive(PrimitiveType::I64),
                Type::Primitive(PrimitiveType::I64),
            ],
            return_type: Type::Primitive(PrimitiveType::Unit),
            ptr: blinc_set_overlay_border_color_computed as *const u8,
        },
    ]
}

/// Map a typed-AST `Type` to the wire-format `TypeTag` for `ZrtlSymbolSig`.
pub(crate) fn type_to_tag(ty: &Type) -> TypeTag {
    match ty {
        Type::Primitive(PrimitiveType::Unit) => TypeTag::VOID,
        Type::Primitive(PrimitiveType::Bool) => TypeTag::BOOL,
        Type::Primitive(PrimitiveType::String) => TypeTag::STRING,
        Type::Primitive(PrimitiveType::I32) => TypeTag::I32,
        Type::Primitive(PrimitiveType::I64) => TypeTag::I64,
        Type::Primitive(PrimitiveType::U64) => TypeTag::U64,
        Type::Primitive(PrimitiveType::F64) => TypeTag::F64,
        // Panic loudly — silent VOID would break codegen.
        _ => panic!(
            "blinc_dsl_core: no TypeTag mapping for {ty:?} \
             — extend `type_to_tag` when adding new builtin types"
        ),
    }
}

/// Map a typed-AST `Type` to the runtime `NativeType` for `call_function`.
/// Strings cross the FFI as `NativeType::Ptr` (length-prefixed buffer).
pub(crate) fn type_to_native(ty: &Type) -> Result<NativeType, &Type> {
    match ty {
        Type::Primitive(PrimitiveType::Unit) => Ok(NativeType::Void),
        Type::Primitive(PrimitiveType::Bool) => Ok(NativeType::Bool),
        Type::Primitive(PrimitiveType::String) => Ok(NativeType::Ptr),
        Type::Primitive(PrimitiveType::I32) => Ok(NativeType::I32),
        Type::Primitive(PrimitiveType::I64) => Ok(NativeType::I64),
        // u64 has the same calling-convention width as i64 — Cranelift
        // doesn't distinguish signedness for register passing.
        Type::Primitive(PrimitiveType::U64) => Ok(NativeType::I64),
        Type::Primitive(PrimitiveType::F64) => Ok(NativeType::F64),
        other => Err(other),
    }
}

/// Process-global set of widget-view symbols built into this crate
/// (substrate primitives). Populated by [`register_builtins`] at
/// `BlincDsl::new()` time; consulted both here ([`descriptor_to_sig`]
/// auto-inflates their signatures with a leading `U64`) and by
/// [`crate::passes::inject_call_site_keys`] (which prepends the matching
/// `i64` literal arg at every call site).
///
/// External widgets registered via
/// [`crate::BlincDsl::register_extern_widget_spec`] are NOT added here —
/// they keep the caller-supplied signature exactly. The auto-injection
/// is a built-in convention, not a contract imposed on the user.
static SUBSTRATE_WIDGET_NAMES: std::sync::OnceLock<
    std::sync::RwLock<std::collections::HashSet<&'static str>>,
> = std::sync::OnceLock::new();

fn substrate_widget_names() -> &'static std::sync::RwLock<std::collections::HashSet<&'static str>> {
    SUBSTRATE_WIDGET_NAMES.get_or_init(|| std::sync::RwLock::new(std::collections::HashSet::new()))
}

/// Returns `true` if `name` is a built-in substrate widget view that
/// receives the auto-injected `u64` call-site key as a leading arg.
/// Internal helper — the public-facing query goes through
/// [`is_substrate_widget_view_public`] so callers outside this module
/// don't reach into the OnceLock directly.
fn is_substrate_widget_view(name: &str) -> bool {
    is_substrate_widget_view_public(name)
}

/// Same as [`is_substrate_widget_view`] but pub(crate) so the lowering
/// pass can consult the set without touching the OnceLock.
pub(crate) fn is_substrate_widget_view_public(name: &str) -> bool {
    substrate_widget_names()
        .read()
        .map(|set| set.contains(name))
        .unwrap_or(false)
}

/// Build the ZRTL signature for a builtin (stored in `backend.symbol_signatures`).
fn descriptor_to_sig(b: &BuiltinDescriptor) -> ZrtlSymbolSig {
    // Widget views auto-receive a leading u64 call-site key — see
    // [`crate::passes::inject_call_site_keys`]. We inflate the signature
    // here so each Rust impl can be written with `_call_id: u64` as its
    // first param without touching ~30 inline abi.rs descriptors.
    let extra_lead = if is_substrate_widget_view(b.name) {
        1usize
    } else {
        0
    };
    let total_params = b.param_types.len() + extra_lead;

    assert!(
        total_params <= ZRTL_MAX_PARAMS,
        "{}: parameter count {} (incl. {} auto-injected widget-call-id) exceeds ZRTL_MAX_PARAMS ({})",
        b.name,
        total_params,
        extra_lead,
        ZRTL_MAX_PARAMS
    );

    let mut params = [TypeTag::VOID; ZRTL_MAX_PARAMS];
    if extra_lead > 0 {
        params[0] = TypeTag::U64;
    }
    for (i, ty) in b.param_types.iter().enumerate() {
        params[i + extra_lead] = type_to_tag(ty);
    }

    ZrtlSymbolSig {
        param_count: total_params as u8,
        flags: ZrtlSigFlags::NONE,
        return_type: type_to_tag(&b.return_type),
        params,
    }
}

/// Register all `$Blinc$*` builtins on the runtime with full signatures.
/// Also populates [`SUBSTRATE_WIDGET_NAMES`] with every widget-view
/// builtin so the lowering pass + sig inflation share a single source
/// of truth.
pub(crate) fn register_builtins(runtime: &mut ZyntaxRuntime) {
    // Populate the substrate-widget-name set FIRST so `descriptor_to_sig`
    // sees it when inflating each widget's signature with the leading
    // `U64` call-site key.
    {
        let mut set = substrate_widget_names().write().expect("RwLock poisoned");
        for b in builtins() {
            if b.name.starts_with("$Blinc$") && b.name.ends_with("$view") {
                set.insert(b.name);
            }
        }
    }
    for b in builtins() {
        let sig = descriptor_to_sig(&b);
        runtime.register_function_typed(b.name, b.ptr, sig);
    }
}
