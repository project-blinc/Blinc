//! Global theme state singleton
//!
//! ThemeState is designed to avoid triggering full layout rebuilds on theme changes.
//! - Visual tokens (colors, shadows) can be animated and only trigger repaints
//! - Layout tokens (spacing, typography, radii) trigger partial layout recomputation

use crate::theme::{ColorScheme, ThemeBundle};
use crate::tokens::*;
use blinc_animation::{AnimatedValue, AnimationScheduler, SchedulerHandle, SpringConfig};
use blinc_core::Color;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock, atomic::AtomicBool, atomic::Ordering};

/// Global theme state instance
static THEME_STATE: OnceLock<ThemeState> = OnceLock::new();

/// Global redraw callback - set by the app layer to trigger UI updates
static REDRAW_CALLBACK: Mutex<Option<fn()>> = Mutex::new(None);

/// Set the redraw callback function
///
/// This should be called by the app layer (e.g., blinc_app) to register
/// a function that triggers UI redraws when theme changes.
pub fn set_redraw_callback(callback: fn()) {
    *REDRAW_CALLBACK.lock().unwrap() = Some(callback);
}

/// Trigger a redraw via the registered callback
fn trigger_redraw() {
    if let Some(callback) = *REDRAW_CALLBACK.lock().unwrap() {
        callback();
    }
}

/// Theme transition animation state
#[derive(Default)]
struct ThemeTransition {
    /// Animated progress value (0.0 = old theme, 1.0 = new theme)
    /// Uses AnimatedValue which is automatically ticked by the animation scheduler
    progress: Option<AnimatedValue>,
    /// Colors from the old theme (for interpolation)
    from_colors: Option<ColorTokens>,
    /// Colors from the new theme (target)
    to_colors: Option<ColorTokens>,
}

/// Global theme state - accessed directly by widgets during render
pub struct ThemeState {
    /// The current theme bundle (light/dark pair).
    ///
    /// Wrapped in `RwLock` so [`Self::init`] can swap it when the user
    /// calls `init` after the framework's automatic platform-theme
    /// init has already populated `THEME_STATE`. Read by
    /// [`Self::set_scheme`] to derive light/dark token sets.
    bundle: RwLock<ThemeBundle>,

    /// Current color scheme
    scheme: RwLock<ColorScheme>,

    /// Current color tokens (can be animated)
    colors: RwLock<ColorTokens>,

    /// Current shadow tokens (can be animated)
    shadows: RwLock<ShadowTokens>,

    /// Current spacing tokens
    spacing: RwLock<SpacingTokens>,

    /// Current typography tokens
    typography: RwLock<TypographyTokens>,

    /// Current radius tokens
    radii: RwLock<RadiusTokens>,

    /// Current corner-shape tokens (squircle / superellipse policy).
    /// Read by the paint walker to substitute the effective `n` on
    /// rounded corners when the element doesn't carry an explicit
    /// `corner_shape` override. Existing themes return the default
    /// `ShapeTokens::default()` (off) via the trait default impl, so
    /// they keep their circular-arc behaviour.
    shape: RwLock<ShapeTokens>,

    /// Current animation tokens
    animations: RwLock<AnimationTokens>,

    /// Dynamic color overrides
    color_overrides: RwLock<FxHashMap<ColorToken, Color>>,

    /// Dynamic spacing overrides
    spacing_overrides: RwLock<FxHashMap<SpacingToken, f32>>,

    /// Dynamic radius overrides
    radius_overrides: RwLock<FxHashMap<RadiusToken, f32>>,

    /// Flag indicating theme needs repaint (colors changed)
    needs_repaint: AtomicBool,

    /// Flag indicating theme needs layout (spacing/typography changed)
    needs_layout: AtomicBool,

    /// Animation scheduler handle (set after window creation)
    scheduler_handle: RwLock<Option<SchedulerHandle>>,

    /// Theme transition animation state
    transition: Mutex<ThemeTransition>,
}

