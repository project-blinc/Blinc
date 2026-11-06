use crate::demos::DemoState;
use blinc_layout::prelude::*;

pub fn image_demo(_: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        div().class("demo-image").child(
            img(include_str!("../../assets/avatar.data-uri"))
                .w((viewport.0 - 64.0).clamp(1.0, 420.0))
                .h(216.0)
                .cover()
                .no_fade(),
        ),
    )
}

pub fn waveform_demo(_: &DemoState, _: (f32, f32)) -> Div {
    let samples: Vec<f32> =
        serde_json::from_str(include_str!("../../assets/night-drive-waveform.json")).unwrap();
    div()
        .class("demo-wide")
        .child(
            blinc_layout::widgets::media::waveform(std::sync::Arc::new(samples))
                .progress(0.42)
                .w_full()
                .h(112.0)
                .into_div(),
        )
        .child(text("Amplitude · played and remaining").class("demo-caption"))
}

pub fn audio_player_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let player = values.audio.get_or_init(|| {
        let player = std::rc::Rc::new(blinc_media::AudioPlayer::new());
        #[cfg(not(target_arch = "wasm32"))]
        {
            player.set_volume(0.0);
            player.play(blinc_media::AudioSource::bytes(
                include_bytes!("../../assets/night-drive.flac").to_vec(),
            ));
            player.pause();
        }
        #[cfg(target_arch = "wasm32")]
        player.load_url("/components/source/assets/night-drive.flac");
        player
    });
    let peaks: Vec<f32> =
        serde_json::from_str(include_str!("../../assets/night-drive-waveform.json")).unwrap();
    let samples = blinc_media::AudioSamples::from_f32(&peaks, 1, 22050);
    div()
        .class("demo-wide")
        .child(
            blinc_layout::widgets::media::audio_player(player.clone())
                .waveform_data(&samples)
                .w_full()
                .into_div(),
        )
        .child(text("Night Drive · Ash Drive").class("demo-caption"))
}

pub fn video_player_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let player = values.video.get_or_init(|| {
        let player = std::rc::Rc::new(blinc_media::VideoPlayer::new());
        #[cfg(not(target_arch = "wasm32"))]
        {
            player.load_bytes(include_bytes!("../../assets/handpan.mp4").to_vec());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while player.current_frame().is_none() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            player.pause();
        }
        #[cfg(target_arch = "wasm32")]
        player.load_url("/components/source/assets/handpan.mp4");
        player
    });
    div().class("demo-wide").child(
        blinc_layout::widgets::media::video_player(player.clone())
            .w_full()
            .h(240.0)
            .into_div(),
    )
}
