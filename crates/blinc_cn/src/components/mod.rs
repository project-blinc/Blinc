//! Themed components built on blinc_layout primitives
//!
//! Each component follows a consistent pattern:
//! - Builder function (e.g., `button("Label")`)
//! - Variant enum (e.g., `ButtonVariant`)
//! - Size enum (e.g., `ButtonSize`)
//! - Implements `ElementBuilder` for rendering
//! - Implements `Deref` to inner element for full customization

pub mod accordion;
pub mod alert;
pub mod aspect_ratio;
pub mod avatar;
pub mod badge;
pub mod breadcrumb;
pub mod button;
pub mod card;
pub mod chart;
pub mod checkbox;
pub mod collapsible;
pub mod combobox;
pub mod context_menu;
pub mod dialog;
pub mod drawer;
pub mod dropdown_menu;
pub mod hover_card;
pub mod icon;
pub mod input;
pub mod input_otp;
pub mod kbd;
pub mod label;
pub mod menubar;
pub mod navigation_menu;
pub mod number_input;
pub mod pagination;
pub mod popover;
pub mod progress;
pub mod radio;
pub mod resizable;
pub mod scroll_area;
pub mod select;
pub mod separator;
pub mod sheet;
pub mod sidebar;
pub mod skeleton;
pub mod slider;
pub mod spinner;
pub mod switch;
pub mod table;
pub mod tabs;
pub mod textarea;
pub mod toast;
pub mod toggle;
pub mod toggle_group;
pub mod tooltip;
pub mod tree;
pub mod typography;

// Re-export all components
pub use accordion::{Accordion, AccordionBuilder, AccordionMode, accordion};
pub use alert::{Alert, AlertBox, AlertVariant, alert, alert_box};
pub use badge::{Badge, BadgeStyle, BadgeVariant, badge};
pub use breadcrumb::{
    Breadcrumb, BreadcrumbBuilder, BreadcrumbItem, BreadcrumbSeparator, BreadcrumbSize, breadcrumb,
};
pub use button::{Button, ButtonBuilder, ButtonSize, ButtonVariant, IconPosition, button};
pub use collapsible::{
    Collapsible, CollapsibleBuilder, CollapsibleTrigger, collapsible, collapsible_section,
};
// Re-export ButtonState for users who need it
pub use blinc_layout::stateful::ButtonState;
pub use card::{
    Card, CardContent, CardFooter, CardHeader, card, card_content, card_footer, card_header,
};
pub use chart::{
    BarChart, BarChartBuilder, ChartGrid, ComparisonBarChart, ComparisonBarChartBuilder, DataPoint,
    DataSeries, Histogram, HistogramBuilder, LineChart, LineChartBuilder, SparkLine,
    SparkLineBuilder, ThresholdBand, ThresholdLineChart, ThresholdLineChartBuilder, bar_chart,
    comparison_bar_chart, histogram, line_chart, spark_line, threshold_line_chart,
};
pub use checkbox::{Checkbox, CheckboxSize, checkbox};
pub use combobox::{Combobox, ComboboxBuilder, ComboboxOption, ComboboxSize, combobox};
pub use context_menu::{ContextMenuBuilder, ContextMenuItem, SubmenuBuilder, context_menu};
pub use dialog::{AlertDialogBuilder, DialogBuilder, DialogSize, alert_dialog, dialog};
pub use drawer::{DrawerBuilder, DrawerSide, DrawerSize, drawer, drawer_left, drawer_right};
pub use dropdown_menu::{
    DropdownAlign, DropdownMenuBuilder, DropdownPosition, dropdown_menu, dropdown_menu_custom,
};
pub use hover_card::{HoverCard, HoverCardAlign, HoverCardBuilder, HoverCardSide, hover_card};
pub use icon::{Icon, IconBuilder, IconSize, icon};
pub use input::{Input, InputBgColors, InputBorderColors, InputSize, input};
pub use input_otp::{InputOtp, InputOtpBuilder, input_otp};
pub use kbd::{Kbd, KbdBuilder, KbdSize, kbd};
pub use label::{Label, LabelBuilder, LabelSize, label};
pub use menubar::{
    MenuTriggerMode, MenuTriggerStyle, Menubar, MenubarBuilder, MenubarMenu, MenubarTrigger,
    menubar,
};
pub use navigation_menu::{
    NavigationLink, NavigationLinkBuilder, NavigationMenu, NavigationMenuBuilder, navigation_link,
    navigation_menu,
};
pub use number_input::{NumberInput, NumberInputBuilder, number_input};
pub use pagination::{Pagination, PaginationBuilder, PaginationSize, pagination};
pub use popover::{Popover, PopoverAlign, PopoverBuilder, PopoverSide, popover};
pub use progress::{AnimatedProgress, Progress, ProgressSize, progress, progress_animated};
pub use radio::{RadioGroup, RadioGroupBuilder, RadioLayout, RadioSize, radio_group};
pub use resizable::{
    ResizableGroup, ResizableGroupBuilder, ResizablePanelBuilder, ResizeDirection, resizable_group,
    resizable_panel,
};
pub use select::{Select, SelectBuilder, SelectOption, SelectSize, select};
pub use separator::{Separator, SeparatorOrientation, separator};
pub use sheet::{
    SheetBuilder, SheetSide, SheetSize, sheet, sheet_bottom, sheet_left, sheet_right, sheet_top,
};
pub use sidebar::{Sidebar, SidebarBuilder, SidebarItem, SidebarSection, sidebar};
pub use skeleton::{Skeleton, skeleton, skeleton_circle};
pub use slider::{Slider, SliderBuilder, SliderSize, slider};
pub use spinner::{Spinner, SpinnerBuilder, SpinnerSize, spinner};
pub use switch::{Switch, SwitchSize, switch};
pub use table::{
    TableRow, table, table_body, table_caption, table_cell, table_footer, table_head, table_header,
    table_row,
};
pub use tabs::{TabMenuItem, Tabs, TabsBuilder, TabsSize, TabsTransition, tab_item, tabs};
pub use textarea::{Textarea, TextareaSize, textarea};
pub use toast::{
    ToastBuilder, ToastVariant, toast, toast_custom, toast_error, toast_success, toast_warning,
};
pub use toggle::{Toggle, ToggleBuilder, ToggleSize, ToggleVariant, toggle};
pub use toggle_group::{ToggleGroup, ToggleGroupBuilder, ToggleItem, toggle_group, toggle_item};
pub use tooltip::{Tooltip, TooltipAlign, TooltipBuilder, TooltipSide, tooltip};
pub use tree::{TreeNodeConfig, TreeNodeDiff, TreeView, TreeViewBuilder, tree_view};
// Typography helpers (label excluded - use Label component instead)
pub use aspect_ratio::{
    AspectRatio, AspectRatioBuilder, AspectRatioPreset, aspect_ratio, aspect_ratio_4_3,
    aspect_ratio_9_16, aspect_ratio_16_9, aspect_ratio_21_9, aspect_ratio_square,
};
pub use avatar::{
    Avatar, AvatarBuilder, AvatarGroup, AvatarGroupBuilder, AvatarShape, AvatarSize, AvatarStatus,
    avatar, avatar_group,
};
pub use scroll_area::{
    ScrollArea, ScrollAreaBuilder, ScrollAreaSize, ScrollbarVisibility, scroll_area,
};
pub use typography::{
    b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, muted, p, small, span,
    strong,
};
