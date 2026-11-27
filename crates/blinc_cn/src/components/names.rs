//! The CSS classes and element id given to a builder, for the element it builds.

use blinc_layout::div::Div;

/// Classes and an element id, held until a builder builds its element.
#[derive(Clone, Debug, Default)]
pub(crate) struct Names {
    classes: Vec<String>,
    id: Option<String>,
}

impl Names {
    pub(crate) fn add_class(&mut self, name: &str) {
        self.classes.push(name.to_string());
    }

    pub(crate) fn set_id(&mut self, id: &str) {
        self.id = Some(id.to_string());
    }

    pub(crate) fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// `element` with the classes added to its own, and the id in place of its own.
    pub(crate) fn apply(&self, mut element: Div) -> Div {
        for class in &self.classes {
            element = element.class(class);
        }
        match &self.id {
            Some(id) => element.id(id),
            None => element,
        }
    }

    /// [`Names::apply`] on an element held in place.
    pub(crate) fn apply_to(&self, element: &mut Div) {
        *element = self.apply(std::mem::take(element));
    }
}

/// The `class` and `id` methods of a builder that keeps its [`Names`] at the given field.
macro_rules! name_methods {
    ($($field:ident).+) => {
        /// Add a CSS class to the element, for selector matching
        pub fn class(mut self, name: impl AsRef<str>) -> Self {
            self.$($field).+.add_class(name.as_ref());
            self
        }

        /// Set the element ID for CSS selector matching
        pub fn id(mut self, id: &str) -> Self {
            self.$($field).+.set_id(id);
            self
        }
    };
}
pub(crate) use name_methods;
