use super::*;
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
    };
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&4036u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1000u32.to_le_bytes());
    wav.extend_from_slice(&2000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&4000u32.to_le_bytes());
    wav.resize(4044, 0);
    inner.set_source(rodio::Decoder::new(std::io::Cursor::new(wav)).unwrap());
    assert_eq!(inner.duration_ms, 2000);
}
