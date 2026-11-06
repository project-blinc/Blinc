//! Pre-resolved CSS state-style cascade table — Phase 5 of the unified
//! property channel ([[project-reactive-architecture-v2]]).
//!
//! # What it replaces
//!
//! The current `apply_stylesheet_state_styles` (in
//! `renderer/stylesheet/state.rs`) walks every registered element id
//! every frame, asks the `Stylesheet` for `:hover` / `:active` /
//! `:focus` matches, and writes the resolved style into `RenderProps` +
//! the taffy `Style`. The rule-walk cost is proportional to the
//! stylesheet's rule count — on `cn_demo`'s ~500-rule `.cn-*` cascade
//! this is the dominant `apply_stylesheet_state_styles` cost.
//!
//! # The table
//!
//! Pre-resolve the cascade at stylesheet-bind time (or on demand the
//! first frame after a class-set change). For each registered element
//! id × `ElementState`, store the `ElementStyle` the cascade resolves
//! to. Runtime apply becomes a `HashMap` lookup keyed by
//! `(StableNodeId, ElementState)` plus a property-channel queue push —
//! no rule walk, no string concatenation, no per-frame stylesheet API
//! calls.
//!
//! # Phase 5 substep status
//!
//! - **5.1 (this commit):** ship the data structure + builder + tests.
//!   No consumer migration — the table is built but unread, so existing
//!   behaviour is preserved exactly. Validates the structure with unit
//!   tests before touching the hot path.
//! - **5.2 (next):** migrate `apply_state_styles` to read from the
//!   table when present; fall back to the rule-walk when absent or
//!   stale.
//! - **5.3 (final):** wire build/invalidate triggers (stylesheet-bind,
//!   class-set change), default-on, retire the rule-walk path.

use std::collections::HashMap;

use crate::css_parser::{ElementState, Stylesheet};
use crate::element_style::ElementStyle;
use crate::tree::StableNodeId;

/// Pre-resolved state-style cascade. For each (stable node, state),
/// stores the `ElementStyle` that should apply when the node is in
/// that state. Base styles (no state) live in a parallel map keyed
/// by `StableNodeId` alone.
#[derive(Debug, Default)]
pub struct StateStyleTable {
    /// `#id` base styles. Lookup at apply-time replaces
    /// `stylesheet.get(&element_id)`.
    base: HashMap<StableNodeId, ElementStyle>,
    /// `#id:state` styles. Lookup at apply-time replaces
    /// `stylesheet.get_with_state(&element_id, state)`.
    by_state: HashMap<(StableNodeId, ElementState), ElementStyle>,
    /// Build-generation watermark — the `RenderTree.build_generation`
    /// the table was built against. Used by consumers to detect
    /// staleness after a structural rebuild and trigger a rebuild.
    build_generation: u64,
    /// True if the source stylesheet was present and at least one
    /// entry was resolved. Otherwise the table is a no-op and the
    /// consumer should fall back to the rule-walk path.
    populated: bool,
}

impl StateStyleTable {
    /// Empty table. Consumers should treat this as "no table" and use
    /// the rule-walk fallback.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the table has any resolved entries. False if no
    /// stylesheet was bound, or if the bound stylesheet had no rules
    /// matching any registered element.
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Build-generation watermark. Caller compares against
    /// `RenderTree.build_generation`; mismatch = rebuild needed.
    pub fn build_generation(&self) -> u64 {
        self.build_generation
    }

    /// Number of `(node, state)` entries — diagnostics / tests.
    pub fn state_entry_count(&self) -> usize {
        self.by_state.len()
    }

    /// Number of nodes with a base-rule entry — diagnostics / tests.
    pub fn base_entry_count(&self) -> usize {
        self.base.len()
    }

    /// Look up the base style for a node. `None` if no `#id { ... }`
    /// rule matched at build time.
    pub fn get_base(&self, stable_id: StableNodeId) -> Option<&ElementStyle> {
        self.base.get(&stable_id)
    }

    /// Look up the state style for `(node, state)`. `None` if no
    /// `#id:state { ... }` rule matched at build time.
    pub fn get_state(&self, stable_id: StableNodeId, state: ElementState) -> Option<&ElementStyle> {
        self.by_state.get(&(stable_id, state))
    }

    /// Build the table from a stylesheet + a snapshot of registered
    /// elements (id → stable node id).
    ///
    /// Walks each registered element, looks up the cascade for base +
    /// all `ElementState` variants, stores hits.
    ///
    /// This is the foundation pass — id-based rules only. Class-based
    /// rules (`.cn-button:hover { ... }`) flow through the separate
    /// `apply_complex_selector_styles` path and aren't pre-resolved
    /// here yet (would need to resolve each node's class set against
    /// the stylesheet's class rules + handle specificity).
    pub fn build(
        stylesheet: &Stylesheet,
        elements: impl IntoIterator<Item = (String, StableNodeId)>,
        build_generation: u64,
    ) -> Self {
        const ALL_STATES: &[ElementState] = &[
            ElementState::Hover,
            ElementState::Active,
            ElementState::Focus,
            ElementState::FocusVisible,
            ElementState::Disabled,
            ElementState::Checked,
        ];

        let mut base: HashMap<StableNodeId, ElementStyle> = HashMap::new();
        let mut by_state: HashMap<(StableNodeId, ElementState), ElementStyle> = HashMap::new();

        for (element_id, stable_id) in elements {
            if let Some(style) = stylesheet.get(&element_id) {
                base.insert(stable_id, style.clone());
            }
            for &state in ALL_STATES {
                if let Some(style) = stylesheet.get_with_state(&element_id, state) {
                    by_state.insert((stable_id, state), style.clone());
                }
            }
        }

        let populated = !base.is_empty() || !by_state.is_empty();

        Self {
            base,
            by_state,
            build_generation,
            populated,
        }
    }

