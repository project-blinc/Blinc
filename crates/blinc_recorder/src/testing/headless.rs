//! Build, drive and capture a UI with no window.
//!
//! [`Headless`] runs the same frame as the runners (stylesheet, state rules,
//! stateful and CSS animation, layout, paint) on an offscreen texture, with
//! the pointer and the clock under the caller's control. Use it to look at a
//! component in a given state, or to step a transition, without a display.
//!
//! ```ignore
//! let mut ui = Headless::new(320, 120)?;
//! ui.mount(|_| cn::button("Save"));
//! ui.hover(".cn-button");
//! ui.settle();
//! ui.assert_snapshot("button_hover");
//! ```
//!
//! Goldens live in `.blinc/snapshots/<crate>/<os>/` at the workspace root and
//! are not tracked. A missing
//! golden fails; `BLINC_SNAPSHOT=bless` writes it. A platform with no goldens
//! skips, and `BLINC_SNAPSHOT=off` skips everywhere. `BLINC_SNAPSHOT=report`
//! logs every difference without failing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once, OnceLock};

use blinc_animation::AnimationScheduler;
use blinc_layout::div::Div;
use blinc_layout::event_router::MouseButton;
use blinc_layout::render_state::{CssAnimationStore, RenderState};
use blinc_layout::renderer::RenderTree;
use blinc_layout::tree::LayoutNodeId;
use blinc_layout::widgets::OverlayManagerExt;
use blinc_layout::{ElementBuilder, UpdateResult};
use blinc_theme::{ColorScheme, ColorToken, ThemeBundle, ThemeState};

use blinc_app::windowed::WindowedContext;
use blinc_app::{BlincApp, Result};

use super::framebuffer::CapturedFrame;
use super::golden::Golden;
#[cfg(test)]
#[path = "headless/tests/mod.rs"]
mod tests;
use crate::{
    EventSimulator, RecordingConfig, RecordingExport, SharedRecordingSession, SimulatedInput,
    install_hooks, install_recorder, uninstall_hooks,
};

/// Milliseconds one frame advances the clock by.
const FRAME_MS: u32 = 16;

/// The most [`Headless::settle`] will advance.
const SETTLE_LIMIT_MS: u32 = 10_000;

/// One scheduler for the process: the animation globals are set once.
fn shared_scheduler() -> Arc<Mutex<AnimationScheduler>> {
    static SCHEDULER: OnceLock<Arc<Mutex<AnimationScheduler>>> = OnceLock::new();
    SCHEDULER
        .get_or_init(|| {
            let scheduler = AnimationScheduler::new();
            if !blinc_animation::is_scheduler_initialized() {
                let handle = scheduler.handle();
                blinc_animation::set_global_scheduler(handle.clone());
                blinc_layout::render_state::set_global_scheduler(handle);
            }
            Arc::new(Mutex::new(scheduler))
        })
        .clone()
}

/// The user's closure, with its element type erased.
trait Mounted: 'static {
    fn build(
        &mut self,
        ctx: &mut WindowedContext,
        registry: Arc<blinc_layout::selector::ElementRegistry>,
    ) -> RenderTree;
    fn update(&mut self, ctx: &mut WindowedContext, tree: &mut RenderTree) -> UpdateResult;
}

impl<F, E> Mounted for F
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    fn build(
        &mut self,
        ctx: &mut WindowedContext,
        registry: Arc<blinc_layout::selector::ElementRegistry>,
    ) -> RenderTree {
        let composed = compose_with_overlays(self(ctx), ctx);
        RenderTree::from_element_with_registry(&composed, registry)
    }

    fn update(&mut self, ctx: &mut WindowedContext, tree: &mut RenderTree) -> UpdateResult {
        let composed = compose_with_overlays(self(ctx), ctx);
        tree.incremental_update(&composed)
    }
}

