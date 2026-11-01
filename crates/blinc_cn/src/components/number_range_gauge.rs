//! NumberRangeGauge: an interval over a range, a fill between two grips,
//! after ashui's.
//!
//! Two gauge faces share one box: the upper paints the interval up to its
//! grip, the lower lies above it and covers the part before its own grip with
//! the track colour. A press or the wheel moves whichever grip is nearer the
//! pointer, and grips at the same place take turns. Keys move the grip last
//! pressed. The endpoints never cross.
//!
//! ```ignore
//! let band = ctx.use_signal(18.0..=24.0);
//! cn::number_range_gauge(band)
//!     .range(10.0..=30.0)
//!     .format(|r| format!("{}–{} °C", r.start(), r.end()))
//! ```

use blinc_core::BlincContextState;
use blinc_core::reactive::{Computed, ReactiveGraph, Signal};
use blinc_layout::InstanceKey;
use blinc_layout::binding::IntoReactive;
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use std::ops::RangeInclusive;
use std::sync::Arc;

use super::number_gauge::{
    BORDER, Edits, GRIP_WIDTH, GaugeRange, GaugeSize, bound_flag, face, interactive, plain, shell,
    value_label, width_for,
};

type RangeFormat = Arc<dyn Fn(RangeInclusive<f64>) -> String + Send + Sync>;
type RangeChange = Arc<dyn Fn(RangeInclusive<f64>) + Send + Sync>;

/// The interval `value` holds, or the whole range before it holds one.
fn held(value: Signal<RangeInclusive<f64>>, range: GaugeRange) -> RangeInclusive<f64> {
    value.try_get().unwrap_or(range.min..=range.max)
}

/// The two endpoints as shown: each snapped, and never crossed.
fn shown(range: GaugeRange, value: &RangeInclusive<f64>) -> (f64, f64) {
    let (lo, hi) = (range.snap(*value.start()), range.snap(*value.end()));
    (lo.min(hi), hi.max(lo))
}

