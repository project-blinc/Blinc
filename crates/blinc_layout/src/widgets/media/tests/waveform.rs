use super::*;
use crate::div::ElementBuilder;
use blinc_core::{Brush, DrawCommand, RecordingContext, Size};
use blinc_media::Player;
use std::cell::Cell;

#[derive(Default)]
struct Transport {
    position: Cell<u64>,
    duration: Cell<u64>,
    playing: Cell<bool>,
}
impl Player for Transport {
    fn play(&self) {
        self.playing.set(true);
    }
    fn pause(&self) {
        self.playing.set(false);
    }
    fn stop(&self) {
        self.pause();
        self.seek(0);
    }
    fn seek(&self, value: u64) {
        self.position.set(value);
    }
    fn position_ms(&self) -> u64 {
        self.position.get()
    }
    fn duration_ms(&self) -> u64 {
        self.duration.get()
    }
    fn volume(&self) -> f32 {
        1.0
    }
    fn set_volume(&self, _: f32) {}
    fn is_playing(&self) -> bool {
        self.playing.get()
    }
}

fn played_width(wave: &Div) -> f32 {
    let render = wave.children_builders()[0].canvas_render_info().unwrap();
    let mut ctx = RecordingContext::new(Size::new(100.0, 60.0));
    render(
        &mut ctx,
        crate::CanvasBounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 60.0,
        },
    );
    ctx.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect {
                rect,
                brush: Brush::Solid(color),
                ..
            } if *color == Color::RED => Some(rect.width()),
            _ => None,
        })
        .sum()
}

#[test]
fn waveform_tracks_live_transport_and_seeks_without_rebuilding() {
    let player = Rc::new(Transport::default());
    player.duration.set(1000);
    let wave = waveform(Arc::new(vec![0.5; 10]))
        .playback(player.clone())
        .played_color(Color::RED)
        .unplayed_color(Color::BLUE)
        .into_div();
    let wants_frames = wave.children_builders()[0].canvas_redraw_while().unwrap();
    assert_eq!(played_width(&wave), 0.0);
    assert!(!wants_frames());
    player.play();
    player.seek(250);
    let first = played_width(&wave);
    assert!(first > 0.0 && wants_frames());
    player.seek(500);
    assert!(played_width(&wave) > first);
    player.pause();
    let paused = played_width(&wave);
    assert_eq!(played_width(&wave), paused);
    assert!(!wants_frames());
    player.seek(100);
    assert!(played_width(&wave) < first);
    player.seek(1500);
    assert!((played_width(&wave) - 70.0).abs() < 0.01);
    player.duration.set(0);
    assert_eq!(played_width(&wave), 0.0);
}

#[test]
fn fixed_waveform_progress_does_not_request_animation_frames() {
    let wave = waveform(Arc::new(vec![0.5; 10]))
        .progress(0.5)
        .played_color(Color::RED)
        .unplayed_color(Color::BLUE)
        .into_div();
    assert!((played_width(&wave) - 35.0).abs() < 0.01);
    assert!(!wave.children_builders()[0].canvas_redraw_while().unwrap()());
}
