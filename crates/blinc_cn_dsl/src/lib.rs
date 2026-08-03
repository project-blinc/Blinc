//! DSL bindings for the `blinc_cn` widget pack.
//!
//! Exposes shadcn-style components to the Blinc DSL under the `cn.*`
//! namespace:
//!
//! ```dsl,ignore
//! view {
//!     cn.Button("Save", variant = "primary", on_click = || {
//!         saved.set(true)
//!     })
//! }
//! ```
//!
//! Each widget wrapper lives in its own module and uses
//! `#[extern_widget(namespace = "cn", name = "<Name>")]` to register
//! under the qualified DSL name. The grammar's namespaced
//! component-call rule routes `cn.<Name>(...)` to the matching
//! wrapper.
//!
//! ## Adoption
//!
//! ```ignore
//! let dsl = BlincDsl::new()?;
//! blinc_cn_dsl::register_all(&dsl)?;
//! dsl.compile_source(src, file)?;
//! ```
//!
//! `register_all` registers every widget this crate exposes. Pick a
//! focused subset via the per-category helpers ([`register_basics`])
//! when binary size matters or you only want a slice.
//!
//! ## What's exposed
//!
//! Leaf widgets shipping today:
//! - [`button`] — `cn.Button`, with `on_click` closure prop.
//! - [`badge`] — `cn.Badge`.
//! - [`alert`] — `cn.Alert`.
//! - [`label`] — `cn.Label`.
//! - [`separator`] — `cn.Separator`.
//! - [`spinner`] — `cn.Spinner`.
//!
//! Container widgets:
//! - [`card`] — `cn.Card { children… }`. Body block flows through
//!   the macro's existing `#[children]` plumbing.
//!
//! Heavier container surface (`Dialog`, `Combobox`, `Tabs`, `Drawer`,
//! `Table`, …) lands incrementally as each widget's prop / slot
//! shape gets wired.

pub mod accordion;
pub mod accordion_item;
pub mod alert;
pub mod aspect_ratio;
pub mod avatar;
pub mod badge;
pub mod breadcrumb;
pub mod bridge;
pub mod button;
pub mod card;
pub mod checkbox;
pub mod collapsible;
pub mod combobox;
pub mod dialog;
pub mod drawer;
pub mod hover_card;
pub mod icon;
pub mod input;
pub mod input_otp;
pub mod kbd;
pub mod label;
pub mod modal;
pub mod number_input;
pub mod option;
pub mod popover;
pub mod popover_slots;
pub mod progress;
pub mod radio;
pub mod radio_group;
pub mod resizable;
pub mod scroll_area;
pub mod select;
pub mod separator;
pub mod shared_child;
pub mod sheet;
pub mod sidebar;
pub mod sidebar_content;
pub mod sidebar_item;
pub mod sidebar_section;
pub mod skeleton;
pub mod slider;
pub mod spinner;
pub mod switch;
pub mod tab;
pub mod tabs;
pub mod textarea;
pub mod toggle;
pub mod toggle_group;
pub mod tooltip;
pub mod typography;

// Internal — shared helpers used by per-widget modules. Not
// re-exported; widgets pull what they need via `crate::color::…`.
pub(crate) mod color;

pub use accordion::CnAccordion;
pub use accordion_item::CnAccordionItem;
pub use alert::CnAlert;
pub use aspect_ratio::CnAspectRatio;
pub use avatar::CnAvatar;
pub use badge::CnBadge;
pub use breadcrumb::CnBreadcrumb;
pub use button::CnButton;
pub use card::CnCard;
pub use checkbox::CnCheckbox;
pub use collapsible::CnCollapsible;
pub use combobox::CnCombobox;
pub use dialog::CnDialog;
pub use drawer::CnDrawer;
pub use hover_card::{CnHoverCard, CnHoverCardContent, CnHoverCardTrigger};
pub use icon::CnIcon;
pub use input::CnInput;
pub use input_otp::CnInputOTP;
pub use kbd::CnKbd;
pub use label::CnLabel;
pub use number_input::CnNumberInput;
pub use option::CnOption;
pub use popover::CnPopover;
pub use popover_slots::{CnPopoverContent, CnPopoverTrigger};
pub use progress::CnProgress;
pub use radio::CnRadio;
pub use radio_group::CnRadioGroup;
pub use resizable::{CnResizableGroup, CnResizablePanel};
pub use scroll_area::CnScrollArea;
pub use select::CnSelect;
pub use separator::CnSeparator;
pub use sheet::CnSheet;
pub use sidebar::CnSidebar;
pub use sidebar_content::CnSidebarContent;
pub use sidebar_item::CnSidebarItem;
pub use sidebar_section::CnSidebarSection;
pub use skeleton::CnSkeleton;
pub use slider::CnSlider;
pub use spinner::CnSpinner;
pub use switch::CnSwitch;
pub use tab::CnTab;
pub use tabs::CnTabs;
pub use textarea::CnTextarea;
pub use toggle::CnToggle;
pub use toggle_group::CnToggleGroup;
pub use tooltip::CnTooltip;
pub use typography::{
    CnB, CnCaption, CnChainedText, CnH1, CnH2, CnH3, CnH4, CnH5, CnH6, CnInlineCode, CnMuted, CnP,
    CnSmall, CnSpan, CnStrong,
};

use blinc_dsl_core::{BlincDsl, BlincDslResult};

// =====================================================================
// Registration helpers
// =====================================================================

