//! Tree construction: walk an `ElementBuilder` and populate the
//! `RenderTree` data structures.
//!
//! Four `pub(crate)` entry points on `RenderTree`:
//!
//! - `build_element` — top-level entry. Calls `element.build()` to
//!   produce the layout tree, then recurses into
//!   `collect_render_props` to fill in per-node `RenderNode`s.
//! - `collect_render_props<E: ElementBuilder>` — generic-typed
//!   collector. Inserts the `RenderNode`, registers element ids /
//!   parent / index in `ElementRegistry`, materialises `ElementType`
//!   via `determine_element_type`, applies the stylesheet's base
//!   `#id` styles inline (since the stylesheet may be missing here
//!   when `apply_stylesheet_base_styles` runs later), and recurses
//!   into children.
//! - `collect_render_props_boxed(&dyn ElementBuilder)` — trait-object
//!   variant for sites that hold a boxed builder (subtree rebuilds,
//!   mixed-type fragment children).
//! - `collect_render_props_boxed_with_motion` — secondary entry for
//!   the motion-pre-replay path: like the boxed variant but also
//!   resolves the parent's motion config so child `motion(...)`
//!   wrappers can re-anchor onto the right enter / exit animation
//!   key.
//!
//! `Self::apply_element_style_to_props` (stylesheet/apply.rs),
//! `Self::build_text_data` (build/text.rs),
//! `Self::determine_element_type*` (build/element_type.rs), and
//! `self.inherit_text_props_from_parent` (build/text.rs) are all
//! crossed by every collect path.

use crate::diff::DivHash;
use crate::div::{ElementBuilder, ElementTypeId};
use crate::tree::LayoutNodeId;

use super::super::{
    CanvasData, ElementType, ImageData, RenderNode, RenderTree, StyledTextData, StyledTextSpan,
    SvgData,
};

impl RenderTree {
    /// Recursively build elements into the tree
    ///
    /// Three-phase walk per build pass: layout-build, mint stable
    /// ids, then collect render props.
    ///
    /// `element.build(layout_tree)` materialises all layout nodes
    /// and parent/child relationships. `root` is set so
    /// `mint_stable_ids_walk` can find its seed.
    /// `mint_stable_ids_walk` walks the freshly-built layout tree
    /// and assigns each node a `StableNodeId` derived from
    /// `(parent_stable, sibling_index, element_id_if_set)`.
    /// `collect_render_props` runs last so every collect site has
    /// `self.stable_id(node_id)` available — handler / scroll
    /// physics / motion bindings register under stable keys and
    /// survive subsequent rebuilds. See
    /// `project_stable_node_id_design` (memory) for the migration
    /// plan.
    pub(crate) fn build_element<E: ElementBuilder>(&mut self, element: &E) -> LayoutNodeId {
        let root_id = element.build(&mut self.layout_tree);
        self.root = Some(root_id);
        // CRITICAL ordering: `mint_stable_ids_walk` derives each
        // stable id from `(parent_stable, sibling_index,
        // widget_key)` where `widget_key` is the node's
        // `element_id` as looked up from `element_registry`. If we
        // mint BEFORE registering ids, every node with `.id()` is
        // derived with `widget_key=None` the first time but with
        // `widget_key=Some("...")` on every subsequent mint — so
        // its stable id (and every descendant's, since `derive_child`
        // recurses on `parent_stable`) changes between the initial
        // build and the next subtree rebuild. Handlers registered
        // under the initial stable id end up orphaned and the
        // post-rebuild `sweep_stale_handlers` evicts them, leaving
        // the live node with no handler — click handlers stop
        // firing on whole subtrees rooted at any `.id()`'d node.
        // Pre-walk the subtree so `element_registry.get_id(node)`
        // returns the same value on every mint pass.
        self.register_element_ids_walk(element, root_id);
        self.build_generation = self.build_generation.wrapping_add(1);
        self.mint_stable_ids_walk();
        self.collect_render_props(element, root_id);
        self.auto_fill_animation_stable_keys();
        // Evict handler entries whose stable id didn't survive this
        // build pass — replacement for the destructive wipe pattern
        // that used to live in `update_if_changed`.
        self.sweep_stale_handlers();
        root_id
    }

