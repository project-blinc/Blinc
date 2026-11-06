//! Platform-neutral accessibility data and actions.

use std::sync::Arc;

use crate::{element::ElementBounds, tree::StableNodeId};

/// The purpose of an accessible element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibilityRole {
    Text,
    Heading,
    Button,
    Link,
    TextField,
    Checkbox,
    Switch,
    Radio,
    Slider,
    Tab,
    MenuItem,
    Image,
}

/// An action requested by assistive technology.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessibilityAction {
    Activate,
    Focus,
    SetValue(String),
    /// Replace a range in the exposed value, measured in UTF-16 code units.
    ReplaceText {
        start: usize,
        end: usize,
        text: String,
    },
    Increment,
    Decrement,
}

/// Live editable content supplied by a widget.
#[derive(Clone, Debug, Default)]
pub struct AccessibilityValue {
    pub label: Option<String>,
    pub value: String,
    pub placeholder: String,
    pub secure: bool,
    pub focused: bool,
    pub selection: Option<[usize; 2]>,
}

/// Connects a widget's live value and editing actions to its semantic node.
#[derive(Clone)]
pub struct AccessibilityBinding {
    pub read: Arc<dyn Fn() -> AccessibilityValue + Send + Sync>,
    pub action: Arc<dyn Fn(&AccessibilityAction) -> bool + Send + Sync>,
}

/// A visible semantic element, in logical viewport coordinates.
#[derive(Clone, Debug)]
pub struct AccessibilityNode {
    pub id: StableNodeId,
    pub identifier: Option<String>,
    pub role: AccessibilityRole,
    pub label: String,
    pub value: String,
    pub hint: String,
    pub placeholder: String,
    pub bounds: ElementBounds,
    pub disabled: bool,
    pub selected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub secure: bool,
    pub focused: bool,
    pub selection: Option<[usize; 2]>,
}