/// Register every `cn.*` widget this crate exposes with the supplied
/// `BlincDsl`. Call once after `BlincDsl::new()`, before
/// `compile_source`.
///
/// Also queues `blinc_cn::cn_styles::CN_STYLES` through the global
/// `BlincContextState` so the widgets render with their default
/// shadcn-style appearance once the renderer starts pulling from the
/// stylesheet queue. Without this, `cn.Button` ships with no
/// background / padding / typography and reads as invisible — a
/// common "the buttons don't work" symptom on first wiring.
///
/// Returns the first registration error if one occurs; subsequent
/// widgets are not attempted on failure. The error type is
/// [`blinc_dsl_core::BlincDslError`] from the underlying
/// `register_extern_widget` call.
pub fn register_all(dsl: &BlincDsl) -> BlincDslResult<()> {
    register_basics(dsl)?;
    // Queue cn's default stylesheet through the free-function entry
    // point. `BlincContextState::get()` panics pre-init (matters in
    // tests / headless harnesses that never construct a context),
    // but `queue_pending_stylesheet` buffers strings into a static
    // queue that the runner drains once it constructs the context.
    // So this is safe to call from any host setup path, including
    // unit tests.
    blinc_core::context_state::queue_pending_stylesheet(blinc_cn::cn_styles::CN_STYLES);
    Ok(())
}

/// Register the leaf-widget basics — every `cn.*` wrapper currently
/// shipped by this crate. Stays callable independently so an app that
/// adds heavier container widgets later can pick categories instead
/// of always paying for the full surface.
pub fn register_basics(dsl: &BlincDsl) -> BlincDslResult<()> {
    dsl.register_extern_widget::<CnButton>()?;
    dsl.register_extern_widget::<CnBadge>()?;
    dsl.register_extern_widget::<CnBreadcrumb>()?;
    dsl.register_extern_widget::<CnAccordion>()?;
    dsl.register_extern_widget::<CnAccordionItem>()?;
    dsl.register_extern_widget::<CnAlert>()?;
    dsl.register_extern_widget::<CnLabel>()?;
    dsl.register_extern_widget::<CnScrollArea>()?;
    dsl.register_extern_widget::<CnSeparator>()?;
    dsl.register_extern_widget::<CnSidebar>()?;
    dsl.register_extern_widget::<CnSidebarSection>()?;
    dsl.register_extern_widget::<CnSidebarItem>()?;
    dsl.register_extern_widget::<CnSidebarContent>()?;
    dsl.register_extern_widget::<CnSpinner>()?;
    dsl.register_extern_widget::<CnAspectRatio>()?;
    dsl.register_extern_widget::<CnCard>()?;
    dsl.register_extern_widget::<CnCollapsible>()?;
    dsl.register_extern_widget::<CnProgress>()?;
    dsl.register_extern_widget::<CnAvatar>()?;
    dsl.register_extern_widget::<CnSkeleton>()?;
    dsl.register_extern_widget::<CnKbd>()?;
    dsl.register_extern_widget::<CnIcon>()?;
    dsl.register_extern_widget::<CnInput>()?;
    dsl.register_extern_widget::<CnInputOTP>()?;
    dsl.register_extern_widget::<CnTextarea>()?;
    dsl.register_extern_widget::<CnSwitch>()?;
    dsl.register_extern_widget::<CnCheckbox>()?;
    dsl.register_extern_widget::<CnToggle>()?;
    dsl.register_extern_widget::<CnToggleGroup>()?;
    dsl.register_extern_widget::<CnSlider>()?;
    dsl.register_extern_widget::<CnRadioGroup>()?;
    dsl.register_extern_widget::<CnSelect>()?;
    dsl.register_extern_widget::<CnCombobox>()?;
    dsl.register_extern_widget::<CnOption>()?;
    dsl.register_extern_widget::<CnRadio>()?;
    dsl.register_extern_widget::<CnResizableGroup>()?;
    dsl.register_extern_widget::<CnResizablePanel>()?;
    dsl.register_extern_widget::<CnTabs>()?;
    dsl.register_extern_widget::<CnTab>()?;
    dsl.register_extern_widget::<CnNumberInput>()?;
    dsl.register_extern_widget::<CnTooltip>()?;
    dsl.register_extern_widget::<CnH1>()?;
    dsl.register_extern_widget::<CnH2>()?;
    dsl.register_extern_widget::<CnH3>()?;
    dsl.register_extern_widget::<CnH4>()?;
    dsl.register_extern_widget::<CnH5>()?;
    dsl.register_extern_widget::<CnH6>()?;
    dsl.register_extern_widget::<CnP>()?;
    dsl.register_extern_widget::<CnMuted>()?;
    dsl.register_extern_widget::<CnCaption>()?;
    dsl.register_extern_widget::<CnSpan>()?;
    dsl.register_extern_widget::<CnB>()?;
    dsl.register_extern_widget::<CnStrong>()?;
    dsl.register_extern_widget::<CnSmall>()?;
    dsl.register_extern_widget::<CnInlineCode>()?;
    dsl.register_extern_widget::<CnChainedText>()?;
    dsl.register_extern_widget::<CnPopover>()?;
    dsl.register_extern_widget::<CnPopoverTrigger>()?;
    dsl.register_extern_widget::<CnPopoverContent>()?;
    dsl.register_extern_widget::<CnHoverCard>()?;
    dsl.register_extern_widget::<CnHoverCardTrigger>()?;
    dsl.register_extern_widget::<CnHoverCardContent>()?;
    dsl.register_extern_widget::<CnDialog>()?;
    dsl.register_extern_widget::<CnSheet>()?;
    dsl.register_extern_widget::<CnDrawer>()?;
    Ok(())
}
