#![cfg(feature = "headless")]
//! An indeterminate progress bar stays inside its track.

use blinc_cn::progress_indeterminate;
use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::units::px;
use blinc_layout::widgets::scroll::scroll;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 240;
const H: u32 = 120;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn near(pixel: [u8; 4], c: Color) -> bool {
    let to8 = |v: f32| (v * 255.0).round() as i32;
    (pixel[0] as i32 - to8(c.r)).abs() <= 6
        && (pixel[1] as i32 - to8(c.g)).abs() <= 6
        && (pixel[2] as i32 - to8(c.b)).abs() <= 6
}

/// Count the pixels outside `track` that are the bar's colour.
fn strays(ui: &mut Headless, track: (f32, f32, f32, f32)) -> usize {
    let primary = ThemeState::get().color(ColorToken::Primary);
    let frame = ui.snapshot().expect("snapshot");
    let (x0, y0, w, h) = track;
    let mut count = 0;
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let inside =
                fx >= x0 - 1.0 && fx <= x0 + w + 1.0 && fy >= y0 - 1.0 && fy <= y0 + h + 1.0;
            if !inside && near(frame.get_pixel(x, y).unwrap(), primary) {
                count += 1;
            }
        }
    }
    count
}

fn track_bounds(ui: &Headless) -> (f32, f32, f32, f32) {
    let track = ui.find(".cn-progress").expect("the track");
    let b = ui.tree().get_absolute_bounds(track).unwrap();
    (b.x, b.y, b.width, b.height)
}

#[test]
fn an_indeterminate_bar_stays_inside_its_track() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(px(20.0))
            .child(progress_indeterminate().w(100.0))
    });
    ui.advance(100);
    for _ in 0..6 {
        ui.advance(150);
        let track = track_bounds(&ui);
        assert_eq!(strays(&mut ui, track), 0, "track {track:?}");
    }
}

#[test]
fn an_indeterminate_bar_stays_inside_its_track_in_a_scroll_container() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        scroll().w(W as f32).h(H as f32).child(
            div()
                .w(W as f32)
                .padding(px(20.0))
                .gap(8.0)
                .flex_col()
                .child(div().h(60.0).w(100.0))
                .child(progress_indeterminate().w(100.0))
                .child(div().h(300.0)),
        )
    });
    ui.advance(100);
    ui.scroll(0.0, -30.0);
    ui.advance(100);
    for _ in 0..6 {
        ui.advance(150);
        let track = track_bounds(&ui);
        assert_eq!(strays(&mut ui, track), 0, "track {track:?}");
    }
}

#[test]
fn an_indeterminate_bar_scrolled_under_a_header_is_not_drawn_over_it() {
    let Some(mut ui) = session() else { return };
    const HEADER: f32 = 40.0;
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .flex_col()
            .child(
                div()
                    .w(W as f32)
                    .h(HEADER)
                    .bg(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            )
            .child(
                scroll()
                    .w(W as f32)
                    .h(H as f32 - HEADER)
                    .viewport_cull(true)
                    .child(
                        div()
                            .w(W as f32)
                            .padding(px(10.0))
                            .flex_col()
                            .child(progress_indeterminate().w(100.0))
                            .child(div().h(400.0)),
                    ),
            )
    });
    ui.advance(100);
    // Scroll the bar up past the top of the viewport, under the header.
    ui.move_to(120.0, 80.0);
    ui.scroll(0.0, -30.0);
    ui.scroll_end();
    ui.advance(300);
    let track = track_bounds(&ui);
    assert!(
        track.1 < HEADER,
        "the track is not under the header: {track:?}"
    );
    // The bar pulses between partly transparent and opaque, so any pixel that is not the
    // header's own colour is the bar.
    let header = Color::rgba(0.1, 0.1, 0.1, 1.0);
    for step in 0..8 {
        ui.advance(110);
        let frame = ui.snapshot().expect("snapshot");
        let mut drawn = 0;
        for y in 0..HEADER as u32 {
            for x in 0..W {
                if !near(frame.get_pixel(x, y).unwrap(), header) {
                    drawn += 1;
                }
            }
        }
        assert_eq!(
            drawn, 0,
            "the bar is drawn over the header at step {step}, track {track:?}"
        );
    }
}

