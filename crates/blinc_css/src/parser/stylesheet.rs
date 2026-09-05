//! The parsed stylesheet and the active-stylesheet slot.
//!
//! A [`Stylesheet`] holds simple `#id` and `.class` rules in hash maps for
//! O(1) lookup, complex selector chains in a list that is walked on match,
//! plus `:root` variables, `@keyframes` and `@flow` definitions.
//!
//! One stylesheet at a time can be installed as the process-wide active
//! sheet, which is how widgets reach theme rules without threading a
//! reference through every call.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use blinc_core::FlowGraph;
use nom::Finish;

use crate::element_style::ElementStyle;
use crate::parser::*;

/// Known SVG shape tag names for CSS tag-name selectors targeting SVG sub-elements.
pub const SVG_TAG_NAMES: &[&str] = &[
    "path", "circle", "rect", "ellipse", "line", "polygon", "polyline", "g",
];

/// A parsed stylesheet containing styles keyed by element ID
#[derive(Clone, Default, Debug)]
pub struct Stylesheet {
    /// Simple rules: styles keyed by selector (id or id:state) for O(1) lookup
    styles: HashMap<String, ElementStyle>,
    /// Class-based rules: styles keyed by class name (or class:state) for O(1) lookup
    /// Populated alongside `complex_rules` for simple `.class` and `.class:state` selectors.
    class_styles: HashMap<String, ElementStyle>,
    /// Complex selector rules (class selectors, combinators, structural pseudos)
    complex_rules: Vec<(ComplexSelector, ElementStyle)>,
    /// CSS custom properties (variables) defined in :root
    variables: HashMap<String, String>,
    /// Keyframe animations defined with @keyframes
    keyframes: HashMap<String, CssKeyframes>,
    /// Flow DAGs defined with @flow
    flows: HashMap<String, FlowGraph>,
    /// Identifiers (ids without `#`, classes without `.`) that appear in
    /// a compound containing `:hover`. Computed at parse time by
    /// [`Self::index_hover_participants`]; consulted by the windowed
    /// runner to gate `invalidate_render_cache` on POINTER_ENTER /
    /// POINTER_LEAVE events. Pre-fix every hover-changing pointer move
    /// invalidated the compositor's static cache regardless of whether
    /// the entering / leaving element had any `:hover` styling, which
    /// made mouse-over-non-hoverable-content drop the fast path on
    /// every frame.
    hover_participants: std::collections::HashSet<String>,
}

impl Stylesheet {
    /// Create an empty stylesheet
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse CSS text into a stylesheet with full error collection
    ///
    /// This is the recommended method for parsing CSS as it collects all
    /// errors and warnings during parsing, allowing you to report them
    /// to users in a human-readable format.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = "#card { opacity: 0.5; unknown: value; }";
    /// let result = Stylesheet::parse_with_errors(css);
    ///
    /// // Print any warnings to stderr
    /// result.print_diagnostics();
    ///
    /// // Use the stylesheet (partial results are still available)
    /// let stylesheet = result.stylesheet;
    /// ```
    pub fn parse_with_errors(css: &str) -> CssParseResult {
        let mut errors: Vec<ParseError> = Vec::new();
        let initial_vars = HashMap::new();

        match parse_stylesheet_with_errors(css, &mut errors, &initial_vars).finish() {
            Ok((remaining, parsed)) => {
                // Warn if there's unparsed content
                let remaining = remaining.trim();
                if !remaining.is_empty() {
                    let (line, column, fragment) = calculate_position(css, remaining);
                    errors.push(ParseError {
                        severity: Severity::Warning,
                        message: format!("Unparsed content remaining ({} chars)", remaining.len()),
                        line,
                        column,
                        fragment,
                        contexts: vec![],
                        property: None,
                        value: None,
                    });
                }

                let mut stylesheet = Stylesheet::new();
                stylesheet.variables = parsed.variables;
                for (id, style) in parsed.rules {
                    stylesheet.styles.insert(id, style);
                }
                stylesheet.complex_rules = parsed.complex_rules;
                stylesheet.index_class_styles();
                stylesheet.index_hover_participants();
                for keyframes in parsed.keyframes {
                    stylesheet
                        .keyframes
                        .insert(keyframes.name.clone(), keyframes);
                }
                for flow in parsed.flows {
                    stylesheet.flows.insert(flow.name.clone(), flow);
                }

                CssParseResult { stylesheet, errors }
            }
            Err(e) => {
                let parse_error = ParseError::from_verbose(css, e);
                errors.push(parse_error);

                CssParseResult {
                    stylesheet: Stylesheet::new(),
                    errors,
                }
            }
        }
    }

