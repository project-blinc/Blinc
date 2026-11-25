//! The find and replace bar the cn code editor floats at its top right.

use std::sync::Arc;

use blinc_core::Signal;
use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::computed;
use blinc_icons::icons;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::{Div, div};
use blinc_layout::widgets::code::{
    CodeEditorData, SharedCodeEditorState, focus_code_editor_deferred, refresh_code_editor,
    replace_all_code_matches, replace_code_match,
};
use blinc_layout::widgets::text_input::{
    SharedTextInputData, blur_all_text_inputs, focus_text_input_deferred,
};

use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::input::{InputSize, input};
use crate::components::toggle::{ToggleSize, toggle};
use crate::components::tooltip::{TooltipSide, tooltip};

/// The side of an icon button, which the replace row indents its field by.
const BUTTON: f32 = 28.0;
/// The bar's widest.
const WIDTH: f32 = 460.0;
/// The side of a small toggle, which each option is.
const TOGGLE: f32 = 32.0;
/// The room for the count, which also takes what the others leave.
const COUNT: f32 = 76.0;
/// The space between the tools.
const GAP: f32 = 2.0;
/// The width of what follows each field, the same in both rows so the fields line up: the three
/// options, the count and the three buttons.
const TOOLS: f32 = 3.0 * TOGGLE + COUNT + 3.0 * BUTTON + 6.0 * GAP;

/// What the bar shows of itself. It is made once per editor state, so it outlives rebuilds.
#[derive(Clone, Copy)]
pub(super) struct FindUi {
    pub open: Signal<bool>,
    replacing: Signal<bool>,
    /// Bumped when the search changes, for what the bar says of it.
    revision: Signal<u32>,
    match_case: Signal<bool>,
    whole_word: Signal<bool>,
    regex: Signal<bool>,
}

/// One of the search's options.
#[derive(Clone, Copy)]
enum Choice {
    Case,
    Word,
    Regex,
}

impl Choice {
    fn read(self, data: &CodeEditorData) -> bool {
        match self {
            Self::Case => data.search_case_sensitive,
            Self::Word => data.search_whole_word,
            Self::Regex => data.search_regex,
        }
    }

    fn write(self, data: &mut CodeEditorData, on: bool) {
        match self {
            Self::Case => data.search_case_sensitive = on,
            Self::Word => data.search_whole_word = on,
            Self::Regex => data.search_regex = on,
        }
        data.execute_search();
    }
}

impl FindUi {
    pub fn new(state: &SharedCodeEditorState) -> Self {
        let key = format!("cn-code-editor-find-{:p}", Arc::as_ptr(state));
        let context = BlincContextState::get();
        let flag = |name: &str| context.use_signal_keyed(&format!("{key}-{name}"), || false);
        Self {
            open: flag("open"),
            replacing: flag("replacing"),
            revision: context.use_signal_keyed(&format!("{key}-revision"), || 0),
            match_case: flag("case"),
            whole_word: flag("word"),
            regex: flag("regex"),
        }
    }

    fn signal(self, choice: Choice) -> Signal<bool> {
        match choice {
            Choice::Case => self.match_case,
            Choice::Word => self.whole_word,
            Choice::Regex => self.regex,
        }
    }

    /// Draw what the search found, in the bar and the editor.
    fn changed(self, state: &SharedCodeEditorState) {
        self.revision.update(|n| n.wrapping_add(1));
        refresh_code_editor(state);
    }

    /// Open the bar, searching for a one-line selection or, without one, the last query, and
    /// put the keyboard in the find field with its text selected.
    pub fn open(self, state: &SharedCodeEditorState, replacing: bool) {
        let field = {
            let Ok(mut d) = state.lock() else {
                return;
            };
            let query = d
                .take_search_seed()
                .unwrap_or_else(|| d.search_query.clone());
            if let Ok(mut field) = d.search_input_state.lock() {
                field.cursor = query.chars().count();
                field.selection_start = (!query.is_empty()).then_some(0);
                field.value = query.clone();
            }
            d.find_open = true;
            d.apply_search_query(query);
            for choice in [Choice::Case, Choice::Word, Choice::Regex] {
                self.signal(choice).set(choice.read(&d));
            }
            Arc::clone(&d.search_input_state)
        };
        if replacing {
            self.replacing.set(true);
        }
        self.open.set(true);
        self.changed(state);
        // The editor lets the keyboard go, or it would take the field's typing too.
        blur_all_text_inputs();
        focus_text_input_deferred(&field);
    }

    /// Close the bar and give the editor the keyboard.
    pub fn close(self, state: &SharedCodeEditorState) {
        if let Ok(mut d) = state.lock() {
            d.find_open = false;
        }
        self.open.set(false);
        // After every handler of this key or press, so none takes the keyboard back.
        focus_code_editor_deferred(state);
    }

    /// Go to the next match or the one before, opening the bar if it is closed.
    pub fn step(self, state: &SharedCodeEditorState, forward: bool) {
        if !self.open.get() {
            return self.open(state, false);
        }
        if let Ok(mut d) = state.lock() {
            if forward {
                d.search_next();
            } else {
                d.search_prev();
            }
        }
        self.changed(state);
    }

