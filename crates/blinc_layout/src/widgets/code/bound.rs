//! A read-only code block whose text follows a signal: its lines are a region keyed by number
//! and content, so a change rebuilds only the lines it touched, and the gutter follows the line
//! count.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use blinc_core::reactive::computed;

use super::{CodeConfig, build_gutter, build_styled_line, scroll_sideways};
use crate::binding::Reactive;
use crate::div::{Div, div};
use crate::styled_text::{StyledLine, StyledText};
use crate::syntax::SyntaxHighlighter;

/// One line as the region holds it: its number and its styled text.
type Line = (usize, StyledLine);

/// The key a line keeps while its number, text and colours stay the same.
fn line_key((index, line): &Line) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    index.hash(&mut h);
    line.text.hash(&mut h);
    for span in &line.spans {
        (span.start, span.end, span.bold).hash(&mut h);
        [span.color.r, span.color.g, span.color.b, span.color.a]
            .map(f32::to_bits)
            .hash(&mut h);
    }
    h.finish()
}

/// The gutter and the lines of `source`, highlighted by `highlighter` or plain.
pub(super) fn structure(
    source: Reactive<String>,
    highlighter: Option<Arc<dyn SyntaxHighlighter>>,
    config: &CodeConfig,
    container: Div,
) -> Div {
    let line_height_px = config.font_size * config.line_height;
    let ink = config.plain_ink();
    let plain = highlighter.is_none();
    let lines = computed(move |g| {
        let text = source.read(g).unwrap_or_default();
        let styled = match &highlighter {
            Some(h) => h.highlight(&text),
            None => StyledText::plain(&text, ink),
        };
        styled.lines.into_iter().enumerate().collect::<Vec<Line>>()
    });

    let mut container = container;
    if config.line_numbers {
        let all = lines.clone();
        let count = computed(move |g| {
            vec![
                g.read_derived_in_flight(all.derived())
                    .map_or(1, |l| l.len().max(1)),
            ]
        });
        let gutter_config = config.clone();
        container = container.child(div().for_each(
            count,
            |n: &usize| *n,
            move |n: usize| {
                let numbers: Vec<usize> = (0..n).collect();
                build_gutter(
                    &numbers,
                    line_height_px,
                    &gutter_config,
                    &[],
                    &[],
                    None,
                    gutter_config.padding,
                )
            },
        ));
    }

    let line_config = config.clone();
    container.child(scroll_sideways(
        div()
            .flex_col()
            .flex_grow()
            .padding_x_px(config.padding)
            .padding_y_px(config.padding)
            .for_each(lines, line_key, move |(_, line): Line| {
                build_styled_line(&line, &line_config, line_height_px, plain)
            }),
    ))
}