    /// Parse CSS with pre-seeded external variables (e.g. from theme or prior stylesheets).
    ///
    /// `var(--name)` references in the CSS will resolve against both `:root` variables
    /// defined in this CSS and the provided external variables. CSS-defined variables
    /// take precedence over external ones.
    #[allow(clippy::result_large_err)]
    pub fn parse_with_variables(
        css: &str,
        external_vars: &HashMap<String, String>,
    ) -> Result<Self, ParseError> {
        let result = Self::parse_with_errors_and_variables(css, external_vars);
        result.log_diagnostics();
        if result.has_errors() {
            Err(result
                .errors
                .into_iter()
                .find(|e| e.severity == Severity::Error)
                .unwrap())
        } else {
            Ok(result.stylesheet)
        }
    }

    /// Parse CSS with external variables and full error collection.
    pub fn parse_with_errors_and_variables(
        css: &str,
        external_vars: &HashMap<String, String>,
    ) -> CssParseResult {
        let mut errors: Vec<ParseError> = Vec::new();

        match parse_stylesheet_with_errors(css, &mut errors, external_vars).finish() {
            Ok((remaining, parsed)) => {
                let remaining = remaining.trim();
                if !remaining.is_empty() {
                    let (line, column, fragment) = calculate_position(css, remaining);
                    errors.push(ParseError {
                        severity: Severity::Warning,
                        message: format!("Unparsed content remaining ({} chars)", remaining.len()),
                        line,
                        column,
                        fragment,
                        contexts: vec![],
                        property: None,
                        value: None,
                    });
                }

                let mut stylesheet = Stylesheet::new();
                stylesheet.variables = parsed.variables;
                for (id, style) in parsed.rules {
                    stylesheet.styles.insert(id, style);
                }
                stylesheet.complex_rules = parsed.complex_rules;
                stylesheet.index_class_styles();
                stylesheet.index_hover_participants();
                for keyframes in parsed.keyframes {
                    stylesheet
                        .keyframes
                        .insert(keyframes.name.clone(), keyframes);
                }
                for flow in parsed.flows {
                    stylesheet.flows.insert(flow.name.clone(), flow);
                }

                CssParseResult { stylesheet, errors }
            }
            Err(e) => {
                let parse_error = ParseError::from_verbose(css, e);
                errors.push(parse_error);

                CssParseResult {
                    stylesheet: Stylesheet::new(),
                    errors,
                }
            }
        }
    }

    /// Parse CSS text into a stylesheet
    ///
    /// Parse errors are logged via tracing at DEBUG level with full context.
    /// When parsing fails, an error is returned but the application can
    /// fall back to built-in theme styles.
    ///
    /// For full error collection, use `parse_with_errors()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = "#card { opacity: 0.5; }";
    /// let stylesheet = Stylesheet::parse(css)?;
    /// ```
    #[allow(clippy::result_large_err)]
    pub fn parse(css: &str) -> Result<Self, ParseError> {
        let result = Self::parse_with_errors(css);

        // Log all diagnostics via tracing
        result.log_diagnostics();

        if result.has_errors() {
            // Return the first error
            Err(result
                .errors
                .into_iter()
                .find(|e| e.severity == Severity::Error)
                .unwrap())
        } else {
            Ok(result.stylesheet)
        }
    }

    /// Parse CSS text, logging errors and returning an empty stylesheet on failure
    ///
    /// This is a convenience method for cases where you want to gracefully
    /// fall back to an empty stylesheet rather than handle errors explicitly.
    pub fn parse_or_empty(css: &str) -> Self {
        Self::parse(css).unwrap_or_default()
    }

    /// Insert a style for an element ID (without the # prefix)
    ///
    /// This is the programmatic equivalent of parsing `#id { ... }` in CSS.
    /// If a style already exists for this ID, it is replaced.
    pub fn insert(&mut self, id: impl Into<String>, style: ElementStyle) {
        self.styles.insert(id.into(), style);
    }

