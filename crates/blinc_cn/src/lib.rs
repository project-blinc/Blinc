//! # Blinc Component Library (blinc_cn)
//!
//! A shadcn-inspired component library built on `blinc_layout` primitives.
//!
//! ## Philosophy
//!
//! Like shadcn/ui builds styled components on top of Radix UI primitives,
//! `blinc_cn` builds themed, accessible components on top of `blinc_layout`.
//!
//! - **Primitives**: `blinc_layout` provides low-level building blocks (div, text, scroll, etc.)
//! - **Theme Tokens**: `blinc_theme` provides design tokens (colors, spacing, radii, shadows)
//! - **Components**: `blinc_cn` provides styled components that use theme tokens
//!
//! ## Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Button with variants
//! cn::button("Click me")
//!     .variant(ButtonVariant::Primary)
//!     .size(ButtonSize::Medium)
//!
//! // Destructive button
//! cn::button("Delete")
//!     .variant(ButtonVariant::Destructive)
//!
//! // Ghost button (minimal styling)
//! cn::button("Cancel")
//!     .variant(ButtonVariant::Ghost)
//! ```
//!
//! ## Components
//!
//! Available components:
//!
//! - **Button** - Clickable button with variants (primary, secondary, destructive, outline, ghost)
//!
//! Planned components:
//! - Card, Input, Badge, Alert, Dialog, Tooltip, Avatar, Separator, Switch, Checkbox, Select, Tabs

pub mod components;

pub use components::*;

// Re-export InstanceKey from blinc_layout (the canonical location)
pub use blinc_layout::InstanceKey;

/// Convenience module for accessing components with `cn::` prefix
pub mod cn {
    pub use crate::components::alert::{alert, alert_box};
    pub use crate::components::badge::badge;
    pub use crate::components::button::button;
    pub use crate::components::card::{card, card_content, card_footer, card_header};
    pub use crate::components::checkbox::checkbox;
    pub use crate::components::context_menu::context_menu;
    pub use crate::components::dialog::{alert_dialog, dialog};
    pub use crate::components::dropdown_menu::{dropdown_menu, dropdown_menu_custom};
    pub use crate::components::input::input;
    pub use crate::components::label::label;
    pub use crate::components::progress::{progress, progress_animated};
    pub use crate::components::radio::radio_group;
    pub use crate::components::select::select;
    pub use crate::components::separator::separator;
    pub use crate::components::skeleton::{skeleton, skeleton_circle};
    pub use crate::components::slider::slider;
    pub use crate::components::spinner::spinner;
    pub use crate::components::switch::switch;
    pub use crate::components::tabs::{tab_item, tabs, TabsSize, TabsTransition};
    pub use crate::components::textarea::textarea;
    pub use crate::components::toast::{toast, toast_custom, toast_error, toast_success, toast_warning};
}

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cn;
    // Components
    pub use crate::components::alert::{alert, alert_box, Alert, AlertBox, AlertVariant};
    pub use crate::components::badge::{badge, Badge, BadgeVariant};
    pub use crate::components::button::{
        button, Button, ButtonBuilder, ButtonSize, ButtonVariant, IconPosition,
    };
    // Re-export ButtonState for use with buttons
    pub use crate::components::card::{
        card, card_content, card_footer, card_header, Card, CardContent, CardFooter, CardHeader,
    };
    pub use crate::components::checkbox::{checkbox, Checkbox, CheckboxSize};
    pub use crate::components::context_menu::{
        context_menu, ContextMenuBuilder, ContextMenuItem, SubmenuBuilder,
    };
    pub use crate::components::dialog::{
        alert_dialog, dialog, AlertDialogBuilder, DialogBuilder, DialogSize,
    };
    pub use crate::components::dropdown_menu::{
        dropdown_menu, dropdown_menu_custom, DropdownAlign, DropdownMenuBuilder, DropdownPosition,
    };
    pub use crate::components::input::{input, Input, InputBgColors, InputBorderColors, InputSize};
    pub use crate::components::label::{label, Label, LabelBuilder, LabelSize};
    pub use crate::components::progress::{
        progress, progress_animated, AnimatedProgress, Progress, ProgressSize,
    };
    pub use crate::components::radio::{
        radio_group, RadioGroup, RadioGroupBuilder, RadioLayout, RadioSize,
    };
    pub use crate::components::select::{select, Select, SelectBuilder, SelectOption, SelectSize};
    pub use crate::components::separator::{separator, Separator, SeparatorOrientation};
    pub use crate::components::skeleton::{skeleton, skeleton_circle, Skeleton};
    pub use crate::components::slider::{slider, Slider, SliderBuilder, SliderSize};
    pub use crate::components::spinner::{spinner, Spinner, SpinnerSize};
    pub use crate::components::switch::{switch, Switch, SwitchSize};
    pub use crate::components::tabs::{
        tab_item, tabs, TabMenuItem, Tabs, TabsBuilder, TabsSize, TabsTransition,
    };
    pub use crate::components::textarea::{textarea, Textarea, TextareaSize};
    pub use crate::components::toast::{
        toast, toast_custom, toast_error, toast_success, toast_warning, ToastBuilder, ToastVariant,
    };
    pub use blinc_layout::stateful::ButtonState;
    // Re-export State for checkbox/switch/radio usage
    pub use blinc_core::State;
    // Re-export SchedulerHandle for slider/switch usage
    pub use blinc_animation::SchedulerHandle;
    // Re-export text_area_state for textarea usage
    pub use blinc_layout::widgets::text_area::{text_area_state, SharedTextAreaState};
    // Re-export commonly needed theme types
    pub use blinc_theme::{ColorToken, RadiusToken, ShadowToken, SpacingToken, ThemeState};
}
