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
    pub use crate::components::button::button;
    // More components will be added here
}

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cn;
    pub use crate::components::button::{button, Button, ButtonSize, ButtonVariant};
    // Re-export commonly needed theme types
    pub use blinc_theme::{ColorToken, RadiusToken, ShadowToken, SpacingToken, ThemeState};
}