/// Which grip a pointer at `x` in a gauge `width` wide takes: the nearer one.
/// Grips at the same place give `active` on the wheel and the other one on a
/// press, so pressing again picks up the grip beneath.
fn nearest(
    range: GaugeRange,
    (lo, hi): (f64, f64),
    x: f32,
    width: f32,
    active: Endpoint,
    press: bool,
) -> Endpoint {
    let travel = width - 2.0 * BORDER - GRIP_WIDTH;
    let centre = |v: f64| BORDER + GRIP_WIDTH / 2.0 + range.fraction(v) * travel;
    let (dl, dh) = ((x - centre(lo)).abs(), (x - centre(hi)).abs());
    if (dl - dh).abs() < 0.001 {
        if press { active.other() } else { active }
    } else if dl < dh {
        Endpoint::Lower
    } else {
        Endpoint::Upper
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endpoint {
    Lower,
    Upper,
}

impl Endpoint {
    fn other(self) -> Self {
        match self {
            Endpoint::Lower => Endpoint::Upper,
            Endpoint::Upper => Endpoint::Lower,
        }
    }
}

struct Config {
    value: Signal<RangeInclusive<f64>>,
    bounds: RangeInclusive<f64>,
    step: f64,
    size: GaugeSize,
    format: Option<RangeFormat>,
    disabled: Computed<bool>,
    width: Option<f32>,
    classes: Vec<String>,
    on_change: Option<RangeChange>,
}

/// Lazy builder for [`NumberRangeGauge`].
pub struct NumberRangeGaugeBuilder {
    key: InstanceKey,
    config: Config,
    built: std::cell::OnceCell<NumberRangeGauge>,
}

pub struct NumberRangeGauge {
    inner: Div,
}

impl NumberRangeGaugeBuilder {
    #[track_caller]
    pub fn new(value: Signal<RangeInclusive<f64>>) -> Self {
        Self {
            key: InstanceKey::new("cn_number_range_gauge"),
            config: Config {
                value,
                bounds: 0.0..=100.0,
                step: 1.0,
                size: GaugeSize::Medium,
                format: None,
                disabled: bound_flag(false),
                width: None,
                classes: Vec::new(),
                on_change: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &NumberRangeGauge {
        ::blinc_layout::build_once::build_once(&self.built, || {
            NumberRangeGauge::from_config(self.key.get(), &self.config)
        })
    }

    /// The values it covers, `0.0..=100.0` unless set.
    pub fn range(mut self, bounds: RangeInclusive<f64>) -> Self {
        self.config.bounds = bounds;
        self
    }

    /// The step both endpoints snap to, counted from the range's start; 1
    /// unless set.
    pub fn step(mut self, step: f64) -> Self {
        self.config.step = step;
        self
    }

    pub fn size(mut self, size: GaugeSize) -> Self {
        self.config.size = size;
        self
    }

    /// How the centred interval is written.
    pub fn format<F>(mut self, format: F) -> Self
    where
        F: Fn(RangeInclusive<f64>) -> String + Send + Sync + 'static,
    {
        self.config.format = Some(Arc::new(format));
        self
    }

    /// Whether it is inert: a value, a Signal or a Computed, followed in place.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = bound_flag(disabled);
        self
    }

    /// A fixed width in place of the one the formatted bounds give.
    pub fn w(mut self, px: f32) -> Self {
        self.config.width = Some(px);
        self
    }

    /// A class for rules of your own to match.
    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.config.classes.push(name.into());
        self
    }

    /// Called with the interval after each edit the user makes; a value set
    /// from code is not reported.
    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(RangeInclusive<f64>) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

impl NumberRangeGauge {
    fn from_config(instance: &str, config: &Config) -> Self {
        let range = GaugeRange::over(&config.bounds, config.step);
        let format = config.format.clone().unwrap_or_else(|| {
            Arc::new(|r: RangeInclusive<f64>| {
                format!("{} – {}", plain(*r.start()), plain(*r.end()))
            })
        });
        let (size, disabled, value) = (config.size, config.disabled.clone(), config.value);
        let upper_active =
            BlincContextState::get().use_signal_keyed(&format!("{instance}_upper_active"), || true);
        let active = move || {
            if upper_active.get() {
                Endpoint::Upper
            } else {
                Endpoint::Lower
            }
        };

        let ends =
            move |g: &ReactiveGraph| shown(range, &g.get(value).unwrap_or(range.min..=range.max));
        let label = {
            let format = Arc::clone(&format);
            move |g: &ReactiveGraph| {
                let (lo, hi) = ends(g);
                format(lo..=hi)
            }
        };

        // An edit to one endpoint: snapped, held on its side of the other,
        // written only when it changes, then reported.
        let set: Arc<dyn Fn(Endpoint, f64) + Send + Sync> = {
            let on_change = config.on_change.clone();
            Arc::new(move |end: Endpoint, v: f64| {
                let (lo, hi) = shown(range, &held(value, range));
                let v = range.snap(v);
                let next = match end {
                    Endpoint::Lower => v.min(hi)..=hi,
                    Endpoint::Upper => lo..=v.max(lo),
                };
                if next == held(value, range) {
                    return;
                }
                value.set(next.clone());
                if let Some(ref cb) = on_change {
                    cb(next);
                }
            })
        };
        // The grip a pointer at `x` takes, which becomes the active one.
        let choose = move |x: f32, width: f32, press: bool| {
            let current = active();
            let end = if width > 0.0 {
                nearest(
                    range,
                    shown(range, &held(value, range)),
                    x,
                    width,
                    current,
                    press,
                )
            } else {
                current
            };
            if end != current {
                upper_active.set(end == Endpoint::Upper);
            }
            end
        };
        // Where the active endpoint is now.
        let from = move |end: Endpoint| {
            let (lo, hi) = shown(range, &held(value, range));
            if end == Endpoint::Lower { lo } else { hi }
        };

        let near_min = range.snap(range.min + range.step);
        let near_max = range.snap(range.max - range.step);
        let samples: Vec<String> = [
            (range.min, range.max),
            (range.min, range.min),
            (range.max, range.max),
            (near_min, near_max),
            (near_min, near_min),
            (near_max, near_max),
        ]
        .into_iter()
        .map(|(a, b)| format(a..=b))
        .collect();
        let fit = width_for(size, &samples);

        // Each face fills the box inside the border; the lower lies on top.
        let layer = |end: &str| {
            div()
                .class("cn-number-gauge-layer")
                .attr("data-endpoint", end)
                .absolute()
                .inset(BORDER)
                .flex_row()
                .pointer_events_none()
        };
        let upper_face = face(layer("upper"), move |g| range.fraction(ends(g).1));
        let lower_face = face(layer("lower"), move |g| range.fraction(ends(g).0));

        let gauge = interactive(
            config
                .classes
                .iter()
                .fold(shell(size, config.width, fit, &disabled), |d, c| d.class(c))
                .class("cn-number-range-gauge")
                .child(upper_face)
                .child(lower_face)
                .child(value_label(size, label)),
            instance,
            disabled,
            Edits {
                seek: Arc::new({
                    let set = Arc::clone(&set);
                    move |x, width, press| {
                        let end = if press {
                            choose(x, width, true)
                        } else {
                            active()
                        };
                        set(end, range.at(x, width));
                    }
                }),
                wheel: Arc::new({
                    let set = Arc::clone(&set);
                    move |steps, x, width| {
                        let end = choose(x, width, false);
                        set(end, from(end) + steps as f64 * range.step);
                    }
                }),
                key: Arc::new(move |key_code| {
                    let end = active();
                    match range.key(from(end), key_code) {
                        Some(next) => {
                            set(end, next);
                            true
                        }
                        None => false,
                    }
                }),
            },
        );
        Self { inner: gauge }
    }
}

impl ElementBuilder for NumberRangeGaugeBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.get_or_build().inner)
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.get_or_build().inner)
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.get_or_build().inner)
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().inner.element_classes()
    }
}

/// An interval over a range, shown as a fill between two grips. See the
/// module docs.
#[track_caller]
pub fn number_range_gauge(value: Signal<RangeInclusive<f64>>) -> NumberRangeGaugeBuilder {
    NumberRangeGaugeBuilder::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossed_endpoints_are_shown_in_order() {
        let r = GaugeRange::new(0.0, 100.0, 1.0);
        assert_eq!(shown(r, &(70.2..=30.0)), (30.0, 70.0));
    }

    #[test]
    fn the_nearer_grip_answers_and_coincident_grips_alternate() {
        let r = GaugeRange::new(0.0, 100.0, 1.0);
        let width = 2.0 * BORDER + GRIP_WIDTH + 100.0;
        let at = |v: f32| BORDER + GRIP_WIDTH / 2.0 + v;
        let pick = |ends, x, active, press| nearest(r, ends, at(x), width, active, press);
        assert_eq!(
            pick((20.0, 80.0), 30.0, Endpoint::Upper, true),
            Endpoint::Lower
        );
        assert_eq!(
            pick((20.0, 80.0), 60.0, Endpoint::Lower, true),
            Endpoint::Upper
        );
        // Both grips at 50: a press takes the other one, the wheel keeps it.
        assert_eq!(
            pick((50.0, 50.0), 50.0, Endpoint::Upper, true),
            Endpoint::Lower
        );
        assert_eq!(
            pick((50.0, 50.0), 50.0, Endpoint::Upper, false),
            Endpoint::Upper
        );
    }
}
