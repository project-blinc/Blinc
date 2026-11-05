use std::sync::Arc;

use blinc_core::{Brush, Color};

use super::ElementStyle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Priority {
    component: bool,
    specificity: (u32, u32, u32),
    order: usize,
}

/// Winning `!important` colour declarations and their cascade priorities.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct ImportantColors {
    background: Option<(Priority, Brush)>,
    color: Option<(Priority, Color)>,
    border_color: Option<(Priority, Color)>,
}

fn winner<T: Clone>(a: &Option<(Priority, T)>, b: &Option<(Priority, T)>) -> Option<(Priority, T)> {
    match (a, b) {
        (Some(a), Some(b)) if a.0 > b.0 => Some(a.clone()),
        (_, Some(b)) => Some(b.clone()),
        (a, _) => a.clone(),
    }
}

impl ImportantColors {
    pub fn combine(a: Option<&Arc<Self>>, b: Option<&Arc<Self>>) -> Option<Arc<Self>> {
        match (a, b) {
            (Some(a), Some(b)) if Arc::ptr_eq(a, b) => Some(Arc::clone(a)),
            (Some(a), Some(b)) => Some(Arc::new(Self {
                background: winner(&a.background, &b.background),
                color: winner(&a.color, &b.color),
                border_color: winner(&a.border_color, &b.border_color),
            })),
            (a, b) => a.or(b).cloned(),
        }
    }

    pub fn background(&self) -> Option<&Brush> {
        self.background.as_ref().map(|(_, value)| value)
    }

    pub fn color(&self) -> Option<Color> {
        self.color.as_ref().map(|(_, value)| *value)
    }

    pub fn border_color(&self) -> Option<Color> {
        self.border_color.as_ref().map(|(_, value)| *value)
    }

    fn priorities(&mut self) -> impl Iterator<Item = &mut Priority> {
        self.background
            .iter_mut()
            .map(|(p, _)| p)
            .chain(self.color.iter_mut().map(|(p, _)| p))
            .chain(self.border_color.iter_mut().map(|(p, _)| p))
    }

    pub(crate) fn latest_order(&self) -> usize {
        self.background
            .iter()
            .map(|(p, _)| p.order)
            .chain(self.color.iter().map(|(p, _)| p.order))
            .chain(self.border_color.iter().map(|(p, _)| p.order))
            .max()
            .unwrap_or(0)
    }
}

impl ElementStyle {
    pub(crate) fn protect_color(&mut self, property: &str) {
        let priority = Priority {
            component: self.component,
            ..Default::default()
        };
        let mut colors = ImportantColors::default();
        match property {
            "background" | "background-color" => {
                colors.background = self.background.clone().map(|v| (priority, v))
            }
            "color" => colors.color = self.text_color.map(|v| (priority, v)),
            "border-color" => colors.border_color = self.border_color.map(|v| (priority, v)),
            _ => return,
        }
        self.important_colors =
            ImportantColors::combine(self.important_colors.as_ref(), Some(&Arc::new(colors)));
    }

    pub(crate) fn restore_important_colors(&mut self) {
        if let Some(colors) = &self.important_colors {
            if let Some(background) = colors.background() {
                self.background = Some(background.clone());
            }
            if let Some(color) = colors.color() {
                self.text_color = Some(color);
            }
            if let Some(color) = colors.border_color() {
                self.border_color = Some(color);
                for side in [
                    &mut self.border_top,
                    &mut self.border_right,
                    &mut self.border_bottom,
                    &mut self.border_left,
                ]
                .into_iter()
                .flatten()
                {
                    side.color = Some(color);
                }
            }
        }
    }

    pub(crate) fn important_selector(&mut self, specificity: (u32, u32, u32)) {
        if let Some(colors) = &mut self.important_colors {
            for priority in Arc::make_mut(colors).priorities() {
                priority.specificity = specificity;
            }
        }
    }

    pub(crate) fn important_order(&mut self, order: usize) {
        if let Some(colors) = &mut self.important_colors {
            for priority in Arc::make_mut(colors).priorities() {
                priority.order = order;
            }
        }
    }

    pub(crate) fn offset_important_order(&mut self, offset: usize) {
        if let Some(colors) = &mut self.important_colors {
            for priority in Arc::make_mut(colors).priorities() {
                priority.order += offset;
            }
        }
    }

    pub(crate) fn mark_component(&mut self) {
        self.component = true;
        self.mark_important_component();
    }

    pub(crate) fn mark_important_component(&mut self) {
        if let Some(colors) = &mut self.important_colors {
            for priority in Arc::make_mut(colors).priorities() {
                priority.component = true;
            }
        }
    }
}