    pub fn replace_one(self, state: &SharedCodeEditorState) {
        take_replacement(state);
        replace_code_match(state);
        self.changed(state);
    }

    pub fn replace_all(self, state: &SharedCodeEditorState) {
        take_replacement(state);
        replace_all_code_matches(state);
        self.changed(state);
    }

    /// Flip the option Alt with `key_code` names.
    pub fn flip(self, state: &SharedCodeEditorState, key_code: u32) {
        let choice = match key_code {
            67 => Choice::Case,
            87 => Choice::Word,
            _ => Choice::Regex,
        };
        let on = !self.signal(choice).get();
        self.choose(state, choice, on);
    }

    fn choose(self, state: &SharedCodeEditorState, choice: Choice, on: bool) {
        if let Ok(mut d) = state.lock() {
            choice.write(&mut d, on);
        }
        self.signal(choice).set(on);
        self.changed(state);
    }
}

/// Take the replace field's text as what to replace with.
fn take_replacement(state: &SharedCodeEditorState) {
    if let Ok(mut d) = state.lock() {
        let field = Arc::clone(&d.replace_input_state);
        d.replace_text = field.lock().map(|f| f.value.clone()).unwrap_or_default();
    }
}

/// What the count says: nothing without a query, or which match of how many.
fn count_text(data: &CodeEditorData) -> String {
    if data.search_query.is_empty() {
        String::new()
    } else if data.search_matches.is_empty() {
        "No results".to_string()
    } else {
        format!(
            "{} of {}",
            data.search_match_idx + 1,
            data.search_matches.len()
        )
    }
}

/// A value read from the editor each time the search changes.
fn watch<T: Clone + PartialEq + Default + Send + 'static>(
    state: &SharedCodeEditorState,
    ui: FindUi,
    read: impl Fn(&CodeEditorData) -> T + Send + Sync + 'static,
) -> blinc_core::reactive::Computed<T> {
    let state = Arc::clone(state);
    computed(move |g| {
        g.get(ui.revision);
        state.lock().map(|d| read(&d)).unwrap_or_default()
    })
}

/// An icon button with a tooltip naming it.
fn icon_button(
    icon: &'static str,
    label: &'static str,
    class: &'static str,
    disabled: Reactive<bool>,
    on_click: impl Fn() + Send + Sync + 'static,
) -> Div {
    let on_click = Arc::new(on_click);
    let trigger = move || {
        let on_click = Arc::clone(&on_click);
        div().child(
            button("")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Custom(BUTTON, BUTTON))
                .icon(icon)
                .icon_size(15.0)
                .disabled(disabled.clone())
                .on_click(move |_| on_click())
                .build_component()
                .class(class)
                .attr("aria-label", label),
        )
    };
    div()
        .flex_shrink_0()
        .child(tooltip(trigger).text(label).side(TooltipSide::Bottom))
}

/// A toggle for one of the search's options.
fn option(
    state: &SharedCodeEditorState,
    ui: FindUi,
    choice: Choice,
    icon: &'static str,
    label: &'static str,
) -> Div {
    let state = Arc::clone(state);
    let trigger = move || {
        let state = Arc::clone(&state);
        div().class("cn-code-editor-find-option").child(
            toggle(ui.signal(choice))
                .size(ToggleSize::Small)
                .icon(blinc_icons::to_svg(icon, 15.0))
                .aria_label(label)
                .on_change(move |on| {
                    ui.choose(&state, choice, on);
                    refocus_query(&state);
                }),
        )
    };
    div()
        .flex_shrink_0()
        .child(tooltip(trigger).text(label).side(TooltipSide::Bottom))
}

/// A press on the bar leaves no field focused; the find field takes the keyboard back.
fn refocus_query(state: &SharedCodeEditorState) {
    let field: Option<SharedTextInputData> =
        state.lock().ok().map(|d| Arc::clone(&d.search_input_state));
    if let Some(field) = field {
        focus_text_input_deferred(&field);
    }
}

