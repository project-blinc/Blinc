#![allow(unused, dead_code, deprecated)]
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

pub mod cn_styles;
pub mod components;
pub mod css_overrides;
pub mod reactive_props;
pub mod theme;

pub use components::*;
pub use reactive_props::NumberValue;
pub use theme::cn_bundle;

// Re-export InstanceKey from blinc_layout (the canonical location)
pub use blinc_layout::InstanceKey;

/// Convenience module for accessing components with `cn::` prefix
pub mod cn {
    pub use crate::components::accordion::accordion;
    pub use crate::components::alert::{alert, alert_box};
    pub use crate::components::badge::{BadgeStyle, BadgeVariant, badge};
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
    pub use crate::components::icon::{IconSize, icon};
    pub use crate::components::input::input;
    pub use crate::components::input_otp::input_otp;
    pub use crate::components::kbd::{KbdSize, kbd};
    pub use crate::components::label::label;
    pub use crate::components::menubar::{MenuTriggerMode, MenuTriggerStyle, menubar};
    pub use crate::components::navigation_menu::{navigation_link, navigation_menu};
    pub use crate::components::number_input::number_input;
    pub use crate::components::pagination::pagination;
    pub use crate::components::popover::{PopoverAlign, PopoverSide, popover};
    pub use crate::components::progress::{progress, progress_animated};
    pub use crate::components::radio::radio_group;
    pub use crate::components::resizable::{resizable_group, resizable_panel};
    pub use crate::components::select::select;
    pub use crate::components::separator::separator;
    pub use crate::components::sheet::{sheet, sheet_bottom, sheet_left, sheet_right, sheet_top};
    pub use crate::components::sidebar::sidebar;
    pub use crate::components::skeleton::{skeleton, skeleton_circle};
    pub use crate::components::slider::slider;
    pub use crate::components::spinner::spinner;
    pub use crate::components::switch::switch;
    pub use crate::components::table::{
        table, table_body, table_caption, table_cell, table_footer, table_head, table_header,
        table_row,
    };
    pub use crate::components::tabs::{TabsSize, TabsTransition, tab_item, tabs};
    pub use crate::components::textarea::textarea;
    pub use crate::components::toast::{
        toast, toast_custom, toast_error, toast_success, toast_warning,
    };
    pub use crate::components::toggle::{Toggle, ToggleBuilder, ToggleSize, ToggleVariant, toggle};
    pub use crate::components::toggle_group::{ToggleItem, toggle_group, toggle_item};
    pub use crate::components::tooltip::tooltip;
    pub use crate::components::tree::tree_view;
    // Typography helpers (label excluded - use cn::label component instead)
    pub use crate::components::typography::{
        b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, muted, p, small,
        span, strong,
    };
    // Scroll Area
    pub use crate::components::scroll_area::{ScrollbarVisibility, scroll_area};
    // Aspect Ratio
    pub use crate::components::aspect_ratio::{
        aspect_ratio, aspect_ratio_4_3, aspect_ratio_9_16, aspect_ratio_16_9, aspect_ratio_21_9,
        aspect_ratio_square,
    };
    // Avatar
    pub use crate::components::avatar::{
        AvatarShape, AvatarSize, AvatarStatus, avatar, avatar_group,
    };
}

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cn;
    // Components
    pub use crate::components::accordion::{Accordion, AccordionBuilder, AccordionMode, accordion};
    pub use crate::components::alert::{Alert, AlertBox, AlertVariant, alert, alert_box};
    pub use crate::components::badge::{Badge, BadgeStyle, BadgeVariant, badge};
    pub use crate::components::breadcrumb::{
        Breadcrumb, BreadcrumbBuilder, BreadcrumbItem, BreadcrumbSeparator, BreadcrumbSize,
        breadcrumb,
    };
    pub use crate::components::button::{
        Button, ButtonBuilder, ButtonSize, ButtonVariant, IconPosition, button,
    };
    // Re-export ButtonState for use with buttons
    pub use crate::components::card::{
        Card, CardContent, CardFooter, CardHeader, card, card_content, card_footer, card_header,
    };
    pub use crate::components::chart::{
        BarChart, BarChartBuilder, ChartGrid, ComparisonBarChart, ComparisonBarChartBuilder,
        DataPoint, DataSeries, Histogram, HistogramBuilder, LineChart, LineChartBuilder, SparkLine,
        SparkLineBuilder, ThresholdBand, ThresholdLineChart, ThresholdLineChartBuilder, bar_chart,
        comparison_bar_chart, histogram, line_chart, spark_line, threshold_line_chart,
    };
    pub use crate::components::checkbox::{Checkbox, CheckboxSize, checkbox};
    pub use crate::components::collapsible::{
        Collapsible, CollapsibleBuilder, CollapsibleTrigger, collapsible, collapsible_section,
    };
    pub use crate::components::context_menu::{
        ContextMenuBuilder, ContextMenuItem, SubmenuBuilder, context_menu,
    };
    pub use crate::components::dialog::{
        AlertDialogBuilder, DialogBuilder, DialogSize, alert_dialog, dialog,
    };
    pub use crate::components::drawer::{
        DrawerBuilder, DrawerSide, DrawerSize, drawer, drawer_left, drawer_right,
    };
    pub use crate::components::dropdown_menu::{
        DropdownAlign, DropdownMenuBuilder, DropdownPosition, dropdown_menu, dropdown_menu_custom,
    };
    pub use crate::components::hover_card::{
        HoverCard, HoverCardAlign, HoverCardBuilder, HoverCardSide, hover_card,
    };
    pub use crate::components::icon::{Icon, IconBuilder, IconSize, icon};
    pub use crate::components::input::{Input, InputBgColors, InputBorderColors, InputSize, input};
    pub use crate::components::input_otp::{InputOtp, InputOtpBuilder, input_otp};
    pub use crate::components::kbd::{Kbd, KbdBuilder, KbdSize, kbd};
    pub use crate::components::label::{Label, LabelBuilder, LabelSize, label};
    pub use crate::components::menubar::{
        MenuTriggerMode, MenuTriggerStyle, Menubar, MenubarBuilder, MenubarMenu, MenubarTrigger,
        menubar,
    };
    pub use crate::components::navigation_menu::{
        NavigationLink, NavigationLinkBuilder, NavigationMenu, NavigationMenuBuilder,
        navigation_link, navigation_menu,
    };
    pub use crate::components::pagination::{
        PageValue, Pagination, PaginationBuilder, PaginationSize, pagination,
    };
    pub use crate::components::popover::{
        Popover, PopoverAlign, PopoverBuilder, PopoverSide, popover,
    };
    pub use crate::components::progress::{
        AnimatedProgress, Progress, ProgressSize, progress, progress_animated,
    };
    pub use crate::components::radio::{
        RadioGroup, RadioGroupBuilder, RadioLayout, RadioSize, radio_group,
    };
    pub use crate::components::resizable::{
        ResizableGroup, ResizableGroupBuilder, ResizablePanelBuilder, ResizeDirection,
        resizable_group, resizable_panel,
    };
    pub use crate::components::select::{Select, SelectBuilder, SelectOption, SelectSize, select};
    pub use crate::components::separator::{Separator, SeparatorOrientation, separator};
    pub use crate::components::sheet::{
        SheetBuilder, SheetSide, SheetSize, sheet, sheet_bottom, sheet_left, sheet_right, sheet_top,
    };
    pub use crate::components::sidebar::{
        Sidebar, SidebarBuilder, SidebarItem, SidebarSection, sidebar,
    };
    pub use crate::components::skeleton::{Skeleton, skeleton, skeleton_circle};
    pub use crate::components::slider::{Slider, SliderBuilder, SliderSize, slider};
    pub use crate::components::spinner::{Spinner, SpinnerBuilder, SpinnerSize, spinner};
    pub use crate::components::switch::{Switch, SwitchSize, switch};
    pub use crate::components::table::{
        TableRow, table, table_body, table_caption, table_cell, table_footer, table_head,
        table_header, table_row,
    };
    pub use crate::components::tabs::{
        TabMenuItem, Tabs, TabsBuilder, TabsSize, TabsTransition, tab_item, tabs,
    };
    pub use crate::components::textarea::{Textarea, TextareaSize, textarea};
    pub use crate::components::toast::{
        ToastBuilder, ToastVariant, toast, toast_custom, toast_error, toast_success, toast_warning,
    };
    pub use crate::components::tooltip::{
        Tooltip, TooltipAlign, TooltipBuilder, TooltipSide, tooltip,
    };
    pub use crate::components::tree::{
        TreeNodeConfig, TreeNodeDiff, TreeView, TreeViewBuilder, tree_view,
    };
    pub use crate::reactive_props::NumberValue;
    // Typography helpers (label excluded - use Label component instead)
    pub use crate::components::typography::{
        b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, muted, p, small,
        span, strong,
    };
    // Scroll Area
    pub use crate::components::scroll_area::{
        ScrollArea, ScrollAreaBuilder, ScrollAreaSize, ScrollbarVisibility, scroll_area,
    };
    // Aspect Ratio
    pub use crate::components::aspect_ratio::{
        AspectRatio, AspectRatioBuilder, AspectRatioPreset, aspect_ratio, aspect_ratio_4_3,
        aspect_ratio_9_16, aspect_ratio_16_9, aspect_ratio_21_9, aspect_ratio_square,
    };
    // Avatar
    pub use crate::components::avatar::{
        Avatar, AvatarBuilder, AvatarGroup, AvatarGroupBuilder, AvatarShape, AvatarSize,
        AvatarStatus, avatar, avatar_group,
    };
    pub use blinc_layout::stateful::ButtonState;
    // Re-export State for checkbox/switch/radio usage
    pub use blinc_core::State;
    // Re-export SchedulerHandle for slider/switch usage
    pub use blinc_animation::SchedulerHandle;
    // Re-export text_area_state for textarea usage
    pub use blinc_layout::widgets::text_area::{SharedTextAreaState, text_area_state};
    // Re-export commonly needed theme types
    pub use blinc_theme::{ColorToken, RadiusToken, ShadowToken, SpacingToken, ThemeState};
    // Re-export icons module + raw SVG generators for inline use
    // (e.g. inside a badge or anywhere the auto-tinted `cn::icon`
    // would set the wrong fixed colour).
    pub use blinc_icons::{icons, to_svg, to_svg_with_stroke};
}