/// The user's UI with the overlay surfaces beside it, as a window has them.
fn compose_with_overlays<E: ElementBuilder + 'static>(
    user_ui: E,
    ctx: &mut WindowedContext,
) -> Div {
    use blinc_layout::overlay_state::{overlay_stack, toast_tray};

    let manager = ctx.overlay_manager();
    manager.set_viewport_with_scale(ctx.width, ctx.height, ctx.scale_factor as f32);
    let overlay_layer = manager.build_overlay_layer();
    let stack_layer = match overlay_stack().lock() {
        Ok(mut s) => {
            s.set_viewport_with_scale(ctx.width, ctx.height, ctx.scale_factor as f32);
            s.build_overlay_layer()
        }
        Err(_) => Div::new(),
    };
    let viewport = (ctx.width, ctx.height);
    let tray_layer = toast_tray()
        .lock()
        .ok()
        .map(|t| t.build_tray_layer(viewport))
        .unwrap_or_default();
    // The surface shows the theme's background, as a window does.
    Div::new()
        .w(ctx.width)
        .h(ctx.height)
        .bg(ThemeState::get().color(ColorToken::Background))
        .relative()
        .child(user_ui)
        .child(overlay_layer)
        .child(stack_layer)
        .child(tray_layer)
}

/// A UI built, driven and painted offscreen.
pub struct Headless {
    app: BlincApp,
    ctx: WindowedContext,
    render_state: RenderState,
    css_anim_store: Arc<Mutex<CssAnimationStore>>,
    mounted: Option<Box<dyn Mounted>>,
    tree: Option<RenderTree>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    now_ms: u64,
    needs_rebuild: bool,
    /// The scope each build runs in, as a runner holds it.
    build_scope: blinc_core::owner::BuildScope,
    pointer: (f32, f32),
    frames: u64,
    recording: Option<Arc<SharedRecordingSession>>,
}

/// Sets the process-wide context state for the first session; a later session leaves it. Sessions
/// that start together cannot both pass the check and set it.
fn init_context_state(ctx: &WindowedContext) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init_with_callback(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                ctx.dirty_flag().clone(),
                Arc::new(|ids| {
                    blinc_layout::check_stateful_deps(ids);
                }),
            );
        }
    });
}

/// The overlay context, set once in the same way.
fn init_overlay_context(ctx: &WindowedContext) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if !blinc_layout::overlay_state::OverlayContext::is_initialized() {
            blinc_layout::overlay_state::OverlayContext::init(ctx.overlay_manager().clone());
        }
    });
}