    /// Insert a state-specific style for an element ID
    ///
    /// This is the programmatic equivalent of parsing `#id:hover { ... }` in CSS.
    pub fn insert_with_state(
        &mut self,
        id: impl Into<String>,
        state: ElementState,
        style: ElementStyle,
    ) {
        let id_str: String = id.into();
        let key = format!("{}:{}", id_str, state);
        self.styles.insert(key, style);
        // Keep the hover-participants index live with programmatic
        // inserts — without this, a runtime-added `#id:hover` style
        // wouldn't register as a hover-invalidation target and the
        // first pointer-enter on the element would silently skip the
        // cache invalidation we need.
        if state == ElementState::Hover {
            self.hover_participants.insert(id_str);
        }
    }

    /// Get a style by element ID (without the # prefix)
    ///
    /// Returns `None` if no style is defined for the given ID.
    pub fn get(&self, id: &str) -> Option<&ElementStyle> {
        self.styles.get(id)
    }

    /// Get a style by element ID and state
    ///
    /// Looks up `#id:state` in the stylesheet.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = "#button:hover { opacity: 0.8; }";
    /// let stylesheet = Stylesheet::parse(css)?;
    /// let hover_style = stylesheet.get_with_state("button", ElementState::Hover);
    /// ```
    pub fn get_with_state(&self, id: &str, state: ElementState) -> Option<&ElementStyle> {
        let key = format!("{}:{}", id, state);
        self.styles.get(&key)
    }

    /// Get a base style by CSS class name (without the `.` prefix)
    ///
    /// Returns `None` if no simple `.class { ... }` rule exists in the stylesheet.
    /// Only populated for simple single-class selectors (not combinators).
    pub fn get_class(&self, class: &str) -> Option<&ElementStyle> {
        self.class_styles.get(class)
    }

    /// Get a class style with state (e.g., `.class:hover`)
    ///
    /// Looks up `class:state` in the class_styles HashMap.
    pub fn get_class_with_state(&self, class: &str, state: ElementState) -> Option<&ElementStyle> {
        let key = format!("{}:{}", class, state);
        self.class_styles.get(&key)
    }

    /// Get the ::placeholder pseudo-element style for an element ID
    ///
    /// Looks up `#id::placeholder` in the stylesheet. The `color` property
    /// in a `::placeholder` block maps to `text_color` on the returned style,
    /// and `placeholder-color` maps directly.
    pub fn get_placeholder_style(&self, id: &str) -> Option<&ElementStyle> {
        let key = format!("{}::placeholder", id);
        self.styles.get(&key)
    }

    /// Whether the stylesheet contains any rule keyed on a pointer state
    /// (`:hover` or `:active`).
    ///
    /// Used by the windowed app's mouse-move path to skip hit testing
    /// entirely when no element has a pointer-driven style or handler —
    /// turns "static UI with no interaction" into a true zero-CPU idle
    /// even while the cursor is moving over the window.
    ///
    /// Walks the simple-rule and class-rule HashMaps for keys with the
    /// `:hover` / `:active` suffix that the parser produces, plus the
    /// complex-selector list for any compound that carries a state
    /// pseudo-class. Cheap enough to call per-frame on a static stylesheet
    /// (workspaces with very large CSS should still consider caching it
    /// alongside the parse result).
    pub fn has_pointer_state_rules(&self) -> bool {
        let suffix_match = |key: &str| key.ends_with(":hover") || key.ends_with(":active");
        if self.styles.keys().any(|k| suffix_match(k)) {
            return true;
        }
        if self.class_styles.keys().any(|k| suffix_match(k)) {
            return true;
        }
        self.complex_rules.iter().any(|(sel, _)| {
            sel.segments.iter().any(|(compound, _)| {
                compound.parts.iter().any(|p| {
                    matches!(
                        p,
                        SelectorPart::State(ElementState::Hover)
                            | SelectorPart::State(ElementState::Active)
                    )
                })
            })
        })
    }

