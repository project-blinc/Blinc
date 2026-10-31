//! `animation` and `transition` declarations.
//!
//! These are the timing half of CSS motion: which named keyframes to run,
//! for how long, with what easing, delay, iteration count, direction and
//! fill mode. The keyframe geometry itself lives in [`super::keyframes`].

use crate::parser::*;

/// A single CSS transition specification
#[derive(Clone, Debug)]
pub struct CssTransition {
    /// Property name to transition (e.g. "opacity", "clip-path", "all")
    pub property: String,
    /// Duration in milliseconds
    pub duration_ms: u32,
    /// Timing function
    pub timing: AnimationTiming,
    /// Delay before starting in milliseconds
    pub delay_ms: u32,
}

/// Set of CSS transitions parsed from `transition:` property
#[derive(Clone, Debug, Default)]
pub struct CssTransitionSet {
    pub transitions: Vec<CssTransition>,
    /// The four longhand lists `transitions` is zipped from, so the longhands
    /// can arrive in any order and in any number; `None` is the initial value.
    lists: TransitionLists,
}

#[derive(Clone, Debug, Default)]
struct TransitionLists {
    properties: Option<Vec<String>>,
    durations: Option<Vec<u32>>,
    timings: Option<Vec<AnimationTiming>>,
    delays: Option<Vec<u32>>,
}

/// Which `transition-*` longhand a value belongs to.
#[derive(Clone, Copy)]
pub(crate) enum TransitionLonghand {
    Property,
    Duration,
    TimingFunction,
    Delay,
}

impl CssTransitionSet {
    pub(crate) fn from_entries(transitions: Vec<CssTransition>) -> Self {
        let lists = TransitionLists {
            properties: Some(transitions.iter().map(|t| t.property.clone()).collect()),
            durations: Some(transitions.iter().map(|t| t.duration_ms).collect()),
            timings: Some(transitions.iter().map(|t| t.timing).collect()),
            delays: Some(transitions.iter().map(|t| t.delay_ms).collect()),
        };
        Self { transitions, lists }
    }

    /// Set one longhand to a comma-separated list and re-zip the entries.
    /// The property list sets how many there are; the others repeat to fit.
    pub(crate) fn set_longhand(&mut self, which: TransitionLonghand, value: &str) -> bool {
        let items: Vec<String> = split_commas_respecting_parens(value)
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if items.is_empty() {
            return false;
        }
        match which {
            TransitionLonghand::Property => {
                self.lists.properties = Some(items.iter().map(|s| s.to_lowercase()).collect());
            }
            TransitionLonghand::Duration => {
                let times: Option<Vec<u32>> = items.iter().map(|s| parse_time_value(s)).collect();
                match times {
                    Some(t) => self.lists.durations = Some(t),
                    None => return false,
                }
            }
            TransitionLonghand::TimingFunction => {
                let timings: Option<Vec<AnimationTiming>> =
                    items.iter().map(|s| AnimationTiming::from_str(s)).collect();
                match timings {
                    Some(t) => self.lists.timings = Some(t),
                    None => return false,
                }
            }
            TransitionLonghand::Delay => {
                let times: Option<Vec<u32>> = items.iter().map(|s| parse_time_value(s)).collect();
                match times {
                    Some(t) => self.lists.delays = Some(t),
                    None => return false,
                }
            }
        }
        self.rezip();
        true
    }

    fn rezip(&mut self) {
        let lists = &self.lists;
        let properties = lists
            .properties
            .clone()
            .unwrap_or_else(|| vec!["all".into()]);
        self.transitions = if properties.len() == 1 && properties[0] == "none" {
            Vec::new()
        } else {
            let at = |i: usize, n: usize| i % n.max(1);
            properties
                .iter()
                .enumerate()
                .map(|(i, property)| CssTransition {
                    property: property.clone(),
                    duration_ms: lists
                        .durations
                        .as_deref()
                        .and_then(|d| d.get(at(i, d.len())))
                        .copied()
                        .unwrap_or(0),
                    timing: lists
                        .timings
                        .as_deref()
                        .and_then(|t| t.get(at(i, t.len())))
                        .copied()
                        .unwrap_or_default(),
                    delay_ms: lists
                        .delays
                        .as_deref()
                        .and_then(|d| d.get(at(i, d.len())))
                        .copied()
                        .unwrap_or(0),
                })
                .collect()
        };
    }

