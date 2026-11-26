use super::*;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rodio::mixer::MixerSource;
#[test]
fn decoded_duration_is_kept_without_an_output_device() {
    let mut inner = AudioPlayerInner {
        state: PlaybackState::Stopped,
        volume: 0.0,
        looping: false,
        play_start: None,
        position_offset_ms: 0,
        duration_ms: 0,
        sink: None,
        _stream: None,
        mixer: None,
        source: None,
    };
    assert!(inner.load_bytes(ramp(2000), false));
    assert_eq!(inner.duration_ms, 2000);
}

/// A mono WAV at 1000 Hz whose sample `i` is `i / 4`, so a sample tells how far in it is.
fn ramp(samples: u32) -> Vec<u8> {
    let data = samples * 2;
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1000u32.to_le_bytes());
    wav.extend_from_slice(&2000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    for i in 0..samples {
        wav.extend_from_slice(&((i / 4) as i16).to_le_bytes());
    }
    wav
}

/// A player on a mixer that no device drains, and the mixer's output.
fn detached() -> (AudioPlayer, MixerSource) {
    let (mixer, output) = rodio::mixer::mixer(
        NonZero::new(1).expect("one channel"),
        NonZero::new(1000).expect("a sample rate"),
    );
    let player = AudioPlayer {
        inner: Rc::new(RefCell::new(AudioPlayerInner {
            state: PlaybackState::Stopped,
            volume: 1.0,
            looping: false,
            play_start: None,
            position_offset_ms: 0,
            duration_ms: 0,
            sink: None,
            _stream: None,
            mixer: Some(mixer),
            source: None,
        })),
    };
    (player, output)
}

/// Runs `call` while another thread takes samples, as an output device does: a seek waits for it.
fn while_draining<R>(output: &mut MixerSource, call: impl FnOnce() -> R) -> R {
    struct Done<'a>(&'a AtomicBool);
    impl Drop for Done<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                output.by_ref().take(64).for_each(drop);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _done = Done(&done);
        call()
    })
}

/// The milliseconds into [`ramp`] of the next sample that is not silence.
fn heard_at(output: &mut MixerSource) -> u32 {
    let sample = output
        .by_ref()
        .take(10_000)
        .find(|sample| *sample > 0.0)
        .expect("a sample");
    (sample * 32768.0) as u32 * 4
}

#[test]
fn a_seek_moves_what_plays() {
    let (player, mut output) = detached();
    player.play(AudioSource::bytes(ramp(120_000)));

    while_draining(&mut output, || player.seek(60_000));

    assert!(player.position_ms() >= 60_000);
    let at = heard_at(&mut output);
    assert!((60_000..90_000).contains(&at), "heard {at} ms");

    while_draining(&mut output, || player.seek(1_000_000));
    assert!((120_000..121_000).contains(&player.position_ms()));
}

#[test]
fn a_seek_after_the_end_holds_the_track_there() {
    let (player, mut output) = detached();
    player.play(AudioSource::bytes(ramp(400)));
    for _ in 0..10_000 {
        if player.state() == PlaybackState::Stopped {
            break;
        }
        output.next();
    }
    assert_eq!(player.state(), PlaybackState::Stopped);

    while_draining(&mut output, || player.seek(200));

    assert_eq!(player.state(), PlaybackState::Paused);
    assert_eq!(player.position_ms(), 200);
    player.resume();
    let at = heard_at(&mut output);
    assert!((200..400).contains(&at), "heard {at} ms");
}

#[test]
fn a_seek_close_to_the_end_keeps_playing() {
    let (player, mut output) = detached();
    player.play(AudioSource::bytes(ramp(1003)));
    let played = || {
        let inner = player.inner.borrow();
        inner.sink.as_ref().expect("a sink").get_pos()
    };
    // A few samples short of the end, where the sink would not answer a seek order.
    while played() < Duration::from_millis(1000) {
        output.next();
    }

    while_draining(&mut output, || player.seek(200));

    assert_eq!(player.state(), PlaybackState::Playing);
    let at = heard_at(&mut output);
    assert!((200..900).contains(&at), "heard {at} ms");
}

#[test]
fn a_finished_track_stays_at_its_end_and_plays_again() {
    let (player, mut output) = detached();
    player.play(AudioSource::bytes(ramp(400)));
    for _ in 0..10_000 {
        if player.state() == PlaybackState::Stopped {
            break;
        }
        output.next();
    }
    assert!(!player.is_playing());
    std::thread::sleep(Duration::from_millis(450));
    assert_eq!(player.position_ms(), 400);

    player.resume();

    assert!(player.is_playing());
    assert!(player.position_ms() < 400);
    let at = heard_at(&mut output);
    assert!(at < 200, "heard {at} ms");
}