impl ThemeState {
    /// Initialize (or replace) the global theme state.
    ///
    /// Safe to call after the framework's auto-initialization. The
    /// windowed / mobile runtimes call [`Self::init_default`] before
    /// the user's UI builder runs, which means a user calling `init`
    /// from inside their builder would previously hit `OnceLock::set`
    /// returning `Err` and have their theme silently dropped. This
    /// path now swaps the bundle and re-derives every token set on
    /// the existing state, then triggers the registered redraw
    /// callback so CSS re-parses against the new theme variables.
    pub fn init(bundle: ThemeBundle, scheme: ColorScheme) {
        let theme = bundle.for_scheme(scheme);

        // Hand any CSS the bundle carries off to the module-level
        // pending queue so the windowed runner registers it via
        // `ctx.add_css` on the next frame — after this init has
        // updated the theme variables. The queue is a
        // `Mutex<Vec<String>>` static in `blinc_core::context_state`,
        // so init order doesn't matter: `run_with_theme` calls
        // `ThemeState::init` before the runner constructs
        // `BlincContextState`, but the runner's
        // `drain_stylesheets()` reads from the same place once it
        // exists.
        for css in &bundle.css_sources {
            blinc_core::context_state::queue_pending_stylesheet(css.clone());
        }

        // Track whether this call is creating the state or mutating
        // an existing one, so we can skip the redraw trigger on the
        // very first install (no UI exists to redraw yet — firing
        // the callback then is harmless, but the trace log it
        // produces would be misleading).
        let mut first_time = false;
        let state = THEME_STATE.get_or_init(|| {
            first_time = true;
            ThemeState {
                bundle: RwLock::new(bundle.clone()),
                scheme: RwLock::new(scheme),
                colors: RwLock::new(theme.colors().clone()),
                shadows: RwLock::new(theme.shadows().clone()),
                spacing: RwLock::new(theme.spacing().clone()),
                typography: RwLock::new(theme.typography().clone()),
                radii: RwLock::new(theme.radii().clone()),
                shape: RwLock::new(*theme.shape()),
                animations: RwLock::new(theme.animations().clone()),
                color_overrides: RwLock::new(FxHashMap::default()),
                spacing_overrides: RwLock::new(FxHashMap::default()),
                radius_overrides: RwLock::new(FxHashMap::default()),
                needs_repaint: AtomicBool::new(false),
                needs_layout: AtomicBool::new(false),
                scheduler_handle: RwLock::new(None),
                transition: Mutex::new(ThemeTransition::default()),
            }
        });

        if first_time {
            // `get_or_init`'s closure already populated every field
            // from the bundle we own; nothing left to do.
            return;
        }

        // Replace path — `get_or_init` returned a previously-built
        // state (the runner's platform default, or an earlier user
        // bundle). Swap the bundle and re-derive each token set so
        // existing readers see the new values, then trigger the
        // redraw callback so cached stylesheets reparse against the
        // new CSS variables.
        *state.bundle.write().unwrap() = bundle;
        *state.scheme.write().unwrap() = scheme;
        *state.colors.write().unwrap() = theme.colors().clone();
        *state.shadows.write().unwrap() = theme.shadows().clone();
        *state.spacing.write().unwrap() = theme.spacing().clone();
        *state.typography.write().unwrap() = theme.typography().clone();
        *state.radii.write().unwrap() = theme.radii().clone();
        *state.shape.write().unwrap() = *theme.shape();
        *state.animations.write().unwrap() = theme.animations().clone();
        state.color_overrides.write().unwrap().clear();
        state.spacing_overrides.write().unwrap().clear();
        state.radius_overrides.write().unwrap().clear();
        state.needs_repaint.store(true, Ordering::SeqCst);
        state.needs_layout.store(true, Ordering::SeqCst);
        trigger_redraw();
    }

    /// Set the animation scheduler for theme transitions
    ///
    /// This should be called by the app layer after the window is created
    /// to enable animated theme transitions.
    pub fn set_scheduler(&self, scheduler: &Arc<Mutex<AnimationScheduler>>) {
        let handle = scheduler.lock().unwrap().handle();
        *self.scheduler_handle.write().unwrap() = Some(handle);
    }

    /// Initialize with platform-native theme and system color scheme
    ///
    /// Detects the current OS and uses the appropriate native theme:
    /// - macOS: Apple Human Interface Guidelines theme
    /// - Windows: Fluent Design System 2 theme
    /// - Linux: GNOME Adwaita theme
    pub fn init_default() {
        use crate::platform::detect_system_color_scheme;
        use crate::themes::platform::platform_theme_bundle;

        let bundle = platform_theme_bundle();
        let scheme = detect_system_color_scheme();
        Self::init(bundle, scheme);
    }

