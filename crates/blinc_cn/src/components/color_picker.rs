//! A bound RGBA picker with a swatch popover or an inline editing panel.
//!
//! Chrome is styled by `.cn-color-picker-*` rules. The selected colour,
//! hue and alpha gradients are value visualizations.

use std::{cell::OnceCell, sync::Arc};

use blinc_core::{
    Color,
    reactive::{Signal, computed, effect, signal},
};
use blinc_layout::InstanceKey;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId, div};
use blinc_layout::element::RenderProps;
use blinc_layout::text::Text;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::text_input::{refresh_text_input, text_input, text_input_data};

use super::color_picker_controls::{Part, surface};

#[derive(Clone)]
pub(super) struct Controller {
    pub value: Signal<Color>,
    pub hsva: Signal<[f32; 4]>,
    pub disabled: Reactive<bool>,
    pub alpha: bool,
    swatches: Reactive<Vec<Color>>,
    on_change: Option<Arc<dyn Fn(Color) + Send + Sync>>,
}

fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl Controller {
    pub fn edit(&self, mut hsva: [f32; 4]) {
        if self.disabled.now().unwrap_or(false) {
            return;
        }
        hsva[0] = if hsva[0].is_finite() {
            hsva[0].clamp(0.0, 360.0)
        } else {
            0.0
        };
        for v in &mut hsva[1..] {
            *v = unit(*v);
        }
        if !self.alpha {
            hsva[3] = unit(self.value.get().a);
        }
        if self.hsva.get() != hsva {
            self.hsva.set(hsva);
        }
        self.commit(Color::from_hsva(hsva[0], hsva[1], hsva[2], hsva[3]));
    }

    fn commit(&self, next: Color) {
        if self.disabled.now().unwrap_or(false) || same_color(self.value.get(), next) {
            return;
        }
        self.value.set(next);
        if let Some(callback) = &self.on_change {
            callback(next);
        }
    }

    fn parse(&self, text: &str) -> Option<Color> {
        let mut color = Color::from_hex_str(text)?;
        let digits = text.trim().trim_start_matches('#').len();
        if !self.alpha || matches!(digits, 3 | 6) {
            color.a = unit(self.value.get().a);
        }
        Some(color)
    }
}

fn same_color(a: Color, b: Color) -> bool {
    a.to_array()
        .into_iter()
        .zip(b.to_array())
        .all(|(a, b)| (a - b).abs() < 0.00001)
}

fn hsva_for(color: Color, previous: [f32; 4]) -> [f32; 4] {
    let color = Color::rgba(unit(color.r), unit(color.g), unit(color.b), unit(color.a));
    let (mut h, mut s, v, a) = color.to_hsva();
    if s < 0.00001 {
        h = previous[0];
    }
    if v < 0.00001 {
        s = previous[1];
    }
    // Keep the right end of the hue strip at red when it was selected there.
    if h < 0.00001 && previous[0] == 360.0 {
        h = 360.0;
    }
    [h, s, v, a]
}

/// A colour picker reading and writing a `Signal<Color>`.
pub struct ColorPickerBuilder {
    value: Signal<Color>,
    alpha: bool,
    inline: bool,
    disabled: Reactive<bool>,
    swatches: Reactive<Vec<Color>>,
    on_change: Option<Arc<dyn Fn(Color) + Send + Sync>>,
    classes: Vec<String>,
    id: Option<String>,
    key: InstanceKey,
    built: OnceCell<Div>,
}

impl ColorPickerBuilder {
    #[track_caller]
    pub fn new(value: Signal<Color>) -> Self {
        Self {
            value,
            alpha: false,
            inline: false,
            disabled: false.into_reactive(),
            swatches: Vec::new().into_reactive(),
            on_change: None,
            classes: Vec::new(),
            id: None,
            key: InstanceKey::new("color_picker"),
            built: OnceCell::new(),
        }
    }

