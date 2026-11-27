# Changelog

## [Unreleased]

### Added
- `Stylesheet::layout_reads_ancestor_position()` says whether a stateless layout rule reads where an ancestor of its subject sits among its siblings, as `.row:last-child .cell` or `.a + .b .c` do.
- `Stylesheet::stateless_layout_rules()` says, for each complex rule, whether it sets a layout property and needs no interaction state. It is worked out with the rule index, once per set of rules.
- `RuleKey` and `Stylesheet::rule_key(index)`: what a complex rule's subject needs of the elements it matches (an id, a class, an attribute name, a type, or nothing), the key the rule is indexed under.
- `Stylesheet::candidate_rules(id, classes, element_type)` lists, in the order they were written, the complex rules that can match an element with that id, those classes (an entry `name=value` is also the attribute `name`) and that type. Each rule is filed under the most selective thing its subject compound needs (an id, a class, an attribute name or a type; a subject that needs none is in every element's candidates), and the index is made on first use and dropped when stylesheets are merged.
- Declaration-level `!important` support for background, text and uniform border colours, with component-layer precedence, specificity and source order. Unsupported importance properties report an error.
- `ElementStyle` gains `aspect_ratio` and optional colour-priority metadata (`important_colors`). Exhaustive struct literals must initialize both; builder and `..Default::default()` construction remain valid.
- CSS `aspect-ratio` accepts positive numbers, fractions and `auto`, and participates in layout-style merging.
- `ElementStyle::inset_shadow`, `inset_shadow_stack` and `shadow_inner` set inset shadows, and `shadow_none` clears them.

### Fixed
- `ElementStyle::merge` treats `shadow` and `inner_shadow` as the one `box-shadow` value: a style that sets either replaces both. `ElementStyle::sets_box_shadow` reports whether a style sets it.
- `font-family` keeps every named family before the first generic keyword, in order (`StyleFontFamily::name` is the stack joined with `, `), and the generic keyword ends the list as in CSS. A quoted name may hold a comma and a quoted `serif` is a family called serif, not the generic. Before, only the first name was kept, so `font-family: "Inter", "Helvetica Neue", Arial, sans-serif` went from Inter straight to the generic sans.
- Root variables apply to rules declared earlier, including rules in component layers.
- Incomplete blocks, strings and comments report syntax errors so live editors can retain the last valid stylesheet.
- `box-shadow` and `text-shadow` accept every order CSS allows: two to four lengths, the colour before or after them or left out (black), and `inset` on either side. As in CSS, a layer that does not parse, or a negative blur, rejects the whole value.
- A shadow with no colour, or `currentColor`, takes its element's text colour, in `box-shadow`, `text-shadow` and `drop-shadow()`. It is carried as `Shadow::CURRENT_COLOR`. `drop-shadow()` also takes its colour before or after the lengths, and a blur is optional.
