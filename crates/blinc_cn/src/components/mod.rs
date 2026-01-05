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
pub mod checkbox;
pub mod combobox;
pub mod context_menu;
pub mod dialog;
pub mod dropdown_menu;
pub mod input;
pub mod label;
pub mod progress;
pub mod radio;
pub mod select;
pub mod separator;
pub mod skeleton;
pub mod slider;
pub mod spinner;
pub mod switch;
pub mod tabs;
pub mod textarea;
pub mod toast;

// Re-export all components
pub use alert::{alert, alert_box, Alert, AlertBox, AlertVariant};
pub use badge::{badge, Badge, BadgeVariant};
pub use button::{button, Button, ButtonBuilder, ButtonSize, ButtonVariant, IconPosition};
// Re-export ButtonState for users who need it
pub use blinc_layout::stateful::ButtonState;
pub use card::{
    card, card_content, card_footer, card_header, Card, CardContent, CardFooter, CardHeader,
};
pub use checkbox::{checkbox, Checkbox, CheckboxSize};
pub use combobox::{combobox, Combobox, ComboboxBuilder, ComboboxOption, ComboboxSize};
pub use context_menu::{context_menu, ContextMenuBuilder, ContextMenuItem, SubmenuBuilder};
pub use dialog::{alert_dialog, dialog, AlertDialogBuilder, DialogBuilder, DialogSize};
pub use dropdown_menu::{
    dropdown_menu, dropdown_menu_custom, DropdownAlign, DropdownMenuBuilder, DropdownPosition,
};
pub use input::{input, Input, InputBgColors, InputBorderColors, InputSize};
pub use label::{label, Label, LabelBuilder, LabelSize};
pub use progress::{progress, progress_animated, AnimatedProgress, Progress, ProgressSize};
pub use radio::{radio_group, RadioGroup, RadioGroupBuilder, RadioLayout, RadioSize};
pub use select::{select, Select, SelectBuilder, SelectOption, SelectSize};
pub use separator::{separator, Separator, SeparatorOrientation};
pub use skeleton::{skeleton, skeleton_circle, Skeleton};
pub use slider::{slider, Slider, SliderSize};
pub use spinner::{spinner, Spinner, SpinnerSize};
pub use switch::{switch, Switch, SwitchSize};
pub use tabs::{tab_item, tabs, TabMenuItem, Tabs, TabsBuilder, TabsSize, TabsTransition};
pub use textarea::{textarea, Textarea, TextareaSize};
pub use toast::{
    toast, toast_custom, toast_error, toast_success, toast_warning, ToastBuilder, ToastVariant,
};