    /// Pre-register `element_id`s for the subtree rooted at
    /// `node_id` so `mint_stable_ids_walk` reads consistent
    /// `widget_key`s on every pass.
    ///
    /// Walks the `ElementBuilder` tree and the layout tree in
    /// lockstep — both have identical shape after `build()`, so
    /// `child_builders[i]` corresponds to `layout_tree.children(node)[i]`.
    /// Registers ids in `element_registry` only; render props,
    /// handlers, classes, and the rest of `collect_render_props_boxed`
    /// run later (after mint).
    pub(crate) fn register_element_ids_walk(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) {
        if let Some(id) = element.element_id() {
            self.element_registry.register(id, node_id);
        }
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            self.register_element_ids_walk(child_builder.as_ref(), child_node_id);
        }
    }

    /// Collect render properties from an element and its children
    pub(crate) fn collect_render_props<E: ElementBuilder>(
        &mut self,
        element: &E,
        node_id: LayoutNodeId,
    ) {
        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Apply base CSS styles and animation from stylesheet if element has an ID
        if let Some(ref stylesheet) = self.stylesheet {
            if let Some(id) = element.element_id() {
                // Apply base styles (background, opacity, border-radius, etc.)
                if let Some(base_style) = stylesheet.get(id) {
                    Self::apply_element_style_to_props(&mut props, base_style);
                }
                // Apply CSS animation (only if no motion animation is already set)
                if props.motion.is_none() {
                    if let Some(motion) = stylesheet.resolve_animation(id) {
                        props.motion = Some(motion);
                    }
                }
            }
        }

        // Inherit CSS text properties from parent (text-decoration, white-space, etc.)
        self.inherit_text_props_from_parent(&mut props, node_id);

        // Determine element type using the trait methods
        let element_type = Self::determine_element_type(element);

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Store per-node hashes for incremental update detection
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        let topology_hash = DivHash::compute_topology_tree(element);
        self.node_hashes
            .insert(node_id, (own_hash, tree_hash, topology_hash));

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            let stable_id = self.stable_id_or_warn(node_id);
            self.handler_registry.register(stable_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
            if element.viewport_cull() {
                self.viewport_cull_scrolls.insert(node_id);
            }
        }

        // Store motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Register layout animation config if element wants animated layout transitions
        if let Some(config) = element.layout_animation_config() {
            tracing::debug!(
                "collect_render_props: registered layout animation config for {:?}",
                node_id
            );
            self.layout_animation_configs.insert(node_id, config);
        }

        // Register visual animation config for new FLIP-style system
        if let Some(config) = element.visual_animation_config() {
            tracing::trace!(
                "[VISUAL_ANIM] collect_render_props: registering config for {:?}, key={:?}",
                node_id,
                config.key
            );
            self.register_visual_animation_config(node_id, config);
        }

        // Element IDs are pre-registered by `register_element_ids_walk`
        // before mint runs, so mint reads consistent `widget_key`s on
        // every pass. Re-registering here would just trip the duplicate
        // warning — skip.

        // Register CSS classes for complex selector matching
        let classes = element.element_classes();
        if !classes.is_empty() {
            self.element_registry
                .register_classes(node_id, classes.to_vec());
        }

        // Register semantic element type for CSS type selector matching
        if let Some(type_name) = element.semantic_type_name() {
            self.element_registry
                .register_element_type(node_id, type_name);
        }

        // Bind ScrollRef if present (for scroll containers)
        if let Some(scroll_ref) = element.bound_scroll_ref() {
            self.register_scroll_ref(node_id, scroll_ref);
        }

        // Get child node IDs from the layout tree
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Log mismatch to help debug stateful/motion issues (in collect_render_props)
        if child_node_ids.len() != child_builders.len() && !child_node_ids.is_empty() {
            tracing::warn!(
                "collect_render_props: node {:?} has {} layout children but {} builder children (mismatch!)",
                node_id,
                child_node_ids.len(),
                child_builders.len()
            );
        }

        let total_children = child_node_ids.len();

        // Match children by index (they were built in order)
        for (index, (child_builder, &child_node_id)) in
            child_builders.iter().zip(child_node_ids.iter()).enumerate()
        {
            // Register parent-child relationship and child index
            self.element_registry
                .register_parent(child_node_id, node_id);
            self.element_registry
                .register_child_index(child_node_id, index, total_children);
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }

    /// Collect render props from a boxed element builder
    pub(crate) fn collect_render_props_boxed(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) {
        // Debug: See all element types being collected
        let eid = element.element_type_id();
        // eprintln!("collect_render_props_boxed: node={:?}, type_id={:?}", node_id, eid);

        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Apply base CSS styles and animation from stylesheet if element has an ID
        if let Some(ref stylesheet) = self.stylesheet {
            if let Some(id) = element.element_id() {
                // Apply base styles (background, opacity, border-radius, etc.)
                if let Some(base_style) = stylesheet.get(id) {
                    Self::apply_element_style_to_props(&mut props, base_style);
                }
                // Apply CSS animation (only if no motion animation is already set)
                if props.motion.is_none() {
                    if let Some(motion) = stylesheet.resolve_animation(id) {
                        props.motion = Some(motion);
                    }
                }
            }
        }

        // Inherit CSS text properties from parent (text-decoration, white-space, etc.)
        self.inherit_text_props_from_parent(&mut props, node_id);

        // Use the element_type_id to determine type
        let type_id_boxed = element.element_type_id();
        if matches!(type_id_boxed, ElementTypeId::Canvas) {
            let render_fn = element.canvas_render_info();
            // eprintln!(
            //     "collect_render_props_boxed: ElementTypeId::Canvas detected! has_render_fn={}",
            //     render_fn.is_some()
            // );
        }
        let element_type = match type_id_boxed {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(Self::build_text_data(info, &props))
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Svg => {
                if let Some(info) = element.svg_render_info() {
                    ElementType::Svg(SvgData {
                        source: info.source,
                        tint: info.tint,
                        fill: info.fill,
                        stroke: info.stroke,
                        stroke_width: info.stroke_width,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Image => {
                if let Some(info) = element.image_render_info() {
                    ElementType::Image(ImageData {
                        source: info.source,
                        object_fit: info.object_fit,
                        object_position: info.object_position,
                        opacity: info.opacity,
                        border_radius: info.border_radius,
                        tint: info.tint,
                        filter: info.filter,
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                        placeholder_image: info.placeholder_image,
                        fade_duration_ms: info.fade_duration_ms,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
                is_static: element.is_static_canvas(),
            }),
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div, // Motion is a transparent container
        };

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Store per-node hashes for incremental update detection
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        let topology_hash = DivHash::compute_topology_tree(element);
        self.node_hashes
            .insert(node_id, (own_hash, tree_hash, topology_hash));

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            let stable_id = self.stable_id_or_warn(node_id);
            self.handler_registry.register(stable_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
            if element.viewport_cull() {
                self.viewport_cull_scrolls.insert(node_id);
            }
        }

        // Store motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Register layout animation config if element wants animated layout transitions
        if let Some(config) = element.layout_animation_config() {
            tracing::debug!(
                "collect_render_props_boxed: registered layout animation config for {:?}",
                node_id
            );
            self.layout_animation_configs.insert(node_id, config);
        }

        // Register visual animation config for new FLIP-style system
        if let Some(config) = element.visual_animation_config() {
            tracing::trace!(
                "[VISUAL_ANIM] collect_render_props_boxed: registering config for {:?}, key={:?}",
                node_id,
                config.key
            );
            self.register_visual_animation_config(node_id, config);
        }

        // Element IDs are pre-registered by `register_element_ids_walk`
        // before mint runs, so mint reads consistent `widget_key`s on
        // every pass. Re-registering here would just trip the duplicate
        // warning — skip.

        // Register CSS classes for complex selector matching
        let classes = element.element_classes();
        if !classes.is_empty() {
            self.element_registry
                .register_classes(node_id, classes.to_vec());
        }

        // Register semantic element type for CSS type selector matching
        if let Some(type_name) = element.semantic_type_name() {
            self.element_registry
                .register_element_type(node_id, type_name);
        }

        // Bind ScrollRef if present (for scroll containers)
        if let Some(scroll_ref) = element.bound_scroll_ref() {
            self.register_scroll_ref(node_id, scroll_ref);
        }

        // Get child node IDs from the layout tree
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Debug: warn on mismatch (in collect_render_props_boxed)
        if child_node_ids.len() != child_builders.len() {
            tracing::warn!(
                "collect_render_props_boxed: node {:?} has {} layout children but {} builder children",
                node_id,
                child_node_ids.len(),
                child_builders.len()
            );
        }

        let total_children = child_node_ids.len();

        // Check if this is a Motion container
        let is_motion = element.element_type_id() == ElementTypeId::Motion;
        // Get stable ID from Motion container (for overlay animations that survive tree rebuilds)
        let motion_stable_id = if is_motion {
            element.motion_stable_id().map(|s| s.to_string())
        } else {
            None
        };
        // Get replay, suspended, and exiting flags from Motion container
        let motion_should_replay = if is_motion {
            element.motion_should_replay()
        } else {
            false
        };
        let motion_is_suspended = if is_motion {
            element.motion_is_suspended()
        } else {
            false
        };
        // DEPRECATED: motion_is_exiting is no longer used for triggering exit.
        // Motion exit is now triggered explicitly via MotionHandle.exit().
        // This field is kept for backwards compatibility but always false.
        #[allow(deprecated)]
        let motion_is_exiting = if is_motion {
            element.motion_is_exiting()
        } else {
            false
        };
        // Get on_ready callback from Motion container for suspended animations
        let motion_on_ready_callback = if is_motion {
            element.motion_on_ready_callback()
        } else {
            None
        };

        // Match children by index (they were built in order)
        for (index, (child_builder, &child_node_id)) in
            child_builders.iter().zip(child_node_ids.iter()).enumerate()
        {
            // Register parent-child relationship and child index
            self.element_registry
                .register_parent(child_node_id, node_id);
            self.element_registry
                .register_child_index(child_node_id, index, total_children);

            // If parent is Motion, propagate motion animation to child
            if is_motion {
                if let Some(motion_config) = element.motion_animation_for_child(index) {
                    // Append child index to stable key for unique stagger animations
                    let child_stable_id = motion_stable_id
                        .as_ref()
                        .map(|key| format!("{}:child:{}", key, index));
                    self.collect_render_props_boxed_with_motion(
                        child_builder.as_ref(),
                        child_node_id,
                        Some(motion_config),
                        child_stable_id,
                        motion_should_replay,
                        motion_is_suspended,
                        motion_is_exiting,
                        motion_on_ready_callback.clone(),
                    );
                    continue;
                }
            }
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }

    /// Collect render props with motion animation config from parent
    #[allow(deprecated, clippy::too_many_arguments)]
    fn collect_render_props_boxed_with_motion(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
        motion_config: Option<crate::element::MotionAnimation>,
        motion_stable_id: Option<String>,
        motion_should_replay: bool,
        motion_is_suspended: bool,
        motion_is_exiting: bool,
        motion_on_ready_callback: Option<
            std::sync::Arc<dyn Fn(crate::element::ElementBounds) + Send + Sync>,
        >,
    ) {
        let mut props = element.render_props();
        props.node_id = Some(node_id);

        // Motion config from parent takes precedence
        if motion_config.is_some() {
            props.motion = motion_config;
            props.motion_stable_id = motion_stable_id.clone();
            props.motion_should_replay = motion_should_replay;
            props.motion_is_suspended = motion_is_suspended;
            props.motion_on_ready_callback = motion_on_ready_callback;
            // DEPRECATED: motion_is_exiting is no longer used for triggering exit.
            // Motion exit is now triggered explicitly via MotionHandle.exit().
            props.motion_is_exiting = motion_is_exiting;

            // Queue replay with the CHILD's stable key (includes :child:N suffix)
            // This ensures replay uses the same key as initialize_motion_animations
            if motion_should_replay {
                if let Some(ref key) = motion_stable_id {
                    crate::render_state::queue_global_motion_replay(key.clone());
                }
            }
        } else {
            // Apply base CSS styles and animation from stylesheet if element has an ID
            if let Some(ref stylesheet) = self.stylesheet {
                if let Some(id) = element.element_id() {
                    // Apply base styles (background, opacity, border-radius, etc.)
                    if let Some(base_style) = stylesheet.get(id) {
                        Self::apply_element_style_to_props(&mut props, base_style);
                    }
                    // Apply CSS animation (only if no motion animation is already set)
                    if props.motion.is_none() {
                        if let Some(motion) = stylesheet.resolve_animation(id) {
                            props.motion = Some(motion);
                        }
                    }
                }
            }
        }

        // Inherit CSS text properties from parent (text-decoration, white-space, etc.)
        self.inherit_text_props_from_parent(&mut props, node_id);

        // Use the element_type_id to determine type
        let element_type = match element.element_type_id() {
            ElementTypeId::Text => {
                if let Some(info) = element.text_render_info() {
                    ElementType::Text(Self::build_text_data(info, &props))
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Svg => {
                if let Some(info) = element.svg_render_info() {
                    ElementType::Svg(SvgData {
                        source: info.source,
                        tint: info.tint,
                        fill: info.fill,
                        stroke: info.stroke,
                        stroke_width: info.stroke_width,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Image => {
                if let Some(info) = element.image_render_info() {
                    ElementType::Image(ImageData {
                        source: info.source,
                        object_fit: info.object_fit,
                        object_position: info.object_position,
                        opacity: info.opacity,
                        border_radius: info.border_radius,
                        tint: info.tint,
                        filter: info.filter,
                        loading_strategy: info.loading_strategy,
                        placeholder_type: info.placeholder_type,
                        placeholder_color: info.placeholder_color,
                        placeholder_image: info.placeholder_image,
                        fade_duration_ms: info.fade_duration_ms,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Canvas => ElementType::Canvas(CanvasData {
                render_fn: element.canvas_render_info(),
                is_static: element.is_static_canvas(),
            }),
            ElementTypeId::StyledText => {
                if let Some(info) = element.styled_text_render_info() {
                    ElementType::StyledText(StyledTextData {
                        content: info.content,
                        spans: info
                            .spans
                            .into_iter()
                            .map(|s| StyledTextSpan {
                                start: s.start,
                                end: s.end,
                                color: s.color,
                                bold: s.bold,
                                italic: s.italic,
                                underline: s.underline,
                                strikethrough: s.strikethrough,
                                link_url: s.link_url,
                            })
                            .collect(),
                        default_color: info.default_color,
                        font_size: info.font_size,
                        align: info.align,
                        v_align: info.v_align,
                        font_family: info.font_family,
                        line_height: info.line_height,
                        weight: info.weight,
                        italic: info.italic,
                        ascender: info.ascender,
                    })
                } else {
                    ElementType::Div
                }
            }
            ElementTypeId::Div => ElementType::Div,
            ElementTypeId::Motion => ElementType::Div,
        };

        self.render_nodes.insert(
            node_id,
            RenderNode {
                props,
                element_type,
            },
        );

        // Store per-node hashes for incremental update detection
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        let topology_hash = DivHash::compute_topology_tree(element);
        self.node_hashes
            .insert(node_id, (own_hash, tree_hash, topology_hash));

        // Register event handlers if present
        if let Some(handlers) = element.event_handlers() {
            let stable_id = self.stable_id_or_warn(node_id);
            self.handler_registry.register(stable_id, handlers.clone());
        }

        // Store scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
            if element.viewport_cull() {
                self.viewport_cull_scrolls.insert(node_id);
            }
        }

        // Store motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Register layout animation config if element wants animated layout transitions
        if let Some(config) = element.layout_animation_config() {
            self.layout_animation_configs.insert(node_id, config);
        }

        // Register visual animation config for new FLIP-style system
        if let Some(config) = element.visual_animation_config() {
            self.register_visual_animation_config(node_id, config);
        }

        // Element IDs are pre-registered by `register_element_ids_walk`
        // before mint runs, so mint reads consistent `widget_key`s on
        // every pass. Re-registering here would just trip the duplicate
        // warning — skip.

        // Register CSS classes for complex selector matching
        let classes = element.element_classes();
        if !classes.is_empty() {
            self.element_registry
                .register_classes(node_id, classes.to_vec());
        }

        // Register semantic element type for CSS type selector matching
        if let Some(type_name) = element.semantic_type_name() {
            self.element_registry
                .register_element_type(node_id, type_name);
        }

        // Bind ScrollRef if present (for scroll containers)
        if let Some(scroll_ref) = element.bound_scroll_ref() {
            self.register_scroll_ref(node_id, scroll_ref);
        }

        // Recursively process children (without motion - motion only applies to direct children)
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();
        let total_children = child_node_ids.len();

        for (index, (child_builder, &child_node_id)) in
            child_builders.iter().zip(child_node_ids.iter()).enumerate()
        {
            self.element_registry
                .register_parent(child_node_id, node_id);
            self.element_registry
                .register_child_index(child_node_id, index, total_children);
            self.collect_render_props_boxed(child_builder.as_ref(), child_node_id);
        }
    }
}
