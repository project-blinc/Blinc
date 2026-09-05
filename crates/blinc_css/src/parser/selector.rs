//! Selector data model.
//!
//! A [`ComplexSelector`] is a chain of [`CompoundSelector`]s joined by
//! [`Combinator`]s, read right to left when matching. Each compound is a
//! set of [`SelectorPart`]s that must all hold on one element, plus an
//! optional [`ElementState`] for `:hover`, `:active` and friends.
//!
//! The grammar that builds these lives in `grammar::selector`.

/// Element state for pseudo-class selectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementState {
    /// :hover pseudo-class
    Hover,
    /// :active pseudo-class (pressed)
    Active,
    /// :focus pseudo-class
    Focus,
    /// :disabled pseudo-class
    Disabled,
    /// :checked pseudo-class (checkboxes, radios)
    Checked,
}

impl ElementState {
    /// Parse a state from a pseudo-class string
    pub fn parse_state(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hover" => Some(ElementState::Hover),
            "active" => Some(ElementState::Active),
            "focus" => Some(ElementState::Focus),
            "disabled" => Some(ElementState::Disabled),
            "checked" => Some(ElementState::Checked),
            _ => None,
        }
    }
}

impl std::fmt::Display for ElementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElementState::Hover => write!(f, "hover"),
            ElementState::Active => write!(f, "active"),
            ElementState::Focus => write!(f, "focus"),
            ElementState::Disabled => write!(f, "disabled"),
            ElementState::Checked => write!(f, "checked"),
        }
    }
}

/// A parsed CSS selector with optional state modifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CssSelector {
    /// The element ID (without #)
    pub id: String,
    /// Optional state modifier (:hover, :active, :focus, :disabled)
    pub state: Option<ElementState>,
}

impl CssSelector {
    /// Create a selector for an ID without state
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            state: None,
        }
    }

    /// Create a selector with a state modifier
    pub fn with_state(id: impl Into<String>, state: ElementState) -> Self {
        Self {
            id: id.into(),
            state: Some(state),
        }
    }

    /// Get the storage key for this selector
    pub(crate) fn key(&self) -> String {
        match &self.state {
            Some(state) => format!("{}:{}", self.id, state),
            None => self.id.clone(),
        }
    }
}

// ============================================================================
// Complex Selector Types (Phase 4: Selector Hierarchy)
// ============================================================================

/// Structural pseudo-class selectors
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StructuralPseudo {
    /// :first-child
    FirstChild,
    /// :last-child
    LastChild,
    /// :nth-child(n) — 1-based index
    NthChild(usize),
    /// :nth-last-child(n) — 1-based from end
    NthLastChild(usize),
    /// :only-child
    OnlyChild,
    /// :empty — no children
    Empty,
    /// :root — root element
    Root,
    /// :first-of-type — equivalent to :first-child in single-type DOM (Blinc)
    FirstOfType,
    /// :last-of-type — equivalent to :last-child in single-type DOM (Blinc)
    LastOfType,
    /// :nth-of-type(n) — equivalent to :nth-child(n) in single-type DOM (Blinc)
    NthOfType(usize),
    /// :nth-last-of-type(n) — equivalent to :nth-last-child(n) in single-type DOM (Blinc)
    NthLastOfType(usize),
    /// :only-of-type — equivalent to :only-child in single-type DOM (Blinc)
    OnlyOfType,
}

/// A single part of a compound selector
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SelectorPart {
    /// Element type selector (e.g., button, a, ul, input)
    Type(String),
    /// #id selector
    Id(String),
    /// .class selector
    Class(String),
    /// * universal selector (matches any element)
    Universal,
    /// :hover, :active, :focus, :disabled
    State(ElementState),
    /// :first-child, :last-child, :nth-child(n), :only-child, :empty, :root
    PseudoClass(StructuralPseudo),
    /// :not(selector) — negation pseudo-class
    Not(Box<CompoundSelector>),
    /// :is(selector, ...) / :where(selector, ...) — matches if any inner selector matches
    Is(Vec<CompoundSelector>),
    /// :has(selector, ...) — relational selector. Matches when ANY listed
    /// relative selector matches against the subject's descendants /
    /// children / siblings. Unlike :is, the inner is a full
    /// `ComplexSelector` because :has natively supports combinators
    /// (e.g. `:has(> .child)`, `:has(+ .sibling)`, `:has(.deep .nested)`).
    Has(Vec<ComplexSelector>),
    /// ::placeholder, ::selection, etc. — pseudo-elements
    PseudoElement(String),
}

/// Combinator between compound selectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Combinator {
    /// Descendant combinator (space): `#parent .child`
    Descendant,
    /// Child combinator (>): `#parent > .child`
    Child,
    /// Adjacent sibling combinator (+): `#prev + .next`
    AdjacentSibling,
    /// General sibling combinator (~): `#prev ~ .later`
    GeneralSibling,
}

/// A compound selector is a sequence of simple selectors with no combinator.
/// e.g. `#id.class:hover` = [Id("id"), Class("class"), State(Hover)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompoundSelector {
    pub parts: Vec<SelectorPart>,
}

impl CompoundSelector {
    /// Check if this compound selector contains any state pseudo-class
    pub fn has_state(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, SelectorPart::State(_)))
    }

    /// Get the state pseudo-class if present
    pub fn get_state(&self) -> Option<&ElementState> {
        self.parts.iter().find_map(|p| match p {
            SelectorPart::State(s) => Some(s),
            _ => None,
        })
    }

    /// Check if this compound selector contains any structural pseudo-class
    pub fn has_structural_pseudo(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, SelectorPart::PseudoClass(_)))
    }
}

/// A complex selector is a chain of compound selectors joined by combinators.
/// e.g. `#parent:hover > .child:first-child`
/// segments: [(parent compound, Some(Child)), (child compound, None)]
///
/// The last segment always has `combinator = None` (it's the target element).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComplexSelector {
    pub segments: Vec<(CompoundSelector, Option<Combinator>)>,
}

impl ComplexSelector {
    /// Get the rightmost (target) compound selector
    pub fn target(&self) -> Option<&CompoundSelector> {
        self.segments.last().map(|(compound, _)| compound)
    }

    /// Check if this complex selector has any state pseudo-classes
    pub fn has_state(&self) -> bool {
        self.segments
            .iter()
            .any(|(compound, _)| compound.has_state())
    }

    /// Returns true if this is a simple selector (single compound, no combinators)
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// If this is a simple `.class` selector (single segment, single Class part, no state),
    /// returns the class name. Used for O(1) class-based style lookups.
    pub fn simple_class_name(&self) -> Option<&str> {
        if self.segments.len() != 1 {
            return None;
        }
        let parts = &self.segments[0].0.parts;
        if parts.len() == 1 {
            if let SelectorPart::Class(name) = &parts[0] {
                return Some(name.as_str());
            }
        }
        None
    }

    /// Extract the class name from a single-segment selector that may include state parts.
    /// For `.cn-sidebar-item:hover`, returns `Some("cn-sidebar-item")`.
    /// For `.class` (no state), also returns the class name.
    /// Returns None for multi-segment selectors or selectors without a Class part.
    pub fn class_name_with_state(&self) -> Option<&str> {
        if self.segments.len() != 1 {
            return None;
        }
        let parts = &self.segments[0].0.parts;
        for part in parts {
            if let SelectorPart::Class(name) = part {
                return Some(name.as_str());
            }
        }
        None
    }
}
