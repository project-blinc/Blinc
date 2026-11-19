//! Platform-neutral accessibility data and actions.

use std::sync::Arc;

use crate::{element::ElementBounds, tree::StableNodeId};

/// The purpose of an accessible element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessibilityRole {
    /// A component-defined semantic role, exposed as a named group by generic adapters.
    Custom(&'static str),
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
    MenuItemCheckbox,
    MenuItemRadio,
    MenuBar,
    Menu,
    Separator,
    Tree,
    TreeItem,
    Toolbar,
    Tooltip,
    Dialog,
    AlertDialog,
    Status,
    Alert,
    Region,
    Image,
}

impl AccessibilityRole {
    pub(crate) fn has_children(self) -> bool {
        matches!(
            self,
            Self::Custom(_)
                | Self::MenuBar
                | Self::Menu
                | Self::Tree
                | Self::TreeItem
                | Self::Toolbar
                | Self::Dialog
                | Self::AlertDialog
                | Self::Status
                | Self::Alert
                | Self::Region
        )
    }

    /// Role name used by platform bridges.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Custom(name) => name,
            Self::Text => "text",
            Self::Heading => "heading",
            Self::Button => "button",
            Self::Link => "link",
            Self::TextField => "textfield",
            Self::Checkbox => "checkbox",
            Self::Switch => "switch",
            Self::Radio => "radio",
            Self::Slider => "slider",
            Self::Tab => "tab",
            Self::MenuItem => "menuitem",
            Self::Image => "image",
            Self::MenuItemCheckbox => "menuitemcheckbox",
            Self::MenuItemRadio => "menuitemradio",
            Self::MenuBar => "menubar",
            Self::Menu => "menu",
            Self::Separator => "separator",
            Self::Tree => "tree",
            Self::TreeItem => "treeitem",
            Self::Toolbar => "toolbar",
            Self::Tooltip => "tooltip",
            Self::Dialog => "dialog",
            Self::AlertDialog => "alertdialog",
            Self::Status => "status",
            Self::Alert => "alert",
            Self::Region => "region",
        }
    }
}

/// Assistive actions advertised by an element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AccessibilityActions(u8);

impl AccessibilityActions {
    pub const NONE: Self = Self(0);
    pub const ACTIVATE: Self = Self(1);
    pub const FOCUS: Self = Self(2);
    pub const SET_VALUE: Self = Self(4);
    pub const INCREMENT: Self = Self(8);
    pub const DECREMENT: Self = Self(16);

    pub fn contains(self, action: Self) -> bool {
        self.0 & action.0 == action.0
    }
    pub fn supports(self, action: &AccessibilityAction) -> bool {
        self.contains(match action {
            AccessibilityAction::Activate => Self::ACTIVATE,
            AccessibilityAction::Focus => Self::FOCUS,
            AccessibilityAction::SetValue(_) | AccessibilityAction::ReplaceText { .. } => {
                Self::SET_VALUE
            }
            AccessibilityAction::Increment => Self::INCREMENT,
            AccessibilityAction::Decrement => Self::DECREMENT,
        })
    }
}

impl std::ops::BitOr for AccessibilityActions {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Semantics supplied by a component rather than inferred from its tag.
#[derive(Clone)]
pub struct AccessibilitySemantics {
    pub role: AccessibilityRole,
    pub children: bool,
    pub actions: Option<AccessibilityActions>,
    pub live: Option<AccessibilityLiveRegion>,
    pub binding: Option<AccessibilityBinding>,
}

impl AccessibilitySemantics {
    pub fn new(role: AccessibilityRole) -> Self {
        Self {
            role,
            children: role.has_children(),
            actions: None,
            live: None,
            binding: None,
        }
    }
    pub fn children(mut self, children: bool) -> Self {
        self.children = children;
        self
    }
    pub fn actions(mut self, actions: AccessibilityActions) -> Self {
        self.actions = Some(actions);
        self
    }
    pub fn live(mut self, priority: AccessibilityLive, atomic: bool) -> Self {
        self.live = Some(AccessibilityLiveRegion {
            priority,
            atomic,
            ..Default::default()
        });
        self
    }
    pub fn binding(mut self, binding: AccessibilityBinding) -> Self {
        self.binding = Some(binding);
        self
    }
}

/// Announcement priority for changing content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AccessibilityLive {
    #[default]
    Off,
    Polite,
    Assertive,
}

/// Effective live-region properties, inherited by descendants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AccessibilityLiveRegion {
    pub root: Option<StableNodeId>,
    pub priority: AccessibilityLive,
    pub atomic: bool,
    pub busy: bool,
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
    pub parent: Option<StableNodeId>,
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
    pub focusable: bool,
    pub modal: bool,
    pub selection: Option<[usize; 2]>,
    pub live: AccessibilityLiveRegion,
    pub actions: AccessibilityActions,
}
