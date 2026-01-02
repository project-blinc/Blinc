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

/// Convenience module for accessing components with `cn::` prefix
pub mod cn {
    pub use crate::components::alert::{alert, alert_box};
    pub use crate::components::badge::badge;
    pub use crate::components::button::button;
    pub use crate::components::card::{card, card_footer, card_header};
    pub use crate::components::checkbox::checkbox;
    pub use crate::components::input::input;
    pub use crate::components::label::label;
    pub use crate::components::separator::separator;
    pub use crate::components::skeleton::{skeleton, skeleton_circle};
    pub use crate::components::spinner::spinner;
    pub use crate::components::switch::switch;
}

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cn;
    // Components
    pub use crate::components::alert::{alert, alert_box, Alert, AlertBox, AlertVariant};
    pub use crate::components::badge::{badge, Badge, BadgeVariant};
    pub use crate::components::button::{button, Button, ButtonSize, ButtonVariant};
    pub use crate::components::card::{card, card_footer, card_header, Card, CardFooter, CardHeader};
    pub use crate::components::checkbox::{checkbox, Checkbox, CheckboxSize};
    pub use crate::components::input::{input, Input, InputBgColors, InputBorderColors, InputSize};
    pub use crate::components::label::{label, Label, LabelSize};
    pub use crate::components::separator::{separator, Separator, SeparatorOrientation};
    pub use crate::components::skeleton::{skeleton, skeleton_circle, Skeleton};
    pub use crate::components::spinner::{spinner, Spinner, SpinnerSize};
    pub use crate::components::switch::{switch, Switch, SwitchSize};
    // Re-export State for checkbox/switch usage
    pub use blinc_core::State;
    // Re-export commonly needed theme types
    pub use blinc_theme::{ColorToken, RadiusToken, ShadowToken, SpacingToken, ThemeState};
}