    /// Get all styles for an element, including state variants
    ///
    /// Returns a tuple of (base_style, state_styles) where state_styles is a Vec
    /// of (ElementState, &ElementStyle) pairs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     #button { background: blue; }
    ///     #button:hover { background: lightblue; }
    ///     #button:active { background: darkblue; }
    /// "#;
    /// let stylesheet = Stylesheet::parse(css)?;
    /// let (base, states) = stylesheet.get_all_states("button");
    /// ```
    pub fn get_all_states(
        &self,
        id: &str,
    ) -> (Option<&ElementStyle>, Vec<(ElementState, &ElementStyle)>) {
        let base = self.styles.get(id);

        let mut state_styles = Vec::new();
        for state in [
            ElementState::Hover,
            ElementState::Active,
            ElementState::Focus,
            ElementState::Disabled,
            ElementState::Checked,
        ] {
            let key = format!("{}:{}", id, state);
            if let Some(style) = self.styles.get(&key) {
                state_styles.push((state, style));
            }
        }

        (base, state_styles)
    }

    /// Check if a style exists for the given ID
    pub fn contains(&self, id: &str) -> bool {
        self.styles.contains_key(id)
    }

    /// Check if a style exists for the given ID and state
    pub fn contains_with_state(&self, id: &str, state: ElementState) -> bool {
        let key = format!("{}:{}", id, state);
        self.styles.contains_key(&key)
    }

