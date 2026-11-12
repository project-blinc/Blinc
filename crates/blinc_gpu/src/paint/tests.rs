use super::GpuPaintContext;
use blinc_core::{BlendMode, Color, DrawContext, LayerConfig, Rect, Size, Transform};

#[test]
fn test_gpu_paint_context_creation() {
    let ctx = GpuPaintContext::new(800.0, 600.0);
    assert_eq!(ctx.viewport_size(), Size::new(800.0, 600.0));
    assert!(!ctx.is_3d_context());
    assert_eq!(ctx.current_opacity(), 1.0);
}

#[test]
fn test_fill_rect() {
    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    ctx.fill_rect(
        Rect::new(10.0, 20.0, 100.0, 50.0),
        8.0.into(),
        Color::BLUE.into(),
    );

    assert_eq!(ctx.batch().primitive_count(), 1);
}

#[test]
fn test_transform_stack() {
    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    ctx.push_transform(Transform::translate(10.0, 20.0));
    ctx.fill_rect(
        Rect::new(0.0, 0.0, 100.0, 50.0),
        0.0.into(),
        Color::RED.into(),
    );

    let batch = ctx.batch();
    let prim = &batch.primitives[0];

    // The rect should be translated
    assert_eq!(prim.bounds[0], 10.0);
    assert_eq!(prim.bounds[1], 20.0);
}

#[test]
fn test_opacity_stack() {
    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    ctx.push_opacity(0.5);
    ctx.push_opacity(0.5);

    assert_eq!(ctx.current_opacity(), 0.25);

    ctx.pop_opacity();
    assert_eq!(ctx.current_opacity(), 0.5);
}

#[test]
fn test_execute_commands() {
    use blinc_core::RecordingContext;

    let mut recording = RecordingContext::new(Size::new(800.0, 600.0));
    recording.fill_rect(
        Rect::new(10.0, 20.0, 100.0, 50.0),
        4.0.into(),
        Color::GREEN.into(),
    );

    let commands = recording.take_commands();

    let mut ctx = GpuPaintContext::new(800.0, 600.0);
    ctx.execute_commands(&commands);

    assert_eq!(ctx.batch().primitive_count(), 1);
}

#[test]
fn test_layer_stack_tracking() {
    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    // Initial state
    assert_eq!(ctx.layer_stack.len(), 0);
    assert_eq!(ctx.current_opacity(), 1.0);
    assert_eq!(ctx.current_blend_mode(), BlendMode::Normal);

    // Push a layer with opacity and blend mode
    let config = LayerConfig {
        id: None,
        position: None,
        size: None,
        blend_mode: BlendMode::Multiply,
        opacity: 0.5,
        depth: false,
        effects: Vec::new(),
        transform_3d: None,
    };
    ctx.push_layer(config);

    // Layer should be tracked
    assert_eq!(ctx.layer_stack.len(), 1);
    // Blend mode and opacity should be applied
    assert_eq!(ctx.current_opacity(), 0.5);
    assert_eq!(ctx.current_blend_mode(), BlendMode::Multiply);

    // Draw something within the layer
    ctx.fill_rect(
        Rect::new(10.0, 10.0, 100.0, 100.0),
        0.0.into(),
        Color::RED.into(),
    );

    // Pop the layer
    ctx.pop_layer();

    // State should be restored
    assert_eq!(ctx.layer_stack.len(), 0);
    assert_eq!(ctx.current_opacity(), 1.0);
    assert_eq!(ctx.current_blend_mode(), BlendMode::Normal);
}