    /// The entry that governs `property`: the last one naming it, directly or
    /// through `all` or a shorthand or per-side name that covers it.
    pub fn get(&self, property: &str) -> Option<&CssTransition> {
        self.transitions
            .iter()
            .rev()
            .find(|t| t.property == "all" || covers(&t.property, property))
    }
}

/// Whether a transition named `spec` governs the property `prop`, which is
/// the name the engine animates it under.
fn covers(spec: &str, prop: &str) -> bool {
    if spec == prop {
        return true;
    }
    match spec {
        "background-color" | "background-image" => prop == "background",
        "border" => matches!(prop, "border-color" | "border-width"),
        "border-top-color" | "border-right-color" | "border-bottom-color" | "border-left-color" => {
            prop == "border-color"
        }
        "border-top-width" | "border-right-width" | "border-bottom-width" | "border-left-width" => {
            prop == "border-width"
        }
        "border-top-left-radius"
        | "border-top-right-radius"
        | "border-bottom-right-radius"
        | "border-bottom-left-radius" => prop == "border-radius",
        "outline" => prop.starts_with("outline-"),
        "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => prop == "padding",
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => prop == "margin",
        "row-gap" | "column-gap" => prop == "gap",
        "inset" => matches!(prop, "top" | "right" | "bottom" | "left"),
        "translate" | "scale" | "rotate" => prop == "transform",
        "flex" => matches!(prop, "flex-grow" | "flex-shrink"),
        _ => false,
    }
}

/// CSS animation configuration parsed from `animation:` property
#[derive(Clone, Debug)]
pub struct CssAnimation {
    /// Name of the @keyframes to use
    pub name: String,
    /// Duration in milliseconds
    pub duration_ms: u32,
    /// Timing function
    pub timing: AnimationTiming,
    /// Delay before starting in milliseconds
    pub delay_ms: u32,
    /// Number of iterations (0 = infinite)
    pub iteration_count: u32,
    /// Direction of animation
    pub direction: AnimationDirection,
    /// Fill mode
    pub fill_mode: AnimationFillMode,
}

impl Default for CssAnimation {
    fn default() -> Self {
        Self {
            name: String::new(),
            duration_ms: 0,
            timing: AnimationTiming::Ease,
            delay_ms: 0,
            iteration_count: 1,
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
        }
    }
}

/// Animation timing function
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AnimationTiming {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Literal `cubic-bezier(x1, y1, x2, y2)` — used when `var(--ease-X)`
    /// resolves to a custom curve (e.g. theme-supplied semantic easings
    /// like `--ease-state`, `--ease-spring`, `--ease-sheet`).
    CubicBezier(f32, f32, f32, f32),
}

impl Eq for AnimationTiming {}

impl AnimationTiming {
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        let trimmed = s.trim().to_lowercase();
        match trimmed.as_str() {
            "linear" => Some(AnimationTiming::Linear),
            "ease" => Some(AnimationTiming::Ease),
            "ease-in" => Some(AnimationTiming::EaseIn),
            "ease-out" => Some(AnimationTiming::EaseOut),
            "ease-in-out" => Some(AnimationTiming::EaseInOut),
            _ => parse_cubic_bezier(&trimmed),
        }
    }

    /// Convert CSS animation timing to blinc_animation Easing
    ///
    /// Uses the exact cubic-bezier curves from the CSS specification:
    /// - ease:        cubic-bezier(0.25, 0.1, 0.25, 1.0)
    /// - ease-in:     cubic-bezier(0.42, 0.0, 1.0, 1.0)
    /// - ease-out:    cubic-bezier(0.0, 0.0, 0.58, 1.0)
    /// - ease-in-out: cubic-bezier(0.42, 0.0, 0.58, 1.0)
    pub fn to_easing(&self) -> blinc_animation::Easing {
        use blinc_animation::Easing;
        match self {
            AnimationTiming::Linear => Easing::Linear,
            AnimationTiming::Ease => Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
            AnimationTiming::EaseIn => Easing::CubicBezier(0.42, 0.0, 1.0, 1.0),
            AnimationTiming::EaseOut => Easing::CubicBezier(0.0, 0.0, 0.58, 1.0),
            AnimationTiming::EaseInOut => Easing::CubicBezier(0.42, 0.0, 0.58, 1.0),
            AnimationTiming::CubicBezier(a, b, c, d) => Easing::CubicBezier(*a, *b, *c, *d),
        }
    }
}