    /// Drop every entry. Used when the stylesheet is unbound or
    /// replaced before a fresh build.
    pub fn clear(&mut self) {
        self.base.clear();
        self.by_state.clear();
        self.populated = false;
        // `build_generation` left as-is; the consumer's build call
        // will overwrite it.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a CSS string into a Stylesheet for testing.
    fn parse(css: &str) -> Stylesheet {
        Stylesheet::parse_with_errors(css).stylesheet
    }

    /// `StableNodeId::derive_child` requires real parents; for these
    /// tests we synthesise distinct ids by deriving from ROOT with
    /// different sibling indices.
    fn fake_stable(i: usize) -> StableNodeId {
        crate::tree::StableNodeId::ROOT.derive_child(i, None)
    }

    #[test]
    fn empty_stylesheet_produces_unpopulated_table() {
        let ss = parse("");
        let table = StateStyleTable::build(&ss, std::iter::empty(), 1);
        assert!(!table.is_populated());
        assert_eq!(table.base_entry_count(), 0);
        assert_eq!(table.state_entry_count(), 0);
    }

    #[test]
    fn no_registered_elements_produces_unpopulated_table() {
        let ss = parse("#btn { background: red; } #btn:hover { background: blue; }");
        let table = StateStyleTable::build(&ss, std::iter::empty(), 1);
        assert!(!table.is_populated());
    }

    #[test]
    fn id_with_base_and_hover_lands_in_table() {
        let ss = parse(
            "
            #btn { background: #ff0000; }
            #btn:hover { background: #00ff00; }
            ",
        );
        let nid = fake_stable(0);
        let table = StateStyleTable::build(&ss, std::iter::once(("btn".to_string(), nid)), 1);
        assert!(table.is_populated());
        assert!(table.get_base(nid).is_some(), "base style present");
        assert!(
            table.get_state(nid, ElementState::Hover).is_some(),
            "hover style present"
        );
        assert!(
            table.get_state(nid, ElementState::Active).is_none(),
            "active not in stylesheet"
        );
    }

    #[test]
    fn multiple_states_for_one_node() {
        let ss = parse(
            "
            #btn { background: red; }
            #btn:hover { background: green; }
            #btn:active { background: blue; }
            #btn:focus { background: yellow; }
            #btn:disabled { opacity: 0.5; }
            ",
        );
        let nid = fake_stable(0);
        let table = StateStyleTable::build(&ss, std::iter::once(("btn".to_string(), nid)), 1);
        for state in [
            ElementState::Hover,
            ElementState::Active,
            ElementState::Focus,
        ] {
            assert!(
                table.get_state(nid, state).is_some(),
                "{state:?} entry must be present"
            );
        }
        // `:disabled` and `:checked` rules are complex rules, not entries of the table.
        assert!(table.get_state(nid, ElementState::Disabled).is_none());
        assert!(table.get_state(nid, ElementState::Checked).is_none());
    }

    #[test]
    fn multiple_elements_each_with_state_styles() {
        let ss = parse(
            "
            #a { background: red; }
            #a:hover { background: pink; }
            #b { background: blue; }
            #b:hover { background: cyan; }
            ",
        );
        let a = fake_stable(0);
        let b = fake_stable(1);
        let table = StateStyleTable::build(&ss, [("a".to_string(), a), ("b".to_string(), b)], 1);
        assert_eq!(table.base_entry_count(), 2);
        assert_eq!(table.state_entry_count(), 2);
        assert!(table.get_state(a, ElementState::Hover).is_some());
        assert!(table.get_state(b, ElementState::Hover).is_some());
    }

    #[test]
    fn element_without_matching_rule_produces_no_entry() {
        let ss = parse("#known { background: red; } #known:hover { opacity: 0.8; }");
        let unknown = fake_stable(0);
        let table =
            StateStyleTable::build(&ss, std::iter::once(("unknown".to_string(), unknown)), 1);
        assert!(!table.is_populated());
        assert!(table.get_base(unknown).is_none());
        assert!(table.get_state(unknown, ElementState::Hover).is_none());
    }

    #[test]
    fn clear_resets_table() {
        let ss = parse("#btn { background: red; }");
        let nid = fake_stable(0);
        let mut table = StateStyleTable::build(&ss, std::iter::once(("btn".to_string(), nid)), 1);
        assert!(table.is_populated());
        table.clear();
        assert!(!table.is_populated());
        assert_eq!(table.base_entry_count(), 0);
    }

    #[test]
    fn build_generation_watermark_preserved() {
        let ss = parse("#btn { background: red; }");
        let nid = fake_stable(0);
        let table = StateStyleTable::build(&ss, std::iter::once(("btn".to_string(), nid)), 42);
        assert_eq!(table.build_generation(), 42);
    }
}