    /// Show the alpha strip and accept alpha in hex entry. Hidden alpha is preserved.
    pub fn alpha(mut self, enabled: bool) -> Self {
        self.alpha = enabled;
        self
    }
    /// Show the editing panel directly instead of opening a swatch popover.
    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.disabled = disabled.into_reactive();
        self
    }
    /// Selectable colours, updated in place when supplied as a signal or computed.
    pub fn swatches(mut self, colors: impl IntoReactive<Vec<Color>>) -> Self {
        self.swatches = colors.into_reactive();
        self
    }
    /// Called once per changed user edit, after the bound signal is written.
    pub fn on_change(mut self, callback: impl Fn(Color) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(callback));
        self
    }
    /// On a popover, applies to the trigger. Restyle its detached panel by its part classes.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_owned());
        self
    }

    fn element(&self) -> &Div {
        blinc_layout::build_once::build_once(&self.built, || {
            let hsva = signal(hsva_for(self.value.get(), [0.0; 4]));
            let c = Controller {
                value: self.value,
                hsva,
                disabled: self.disabled.clone(),
                alpha: self.alpha,
                swatches: self.swatches.clone(),
                on_change: self.on_change.clone(),
            };
            let value = c.value;
            effect(move |g| {
                let previous = g.get_untracked(hsva).unwrap_or_default();
                let next = hsva_for(g.get(value).unwrap_or_default(), previous);
                if previous != next {
                    hsva.set(next);
                }
            });
            let instance = self.key.get();
            let mut root = if self.inline {
                panel(&c, instance)
            } else {
                trigger(&c, instance, self.id.as_deref())
            };
            for class in &self.classes {
                root = root.class(class);
            }
            if let Some(id) = &self.id {
                root = root.id(id);
            }
            root
        })
    }
}

fn swatch(c: &Controller) -> Div {
    let value = c.value;
    div()
        .class("cn-color-picker-swatch")
        .relative()
        .overflow_clip()
        .child(super::color_picker_controls::checker())
        .child(
            div()
                .absolute()
                .inset(0.0)
                .bg(computed(move |g| g.get(value).unwrap_or_default())),
        )
}

fn trigger(c: &Controller, instance: &str, user_id: Option<&str>) -> Div {
    let id = user_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("cn-color-picker-{instance}"));
    let value = c.value;
    let alpha = c.alpha;
    let trigger = div()
        .class("cn-color-picker")
        .class("cn-color-picker-trigger")
        .attr("aria-label", "Choose colour")
        .flex_row()
        .items_center()
        .w_fit()
        .child(swatch(c))
        .child(
            Text::bound(
                computed(move |g| g.get(value).unwrap_or_default().to_hex_string(alpha))
                    .into_reactive(),
            )
            .class("cn-color-picker-value"),
        );
    let controller = c.clone();
    let key = instance.to_owned();
    super::picker_popover::picker_popover(
        trigger,
        id,
        c.disabled.clone(),
        (256.0, if c.alpha { 320.0 } else { 272.0 }),
        move |_| panel(&controller, &key),
    )
}

