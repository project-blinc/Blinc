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
    pub use crate::components::accordion::accordion;
    pub use crate::components::alert::{alert, alert_box};
    pub use crate::components::badge::badge;
    pub use crate::components::breadcrumb::breadcrumb;
    pub use crate::components::button::button;
    pub use crate::components::card::{card, card_content, card_footer, card_header};
    pub use crate::components::chart::{
        bar_chart, comparison_bar_chart, histogram, line_chart, spark_line, threshold_line_chart,
    };
    pub use crate::components::checkbox::checkbox;
    pub use crate::components::collapsible::{collapsible, collapsible_section};
    pub use crate::components::combobox::combobox;
    pub use crate::components::context_menu::context_menu;
    pub use crate::components::dialog::{alert_dialog, dialog};
    pub use crate::components::drawer::{drawer, drawer_left, drawer_right};
    pub use crate::components::dropdown_menu::{dropdown_menu, dropdown_menu_custom};
    pub use crate::components::hover_card::hover_card;
    pub use crate::components::icon::{icon, IconSize};
    pub use crate::components::input::input;
    pub use crate::components::kbd::{kbd, KbdSize};
    pub use crate::components::label::label;
    pub use crate::components::menubar::{menubar, MenuTriggerMode, MenuTriggerStyle};
    pub use crate::components::navigation_menu::{navigation_link, navigation_menu};
    pub use crate::components::pagination::pagination;
    pub use crate::components::popover::{popover, PopoverAlign, PopoverSide};
    pub use crate::components::progress::{progress, progress_animated};
    pub use crate::components::radio::radio_group;
    pub use crate::components::select::select;
    pub use crate::components::separator::separator;
    pub use crate::components::sheet::{sheet, sheet_bottom, sheet_left, sheet_right, sheet_top};
    pub use crate::components::resizable::{resizable_group, resizable_panel};
    pub use crate::components::sidebar::sidebar;
    pub use crate::components::skeleton::{skeleton, skeleton_circle};
    pub use crate::components::slider::slider;
    pub use crate::components::spinner::spinner;
    pub use crate::components::switch::switch;
    pub use crate::components::tabs::{tab_item, tabs, TabsSize, TabsTransition};
    pub use crate::components::textarea::textarea;
    pub use crate::components::toast::{
        toast, toast_custom, toast_error, toast_success, toast_warning,
    };
    pub use crate::components::tooltip::tooltip;
    pub use crate::components::tree::tree_view;
    // Typography helpers (label excluded - use cn::label component instead)
    pub use crate::components::typography::{
        b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, muted, p, small,
        span, strong,
    };
}

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cn;
    // Components
    pub use crate::components::accordion::{accordion, Accordion, AccordionBuilder, AccordionMode};
    pub use crate::components::alert::{alert, alert_box, Alert, AlertBox, AlertVariant};
    pub use crate::components::badge::{badge, Badge, BadgeVariant};
    pub use crate::components::breadcrumb::{
        breadcrumb, Breadcrumb, BreadcrumbBuilder, BreadcrumbItem, BreadcrumbSeparator,
        BreadcrumbSize,
    };
    pub use crate::components::button::{
        button, Button, ButtonBuilder, ButtonSize, ButtonVariant, IconPosition,
    };
    // Re-export ButtonState for use with buttons
    pub use crate::components::card::{
        card, card_content, card_footer, card_header, Card, CardContent, CardFooter, CardHeader,
    };
    pub use crate::components::chart::{
        bar_chart, comparison_bar_chart, histogram, line_chart, spark_line, threshold_line_chart,
        BarChart, BarChartBuilder, ChartGrid, ComparisonBarChart, ComparisonBarChartBuilder,
        DataPoint, DataSeries, Histogram, HistogramBuilder, LineChart, LineChartBuilder, SparkLine,
        SparkLineBuilder, ThresholdBand, ThresholdLineChart, ThresholdLineChartBuilder,
    };
    pub use crate::components::checkbox::{checkbox, Checkbox, CheckboxSize};
    pub use crate::components::collapsible::{
        collapsible, collapsible_section, Collapsible, CollapsibleBuilder, CollapsibleTrigger,
    };
    pub use crate::components::context_menu::{
        context_menu, ContextMenuBuilder, ContextMenuItem, SubmenuBuilder,
    };
    pub use crate::components::dialog::{
        alert_dialog, dialog, AlertDialogBuilder, DialogBuilder, DialogSize,
    };
    pub use crate::components::drawer::{
        drawer, drawer_left, drawer_right, DrawerBuilder, DrawerSide, DrawerSize,
    };
    pub use crate::components::dropdown_menu::{
        dropdown_menu, dropdown_menu_custom, DropdownAlign, DropdownMenuBuilder, DropdownPosition,
    };
    pub use crate::components::hover_card::{
        hover_card, HoverCard, HoverCardAlign, HoverCardBuilder, HoverCardSide,
    };
    pub use crate::components::icon::{icon, Icon, IconBuilder, IconSize};
    pub use crate::components::input::{input, Input, InputBgColors, InputBorderColors, InputSize};
    pub use crate::components::kbd::{kbd, Kbd, KbdBuilder, KbdSize};
    pub use crate::components::label::{label, Label, LabelBuilder, LabelSize};
    pub use crate::components::menubar::{
        menubar, MenuTriggerMode, MenuTriggerStyle, Menubar, MenubarBuilder, MenubarMenu,
        MenubarTrigger,
    };
    pub use crate::components::navigation_menu::{
        navigation_link, navigation_menu, NavigationLink, NavigationLinkBuilder, NavigationMenu,
        NavigationMenuBuilder,
    };
    pub use crate::components::pagination::{
        pagination, Pagination, PaginationBuilder, PaginationSize,
    };
    pub use crate::components::popover::{
        popover, Popover, PopoverAlign, PopoverBuilder, PopoverSide,
    };
    pub use crate::components::progress::{
        progress, progress_animated, AnimatedProgress, Progress, ProgressSize,
    };
    pub use crate::components::radio::{
        radio_group, RadioGroup, RadioGroupBuilder, RadioLayout, RadioSize,
    };
    pub use crate::components::select::{select, Select, SelectBuilder, SelectOption, SelectSize};
    pub use crate::components::separator::{separator, Separator, SeparatorOrientation};
    pub use crate::components::sheet::{
        sheet, sheet_bottom, sheet_left, sheet_right, sheet_top, SheetBuilder, SheetSide, SheetSize,
    };
    pub use crate::components::resizable::{
        resizable_group, resizable_panel, ResizableGroup, ResizableGroupBuilder,
        ResizablePanelBuilder, ResizeDirection,
    };
    pub use crate::components::sidebar::{
        sidebar, Sidebar, SidebarBuilder, SidebarItem, SidebarSection,
    };
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
    pub use crate::components::tooltip::{
        tooltip, Tooltip, TooltipAlign, TooltipBuilder, TooltipSide,
    };
    pub use crate::components::tree::{
        tree_view, TreeNodeConfig, TreeNodeDiff, TreeView, TreeViewBuilder,
    };
    // Typography helpers (label excluded - use Label component instead)
    pub use crate::components::typography::{
        b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, muted, p, small,
        span, strong,
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
    // Re-export icons module for easy access
    pub use blinc_icons::icons;
}
