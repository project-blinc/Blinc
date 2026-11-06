//! Native text editing, with UTF-16 offsets shared by UIKit and Android.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// The native editor's complete text and selection after an edit.
#[derive(Clone, Debug, PartialEq)]
pub struct ImeEdit {
    pub session: u64,
    pub sequence: u64,
    pub text: String,
    /// Selection anchor and caret, measured in UTF-16 code units.
    pub selection: [usize; 2],
    /// Provisional text awaiting an IME commit, in UTF-16 code units.
    pub composing: Option<[usize; 2]>,
}

/// Private editing content for a platform's native keyboard connection.
///
/// Unlike accessibility values, `text` is not masked. Do not log it or expose
/// it as an accessibility value for secure fields.
#[derive(Clone, Debug)]
pub struct ImeContent {
    pub edit: ImeEdit,
    pub focused: bool,
    pub disabled: bool,
    pub secure: bool,
    pub multiline: bool,
    /// Whether the native keyboard may correct prose and substitute punctuation.
    pub autocorrect: bool,
}

/// A widget's live document and native editing transaction.
#[derive(Clone)]
pub struct ImeBinding {
    pub read: Arc<dyn Fn() -> Option<ImeContent> + Send + Sync>,
    pub edit: Arc<dyn Fn(&ImeEdit) -> bool + Send + Sync>,
}

/// A semantic ID and its currently focused native editing session.
#[derive(Clone, Debug)]
pub struct ImeSnapshot {
    pub id: crate::tree::StableNodeId,
    pub content: ImeContent,
    pub bounds: crate::element::ElementBounds,
}

#[derive(Clone, Debug)]
pub(crate) struct ImeSession<T> {
    pub original: Option<T>,
    pub composing: Option<[usize; 2]>,
    session: u64,
    sequence: u64,
    focused: bool,
    preedit: Option<String>,
}

impl<T> Default for ImeSession<T> {
    fn default() -> Self {
        Self {
            original: None,
            composing: None,
            session: 0,
            sequence: 0,
            focused: false,
            preedit: None,
        }
    }
}

impl<T> ImeSession<T> {
    pub fn blur(&mut self) {
        self.focused = false;
        self.preedit = None;
    }

    pub fn snapshot(&mut self, focused: bool, text: String, selection: [usize; 2]) -> ImeEdit {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        if focused
            && self
                .preedit
                .as_ref()
                .is_some_and(|expected| expected != &text)
        {
            self.original = None;
            self.composing = None;
            self.preedit = None;
            self.focused = false;
        }
        if focused && !self.focused {
            self.session = NEXT.fetch_add(1, Ordering::Relaxed);
            self.sequence = 0;
        }
        self.focused = focused;
        ImeEdit {
            session: self.session,
            sequence: self.sequence,
            text,
            selection,
            composing: self.composing,
        }
    }

    pub fn receive(&mut self, edit: &ImeEdit) -> bool {
        if !self.focused || edit.session != self.session || edit.sequence <= self.sequence {
            return false;
        }
        self.sequence = edit.sequence;
        if valid_edit(edit) {
            self.preedit = edit.composing.map(|_| edit.text.clone());
        }
        true
    }
}

pub(crate) fn char_offset(text: &str, utf16: usize) -> Option<usize> {
    let mut units = 0;
    for (index, ch) in text.chars().enumerate() {
        if units == utf16 {
            return Some(index);
        }
        units += ch.len_utf16();
    }
    (units == utf16).then_some(text.chars().count())
}

pub(crate) fn utf16_offset(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(char::len_utf16).sum()
}

pub(crate) fn valid_edit(edit: &ImeEdit) -> bool {
    edit.selection
        .iter()
        .all(|offset| char_offset(&edit.text, *offset).is_some())
        && edit.composing.is_none_or(|[start, end]| {
            start <= end
                && char_offset(&edit.text, start).is_some()
                && char_offset(&edit.text, end).is_some()
        })
}

pub(crate) fn line_offset(
    lines: &[String],
    position: crate::widgets::text_area::TextPosition,
) -> usize {
    lines
        .iter()
        .take(position.line)
        .map(|line| line.chars().count() + 1)
        .sum::<usize>()
        + position.column
}

pub(crate) fn line_position(text: &str, offset: usize) -> crate::widgets::text_area::TextPosition {
    let mut position = crate::widgets::text_area::TextPosition::default();
    for ch in text.chars().take(offset) {
        if ch == '\n' {
            position.line += 1;
            position.column = 0;
        } else {
            position.column += 1;
        }
    }
    position
}