    /// Get all style IDs in the stylesheet
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.styles.keys().map(|s| s.as_str())
    }

    /// Get the number of styles in the stylesheet
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    /// Check if the stylesheet is empty
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty() && self.complex_rules.is_empty()
    }

    /// Get all complex selector rules
    pub fn complex_rules(&self) -> &[(ComplexSelector, ElementStyle)] {
        &self.complex_rules
    }

    /// Check if there are any complex rules that involve state changes
    pub fn has_complex_state_rules(&self) -> bool {
        self.complex_rules.iter().any(|(sel, _)| sel.has_state())
    }

    /// Returns complex rules whose rightmost compound selector targets an SVG tag name.
    ///
    /// Each entry returns: (tag_name, ancestor_segments if any, style).
    /// For a bare `path { fill: red; }`, ancestor_segments is empty.
    /// For `#my-svg path { fill: red; }`, ancestor_segments contains the `#my-svg` part.
    #[allow(clippy::type_complexity)]
    pub fn svg_tag_rules(
        &self,
    ) -> Vec<(
        &str,
        &[(CompoundSelector, Option<Combinator>)],
        &ElementStyle,
    )> {
        let mut results = Vec::new();
        for (selector, style) in &self.complex_rules {
            if let Some((target_compound, _)) = selector.segments.last() {
                // Check if the rightmost compound has a Type that matches an SVG tag name
                for part in &target_compound.parts {
                    if let SelectorPart::Type(name) = part {
                        if SVG_TAG_NAMES.contains(&name.as_str()) {
                            // Ancestor segments = everything except the last
                            let ancestors = if selector.segments.len() > 1 {
                                &selector.segments[..selector.segments.len() - 1]
                            } else {
                                &[]
                            };
                            results.push((name.as_str(), ancestors, style));
                            break;
                        }
                    }
                }
            }
        }
        results
    }

    /// Build the [`Self::hover_participants`] set so the windowed runner
    /// can cheaply decide whether a POINTER_ENTER / POINTER_LEAVE event
    /// needs to invalidate the compositor cache.
    ///
    /// Records every id and class that appears in a compound selector
    /// containing `State(Hover)` — that is, any rule whose evaluation
    /// depends on the hover state of that identifier. Walks all three
    /// rule stores so simple `#id:hover` rules in `styles`, simple
    /// `.class:hover` rules in `class_styles`, and complex multi-segment
    /// selectors in `complex_rules` are all covered. Identifiers stripped
    /// of the leading `#`/`.` so they match what the registry returns.
    fn index_hover_participants(&mut self) {
        self.hover_participants.clear();

        // Simple `#id:hover` rules are stored in `styles` with the
        // composite key `"id:hover"`. Match by suffix and pull out
        // the id half.
        for key in self.styles.keys() {
            if let Some(id) = key.strip_suffix(":hover") {
                self.hover_participants.insert(id.to_string());
            }
        }
        // Simple `.class:hover` rules are stored in `class_styles` the
        // same way.
        for key in self.class_styles.keys() {
            if let Some(class) = key.strip_suffix(":hover") {
                self.hover_participants.insert(class.to_string());
            }
        }
        // Complex rules: for each compound containing `State(Hover)`,
        // record every id / class in that compound. The hover applies
        // to that segment's identifier, so any pointer-enter / leave
        // on that element triggers the rule's evaluation.
        for (selector, _) in &self.complex_rules {
            for (compound, _) in &selector.segments {
                let has_hover = compound
                    .parts
                    .iter()
                    .any(|p| matches!(p, SelectorPart::State(ElementState::Hover)));
                if !has_hover {
                    continue;
                }
                for part in &compound.parts {
                    match part {
                        SelectorPart::Id(id) => {
                            self.hover_participants.insert(id.clone());
                        }
                        SelectorPart::Class(class) => {
                            self.hover_participants.insert(class.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Does an element with this id / class identifier participate in
    /// any `:hover` styling? Cheap HashSet lookup against a set
    /// precomputed at parse time (see the internal
    /// `index_hover_participants` helper).
    pub fn participates_in_hover(&self, ident: &str) -> bool {
        self.hover_participants.contains(ident)
    }

    /// Index simple class selectors from complex_rules into class_styles for O(1) lookup.
    ///
    /// A "simple class selector" is a ComplexSelector with exactly one segment
    /// whose CompoundSelector has exactly one Class part, or one Class + one State part.
    fn index_class_styles(&mut self) {
        for (selector, style) in &self.complex_rules {
            if !selector.is_simple() {
                continue;
            }
            let compound = &selector.segments[0].0;
            let parts = &compound.parts;

            match parts.len() {
                1 => {
                    // Single .class selector
                    if let SelectorPart::Class(class_name) = &parts[0] {
                        self.class_styles.insert(class_name.clone(), style.clone());
                    }
                }
                2 => {
                    // .class:state selector
                    let (class_part, state_part) = (&parts[0], &parts[1]);
                    if let (SelectorPart::Class(class_name), SelectorPart::State(state)) =
                        (class_part, state_part)
                    {
                        let key = format!("{}:{}", class_name, state);
                        self.class_styles.insert(key, style.clone());
                    }
                }
                _ => {}
            }
        }
    }

    /// Merge another stylesheet into this one (cascade — later rules override earlier)
    ///
    /// This follows CSS cascade rules: styles from `other` override matching
    /// styles in `self`. Variables and keyframes are also merged.
    pub fn merge(&mut self, other: Stylesheet) {
        for (key, style) in other.styles {
            self.styles.insert(key, style);
        }
        self.complex_rules.extend(other.complex_rules);
        for (key, value) in other.variables {
            self.variables.insert(key, value);
        }
        for (key, kf) in other.keyframes {
            self.keyframes.insert(key, kf);
        }
        for (key, style) in other.class_styles {
            self.class_styles.insert(key, style);
        }
        // Re-index now that simple + class + complex stores have the
        // merged content. `other.hover_participants` is dropped — the
        // re-index walks the merged stores directly so we don't have
        // to reason about whether an `id:hover` key in one stylesheet
        // got cascade-overridden by an `id` key in the other.
        self.index_hover_participants();
    }

    /// Load and parse a `.css` file from disk
    #[allow(clippy::result_large_err)]
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, ParseError> {
        let path = path.as_ref();
        let css = std::fs::read_to_string(path).map_err(|e| {
            ParseError::new(
                Severity::Error,
                format!("Failed to read CSS file '{}': {}", path.display(), e),
                0,
                0,
            )
        })?;
        Self::parse(&css)
    }

    // =========================================================================
    // CSS Variables (Custom Properties)
    // =========================================================================

    /// Get a CSS variable value by name (without the -- prefix)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = ":root { --card-bg: #ffffff; }";
    /// let stylesheet = Stylesheet::parse(css)?;
    /// assert_eq!(stylesheet.get_variable("card-bg"), Some("#ffffff"));
    /// ```
    pub fn get_variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|s| s.as_str())
    }

    /// Set a CSS variable (useful for runtime overrides)
    ///
    /// # Example
    ///
    /// ```ignore
    /// stylesheet.set_variable("primary-color", "#FF0000");
    /// ```
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(name.into(), value.into());
    }

    /// Get all variable names
    pub fn variable_names(&self) -> impl Iterator<Item = &str> {
        self.variables.keys().map(|s| s.as_str())
    }

    /// Get the number of variables defined
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }

    /// Get all CSS variables as a reference to the internal map
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    // =========================================================================
    // Keyframe Animations
    // =========================================================================

    /// Get a keyframe animation by name
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes fade-in {
    ///         from { opacity: 0; }
    ///         to { opacity: 1; }
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// if let Some(keyframes) = stylesheet.get_keyframes("fade-in") {
    ///     let animation = keyframes.to_enter_animation(300);
    /// }
    /// ```
    pub fn get_keyframes(&self, name: &str) -> Option<&CssKeyframes> {
        self.keyframes.get(name)
    }

    /// Check if keyframes exist with the given name
    pub fn contains_keyframes(&self, name: &str) -> bool {
        self.keyframes.contains_key(name)
    }

    /// Get all keyframe animation names
    pub fn keyframe_names(&self) -> impl Iterator<Item = &str> {
        self.keyframes.keys().map(|s| s.as_str())
    }

    /// Get the number of keyframe animations defined
    pub fn keyframe_count(&self) -> usize {
        self.keyframes.len()
    }

    /// Add a keyframe animation to the stylesheet
    pub fn add_keyframes(&mut self, keyframes: CssKeyframes) {
        self.keyframes.insert(keyframes.name.clone(), keyframes);
    }

    // =========================================================================
    // Flow DAGs (@flow)
    // =========================================================================

    /// Look up a flow DAG by name
    pub fn get_flow(&self, name: &str) -> Option<&FlowGraph> {
        self.flows.get(name)
    }

    /// Check if a flow exists with the given name
    pub fn contains_flow(&self, name: &str) -> bool {
        self.flows.contains_key(name)
    }

    /// Get all flow names
    pub fn flow_names(&self) -> impl Iterator<Item = &str> {
        self.flows.keys().map(|s| s.as_str())
    }

    /// Get the number of flows defined
    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }

    /// Add a flow DAG to the stylesheet
    pub fn add_flow(&mut self, flow: FlowGraph) {
        self.flows.insert(flow.name.clone(), flow);
    }

    // =========================================================================
    // Resolved Animations
    // =========================================================================

    /// Resolve a full motion animation for an element by its ID
    ///
    /// This combines:
    /// 1. The element's `animation:` property (from its style)
    /// 2. The referenced `@keyframes` definition
    ///
    /// Returns `Some(MotionAnimation)` if the element has an animation configured
    /// and the keyframes exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes fade-in {
    ///         from { opacity: 0; transform: translateY(20px); }
    ///         to { opacity: 1; transform: translateY(0); }
    ///     }
    ///     #card {
    ///         animation: fade-in 300ms ease-out;
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    ///
    /// if let Some(motion) = stylesheet.resolve_animation("card") {
    ///     // Apply motion animation to the element
    /// }
    /// ```
    pub fn resolve_animation(&self, id: &str) -> Option<crate::motion::MotionAnimation> {
        // Get the element's style
        let style = self.get(id)?;

        // Check if it has an animation property
        let anim_config = style.animation.as_ref()?;

        // Look up the keyframes by name
        let keyframes = self.get_keyframes(&anim_config.name)?;

        // Convert to MotionAnimation
        // For enter animation, use the configured duration
        // For exit animation, use the same duration (can be customized later)
        let mut motion =
            keyframes.to_motion_animation(anim_config.duration_ms, anim_config.duration_ms);

        // Apply delay from config
        motion.enter_delay_ms = anim_config.delay_ms;

        Some(motion)
    }

    /// Resolve animation for an element considering its current state
    ///
    /// This checks both the base style and state-specific styles for animations.
    pub fn resolve_animation_with_state(
        &self,
        id: &str,
        state: ElementState,
    ) -> Option<crate::motion::MotionAnimation> {
        // First try state-specific animation
        if let Some(style) = self.get_with_state(id, state) {
            if let Some(anim_config) = &style.animation {
                if let Some(keyframes) = self.get_keyframes(&anim_config.name) {
                    let mut motion = keyframes
                        .to_motion_animation(anim_config.duration_ms, anim_config.duration_ms);
                    motion.enter_delay_ms = anim_config.delay_ms;
                    return Some(motion);
                }
            }
        }

        // Fall back to base animation
        self.resolve_animation(id)
    }

    /// Resolve CSS animation to full MultiKeyframeAnimation with all keyframes preserved
    ///
    /// Unlike `resolve_animation()` which only captures first/last keyframes for simple
    /// enter/exit animations, this method preserves ALL keyframes for complex multi-step
    /// animations like pulse, bounce, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes pulse {
    ///         0%, 100% { opacity: 1; transform: scale(1); }
    ///         50% { opacity: 0.8; transform: scale(1.05); }
    ///     }
    ///     #button { animation: pulse 1000ms ease-in-out infinite; }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    /// if let Some(mut anim) = stylesheet.resolve_keyframe_animation("button") {
    ///     anim.start();
    ///     // Animation will interpolate through all 3 keyframes
    /// }
    /// ```
    pub fn resolve_keyframe_animation(
        &self,
        id: &str,
    ) -> Option<blinc_animation::MultiKeyframeAnimation> {
        let style = self.get(id)?;
        let anim_config = style.animation.as_ref()?;
        let keyframes = self.get_keyframes(&anim_config.name)?;

        let mut anim = keyframes
            .to_multi_keyframe_animation(anim_config.duration_ms, anim_config.timing.to_easing());

        // Apply CssAnimation configuration
        let iterations = if anim_config.iteration_count == 0 {
            -1 // infinite
        } else {
            anim_config.iteration_count as i32
        };
        anim.set_iterations(iterations);
        anim.set_delay(anim_config.delay_ms);
        anim.set_direction(anim_config.direction.to_play_direction());
        anim.set_fill_mode(anim_config.fill_mode.to_fill_mode());

        // Handle AlternateReverse by starting in reverse
        if anim_config.direction.starts_reversed() {
            anim.set_reversed(true);
        }

        Some(anim)
    }

    /// Resolve keyframe animation for an element considering its current state
    ///
    /// This checks both the base style and state-specific styles for animations,
    /// returning a full MultiKeyframeAnimation with all keyframes preserved.
    pub fn resolve_keyframe_animation_with_state(
        &self,
        id: &str,
        state: ElementState,
    ) -> Option<blinc_animation::MultiKeyframeAnimation> {
        // First try state-specific animation
        if let Some(style) = self.get_with_state(id, state) {
            if let Some(anim_config) = &style.animation {
                if let Some(keyframes) = self.get_keyframes(&anim_config.name) {
                    let mut anim = keyframes.to_multi_keyframe_animation(
                        anim_config.duration_ms,
                        anim_config.timing.to_easing(),
                    );

                    let iterations = if anim_config.iteration_count == 0 {
                        -1
                    } else {
                        anim_config.iteration_count as i32
                    };
                    anim.set_iterations(iterations);
                    anim.set_delay(anim_config.delay_ms);
                    anim.set_direction(anim_config.direction.to_play_direction());
                    anim.set_fill_mode(anim_config.fill_mode.to_fill_mode());

                    if anim_config.direction.starts_reversed() {
                        anim.set_reversed(true);
                    }

                    return Some(anim);
                }
            }
        }

        // Fall back to base animation
        self.resolve_keyframe_animation(id)
    }
}

// ============================================================================
// Global Active Stylesheet
// ============================================================================

pub(crate) static ACTIVE_STYLESHEET: RwLock<Option<Arc<Stylesheet>>> = RwLock::new(None);

/// Set the active stylesheet for form widget CSS override resolution.
///
/// Called automatically when `set_stylesheet_arc()` is invoked on the RenderTree.
/// This allows TextInput/TextArea state_callbacks to query the current stylesheet
/// without needing a direct reference to the RenderTree.
pub fn set_active_stylesheet(stylesheet: Arc<Stylesheet>) {
    if let Ok(mut guard) = ACTIVE_STYLESHEET.write() {
        *guard = Some(stylesheet);
    }
}

/// Get the currently active stylesheet, if any.
///
/// Used by form widget state_callbacks to resolve CSS overrides.
pub fn active_stylesheet() -> Option<Arc<Stylesheet>> {
    ACTIVE_STYLESHEET.read().ok()?.clone()
}

/// Drop the global active stylesheet. Used by hot-reload's
/// `WindowedContext::reset_for_hot_reload` so the next `ctx.add_css`
/// call repopulates a fresh sheet — without this, stateful widgets
/// looking up CSS overrides during the rebuild would briefly see
/// stale rules from the pre-patch run.
pub fn clear_active_stylesheet() {
    if let Ok(mut guard) = ACTIVE_STYLESHEET.write() {
        *guard = None;
    }
}

// ============================================================================
// Nom Parsers with VerboseError for diagnostics
// ============================================================================