impl Headless {
    /// An offscreen surface of `width` x `height` logical pixels. Fails when
    /// there is no GPU adapter.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Self::with_scale(width, height, 1.0)
    }

    /// Render a logical viewport at a higher pixel density without changing layout.
    pub fn with_scale(logical_width: u32, logical_height: u32, scale: f64) -> Result<Self> {
        assert!(logical_width > 0 && logical_height > 0 && scale.is_finite() && scale > 0.0);
        let width = (logical_width as f64 * scale).round().max(1.0) as u32;
        let height = (logical_height as f64 * scale).round().max(1.0) as u32;
        let app = BlincApp::new()?;
        blinc_layout::init_text_measurer();
        // A caret blinks on the wall clock; frames compared across runs need it held.
        blinc_layout::widgets::cursor::hold_blink(true);
        // The layout widgets' default look, as the runners register it. Queued
        // per session: each session's context drains the queue, so a run-once
        // install would leave every later session in the process without it.
        blinc_core::context_state::queue_pending_stylesheet(blinc_layout::ua_styles::UA_STYLES);

        let mut ctx = WindowedContext::headless_with_scale(
            logical_width as f32,
            logical_height as f32,
            scale,
        );
        ctx.animations = shared_scheduler();
        init_context_state(&ctx);
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        blinc_core::BlincContextState::get()
            .set_viewport_size(logical_width as f32, logical_height as f32);
        init_overlay_context(&ctx);

        let texture = app.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("blinc headless surface"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: app.texture_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut render_state = RenderState::new(Arc::clone(&ctx.animations));
        render_state.set_viewport(0.0, 0.0, logical_width as f32, logical_height as f32);

        Ok(Self {
            app,
            ctx,
            render_state,
            css_anim_store: Arc::new(Mutex::new(CssAnimationStore::new())),
            mounted: None,
            tree: None,
            texture,
            view,
            width,
            height,
            now_ms: 0,
            needs_rebuild: false,
            build_scope: blinc_core::owner::BuildScope::new(),
            pointer: (-1.0, -1.0),
            frames: 0,
            recording: None,
        })
    }

    /// Whether this machine can render offscreen. Tests use it to skip.
    pub fn available() -> bool {
        BlincApp::new().is_ok()
    }

    /// Use `bundle` in `scheme`, with the CSS it carries. Call before
    /// [`Self::mount`].
    pub fn theme(&mut self, bundle: ThemeBundle, scheme: ColorScheme) -> &mut Self {
        ThemeState::init(bundle, scheme);
        self
    }

    /// Add a stylesheet after the theme's, as `add_css` does in a window.
    pub fn css(&mut self, css: &str) -> &mut Self {
        self.ctx.add_css(css);
        self.needs_rebuild = true;
        self
    }

    /// Register font bytes shared with a web fixture before mounting it.
    pub fn load_font_data(&mut self, bytes: Vec<u8>) -> usize {
        let faces = self.app.load_font_data_to_registry(bytes);
        self.app.refresh_generic_font_styles();
        blinc_layout::init_text_measurer_with_registry(self.app.font_registry());
        faces
    }

    /// The window context the UI is built with.
    pub fn context(&mut self) -> &mut WindowedContext {
        &mut self.ctx
    }

    /// The node holding focus.
    pub fn focused(&self) -> Option<LayoutNodeId> {
        self.ctx.event_router.focused()
    }

    /// The tree as last built, for assertions on render props and layout.
    pub fn tree(&self) -> &RenderTree {
        self.tree.as_ref().expect("nothing is mounted")
    }

    /// Build `ui` and show it. `ui` runs again whenever a state it reads
    /// changes, as in a window.
    pub fn mount<E, F>(&mut self, ui: F) -> &mut Self
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        self.mounted = Some(Box::new(ui));
        self.tree = None;
        // What a previous UI had in flight must not land on this one's nodes,
        // which can share their stable ids.
        self.css_anim_store = Arc::new(Mutex::new(CssAnimationStore::new()));
        self.needs_rebuild = true;
        self.frame(0);
        self
    }

    // ---- the frame -----------------------------------------------------

    /// Run one frame, advancing the clock by `dt_ms`.
    pub fn frame(&mut self, dt_ms: u32) {
        self.now_ms += dt_ms as u64;
        self.frames += 1;
        blinc_core::FrameClock::begin_frame_at(std::time::Duration::from_millis(self.now_ms));
        let dt = dt_ms as f32;
        self.ctx.animations.lock().unwrap().tick_by(dt / 1000.0);

        // A scroll a handler asked for (`ScrollRef::scroll_to`), as the
        // window applies it at the top of its frame.
        if let Some(tree) = self.tree.as_mut() {
            tree.process_pending_scroll_refs();
        }
        self.drain_queued_css();
        self.rebuild_if_needed();
        self.apply_queued_updates();

        self.render_state.process_global_motion_exit_cancels();
        self.render_state.process_global_motion_exit_starts();
        self.render_state.process_global_motion_starts();
        let _ = self.render_state.tick(self.now_ms);

        if let Some(tree) = self.tree.as_mut() {
            let store = tree.css_anim_store();
            store.lock().unwrap().tick(dt);
            tree.tick_flip_animations(dt);
        }
        self.render_state.sync_shared_motion_states();
        let _ = ThemeState::get().tick();

        if blinc_layout::has_animating_statefuls() {
            blinc_layout::check_stateful_animations();
        }
        self.ctx.overlay_manager().update(self.now_ms);
        // The overlay stack and toast tray run their timers and reap what has
        // finished leaving, as the window ticks them each frame.
        {
            use blinc_layout::overlay_state::{overlay_stack, toast_tray};
            if let Ok(mut stack) = overlay_stack().lock() {
                stack.update(self.now_ms);
            }
            if let Ok(mut tray) = toast_tray().lock() {
                tray.update(self.now_ms);
            }
        }
        blinc_layout::widgets::overlay_stack::drain_close_callbacks();
        self.apply_queued_updates();

        blinc_layout::widgets::process_pending_input_focus();
        blinc_layout::widgets::process_pending_area_focus();
        let text_focus = blinc_layout::widgets::text_input::focused_text_input_node_id()
            .or_else(blinc_layout::widgets::text_input::focused_text_area_node_id);
        self.ctx.event_router.sync_text_focus(text_focus);

        let (w, h) = (self.ctx.width, self.ctx.height);
        if let Some(tree) = self.tree.as_mut() {
            tree.sync_keyboard_focus(&mut self.ctx.event_router);
            if tree.stylesheet().is_some()
                && tree.apply_stylesheet_state_styles(&self.ctx.event_router)
            {
                tree.compute_layout(w, h);
                tree.update_flip_bounds();
            }
            tree.apply_all_css_animation_props();
            tree.apply_all_css_transition_props();
            tree.apply_flip_animation_props();
            if tree.apply_animated_layout_props() {
                tree.compute_layout(w, h);
                tree.update_flip_bounds();
            }
            tree.remove_completed_transitions();
            blinc_layout::recorder_bridge::record_frame(
                tree,
                &self.ctx.event_router,
                (self.width, self.height),
            );
        }
    }

    /// Advance the clock by `ms`, a frame at a time.
    pub fn advance(&mut self, ms: u32) -> &mut Self {
        let mut left = ms;
        while left > 0 {
            let step = left.min(FRAME_MS);
            self.frame(step);
            left -= step;
        }
        self
    }

    /// Advance until nothing is animating or easing.
    pub fn settle(&mut self) -> &mut Self {
        self.frame(0);
        let mut spent = 0;
        while self.busy() && spent < SETTLE_LIMIT_MS {
            self.frame(FRAME_MS);
            spent += FRAME_MS;
        }
        self
    }

    fn busy(&self) -> bool {
        let Some(tree) = self.tree.as_ref() else {
            return false;
        };
        let store = tree.css_anim_store();
        let store = store.lock().unwrap();
        store.has_active_animations()
            || store.has_active_transitions()
            || self.ctx.animations.lock().unwrap().has_active_animations()
            || blinc_layout::overlay_state::overlay_stack()
                .lock()
                .is_ok_and(|s| s.has_animating_overlays())
            || blinc_layout::overlay_state::toast_tray()
                .lock()
                .is_ok_and(|t| t.has_animating())
    }

    fn drain_queued_css(&mut self) {
        let queued = blinc_core::BlincContextState::get().drain_stylesheets();
        if !queued.is_empty() {
            for css in queued {
                self.ctx.add_css(&css);
            }
            self.needs_rebuild = true;
        }
    }

    fn rebuild_if_needed(&mut self) {
        if let Some(tree) = self.tree.as_ref() {
            if tree.needs_rebuild() {
                self.needs_rebuild = true;
            }
        }
        if blinc_layout::widgets::take_needs_rebuild() {
            self.needs_rebuild = true;
        }
        if self
            .ctx
            .dirty_flag()
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.needs_rebuild = true;
        }
        if blinc_layout::widgets::take_needs_css_reparse() {
            self.ctx.reparse_css();
        }
        if blinc_layout::widgets::take_needs_relayout() {
            self.tree = None;
            self.needs_rebuild = true;
        }
        if !self.needs_rebuild {
            return;
        }
        self.needs_rebuild = false;
        let Some(mounted) = self.mounted.as_mut() else {
            return;
        };

        // What the widgets create while building belongs to this build; a fresh tree disposes the
        // builds before it.
        let build_pass = self.build_scope.begin();
        let replaces_tree = self.tree.is_none();
        blinc_layout::reset_call_counters();
        blinc_layout::clear_stateful_base_updaters();
        blinc_layout::click_outside::clear_click_outside_handlers();

        let (w, h) = (self.ctx.width, self.ctx.height);
        if let Some(tree) = self.tree.as_mut() {
            match mounted.update(&mut self.ctx, tree) {
                UpdateResult::NoChanges | UpdateResult::VisualOnly => {}
                UpdateResult::LayoutChanged | UpdateResult::ChildrenChanged => {
                    tree.apply_stylesheet_base_styles();
                    tree.apply_stylesheet_layout_overrides();
                    tree.compute_layout(w, h);
                    tree.apply_flip_transitions();
                    tree.update_flip_bounds();
                    if let Some(sheet) = &self.ctx.stylesheet {
                        self.ctx.pointer_query.register_from_stylesheet(sheet);
                    }
                    tree.initialize_motion_animations(&mut self.render_state);
                    self.render_state.end_stable_motion_frame();
                    self.render_state.process_global_motion_replays();
                    tree.start_all_css_animations();
                }
            }
            tree.clear_dirty();
        } else {
            let registry = Arc::clone(self.ctx.element_registry());
            let mut tree = mounted.build(&mut self.ctx, registry);
            tree.set_animations(&self.ctx.animations);
            tree.set_scale_factor(self.ctx.scale_factor as f32);
            tree.set_css_anim_store(Arc::clone(&self.css_anim_store));
            if let Some(sheet) = &self.ctx.stylesheet {
                tree.set_stylesheet_arc(sheet.clone());
            }
            tree.apply_all_stylesheet_styles();
            if let Some(sheet) = &self.ctx.stylesheet {
                self.ctx.pointer_query.register_from_stylesheet(sheet);
            }
            tree.compute_layout(w, h);
            tree.update_flip_bounds();
            tree.initialize_motion_animations(&mut self.render_state);
            self.render_state.end_stable_motion_frame();
            self.render_state.process_global_motion_replays();
            tree.start_all_css_animations();
            self.ctx.rebuild_count = self.ctx.rebuild_count.saturating_add(1);
            self.tree = Some(tree);
        }
        if replaces_tree {
            self.build_scope.finish(build_pass);
        } else {
            self.build_scope.keep(build_pass);
        }
    }

    /// Build the mounted UI again into a new tree, as a runner does on a resize: the tree is
    /// replaced, and what the builds before it created is disposed.
    pub fn rebuild_fresh(&mut self) -> &mut Self {
        self.tree = None;
        self.needs_rebuild = true;
        self.frame(0);
        self
    }

    /// Land what widgets and signals queued: property writes, subtree
    /// rebuilds, and the layout they call for.
    fn apply_queued_updates(&mut self) {
        let (w, h) = (self.ctx.width, self.ctx.height);
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        // An overlay opened or closed: rebuild its layer in place, as the
        // window does, so what it shows is in the tree.
        {
            use blinc_layout::overlay_state::{overlay_stack, rebuild_overlay_subtree_if_dirty};
            let manager = self.ctx.overlay_manager();
            rebuild_overlay_subtree_if_dirty(
                tree.element_registry(),
                blinc_layout::widgets::overlay::OVERLAY_LAYER_ID,
                manager.take_dirty(),
                || manager.build_overlay_layer(),
            );
            rebuild_overlay_subtree_if_dirty(
                tree.element_registry(),
                blinc_layout::widgets::overlay_stack::OVERLAY_STACK_LAYER_ID,
                overlay_stack().lock().is_ok_and(|s| s.take_dirty()),
                || {
                    overlay_stack()
                        .lock()
                        .ok()
                        .map(|s| s.build_overlay_layer())
                        .unwrap_or_default()
                },
            );
            use blinc_layout::overlay_state::toast_tray;
            rebuild_overlay_subtree_if_dirty(
                tree.element_registry(),
                blinc_layout::widgets::toast_tray::TOAST_TRAY_LAYER_ID,
                toast_tray().lock().is_ok_and(|t| t.take_dirty()),
                || {
                    toast_tray()
                        .lock()
                        .ok()
                        .map(|t| t.build_tray_layer((w, h)))
                        .unwrap_or_default()
                },
            );
        }
        for _ in 0..3 {
            let updates = tree.take_pending_property_updates();
            let mut needs_layout = tree.apply_partial_property_updates(updates).needs_layout;
            let rebuilt = tree.process_pending_subtree_rebuilds();
            needs_layout |= rebuilt;
            if rebuilt {
                self.app
                    .invalidate_render_cache_tagged("headless_subtree_rebuild");
                tree.apply_stylesheet_layout_overrides();
            }
            if needs_layout {
                tree.compute_layout(w, h);
                tree.apply_flip_transitions();
                tree.update_flip_bounds();
                tree.initialize_motion_animations(&mut self.render_state);
                self.render_state.end_stable_motion_frame();
                self.render_state.process_global_motion_replays();
                tree.start_all_css_animations();
            }
            if !needs_layout && !rebuilt && !blinc_layout::has_pending_partial_prop_updates() {
                break;
            }
        }
    }

    // ---- input ---------------------------------------------------------

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>) {
        let (x, y) = self.pointer;
        // A drag carries how far it has gone, as the window gives it.
        let (dx, dy) = self.ctx.event_router.drag_delta();
        if let Some(tree) = self.tree.as_mut() {
            for (node, event) in events {
                let (bx, by, bw, bh) = self
                    .ctx
                    .event_router
                    .get_node_bounds(node)
                    .unwrap_or((0.0, 0.0, 0.0, 0.0));
                let dragging = event == blinc_core::events::event_types::DRAG
                    || event == blinc_core::events::event_types::DRAG_END;
                let (dx, dy) = if dragging { (dx, dy) } else { (0.0, 0.0) };
                tree.dispatch_event_full(
                    node,
                    event,
                    x,
                    y,
                    x - bx,
                    y - by,
                    bx,
                    by,
                    bw,
                    bh,
                    dx,
                    dy,
                    1.0,
                    false,
                    false,
                    false,
                    false,
                );
            }
        }
        self.frame(0);
    }

    /// Move the pointer to `(x, y)`.
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.pointer = (x, y);
        // While a button is held, the window's drag path: DRAG to the
        // pressed element and its ancestors, with no hover walk.
        let events = match self.tree.as_ref() {
            Some(tree) if self.ctx.event_router.is_press_in_flight() => {
                self.ctx.event_router.on_mouse_drag_fast(tree, x, y)
            }
            Some(tree) => self.ctx.event_router.on_mouse_move(tree, x, y),
            None => return self,
        };
        self.deliver(events);
        self
    }

    /// Press the left button at the pointer.
    pub fn mouse_down(&mut self) -> &mut Self {
        let (x, y) = self.pointer;
        let events = match self.tree.as_ref() {
            Some(tree) => self
                .ctx
                .event_router
                .on_mouse_down(tree, x, y, MouseButton::Left),
            None => return self,
        };
        self.deliver(events);
        self
    }

    /// Release the left button at the pointer.
    pub fn mouse_up(&mut self) -> &mut Self {
        let (x, y) = self.pointer;
        let events = match self.tree.as_ref() {
            Some(tree) => self
                .ctx
                .event_router
                .on_mouse_up(tree, x, y, MouseButton::Left),
            None => return self,
        };
        self.deliver(events);
        self
    }

    /// Right-click the centre of `selector`, including pointer enter and release.
    pub fn right_click(&mut self, selector: &str) -> &mut Self {
        self.hover(selector);
        let (x, y) = self.pointer;
        for down in [true, false] {
            let Some(tree) = self.tree.as_ref() else {
                return self;
            };
            let events = if down {
                self.ctx
                    .event_router
                    .on_mouse_down(tree, x, y, MouseButton::Right)
            } else {
                self.ctx
                    .event_router
                    .on_mouse_up(tree, x, y, MouseButton::Right)
            };
            self.deliver(events);
        }
        self
    }

    /// Type into the focused widget, applying focus changes between characters.
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        for ch in text.chars() {
            if let Some(tree) = self.tree.as_mut() {
                tree.broadcast_text_input_event(ch, false, false, false, false);
            }
            self.frame(0);
        }
        self
    }

    /// Move to the first element matching `selector`'s centre.
    pub fn hover(&mut self, selector: &str) -> &mut Self {
        let (x, y) = self.centre(selector);
        self.move_to(x, y)
    }

    /// Hover `selector` and hold the left button down.
    pub fn press(&mut self, selector: &str) -> &mut Self {
        self.hover(selector);
        self.mouse_down()
    }

    /// Release where the pointer is.
    pub fn release(&mut self) -> &mut Self {
        self.mouse_up()
    }

    /// Click the first element matching `selector`.
    pub fn click(&mut self, selector: &str) -> &mut Self {
        self.press(selector);
        self.mouse_up()
    }

    /// Press `key_code` (the codes the window gives: 37 is Left, 33 Page Up).
    /// It reaches every key handler, as in a window; each decides whether it
    /// has focus.
    pub fn key(&mut self, key_code: u32) -> &mut Self {
        self.key_with_shift(key_code, false)
    }

    /// [`Self::key`] with Shift held.
    pub fn key_with_shift(&mut self, key_code: u32, shift: bool) -> &mut Self {
        if key_code == 27 {
            let consumed = blinc_layout::overlay_state::overlay_stack()
                .lock()
                .is_ok_and(|mut stack| stack.handle_escape());
            if consumed {
                self.frame(0);
                return self;
            }
        }
        let navigated = self
            .tree
            .as_mut()
            .is_some_and(|t| t.handle_navigation_key(&mut self.ctx.event_router, key_code, shift));
        if navigated {
            self.frame(0);
            return self;
        }
        if let Some(tree) = self.tree.as_mut() {
            tree.broadcast_key_event(
                blinc_core::events::event_types::KEY_DOWN,
                key_code,
                shift,
                false,
                false,
                false,
            );
        }
        self.frame(0);
        self
    }

    /// Move the pointer out of the window, as the window reports it: a press
    /// in flight is released and nothing stays hovered.
    pub fn leave(&mut self) -> &mut Self {
        self.pointer = (-10.0, -10.0);
        let events = match self.tree.as_ref() {
            Some(tree) => self.ctx.event_router.on_mouse_leave(tree),
            None => return self,
        };
        self.deliver(events);
        self
    }

    /// Scroll under the pointer.
    pub fn scroll(&mut self, delta_x: f32, delta_y: f32) -> &mut Self {
        // As the windowed runner does: the scrollable under the pointer, or
        // the handler nearest it, gets the delta.
        let (x, y) = self.pointer;
        if let Some(tree) = self.tree.as_mut() {
            if let Some(hit) = self.ctx.event_router.hit_test(tree, x, y) {
                tree.dispatch_scroll_chain(hit.node, &hit.ancestors, x, y, delta_x, delta_y);
            }
        }
        self.frame(0);
        self
    }

    /// End the scroll gesture under way, as the fingers lifting from a
    /// trackpad do; scrolls after this are momentum until [`Self::scroll_start`].
    pub fn scroll_end(&mut self) -> &mut Self {
        if let Some(tree) = self.tree.as_mut() {
            tree.on_gesture_end();
            tree.on_scroll_gesture(blinc_layout::ScrollGesturePhase::Lift);
        }
        self.frame(0);
        self
    }

    /// Start a scroll gesture, as fingers touching a trackpad do.
    pub fn scroll_start(&mut self) -> &mut Self {
        if let Some(tree) = self.tree.as_mut() {
            tree.begin_scroll_gesture();
        }
        self
    }

    /// The momentum after a lift stops.
    pub fn scroll_momentum_end(&mut self) -> &mut Self {
        if let Some(tree) = self.tree.as_mut() {
            tree.on_scroll_gesture(blinc_layout::ScrollGesturePhase::MomentumEnd);
        }
        self
    }

    // ---- recording and replay ------------------------------------------

    /// Start recording what the UI is given and how it looks, in the form the
    /// debugger opens. Pair with [`Self::stop_recording`].
    pub fn start_recording(&mut self) -> &mut Self {
        let session = Arc::new(SharedRecordingSession::new(RecordingConfig::debug()));
        install_recorder(Arc::clone(&session));
        install_hooks();
        session.start();
        self.recording = Some(session);
        self
    }

    /// Stop recording and hand back the session.
    pub fn stop_recording(&mut self) -> Option<RecordingExport> {
        let session = self.recording.take()?;
        session.stop();
        uninstall_hooks();
        Some(session.export())
    }

    /// Feed a recorded session's input to the UI in order, advancing the
    /// clock by the time between events, and leave it settled.
    pub fn replay(&mut self, export: &RecordingExport) -> &mut Self {
        use crate::RecordedEvent as Event;

        let mut simulator = EventSimulator::new();
        let mut last = export.events.first().map(|e| e.timestamp.as_micros());
        for recorded in &export.events {
            let at = recorded.timestamp.as_micros();
            if let Some(before) = last {
                // A pause in a recording is not worth waiting out in full.
                let gap_ms = (at.saturating_sub(before) / 1000).min(500) as u32;
                self.advance(gap_ms);
            }
            last = Some(at);
            match simulator.process_event(&recorded.event) {
                SimulatedInput::MouseMove { position, .. } => {
                    self.move_to(position.x, position.y);
                }
                SimulatedInput::MouseDown { position, .. } => {
                    self.move_to(position.x, position.y);
                    self.mouse_down();
                }
                SimulatedInput::MouseUp { position, .. } => {
                    self.move_to(position.x, position.y);
                    self.mouse_up();
                }
                SimulatedInput::Scroll {
                    position,
                    delta_x,
                    delta_y,
                    ..
                } => {
                    self.move_to(position.x, position.y);
                    self.scroll(delta_x, delta_y);
                }
                // A click is its own down and up, already recorded.
                _ if matches!(recorded.event, Event::Click(_)) => {}
                _ => {}
            }
        }
        self.settle()
    }

    // ---- lookup --------------------------------------------------------

    /// The first node matching `selector`: `#id` or `.class`, in tree order.
    pub fn find(&self, selector: &str) -> Option<LayoutNodeId> {
        let tree = self.tree.as_ref()?;
        if let Some(id) = selector.strip_prefix('#') {
            return tree.query_by_id(id);
        }
        let class = selector.strip_prefix('.')?;
        let nodes = tree.element_registry().class_to_nodes_index();
        let matching = nodes.get(class)?;
        let root = tree.root()?;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if matching.contains(&node) {
                return Some(node);
            }
            stack.extend(tree.layout_tree.children(node).into_iter().rev());
        }
        None
    }

    /// Whether the router currently considers this element hovered.
    pub fn is_hovered(&self, selector: &str) -> bool {
        self.find(selector)
            .and_then(|node| self.tree().stable_id(node))
            .is_some_and(|stable| self.ctx.event_router.is_hovered(stable))
    }

    /// The centre of the first node matching `selector`.
    ///
    /// Panics when nothing matches: a test that names a missing element is
    /// wrong, not skipped.
    pub fn centre(&self, selector: &str) -> (f32, f32) {
        let node = self
            .find(selector)
            .unwrap_or_else(|| panic!("nothing matches `{selector}`"));
        let b = self
            .tree()
            .get_absolute_bounds(node)
            .unwrap_or_else(|| panic!("`{selector}` has no bounds"));
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    /// The cursor the window would set with the pointer at `(x, y)`.
    pub fn cursor_at(&self, x: f32, y: f32) -> Option<blinc_layout::element::CursorStyle> {
        self.tree
            .as_ref()?
            .get_cursor_at(&self.ctx.event_router, x, y)
    }

    /// The cursor over the centre of the first node matching `selector`.
    pub fn cursor_over(&self, selector: &str) -> Option<blinc_layout::element::CursorStyle> {
        let (x, y) = self.centre(selector);
        self.cursor_at(x, y)
    }

    // ---- capture -------------------------------------------------------

    /// Paint the current frame and read it back.
    pub fn snapshot(&mut self) -> Result<CapturedFrame> {
        self.snapshot_with(false)
    }

    /// Paint the current frame the way a window paints a frame in which only animated values
    /// changed: patching the batch of the last full paint instead of walking the tree. The first
    /// frame after a mount or rebuild is a full paint whatever is asked, as in a window.
    pub fn snapshot_fast(&mut self) -> Result<CapturedFrame> {
        self.snapshot_with(true)
    }

    fn snapshot_with(&mut self, try_fast_paint: bool) -> Result<CapturedFrame> {
        self.paint(try_fast_paint)?;
        self.read_frame()
    }

    /// Paint a recording annotation after the UI and its overlays.
    /// The annotation does not participate in hit testing or change mounted state.
    pub fn snapshot_with_overlay<E: ElementBuilder>(
        &mut self,
        annotation: &E,
    ) -> Result<CapturedFrame> {
        self.paint(false)?;
        let mut tree = RenderTree::from_element(annotation);
        tree.set_scale_factor(self.ctx.scale_factor as f32);
        tree.compute_layout(self.ctx.width, self.ctx.height);
        let state = RenderState::new(Arc::clone(&self.ctx.animations));
        self.app.render_overlay_tree_with_motion(
            &tree,
            &state,
            &self.view,
            self.width,
            self.height,
        )?;
        self.read_frame()
    }

    fn paint(&mut self, try_fast_paint: bool) -> Result<()> {
        self.frame(0);
        let tree = self.tree.as_ref().expect("nothing is mounted");
        self.app.render_tree_with_motion_opt(
            tree,
            &self.render_state,
            &self.view,
            Some(&self.texture),
            self.width,
            self.height,
            try_fast_paint,
        )?;
        Ok(())
    }

    fn read_frame(&self) -> Result<CapturedFrame> {
        let rgba = self
            .app
            .read_texture_to_rgba8(&self.texture, self.width, self.height)?;
        Ok(CapturedFrame::new(rgba, self.width, self.height).with_frame_number(self.frames))
    }

    /// Compare the current frame with the golden `name` of the crate under
    /// test. See the module docs for blessing and skipping.
    pub fn assert_snapshot(&mut self, name: &str) {
        let golden = Golden::in_crate();
        if golden.skipped() {
            eprintln!(
                "snapshot `{name}` skipped: no goldens for this platform (BLINC_SNAPSHOT=bless records them)"
            );
            return;
        }
        let image = self.snapshot().expect("snapshot");
        golden.check(name, &image);
    }
}
