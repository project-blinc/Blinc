//! Notched shapes: concave corners, bulge and peak edges, with a border.
//!
//! The shape is a union of a body and added pieces, so the joins and the
//! distance inside the piece are where it can go wrong.

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Rect};

/// Create the notch test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("notch");

    suite.add("notch_joins", |ctx| {
        let c = ctx.ctx();

        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        let border = Some((2.0, Color::rgba(0.1, 0.2, 0.5, 1.0)));
        let none = [0.0; 4];
        let white = || Color::WHITE.into();

        // Concave top corners, like a dropdown hanging from a bar.
        c.fill_notch(
            Rect::new(20.0, 24.0, 160.0, 110.0),
            [20.0, 20.0, 10.0, 10.0],
            [1.0, 1.0, 0.0, 0.0],
            none,
            none,
            border,
            None,
            white(),
        );
        // A bulge on top.
        c.fill_notch(
            Rect::new(215.0, 24.0, 160.0, 110.0),
            [10.0; 4],
            none,
            [2.0, 70.0, 22.0, 10.0],
            none,
            border,
            None,
            white(),
        );
        // A peak on top.
        c.fill_notch(
            Rect::new(20.0, 164.0, 160.0, 110.0),
            [10.0; 4],
            none,
            [4.0, 60.0, 24.0, 0.0],
            none,
            border,
            None,
            white(),
        );
        // A bulge underneath, and concave bottom corners.
        c.fill_notch(
            Rect::new(215.0, 164.0, 160.0, 110.0),
            [10.0, 10.0, 20.0, 20.0],
            [0.0, 0.0, 1.0, 1.0],
            none,
            [2.0, 70.0, 22.0, 10.0],
            border,
            None,
            white(),
        );
    });

    suite
}