    /// Get the global theme state instance
    pub fn get() -> &'static ThemeState {
        THEME_STATE
            .get()
            .expect("ThemeState not initialized. Call ThemeState::init() at app startup.")
    }

    /// Try to get the global theme state (returns None if not initialized)
    pub fn try_get() -> Option<&'static ThemeState> {
        THEME_STATE.get()
    }

    // ========== Color Scheme ==========

    /// Get the current color scheme
    pub fn scheme(&self) -> ColorScheme {
        *self.scheme.read().unwrap()
    }

    /// Set the color scheme (animates colors if scheduler is available)
    pub fn set_scheme(&self, scheme: ColorScheme) {
        let mut current = self.scheme.write().unwrap();
        if *current != scheme {
            tracing::debug!(
                "ThemeState::set_scheme - switching from {:?} to {:?}",
                *current,
                scheme
            );
            // Get current colors before switching
            let old_colors = self.colors.read().unwrap().clone();

            *current = scheme;
            drop(current);

            // Get new theme tokens
            let theme = self.bundle.read().unwrap().for_scheme(scheme);
            let new_colors = theme.colors().clone();

            // Update non-color tokens immediately (they don't animate)
            *self.shadows.write().unwrap() = theme.shadows().clone();
            *self.spacing.write().unwrap() = theme.spacing().clone();
            *self.typography.write().unwrap() = theme.typography().clone();
            *self.radii.write().unwrap() = theme.radii().clone();
            *self.shape.write().unwrap() = *theme.shape();
            *self.animations.write().unwrap() = theme.animations().clone();

            // Try to animate colors if scheduler handle is available
            let handle_opt = self.scheduler_handle.read().unwrap().clone();
            if let Some(handle) = handle_opt {
                // Start animated transition using AnimatedValue
                let mut transition = self.transition.lock().unwrap();
                transition.from_colors = Some(old_colors.clone());
                transition.to_colors = Some(new_colors.clone());

                // Create AnimatedValue for progress (0 to 100, scaled to avoid spring epsilon issues)
                // The animation scheduler's background thread will tick this automatically
                let mut progress = AnimatedValue::new(handle, 0.0, SpringConfig::gentle());
                progress.set_target(100.0);
                transition.progress = Some(progress);

                // Initialize colors to starting point (old colors at progress=0)
                // This ensures immediate visual feedback before first tick
                drop(transition);
                *self.colors.write().unwrap() = old_colors;
            } else {
                // No scheduler, instant swap
                *self.colors.write().unwrap() = new_colors;
            }

            // Mark for repaint and layout
            self.needs_repaint.store(true, Ordering::SeqCst);
            self.needs_layout.store(true, Ordering::SeqCst);

            // Trigger UI redraw
            trigger_redraw();
        }
    }

    /// Update theme colors based on animation progress
    ///
    /// This should be called during the render loop to update interpolated colors.
    /// Returns true if animation is still in progress and needs more frames.
    pub fn tick(&self) -> bool {
        let mut transition = self.transition.lock().unwrap();

        // Check if we have an active animation
        let progress_opt = transition.progress.as_ref();
        if progress_opt.is_none() {
            return false;
        }

        let progress_anim = transition.progress.as_ref().unwrap();

        // Get current animated value (0-100 range, normalize to 0-1)
        let raw_progress = progress_anim.get();
        let progress = (raw_progress / 100.0).clamp(0.0, 1.0);

        // Check if animation has reached target (within threshold)
        // AnimatedValue.is_animating() just checks spring existence, not actual progress
        let at_target = (raw_progress - 100.0).abs() < 1.0;

        tracing::trace!(
            "Theme tick: raw={:.1}, progress={:.3}, at_target={}",
            raw_progress,
            progress,
            at_target
        );

        // Interpolate colors based on progress
        if let (Some(from), Some(to)) = (&transition.from_colors, &transition.to_colors) {
            // `at_target` accepts the spring within 1 of 100, so the lerp
            // there lands near 0.99. Assign the target itself on the
            // finishing frame or the scheme settles just short of it.
            let settled = if at_target {
                to.clone()
            } else {
                interpolate_color_tokens(from, to, progress)
            };
            drop(transition);
            *self.colors.write().unwrap() = settled;

            if at_target {
                // Animation complete - clean up
                let mut transition = self.transition.lock().unwrap();
                transition.progress = None;
                transition.from_colors = None;
                transition.to_colors = None;
                return false;
            }

            // Animation still in progress - trigger rebuild so colors are re-read
            trigger_redraw();
            return true;
        }

        // No colors to interpolate, end animation
        transition.progress = None;
        false
    }

    /// Check if a theme transition animation is in progress
    pub fn is_animating(&self) -> bool {
        let transition = self.transition.lock().unwrap();
        transition
            .progress
            .as_ref()
            .map(|p| p.is_animating())
            .unwrap_or(false)
    }

    /// Toggle between light and dark mode
    pub fn toggle_scheme(&self) {
        let current = self.scheme();
        self.set_scheme(current.toggle());
    }

    // ========== Color Access ==========

    /// Get a color token value (checks override first)
    pub fn color(&self, token: ColorToken) -> Color {
        let resolved = {
            if let Some(color) = self.color_overrides.read().unwrap().get(&token) {
                *color
            } else {
                self.colors.read().unwrap().get(token)
            }
        };
        // Diagnostic: when `BLINC_DEBUG_COLOR_TOKEN=1` is set, log
        // every unique (token, resolved Color) pair exactly once.
        // Pair this with `BLINC_DEBUG_FILL_RECT=1` so the same hash
        // appearing on a buggy widget can be traced back to the
        // theme token that produced its colour.
        if matches!(std::env::var("BLINC_DEBUG_COLOR_TOKEN").as_deref(), Ok("1")) {
            use std::sync::{Mutex, OnceLock};
            // (token, r_bits, g_bits, b_bits, a_bits)
            type SeenKey = (ColorToken, u32, u32, u32, u32);
            static SEEN: OnceLock<Mutex<std::collections::HashSet<SeenKey>>> = OnceLock::new();
            let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
            let key = (
                token,
                resolved.r.to_bits(),
                resolved.g.to_bits(),
                resolved.b.to_bits(),
                resolved.a.to_bits(),
            );
            if seen.lock().unwrap().insert(key) {
                eprintln!(
                    "[theme.color] {:?} → ({:.3},{:.3},{:.3},{:.3})",
                    token, resolved.r, resolved.g, resolved.b, resolved.a,
                );
            }
        }
        resolved
    }

    /// Get all color tokens
    pub fn colors(&self) -> ColorTokens {
        self.colors.read().unwrap().clone()
    }

    /// Set a color override (triggers repaint only)
    pub fn set_color_override(&self, token: ColorToken, color: Color) {
        self.color_overrides.write().unwrap().insert(token, color);
        self.needs_repaint.store(true, Ordering::SeqCst);
        trigger_redraw();
    }

    /// Remove a color override
    pub fn remove_color_override(&self, token: ColorToken) {
        self.color_overrides.write().unwrap().remove(&token);
        self.needs_repaint.store(true, Ordering::SeqCst);
        trigger_redraw();
    }

    // ========== CSS Variable Generation ==========

    /// Generate a CSS variable map from every token family on the
    /// active theme.
    ///
    /// Emits ~100 variables covering colour, radius, spacing,
    /// typography (family / size / weight / line-height / tracking)
    /// and motion (durations / easings). Keys are kebab-case
    /// without the `--` prefix. Values are CSS-ready strings:
    ///
    /// - Colour tokens — `#rrggbb` or `rgba(r,g,b,a)`.
    /// - Lengths (radius / spacing / type sizes) — `Npx`.
    /// - Letter-spacing (tracking) — `Nem`.
    /// - Durations — `Nms`.
    /// - Font families — comma-separated CSS family stack.
    /// - Easings — `linear` keyword or `cubic-bezier(...)`.
    /// - Font weights and line-heights — unitless numerics.
    ///
    /// The CN component stylesheet (`blinc_cn::cn_styles::CN_STYLES`)
    /// consumes these via `var(--radius-default)` etc., so the
    /// active theme's ladder flows through to every cn widget
    /// without per-widget Rust glue.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let vars = ThemeState::get().to_css_variable_map();
    /// // vars["text-primary"] == "#1a1a2e"
    /// // vars["surface"] == "#ffffff"
    /// // vars["radius-xl"] == "18px"      (Hybrid)
    /// // vars["text-sm"] == "13px"        (Universal HID)
    /// // vars["duration-fast"] == "180ms" (Hybrid)
    /// // vars["font-sans"] == "\"Noto Sans\", system-ui, …"
    /// ```
    pub fn to_css_variable_map(&self) -> HashMap<String, String> {
        fn hex(c: Color) -> String {
            if c.a < 1.0 {
                format!(
                    "rgba({},{},{},{})",
                    (c.r * 255.0) as u8,
                    (c.g * 255.0) as u8,
                    (c.b * 255.0) as u8,
                    c.a
                )
            } else {
                format!(
                    "#{:02x}{:02x}{:02x}",
                    (c.r * 255.0) as u8,
                    (c.g * 255.0) as u8,
                    (c.b * 255.0) as u8
                )
            }
        }
        fn px(v: f32) -> String {
            // Strip trailing `.0` so `12.0` becomes `"12px"`, matching
            // hand-written CSS conventions. Fractional values keep
            // the decimal (`2.5px`).
            if (v - v.round()).abs() < f32::EPSILON {
                format!("{}px", v as i64)
            } else {
                format!("{}px", v)
            }
        }
        fn em(v: f32) -> String {
            // tracking values are emitted as em; keep the decimal so
            // a value like `-0.025` reads naturally.
            format!("{}em", v)
        }
        fn ms(v: u64) -> String {
            format!("{}ms", v)
        }
        fn family(f: &crate::tokens::FontFamily) -> String {
            // Quote names that contain whitespace (matches the
            // canonical CSS convention `"Noto Sans", system-ui, …`).
            fn quote(s: &str) -> String {
                if s.contains(char::is_whitespace) {
                    format!("\"{}\"", s)
                } else {
                    s.to_string()
                }
            }
            let mut out = quote(&f.name);
            for fb in &f.fallbacks {
                out.push_str(", ");
                out.push_str(&quote(fb));
            }
            out
        }
        fn easing(e: crate::tokens::Easing) -> String {
            use crate::tokens::Easing;
            match e {
                Easing::Linear => "linear".to_string(),
                Easing::EaseIn => "cubic-bezier(0.4, 0, 1, 1)".to_string(),
                Easing::EaseOut => "cubic-bezier(0, 0, 0.2, 1)".to_string(),
                Easing::EaseInOut => "cubic-bezier(0.4, 0, 0.2, 1)".to_string(),
                Easing::CubicBezier(a, b, c, d) => {
                    format!("cubic-bezier({}, {}, {}, {})", a, b, c, d)
                }
            }
        }

        let mut vars = HashMap::with_capacity(110);

        // Use self.color() which checks overrides first
        vars.insert("primary".into(), hex(self.color(ColorToken::Primary)));
        vars.insert(
            "primary-hover".into(),
            hex(self.color(ColorToken::PrimaryHover)),
        );
        vars.insert(
            "primary-active".into(),
            hex(self.color(ColorToken::PrimaryActive)),
        );
        vars.insert("secondary".into(), hex(self.color(ColorToken::Secondary)));
        vars.insert(
            "secondary-hover".into(),
            hex(self.color(ColorToken::SecondaryHover)),
        );
        vars.insert(
            "secondary-active".into(),
            hex(self.color(ColorToken::SecondaryActive)),
        );
        vars.insert("success".into(), hex(self.color(ColorToken::Success)));
        vars.insert("success-bg".into(), hex(self.color(ColorToken::SuccessBg)));
        vars.insert("warning".into(), hex(self.color(ColorToken::Warning)));
        vars.insert("warning-bg".into(), hex(self.color(ColorToken::WarningBg)));
        vars.insert("error".into(), hex(self.color(ColorToken::Error)));
        vars.insert("error-bg".into(), hex(self.color(ColorToken::ErrorBg)));
        vars.insert("info".into(), hex(self.color(ColorToken::Info)));
        vars.insert("info-bg".into(), hex(self.color(ColorToken::InfoBg)));
        vars.insert("background".into(), hex(self.color(ColorToken::Background)));
        vars.insert("surface".into(), hex(self.color(ColorToken::Surface)));
        vars.insert(
            "surface-elevated".into(),
            hex(self.color(ColorToken::SurfaceElevated)),
        );
        vars.insert(
            "surface-overlay".into(),
            hex(self.color(ColorToken::SurfaceOverlay)),
        );
        vars.insert(
            "text-primary".into(),
            hex(self.color(ColorToken::TextPrimary)),
        );
        vars.insert(
            "text-secondary".into(),
            hex(self.color(ColorToken::TextSecondary)),
        );
        vars.insert(
            "text-tertiary".into(),
            hex(self.color(ColorToken::TextTertiary)),
        );
        vars.insert(
            "text-inverse".into(),
            hex(self.color(ColorToken::TextInverse)),
        );
        vars.insert("text-link".into(), hex(self.color(ColorToken::TextLink)));
        vars.insert("border".into(), hex(self.color(ColorToken::Border)));
        vars.insert(
            "border-secondary".into(),
            hex(self.color(ColorToken::BorderSecondary)),
        );
        vars.insert(
            "border-hover".into(),
            hex(self.color(ColorToken::BorderHover)),
        );
        vars.insert(
            "border-focus".into(),
            hex(self.color(ColorToken::BorderFocus)),
        );
        // Faded variant of `--border-focus` for the focus *ring* —
        // the outer `outline` that sits 2 px out from the input
        // edge. Using the same solid colour as the border makes the
        // ring read as a second hard stroke; ~35 % alpha gives it
        // the soft halo the HID expects while keeping the input's
        // own border the clear, solid focus indicator.
        let focus_ring = {
            let c = self.color(ColorToken::BorderFocus);
            blinc_core::Color::rgba(c.r, c.g, c.b, 0.35)
        };
        vars.insert("focus-ring".into(), hex(focus_ring));
        vars.insert(
            "border-error".into(),
            hex(self.color(ColorToken::BorderError)),
        );
        // Same trick for the error focus state.
        let error_ring = {
            let c = self.color(ColorToken::BorderError);
            blinc_core::Color::rgba(c.r, c.g, c.b, 0.35)
        };
        vars.insert("focus-ring-error".into(), hex(error_ring));
        // …and for any "success-state" widget that wants a green
        // affirmative ring.
        let success_ring = {
            let c = self.color(ColorToken::Success);
            blinc_core::Color::rgba(c.r, c.g, c.b, 0.35)
        };
        vars.insert("focus-ring-success".into(), hex(success_ring));
        vars.insert("input-bg".into(), hex(self.color(ColorToken::InputBg)));
        vars.insert(
            "input-bg-hover".into(),
            hex(self.color(ColorToken::InputBgHover)),
        );
        vars.insert(
            "input-bg-focus".into(),
            hex(self.color(ColorToken::InputBgFocus)),
        );
        vars.insert(
            "input-bg-disabled".into(),
            hex(self.color(ColorToken::InputBgDisabled)),
        );
        vars.insert("selection".into(), hex(self.color(ColorToken::Selection)));
        vars.insert(
            "selection-text".into(),
            hex(self.color(ColorToken::SelectionText)),
        );
        vars.insert("accent".into(), hex(self.color(ColorToken::Accent)));
        vars.insert(
            "accent-subtle".into(),
            hex(self.color(ColorToken::AccentSubtle)),
        );
        vars.insert(
            "tooltip-bg".into(),
            hex(self.color(ColorToken::TooltipBackground)),
        );
        vars.insert(
            "tooltip-text".into(),
            hex(self.color(ColorToken::TooltipText)),
        );

        // ===== Radius tokens =====
        {
            let r = self.radii.read().unwrap();
            vars.insert("radius-none".into(), px(r.radius_none));
            vars.insert("radius-sm".into(), px(r.radius_sm));
            vars.insert("radius-default".into(), px(r.radius_default));
            vars.insert("radius-md".into(), px(r.radius_md));
            vars.insert("radius-lg".into(), px(r.radius_lg));
            vars.insert("radius-xl".into(), px(r.radius_xl));
            vars.insert("radius-2xl".into(), px(r.radius_2xl));
            vars.insert("radius-3xl".into(), px(r.radius_3xl));
            vars.insert("radius-full".into(), px(r.radius_full));
        }

        // ===== Spacing tokens (4-px scale) =====
        {
            let s = self.spacing.read().unwrap();
            vars.insert("space-0".into(), px(s.space_0));
            vars.insert("space-0-5".into(), px(s.space_0_5));
            vars.insert("space-1".into(), px(s.space_1));
            vars.insert("space-1-5".into(), px(s.space_1_5));
            vars.insert("space-2".into(), px(s.space_2));
            vars.insert("space-2-5".into(), px(s.space_2_5));
            vars.insert("space-3".into(), px(s.space_3));
            vars.insert("space-3-5".into(), px(s.space_3_5));
            vars.insert("space-4".into(), px(s.space_4));
            vars.insert("space-5".into(), px(s.space_5));
            vars.insert("space-6".into(), px(s.space_6));
            vars.insert("space-7".into(), px(s.space_7));
            vars.insert("space-8".into(), px(s.space_8));
            vars.insert("space-9".into(), px(s.space_9));
            vars.insert("space-10".into(), px(s.space_10));
            vars.insert("space-11".into(), px(s.space_11));
            vars.insert("space-12".into(), px(s.space_12));
            vars.insert("space-14".into(), px(s.space_14));
            vars.insert("space-16".into(), px(s.space_16));
            vars.insert("space-20".into(), px(s.space_20));
            vars.insert("space-24".into(), px(s.space_24));
            vars.insert("space-28".into(), px(s.space_28));
            vars.insert("space-32".into(), px(s.space_32));
        }

        // ===== Typography tokens =====
        {
            let t = self.typography.read().unwrap();
            // Font families — emitted as full CSS family stacks so a
            // single `font-family: var(--font-sans);` declaration in
            // CN_STYLES picks up the theme's "Noto Sans, system-ui, …"
            // (Universal HID) or "system-ui, …" (BlincTheme default).
            vars.insert("font-sans".into(), family(&t.font_sans));
            vars.insert("font-mono".into(), family(&t.font_mono));
            vars.insert("font-serif".into(), family(&t.font_serif));
            // Sizes
            vars.insert("text-xs".into(), px(t.text_xs));
            vars.insert("text-sm".into(), px(t.text_sm));
            vars.insert("text-base".into(), px(t.text_base));
            vars.insert("text-lg".into(), px(t.text_lg));
            vars.insert("text-xl".into(), px(t.text_xl));
            vars.insert("text-2xl".into(), px(t.text_2xl));
            vars.insert("text-3xl".into(), px(t.text_3xl));
            vars.insert("text-4xl".into(), px(t.text_4xl));
            vars.insert("text-5xl".into(), px(t.text_5xl));
            // Weights (numeric 100..900)
            vars.insert("font-thin".into(), (t.font_thin.as_u16()).to_string());
            vars.insert("font-light".into(), (t.font_light.as_u16()).to_string());
            vars.insert("font-normal".into(), (t.font_normal.as_u16()).to_string());
            vars.insert("font-medium".into(), (t.font_medium.as_u16()).to_string());
            vars.insert(
                "font-semibold".into(),
                (t.font_semibold.as_u16()).to_string(),
            );
            vars.insert("font-bold".into(), (t.font_bold.as_u16()).to_string());
            vars.insert("font-black".into(), (t.font_black.as_u16()).to_string());
            // Line heights (unitless multipliers — CSS `line-height` accepts these directly)
            vars.insert("leading-none".into(), t.leading_none.to_string());
            vars.insert("leading-tight".into(), t.leading_tight.to_string());
            vars.insert("leading-snug".into(), t.leading_snug.to_string());
            vars.insert("leading-normal".into(), t.leading_normal.to_string());
            vars.insert("leading-relaxed".into(), t.leading_relaxed.to_string());
            vars.insert("leading-loose".into(), t.leading_loose.to_string());
            // Letter-spacing (em)
            vars.insert("tracking-tighter".into(), em(t.tracking_tighter));
            vars.insert("tracking-tight".into(), em(t.tracking_tight));
            vars.insert("tracking-normal".into(), em(t.tracking_normal));
            vars.insert("tracking-wide".into(), em(t.tracking_wide));
            vars.insert("tracking-wider".into(), em(t.tracking_wider));
        }

        // ===== Motion tokens =====
        {
            let a = self.animations.read().unwrap();
            vars.insert("duration-fastest".into(), ms(a.duration_fastest));
            vars.insert("duration-faster".into(), ms(a.duration_faster));
            vars.insert("duration-fast".into(), ms(a.duration_fast));
            vars.insert("duration-normal".into(), ms(a.duration_normal));
            vars.insert("duration-slow".into(), ms(a.duration_slow));
            vars.insert("duration-slower".into(), ms(a.duration_slower));
            vars.insert("duration-slowest".into(), ms(a.duration_slowest));
            vars.insert("ease-default".into(), easing(a.ease_default));
            vars.insert("ease-in".into(), easing(a.ease_in));
            vars.insert("ease-out".into(), easing(a.ease_out));
            vars.insert("ease-in-out".into(), easing(a.ease_in_out));
            // Semantic easing slots — intent-shaped curves so CSS
            // animations / transitions can read `var(--ease-state)`
            // etc. instead of hard-coding cubic-beziers.
            vars.insert("ease-state".into(), easing(a.ease_state));
            vars.insert("ease-nav".into(), easing(a.ease_nav));
            vars.insert("ease-spring".into(), easing(a.ease_spring));
            vars.insert("ease-sheet".into(), easing(a.ease_sheet));
        }

        vars
    }

    // ========== Spacing Access ==========

    /// Get a spacing token value (checks override first)
    pub fn spacing_value(&self, token: SpacingToken) -> f32 {
        if let Some(value) = self.spacing_overrides.read().unwrap().get(&token) {
            return *value;
        }
        self.spacing.read().unwrap().get(token)
    }

    /// Get all spacing tokens
    pub fn spacing(&self) -> SpacingTokens {
        self.spacing.read().unwrap().clone()
    }

    /// Set a spacing override (triggers layout)
    pub fn set_spacing_override(&self, token: SpacingToken, value: f32) {
        self.spacing_overrides.write().unwrap().insert(token, value);
        self.needs_layout.store(true, Ordering::SeqCst);
        trigger_redraw();
    }

    /// Remove a spacing override
    pub fn remove_spacing_override(&self, token: SpacingToken) {
        self.spacing_overrides.write().unwrap().remove(&token);
        self.needs_layout.store(true, Ordering::SeqCst);
        trigger_redraw();
    }

    // ========== Typography Access ==========

    /// Get all typography tokens
    pub fn typography(&self) -> TypographyTokens {
        self.typography.read().unwrap().clone()
    }

    // ========== Radius Access ==========

    /// Get a radius token value (checks override first)
    pub fn radius(&self, token: RadiusToken) -> f32 {
        if let Some(value) = self.radius_overrides.read().unwrap().get(&token) {
            return *value;
        }
        self.radii.read().unwrap().get(token)
    }

    /// Get all radius tokens
    pub fn radii(&self) -> RadiusTokens {
        self.radii.read().unwrap().clone()
    }

    /// Set a radius override (triggers repaint - radii don't affect layout)
    pub fn set_radius_override(&self, token: RadiusToken, value: f32) {
        self.radius_overrides.write().unwrap().insert(token, value);
        self.needs_repaint.store(true, Ordering::SeqCst);
        trigger_redraw();
    }

    // ========== Shape Access ==========

    /// Get the active corner-shape tokens.
    ///
    /// `ShapeTokens` is 12 bytes (`Copy`), so this hands out a value
    /// rather than a borrowed reference — the paint walker reads
    /// it once per frame and the cost is negligible compared to
    /// locking the `RwLock`.
    pub fn shape(&self) -> ShapeTokens {
        *self.shape.read().unwrap()
    }

    /// Get a single shape token value.
    pub fn shape_token(&self, token: ShapeToken) -> f32 {
        self.shape.read().unwrap().get(token)
    }

    // ========== Shadow Access ==========

    /// Get all shadow tokens
    pub fn shadows(&self) -> ShadowTokens {
        self.shadows.read().unwrap().clone()
    }

    // ========== Animation Access ==========

    /// Get all animation tokens
    pub fn animations(&self) -> AnimationTokens {
        self.animations.read().unwrap().clone()
    }

    // ========== Dirty Flags ==========

    /// Check if theme changes require repaint
    pub fn needs_repaint(&self) -> bool {
        self.needs_repaint.load(Ordering::SeqCst)
    }

    /// Clear the repaint flag
    pub fn clear_repaint(&self) {
        self.needs_repaint.store(false, Ordering::SeqCst);
    }

    /// Check if theme changes require layout
    pub fn needs_layout(&self) -> bool {
        self.needs_layout.load(Ordering::SeqCst)
    }

    /// Clear the layout flag
    pub fn clear_layout(&self) {
        self.needs_layout.store(false, Ordering::SeqCst);
    }

    // ========== Override Management ==========

    /// Clear all overrides
    pub fn clear_overrides(&self) {
        self.color_overrides.write().unwrap().clear();
        self.spacing_overrides.write().unwrap().clear();
        self.radius_overrides.write().unwrap().clear();
        self.needs_repaint.store(true, Ordering::SeqCst);
        self.needs_layout.store(true, Ordering::SeqCst);
        trigger_redraw();
    }
}