/// The find row: the replace row's chevron, the query, the options, the count and the arrows.
fn find_row(state: &SharedCodeEditorState, ui: FindUi, query: &SharedTextInputData) -> Div {
    let chevron = {
        let open = computed(move |g| g.get(ui.replacing).unwrap_or(false));
        div()
            .class("cn-code-editor-find-chevron")
            .attr_when("data-state", "open", open)
            .flex_shrink_0()
            .child(icon_button(
                icons::CHEVRON_RIGHT,
                "Replace",
                "cn-code-editor-find-replace-toggle",
                Reactive::Const(false),
                move || ui.replacing.set(!ui.replacing.get()),
            ))
    };
    let field = {
        let state = Arc::clone(state);
        div()
            .class("cn-code-editor-find-query")
            // From nothing, so the controls share the row while there is room for both.
            .flex_1()
            .min_w(140.0)
            .child(
                input(query)
                    .size(InputSize::Small)
                    .placeholder("Find")
                    .w_full()
                    .on_change(move |text| {
                        if let Ok(mut d) = state.lock() {
                            d.apply_search_query(text.to_string());
                        }
                        ui.changed(&state);
                    }),
            )
    };
    let count = watch(state, ui, count_text);
    let failed = watch(state, ui, |d| {
        !d.search_query.is_empty() && d.search_matches.is_empty()
    });
    let none = watch(state, ui, |d| d.search_matches.is_empty());
    let step = |icon, label, class, forward: bool| {
        let state = Arc::clone(state);
        icon_button(
            icon,
            label,
            class,
            none.clone().into_reactive(),
            move || {
                ui.step(&state, forward);
                refocus_query(&state);
            },
        )
    };
    let close = {
        let state = Arc::clone(state);
        icon_button(
            icons::X,
            "Close",
            "cn-code-editor-find-close",
            Reactive::Const(false),
            move || ui.close(&state),
        )
    };
    let tools = div()
        .class("cn-code-editor-find-tools")
        .flex_row()
        // In an editor narrower than the tools, they wrap too.
        .flex_wrap()
        .items_center()
        .min_w(0.0)
        .w(TOOLS)
        .gap_px(GAP)
        .child(option(
            state,
            ui,
            Choice::Case,
            icons::CASE_SENSITIVE,
            "Match case",
        ))
        .child(option(
            state,
            ui,
            Choice::Word,
            icons::WHOLE_WORD,
            "Whole word",
        ))
        .child(option(
            state,
            ui,
            Choice::Regex,
            icons::REGEX,
            "Regular expression",
        ))
        .child(
            div()
                .class("cn-code-editor-find-count")
                .class_when("cn-code-editor-find-count--none", failed)
                .flex_1()
                .overflow_clip()
                .child(blinc_layout::text::Text::bound(count.into_reactive()).no_wrap()),
        )
        .child(step(
            icons::CHEVRON_UP,
            "Previous match",
            "cn-code-editor-find-previous",
            false,
        ))
        .child(step(
            icons::CHEVRON_DOWN,
            "Next match",
            "cn-code-editor-find-next",
            true,
        ))
        .child(close);
    div()
        .class("cn-code-editor-find-row")
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_px(4.0)
        .child(chevron)
        .child(field)
        .child(tools)
}

/// The replace row: the replacement, one and all.
fn replace_row(
    state: &SharedCodeEditorState,
    ui: FindUi,
    replacement: &SharedTextInputData,
) -> Div {
    let none = watch(state, ui, |d| {
        d.search_matches.is_empty() || d.config.read_only
    });
    let act = |icon, label, class, run: fn(FindUi, &SharedCodeEditorState)| {
        let state = Arc::clone(state);
        icon_button(
            icon,
            label,
            class,
            none.clone().into_reactive(),
            move || {
                run(ui, &state);
                refocus_query(&state);
            },
        )
    };
    let tools = div()
        .class("cn-code-editor-find-replace-tools")
        .flex_row()
        // In an editor narrower than the tools, they wrap too.
        .flex_wrap()
        .items_center()
        .min_w(0.0)
        .w(TOOLS)
        .gap_px(GAP)
        .child(act(
            icons::REPLACE,
            "Replace",
            "cn-code-editor-find-replace-one",
            FindUi::replace_one,
        ))
        .child(act(
            icons::REPLACE_ALL,
            "Replace all",
            "cn-code-editor-find-replace-all",
            FindUi::replace_all,
        ));
    div()
        .class("cn-code-editor-find-replace")
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_px(4.0)
        // Under the find field.
        .child(div().w(BUTTON).flex_shrink_0())
        .child(
            // As the find field's, so the two are the same width.
            div().flex_1().min_w(140.0).child(
                input(replacement)
                    .size(InputSize::Small)
                    .placeholder("Replace")
                    .w_full(),
            ),
        )
        .child(tools)
}

/// The bar, built when it opens.
pub(super) fn bar(state: &SharedCodeEditorState, ui: FindUi) -> Div {
    let Ok(d) = state.lock() else {
        return div();
    };
    let (query, replacement) = (
        Arc::clone(&d.search_input_state),
        Arc::clone(&d.replace_input_state),
    );
    drop(d);
    let rows = {
        let state = Arc::clone(state);
        move || replace_row(&state, ui, &replacement)
    };
    let invalid = watch(state, ui, |d| d.search_error.is_some());
    let why = watch(state, ui, |d| d.search_error.clone().unwrap_or_default());
    let error = move || {
        div()
            .class("cn-code-editor-find-error")
            .child(blinc_layout::text::Text::bound(why.clone().into_reactive()))
    };
    div()
        .class("cn-code-editor-find")
        .flex_col()
        .flex_grow()
        .min_w(0.0)
        .max_w(WIDTH)
        .child(find_row(state, ui, &query))
        // The stylesheet spaces what comes and goes, so an absent row takes no room.
        .child(div().flex_col().show(ui.replacing, rows))
        .child(div().flex_col().show(invalid, error))
}
