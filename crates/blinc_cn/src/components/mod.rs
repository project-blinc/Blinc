//! Themed components built on blinc_layout primitives
//!
//! Each component follows a consistent pattern:
//! - Builder function (e.g., `button("Label")`)
//! - Variant enum (e.g., `ButtonVariant`)
//! - Size enum (e.g., `ButtonSize`)
//! - Implements `ElementBuilder` for rendering
//! - Implements `Deref` to inner element for full customization

pub mod alert;
pub mod badge;
pub mod button;
pub mod card;
pub mod input;
pub mod label;
pub mod separator;
pub mod skeleton;
pub mod spinner;

// Re-export all components
pub use alert::{alert, alert_box, Alert, AlertBox, AlertVariant};
pub use badge::{badge, Badge, BadgeVariant};
pub use button::{button, Button, ButtonSize, ButtonVariant};
pub use card::{card, card_footer, card_header, Card, CardFooter, CardHeader};
pub use input::{input, Input, InputBgColors, InputBorderColors, InputSize};
pub use label::{label, Label, LabelSize};
pub use separator::{separator, Separator, SeparatorOrientation};
pub use skeleton::{skeleton, skeleton_circle, Skeleton};
pub use spinner::{spinner, Spinner, SpinnerSize};