/// Interpolate between two color token sets
fn interpolate_color_tokens(from: &ColorTokens, to: &ColorTokens, t: f32) -> ColorTokens {
    ColorTokens::lerp(from, to, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::universal::HybridTheme;

    /// Spot-check that `to_css_variable_map` emits keys from every
    /// token family the cn stylesheet relies on. Without these the
    /// CN_STYLES `var(...)` references would dead-resolve.
    #[test]
    fn css_variable_map_emits_every_token_family() {
        // Use a known bundle so values are predictable.
        ThemeState::init(HybridTheme::bundle(), ColorScheme::Light);
        let vars = ThemeState::get().to_css_variable_map();

        // Colour
        assert!(vars.contains_key("primary"), "missing --primary");
        assert!(vars.contains_key("surface"), "missing --surface");
        assert!(vars.contains_key("surface-elevated"));
        // Radius
        assert!(vars.contains_key("radius-sm"));
        assert!(vars.contains_key("radius-default"));
        assert!(vars.contains_key("radius-xl"));
        assert!(vars.contains_key("radius-full"));
        // Spacing
        assert!(vars.contains_key("space-0"));
        assert!(vars.contains_key("space-4"));
        assert!(vars.contains_key("space-32"));
        // Typography — family
        assert!(vars.contains_key("font-sans"));
        assert!(vars.contains_key("font-mono"));
        // Typography — size
        assert!(vars.contains_key("text-xs"));
        assert!(vars.contains_key("text-sm"));
        assert!(vars.contains_key("text-base"));
        // Typography — weight
        assert!(vars.contains_key("font-medium"));
        assert!(vars.contains_key("font-bold"));
        // Typography — leading + tracking
        assert!(vars.contains_key("leading-normal"));
        assert!(vars.contains_key("tracking-tight"));
        // Motion
        assert!(vars.contains_key("duration-fast"));
        assert!(vars.contains_key("duration-slowest"));
        assert!(vars.contains_key("ease-default"));
    }

    #[test]
    fn css_variable_values_are_well_formed() {
        ThemeState::init(HybridTheme::bundle(), ColorScheme::Light);
        let vars = ThemeState::get().to_css_variable_map();

        // Lengths carry `px`.
        assert!(
            vars["radius-default"].ends_with("px"),
            "radius-default = {}",
            vars["radius-default"]
        );
        assert!(vars["text-sm"].ends_with("px"));
        assert!(vars["space-4"].ends_with("px"));

        // Durations carry `ms`.
        assert!(vars["duration-fast"].ends_with("ms"));

        // Letter-spacing carries `em`.
        assert!(vars["tracking-tight"].ends_with("em"));

        // Font weights are numeric and unitless.
        let w: u16 = vars["font-medium"]
            .parse()
            .expect("font-medium should be numeric");
        assert!((100..=900).contains(&w));

        // Easings parse to a CSS function or a keyword.
        let ease = &vars["ease-default"];
        assert!(
            ease.starts_with("cubic-bezier(") || ease == "linear",
            "ease-default = {}",
            ease
        );

        // Font family contains the canonical name and a comma-
        // separated stack.
        assert!(
            vars["font-sans"].contains("Noto Sans"),
            "Hybrid promotes Noto Sans to canonical sans; got {}",
            vars["font-sans"]
        );
    }

    /// Universal HID variants differ on `text_sm` (13 vs default 14)
    /// and `radius_xl` (varies per variant). Confirm the css map
    /// picks up the active theme's value, not a fallback constant.
    #[test]
    fn css_variable_map_reflects_active_theme() {
        ThemeState::init(HybridTheme::bundle(), ColorScheme::Light);
        let vars = ThemeState::get().to_css_variable_map();
        assert_eq!(vars["radius-xl"], "18px"); // Hybrid: 18
        assert_eq!(vars["text-sm"], "13px"); // Universal HID: 13 (vs default 14)
    }
}