fn panel(c: &Controller, instance: &str) -> Div {
    let mut panel = div()
        .class("cn-color-picker-panel")
        .flex_col()
        .h_fit()
        .align_self_start()
        .attr_when("disabled", "", c.disabled.clone())
        .child(surface(c, instance, Part::SaturationValue))
        .child(Text::new("Hue").class("cn-color-picker-label"))
        .child(surface(c, instance, Part::Hue));
    if c.alpha {
        let hsva = c.hsva;
        panel = panel
            .child(
                Text::bound(
                    computed(move |g| {
                        format!("Opacity {:.0}%", g.get(hsva).unwrap_or_default()[3] * 100.0)
                    })
                    .into_reactive(),
                )
                .class("cn-color-picker-label"),
            )
            .child(surface(c, instance, Part::Alpha));
    }
    let colors = c.swatches.clone();
    let edit = c.clone();
    let palette = div()
        .class("cn-color-picker-swatches")
        .flex_row()
        .flex_wrap()
        .for_each(
            computed(move |g| {
                colors
                    .read(g)
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .collect()
            }),
            |(index, color): &(usize, Color)| (*index, color.to_hex_string(true)),
            move |(_, color): (usize, Color)| {
                let edit = edit.clone();
                let value = edit.value;
                let alpha = edit.alpha;
                div()
                    .class("cn-color-picker-preset")
                    .relative()
                    .overflow_clip()
                    .attr("tabindex", "0")
                    .attr("role", "button")
                    .attr("aria-label", color.to_hex_string(true))
                    .attr_when("disabled", "", edit.disabled.clone())
                    .attr_when(
                        "data-state",
                        "selected",
                        computed(move |g| {
                            let value = g.get(value).unwrap_or_default();
                            same_color(
                                value,
                                if alpha {
                                    color
                                } else {
                                    color.with_alpha(value.a)
                                },
                            )
                        }),
                    )
                    .child(super::color_picker_controls::checker())
                    .child(div().absolute().inset(0.0).bg(color).pointer_events_none())
                    .on_click(move |_| {
                        let color = if edit.alpha {
                            color
                        } else {
                            color.with_alpha(edit.value.get().a)
                        };
                        edit.commit(color);
                    })
            },
        );
    panel
        .child(
            div()
                .class("cn-color-picker-entry")
                .flex_row()
                .items_center()
                .child(swatch(c))
                .child(hex_field(c)),
        )
        .child(palette)
}

fn hex_field(c: &Controller) -> Div {
    let data = text_input_data();
    let invalid = signal(false);
    let value = c.value;
    let alpha = c.alpha;
    let sync = data.clone();
    effect(move |g| {
        let color = g.get(value).unwrap_or_default();
        let text = color.to_hex_string(alpha);
        let mut d = sync.lock().unwrap();
        if d.value != text {
            d.value = text;
            d.cursor = d.value.len();
            d.selection_start = None;
            drop(d);
            invalid.set(false);
            refresh_text_input(&sync);
        }
    });
    let edits = c.clone();
    let validate = c.clone();
    let submit = data.clone();
    let input = text_input(&data)
        .class("cn-color-picker-hex")
        .class("tabindex=text")
        .placeholder("#rrggbb")
        .disabled(c.disabled.clone())
        .max_length(9)
        .class_when("cn-color-picker-hex--invalid", invalid)
        .on_change(move |text| invalid.set(validate.parse(text).is_none()))
        .on_submit(move |_| {
            if edits.disabled.now().unwrap_or(false) {
                return;
            }
            let text = submit.lock().unwrap().value.clone();
            if let Some(color) = edits.parse(&text) {
                edits.commit(color);
                let mut d = submit.lock().unwrap();
                d.value = edits.value.get().to_hex_string(edits.alpha);
                d.cursor = d.value.len();
                d.selection_start = None;
                drop(d);
                invalid.set(false);
                refresh_text_input(&submit);
            } else {
                invalid.set(true);
            }
        });
    div()
        .class("cn-color-picker-hex-wrap")
        .flex_grow()
        .min_w(0.0)
        .child(input)
}

impl ElementBuilder for ColorPickerBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.element().build(tree)
    }
    fn render_props(&self) -> RenderProps {
        self.element().render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.element().children_builders()
    }
    fn element_type_id(&self) -> ElementTypeId {
        self.element().element_type_id()
    }
    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("color_picker")
    }
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.element().layout_style()
    }
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.element())
    }
    fn element_id(&self) -> Option<&str> {
        self.element().element_id()
    }
    fn element_classes(&self) -> &[Arc<str>] {
        self.element().element_classes()
    }
}

/// Edit an RGBA colour. Use `.inline()` to show the panel without a popover.
#[track_caller]
pub fn color_picker(value: Signal<Color>) -> ColorPickerBuilder {
    ColorPickerBuilder::new(value)
}
