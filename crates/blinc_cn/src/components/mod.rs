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
pub mod kbd;
pub mod label;
pub mod menubar;
pub mod navigation_menu;
pub mod pagination;
pub mod popover;
pub mod progress;
pub mod radio;
pub mod select;
pub mod separator;
pub mod sheet;
pub mod sidebar;
pub mod skeleton;
pub mod slider;
pub mod spinner;
pub mod switch;
pub mod tabs;
pub mod textarea;
pub mod toast;
pub mod tooltip;
pub mod tree;
pub mod typography;
pub mod resizable;

// Re-export all components
pub use accordion::{accordion, Accordion, AccordionBuilder, AccordionMode};
pub use alert::{alert, alert_box, Alert, AlertBox, AlertVariant};
pub use badge::{badge, Badge, BadgeVariant};
pub use breadcrumb::{
    breadcrumb, Breadcrumb, BreadcrumbBuilder, BreadcrumbItem, BreadcrumbSeparator, BreadcrumbSize,
};
pub use button::{button, Button, ButtonBuilder, ButtonSize, ButtonVariant, IconPosition};
pub use collapsible::{
    collapsible, collapsible_section, Collapsible, CollapsibleBuilder, CollapsibleTrigger,
};
// Re-export ButtonState for users who need it
pub use blinc_layout::stateful::ButtonState;
pub use card::{
    card, card_content, card_footer, card_header, Card, CardContent, CardFooter, CardHeader,
};
pub use chart::{
    bar_chart, comparison_bar_chart, histogram, line_chart, spark_line, threshold_line_chart,
    BarChart, BarChartBuilder, ChartGrid, ComparisonBarChart, ComparisonBarChartBuilder, DataPoint,
    DataSeries, Histogram, HistogramBuilder, LineChart, LineChartBuilder, SparkLine,
    SparkLineBuilder, ThresholdBand, ThresholdLineChart, ThresholdLineChartBuilder,
};
pub use checkbox::{checkbox, Checkbox, CheckboxSize};
pub use combobox::{combobox, Combobox, ComboboxBuilder, ComboboxOption, ComboboxSize};
pub use context_menu::{context_menu, ContextMenuBuilder, ContextMenuItem, SubmenuBuilder};
pub use dialog::{alert_dialog, dialog, AlertDialogBuilder, DialogBuilder, DialogSize};
pub use drawer::{drawer, drawer_left, drawer_right, DrawerBuilder, DrawerSide, DrawerSize};
pub use dropdown_menu::{
    dropdown_menu, dropdown_menu_custom, DropdownAlign, DropdownMenuBuilder, DropdownPosition,
};
pub use hover_card::{hover_card, HoverCard, HoverCardAlign, HoverCardBuilder, HoverCardSide};
pub use icon::{icon, Icon, IconBuilder, IconSize};
pub use input::{input, Input, InputBgColors, InputBorderColors, InputSize};
pub use kbd::{kbd, Kbd, KbdBuilder, KbdSize};
pub use label::{label, Label, LabelBuilder, LabelSize};
pub use menubar::{
    menubar, MenuTriggerMode, MenuTriggerStyle, Menubar, MenubarBuilder, MenubarMenu,
    MenubarTrigger,
};
pub use navigation_menu::{
    navigation_link, navigation_menu, NavigationLink, NavigationLinkBuilder, NavigationMenu,
    NavigationMenuBuilder,
};
pub use pagination::{pagination, Pagination, PaginationBuilder, PaginationSize};
pub use popover::{popover, Popover, PopoverAlign, PopoverBuilder, PopoverSide};
pub use progress::{progress, progress_animated, AnimatedProgress, Progress, ProgressSize};
pub use radio::{radio_group, RadioGroup, RadioGroupBuilder, RadioLayout, RadioSize};
pub use select::{select, Select, SelectBuilder, SelectOption, SelectSize};
pub use separator::{separator, Separator, SeparatorOrientation};
pub use sheet::{
    sheet, sheet_bottom, sheet_left, sheet_right, sheet_top, SheetBuilder, SheetSide, SheetSize,
};
pub use sidebar::{sidebar, Sidebar, SidebarBuilder, SidebarItem, SidebarSection};
pub use skeleton::{skeleton, skeleton_circle, Skeleton};
pub use slider::{slider, Slider, SliderSize};
pub use spinner::{spinner, Spinner, SpinnerSize};
pub use switch::{switch, Switch, SwitchSize};
pub use tabs::{tab_item, tabs, TabMenuItem, Tabs, TabsBuilder, TabsSize, TabsTransition};
pub use textarea::{textarea, Textarea, TextareaSize};
pub use toast::{
    toast, toast_custom, toast_error, toast_success, toast_warning, ToastBuilder, ToastVariant,
};
pub use tooltip::{tooltip, Tooltip, TooltipAlign, TooltipBuilder, TooltipSide};
pub use tree::{tree_view, TreeNodeConfig, TreeNodeDiff, TreeView, TreeViewBuilder};
pub use resizable::{
    resizable_group, resizable_panel, ResizableGroup, ResizableGroupBuilder, ResizablePanelBuilder,
    ResizeDirection,
};
// Typography helpers (label excluded - use Label component instead)
pub use typography::{
    b, caption, chained_text, h1, h2, h3, h4, h5, h6, heading, inline_code, muted, p, small, span,
    strong,
};