/// Parse a literal `cubic-bezier(x1, y1, x2, y2)` string into an
/// `AnimationTiming::CubicBezier`. Returns `None` if the input isn't
/// a well-formed cubic-bezier function call.
pub(crate) fn parse_cubic_bezier(s: &str) -> Option<AnimationTiming> {
    let inner = s.strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() == 4 {
        Some(AnimationTiming::CubicBezier(
            parts[0], parts[1], parts[2], parts[3],
        ))
    } else {
        None
    }
}

/// Animation direction
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl AnimationDirection {
    /// Convert CSS animation direction to blinc_animation PlayDirection
    pub fn to_play_direction(&self) -> blinc_animation::PlayDirection {
        use blinc_animation::PlayDirection;
        match self {
            AnimationDirection::Normal => PlayDirection::Forward,
            AnimationDirection::Reverse => PlayDirection::Reverse,
            AnimationDirection::Alternate | AnimationDirection::AlternateReverse => {
                PlayDirection::Alternate
            }
        }
    }

    /// Returns true if animation should start in reverse (for AlternateReverse)
    pub fn starts_reversed(&self) -> bool {
        matches!(self, AnimationDirection::AlternateReverse)
    }
}

/// Animation fill mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

impl AnimationFillMode {
    /// Convert CSS animation fill mode to blinc_animation FillMode
    pub fn to_fill_mode(&self) -> blinc_animation::FillMode {
        use blinc_animation::FillMode;
        match self {
            AnimationFillMode::None => FillMode::None,
            AnimationFillMode::Forwards => FillMode::Forwards,
            AnimationFillMode::Backwards => FillMode::Backwards,
            AnimationFillMode::Both => FillMode::Both,
        }
    }
}

/// Parse CSS animation shorthand: animation: name duration timing-function delay iteration-count direction fill-mode
///
/// Examples:
/// - `animation: fade-in 300ms`
/// - `animation: fade-in 300ms ease-out`
/// - `animation: fade-in 300ms ease-out 100ms`
/// - `animation: fade-in 300ms ease-out 0ms infinite`
/// - `animation: slide-in 0.5s ease-in-out 0s 1 normal forwards`
pub(crate) fn parse_animation(value: &str) -> Option<CssAnimation> {
    // `none` names no keyframes, which overrides an animation an earlier
    // rule set.
    if value.trim().eq_ignore_ascii_case("none") {
        return Some(CssAnimation::default());
    }
    // Use `split_whitespace_respecting_parens` so a `cubic-bezier(a, b,
    // c, d)` (from `var(--ease-state)` resolution) survives as a single
    // token instead of getting chopped on the commas.
    let owned_parts = split_whitespace_respecting_parens(value);
    if owned_parts.is_empty() {
        return None;
    }
    let parts: Vec<&str> = owned_parts.iter().map(|s| s.as_str()).collect();

    let mut anim = CssAnimation::default();
    let mut duration_set = false;
    let mut delay_set = false;

    for part in parts {
        // Try parsing as timing function
        if let Some(timing) = AnimationTiming::from_str(part) {
            anim.timing = timing;
            continue;
        }

        // Try parsing as direction
        if let Some(direction) = parse_animation_direction(part) {
            anim.direction = direction;
            continue;
        }

        // Try parsing as fill mode
        if let Some(fill_mode) = parse_animation_fill_mode(part) {
            anim.fill_mode = fill_mode;
            continue;
        }

        // Try parsing as iteration count
        if part.eq_ignore_ascii_case("infinite") {
            anim.iteration_count = 0; // 0 means infinite
            continue;
        }
        if let Ok(count) = part.parse::<u32>() {
            anim.iteration_count = count;
            continue;
        }

        // Try parsing as duration (first time value is duration, second is delay)
        if let Some(ms) = parse_time_value(part) {
            if !duration_set {
                anim.duration_ms = ms;
                duration_set = true;
            } else if !delay_set {
                anim.delay_ms = ms;
                delay_set = true;
            }
            continue;
        }

        // Otherwise, treat as animation name
        if anim.name.is_empty() {
            anim.name = part.to_string();
        }
    }

    if anim.name.is_empty() {
        return None;
    }

    Some(anim)
}