#[test]
fn test_nested_layers() {
    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    // Push first layer
    let config1 = LayerConfig {
        id: None,
        position: None,
        size: None,
        blend_mode: BlendMode::Normal,
        opacity: 0.8,
        depth: false,
        effects: Vec::new(),
        transform_3d: None,
    };
    ctx.push_layer(config1);
    assert_eq!(ctx.layer_stack.len(), 1);
    assert_eq!(ctx.current_opacity(), 0.8);

    // Push second layer (nested)
    let config2 = LayerConfig {
        id: None,
        position: None,
        size: None,
        blend_mode: BlendMode::Screen,
        opacity: 0.5,
        depth: false,
        effects: Vec::new(),
        transform_3d: None,
    };
    ctx.push_layer(config2);
    assert_eq!(ctx.layer_stack.len(), 2);
    // Opacity should be combined: 0.8 * 0.5 = 0.4
    assert!((ctx.current_opacity() - 0.4).abs() < 0.001);
    assert_eq!(ctx.current_blend_mode(), BlendMode::Screen);

    // Pop second layer
    ctx.pop_layer();
    assert_eq!(ctx.layer_stack.len(), 1);
    assert_eq!(ctx.current_opacity(), 0.8);

    // Pop first layer
    ctx.pop_layer();
    assert_eq!(ctx.layer_stack.len(), 0);
    assert_eq!(ctx.current_opacity(), 1.0);
}

/// P4.3 Option B — when a composite layer is active, primitives
/// emitted into the scratch batch must NOT carry the ancestor
/// clip rect; that ancestor clip is re-applied at blit time. A
/// primitive emitted inside the layer should report
/// `ClipType::None` even though an ancestor clip is on the stack.
///
/// IGNORED 2026-05-26 — strip-clip behavior temporarily disabled
/// because the AABB scissor passed to the blit doesn't preserve
/// the parent's corner radius, producing visible artifacts on
/// `cn::switch` (rounded track corners get squared off). Re-
/// enable when the bake captures the rounded-rect parent's radius
/// too and passes it as the blit shader's scissor radius.
#[test]
fn composite_layer_strips_ancestor_clip_from_emit() {
    use blinc_core::DrawContext as _;
    use blinc_core::Rect;
    use blinc_core::layer::ClipShape;

    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    // Outer clip simulating an `.overflow_clip()` track at
    // [100, 100, 200, 50].
    ctx.push_clip(ClipShape::Rect(Rect::new(100.0, 100.0, 200.0, 50.0)));
    let outer_clip_aabb = ctx.current_clip_aabb();
    assert_eq!(outer_clip_aabb, Some([100.0, 100.0, 200.0, 50.0]));

    // Promote the subtree to a composite layer.
    ctx.push_composite_layer(0xDEAD_BEEF);
    assert_eq!(ctx.composite_layer_clip_base(), Some(1));

    // Emit a primitive INSIDE the composite layer. Its
    // `clip_rect` should be the "no clip" sentinel because the
    // ancestor clip is stripped.
    ctx.fill_rect(
        Rect::new(0.0, 0.0, 50.0, 50.0),
        0.0.into(),
        Color::BLUE.into(),
    );
    let batches = ctx.take_composite_layer_batches();
    let scratch = batches
        .get(&0xDEAD_BEEF)
        .expect("scratch batch should exist for promoted node");
    assert_eq!(scratch.primitives.len(), 1);
    let prim_clip = scratch.primitives[0].clip_bounds;
    // Sentinel "no clip" bounds from `get_clip_data`.
    assert_eq!(prim_clip, [-10000.0, -10000.0, 100000.0, 100000.0]);

    // `ambient_clip_aabb` peels the ancestor for the overlay.
    let ambient = ctx.ambient_clip_aabb(1);
    assert_eq!(ambient, Some([100.0, 100.0, 200.0, 50.0]));

    ctx.pop_composite_layer();
    assert_eq!(ctx.composite_layer_clip_base(), None);
    ctx.pop_clip();
}

