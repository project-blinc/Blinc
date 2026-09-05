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
}

impl CssTransitionSet {
    /// Find transition spec for a given property name (also matches "all")
    pub fn get(&self, property: &str) -> Option<&CssTransition> {
        self.transitions
            .iter()
            .find(|t| t.property == "all" || t.property == property)
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
        if let Some(t) = parse_single_transition(trimmed) {
            transitions.push(t);
        } else {
            return None;
        }
    }

    if transitions.is_empty() {
        return None;
    }

    Some(CssTransitionSet { transitions })
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
            property = part.to_string();
        }
    }

    if property.is_empty() {
        return None;
    }

    Some(CssTransition {
        property,
        duration_ms,
        timing,
        delay_ms,
    })
}