#[test]
fn a_spinner_under_a_toast_is_not_drawn_over_it() {
    use blinc_cn::cn;
    const BW: u32 = 480;
    const BH: u32 = 360;
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(BW, BH).expect("headless");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    // The toast opens in the bottom right corner; put a spinner under it.
    ui.mount(move |_| {
        scroll()
            .w(BW as f32)
            .h(BH as f32)
            .viewport_cull(true)
            .child(
                div()
                    .w(BW as f32)
                    .h(BH as f32)
                    .flex_col()
                    .items_end()
                    .justify_end()
                    .padding(px(40.0))
                    .child(cn::spinner()),
            )
    });
    ui.advance(100);
    // Without the toast the arc is there to be found.
    {
        let spinner = ui.find(".cn-spinner").expect("the spinner");
        let s = ui.tree().get_absolute_bounds(spinner).unwrap();
        let primary = ThemeState::get().color(ColorToken::Primary);
        let frame = ui.snapshot().expect("snapshot");
        let mut found = 0;
        let mut sample = [0u8; 4];
        for y in s.y as u32..(s.y + s.height) as u32 {
            for x in s.x as u32..(s.x + s.width) as u32 {
                let p = frame.get_pixel(x, y).unwrap();
                if near(p, primary) {
                    found += 1;
                }
                if (x, y) == (s.x as u32 + 13, s.y as u32 + 1) {
                    sample = p;
                }
            }
        }
        eprintln!("arc pixels without a toast: {found}, primary {primary:?}, sample {sample:?}");
        assert!(found > 0, "the test cannot see the arc");
    }
    let _toast = cn::toast("Saved")
        .description("Your changes are saved")
        .show();
    ui.advance(40);
    let toasts = {
        let tree = ui.tree();
        let registry = tree.element_registry();
        let mut found = Vec::new();
        let mut stack = vec![tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            if registry.has_class(node, "cn-toast") {
                found.push(node);
            }
            stack.extend(tree.layout_tree.children(node));
        }
        found
    };
    eprintln!("{} toasts", toasts.len());
    let t = toasts
        .iter()
        .map(|n| ui.tree().get_absolute_bounds(*n).unwrap())
        .find(|t| {
            let s = ui
                .tree()
                .get_absolute_bounds(ui.find(".cn-spinner").unwrap())
                .unwrap();
            s.x < t.x + t.width
                && s.x + s.width > t.x
                && s.y < t.y + t.height
                && s.y + s.height > t.y
        })
        .expect("a toast over the spinner");
    let spinner = ui.find(".cn-spinner").expect("the spinner");
    let s = ui.tree().get_absolute_bounds(spinner).unwrap();
    eprintln!("toast {t:?} spinner {s:?}");
    let overlap =
        s.x < t.x + t.width && s.x + s.width > t.x && s.y < t.y + t.height && s.y + s.height > t.y;
    assert!(
        overlap,
        "the spinner is not under the toast: toast {t:?} spinner {s:?}"
    );
    let primary = ThemeState::get().color(ColorToken::Primary);
    ui.snapshot().expect("snapshot");
    for step in 0..24 {
        ui.advance(40);
        let frame = if step % 2 == 0 {
            ui.snapshot_fast()
        } else {
            ui.snapshot()
        }
        .expect("snapshot");
        let mut drawn = 0;
        for y in s.y as u32..(s.y + s.height) as u32 {
            for x in s.x as u32..(s.x + s.width) as u32 {
                if near(frame.get_pixel(x, y).unwrap(), primary) {
                    drawn += 1;
                }
            }
        }
        assert_eq!(drawn, 0, "spinner pixels over the toast at step {step}");
    }
}

#[test]
fn a_spinner_under_a_toast_is_not_drawn_over_it_on_a_hidpi_display() {
    use blinc_cn::cn;
    const LW: u32 = 480;
    const LH: u32 = 360;
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(LW * 2, LH * 2).expect("headless");
    {
        let c = ui.context();
        c.scale_factor = 2.0;
        c.width = LW as f32;
        c.height = LH as f32;
    }
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(move |_| {
        div()
            .w(LW as f32)
            .h(LH as f32)
            .flex_col()
            .items_end()
            .justify_end()
            .padding(px(40.0))
            .child(cn::spinner())
    });
    ui.advance(100);
    let primary = ThemeState::get().color(ColorToken::Primary);
    let spinner = ui.find(".cn-spinner").expect("the spinner");
    let s = ui.tree().get_absolute_bounds(spinner).unwrap();
    let count = |frame: &blinc_recorder::testing::CapturedFrame| {
        let mut n = 0;
        for y in (s.y * 2.0) as u32..((s.y + s.height) * 2.0) as u32 {
            for x in (s.x * 2.0) as u32..((s.x + s.width) * 2.0) as u32 {
                if near(frame.get_pixel(x, y).unwrap(), primary) {
                    n += 1;
                }
            }
        }
        n
    };
    let bare = ui.snapshot().expect("snapshot");
    eprintln!(
        "spinner {s:?}, arc pixels without a toast: {}",
        count(&bare)
    );
    assert!(count(&bare) > 0, "the test cannot see the arc");
    let _toast = cn::toast("Saved")
        .description("Your changes are saved")
        .show();
    ui.advance(600);
    let toast = ui.find(".cn-toast").expect("the toast");
    let t = ui.tree().get_absolute_bounds(toast).unwrap();
    eprintln!("toast {t:?}");
    let overlap =
        s.x < t.x + t.width && s.x + s.width > t.x && s.y < t.y + t.height && s.y + s.height > t.y;
    assert!(overlap, "the spinner is not under the toast");
    ui.snapshot().expect("snapshot");
    for step in 0..24 {
        ui.advance(40);
        let frame = if step % 2 == 0 {
            ui.snapshot_fast()
        } else {
            ui.snapshot()
        }
        .expect("snapshot");
        assert_eq!(
            count(&frame),
            0,
            "spinner pixels over the toast at step {step}"
        );
    }
}