/// Parse animation direction keyword
pub(crate) fn parse_animation_direction(input: &str) -> Option<AnimationDirection> {
    match input.to_lowercase().as_str() {
        "normal" => Some(AnimationDirection::Normal),
        "reverse" => Some(AnimationDirection::Reverse),
        "alternate" => Some(AnimationDirection::Alternate),
        "alternate-reverse" => Some(AnimationDirection::AlternateReverse),
        _ => None,
    }
}

/// Parse animation fill mode keyword
pub(crate) fn parse_animation_fill_mode(input: &str) -> Option<AnimationFillMode> {
    match input.to_lowercase().as_str() {
        "none" => Some(AnimationFillMode::None),
        "forwards" => Some(AnimationFillMode::Forwards),
        "backwards" => Some(AnimationFillMode::Backwards),
        "both" => Some(AnimationFillMode::Both),
        _ => None,
    }
}

// ============================================================================
// Transition Parsing
// ============================================================================

/// Parse CSS transition shorthand
///
/// Supports:
/// - `transition: all 300ms ease`
/// - `transition: opacity 200ms ease-in-out`
/// - `transition: opacity 200ms ease, clip-path 500ms ease-out 100ms`
/// - `transition: none`
pub(crate) fn parse_transition(value: &str) -> Option<CssTransitionSet> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(CssTransitionSet::default());
    }

    let mut transitions = Vec::new();
    // Paren-respecting split — `cubic-bezier(a, b, c, d)` in a
    // comma-separated transition list (from `var(--ease-X)`
    // resolution) must survive as a single segment.
    for segment in split_commas_respecting_parens(value) {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        let t = parse_single_transition(trimmed)?;
        transitions.push(t);
    }

    if transitions.is_empty() {
        return None;
    }

    Some(CssTransitionSet::from_entries(transitions))
}

/// Parse a single transition: `property duration [timing] [delay]`
pub(crate) fn parse_single_transition(value: &str) -> Option<CssTransition> {
    // Use `split_whitespace_respecting_parens` so a `cubic-bezier(a, b,
    // c, d)` (from `var(--ease-X)` resolution) survives as a single
    // token instead of getting chopped on the commas.
    let owned_parts = split_whitespace_respecting_parens(value);
    if owned_parts.is_empty() {
        return None;
    }
    let parts: Vec<&str> = owned_parts.iter().map(|s| s.as_str()).collect();

    let mut property = String::new();
    let mut duration_ms = 0u32;
    let mut timing = AnimationTiming::Ease;
    let mut delay_ms = 0u32;
    let mut duration_set = false;

    for part in parts {
        // Try as timing function
        if let Some(t) = AnimationTiming::from_str(part) {
            timing = t;
            continue;
        }

        // Try as time value (first = duration, second = delay)
        if let Some(ms) = parse_time_value(part) {
            if !duration_set {
                duration_ms = ms;
                duration_set = true;
            } else {
                delay_ms = ms;
            }
            continue;
        }

        // Otherwise treat as property name
        if property.is_empty() {
            property = part.to_lowercase();
        }
    }

    // No property named means all of them.
    if property.is_empty() {
        property = "all".to_string();
    }

    Some(CssTransition {
        property,
        duration_ms,
        timing,
        delay_ms,
    })
}