/// P4.3 Option B — clips pushed INSIDE the composite layer (i.e.
/// the subtree's own intrinsic clips) still apply to primitives
/// emitted within them. Only ancestors below the snapshot are
/// stripped.
///
/// IGNORED 2026-05-26 — paired with the strip-clip test above;
/// re-enable both when the strip-clip behavior comes back with
/// rounded-corner-aware scissor support.
#[test]
fn composite_layer_preserves_inner_clip() {
    use blinc_core::DrawContext as _;
    use blinc_core::Rect;
    use blinc_core::layer::ClipShape;

    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    // Outer ancestor clip (stripped).
    ctx.push_clip(ClipShape::Rect(Rect::new(0.0, 0.0, 50.0, 50.0)));
    ctx.push_composite_layer(42);

    // Inner clip pushed by the subtree itself (e.g. a descendant
    // overflow_clip). This MUST land in per-primitive clip rect
    // because the bake is supposed to capture intrinsic clipping.
    ctx.push_clip(ClipShape::Rect(Rect::new(200.0, 200.0, 100.0, 100.0)));

    ctx.fill_rect(
        Rect::new(0.0, 0.0, 1000.0, 1000.0),
        0.0.into(),
        Color::RED.into(),
    );
    let batches = ctx.take_composite_layer_batches();
    let scratch = batches.get(&42).expect("scratch batch exists");
    let prim_clip = scratch.primitives[0].clip_bounds;
    // Inner clip (NOT the outer ancestor) lands on the primitive.
    assert_eq!(prim_clip, [200.0, 200.0, 100.0, 100.0]);

    ctx.pop_clip();
    ctx.pop_composite_layer();
    ctx.pop_clip();
}

/// P4.3 Option B — nested composite layers. When a promoted
/// subtree contains a descendant that's ALSO promoted, the
/// inner push must not clear the outer's clip-base on its pop.
/// Previously stored as `Option<usize>`, a single-slot field
/// silently dropped the outer's state on the inner's pop —
/// leaving subsequent emits in the outer subtree with no
/// strip-clip applied (so the outer's primitives leaked into
/// scratch with their ancestor clips re-attached). The Vec
/// stack form restores correct behaviour. The outer batch
/// continues to receive all emits — nested promotion shares
/// scratch routing with the outer (an intentional simplification
/// matching the no-nested-routing contract; only the clip-base
/// accounting is stack-aware).
#[test]
fn composite_layer_nested_push_preserves_outer_clip_base() {
    use blinc_core::DrawContext as _;
    use blinc_core::Rect;
    use blinc_core::layer::ClipShape;

    let mut ctx = GpuPaintContext::new(800.0, 600.0);

    // Ancestor clip (outer-of-outer). Should be stripped from
    // every primitive emitted inside any composite layer.
    ctx.push_clip(ClipShape::Rect(Rect::new(0.0, 0.0, 800.0, 600.0)));

    // Outer promoted subtree.
    ctx.push_composite_layer(0xAAAA);
    assert_eq!(ctx.composite_layer_clip_base(), Some(1));

    // Inner promoted subtree (nested). Old single-slot field
    // would have OVERWRITTEN outer's clip_base here; the Vec
    // pushes a new snapshot on top.
    ctx.push_composite_layer(0xBBBB);
    assert_eq!(
        ctx.composite_layer_clip_base(),
        Some(1),
        "outermost clip_base (1) must remain the cutoff while inner is active"
    );

    // Inner emits a primitive. clip_rect strips ancestors below
    // outer base (1) — i.e., the 800x600 ancestor.
    ctx.fill_rect(
        Rect::new(10.0, 10.0, 20.0, 20.0),
        0.0.into(),
        Color::BLUE.into(),
    );

    // Inner pop. Previously this would have cleared the outer's
    // state to None; with Vec it just pops the inner snapshot.
    ctx.pop_composite_layer();
    assert_eq!(
        ctx.composite_layer_clip_base(),
        Some(1),
        "outer's clip_base must survive inner pop"
    );

    // Outer continues to emit; its primitives still strip the
    // ancestor.
    ctx.fill_rect(
        Rect::new(50.0, 50.0, 30.0, 30.0),
        0.0.into(),
        Color::RED.into(),
    );

    let batches = ctx.take_composite_layer_batches();
    let scratch = batches.get(&0xAAAA).expect("outer scratch exists");
    // Both primitives (inner + outer emits) route into outer's
    // scratch batch (no-nested-routing contract) and both have
    // their ancestor clip stripped.
    assert_eq!(scratch.primitives.len(), 2);
    for prim in &scratch.primitives {
        assert_eq!(
            prim.clip_bounds,
            [-10000.0, -10000.0, 100000.0, 100000.0],
            "ancestor clip stripped for both inner- and outer-emitted prims"
        );
    }

    // Outer pop now clears the active state entirely.
    ctx.pop_composite_layer();
    assert_eq!(ctx.composite_layer_clip_base(), None);
    ctx.pop_clip();
}
