//! Images decoded, compressed and uploaded on a worker thread, so an image entering view does not
//! hold up the frame that first needs it. The frame draws without it and the next paint after it
//! arrives draws it.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use blinc_gpu::GpuImage;

/// An image that finished on the worker, with the fade-in it was asked for. `None` for one that
/// failed to load.
pub(super) struct Decoded {
    pub source: String,
    pub image: Option<GpuImage>,
    pub fade_duration_ms: u32,
}

/// A source that failed to load, and when it is tried again.
struct Failure {
    attempts: u32,
    retry_at: Instant,
    fade_duration_ms: u32,
    bc: bool,
}

/// How long after its `attempts`th failure a source is tried again: half a second, doubling, at
/// most half a minute.
fn backoff(attempts: u32) -> Duration {
    Duration::from_millis(500u64.saturating_mul(1 << attempts.saturating_sub(1).min(6)))
        .min(Duration::from_secs(30))
}

pub(super) struct ImageDecoder {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Sources underway, with the fade-in each was asked for and whether it may be compressed.
    pending: HashMap<String, (u32, bool)>,
    /// Sources that failed to load, tried again after a backoff.
    failed: HashMap<String, Failure>,
    tx: Sender<(String, Option<GpuImage>)>,
    rx: Receiver<(String, Option<GpuImage>)>,
}

impl ImageDecoder {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let (tx, rx) = channel();
        Self {
            device,
            queue,
            pending: HashMap::new(),
            failed: HashMap::new(),
            tx,
            rx,
        }
    }

    /// Whether an image is underway.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Start loading `source` unless it is underway or failed too recently to try again. `bc`
    /// says whether the device can sample block-compressed textures.
    pub fn request(&mut self, source: &str, fade_duration_ms: u32, bc: bool) {
        if self.pending.contains_key(source)
            || self
                .failed
                .get(source)
                .is_some_and(|f| Instant::now() < f.retry_at)
        {
            return;
        }
        self.pending
            .insert(source.to_owned(), (fade_duration_ms, bc));
        let (device, queue, tx) = (self.device.clone(), self.queue.clone(), self.tx.clone());
        let key = source.to_owned();
        let source = key.clone();
        let spawned = std::thread::Builder::new()
            .name("blinc-image-decode".into())
            .spawn(move || {
                let image =
                    blinc_image::ImageData::load(blinc_image::ImageSource::from_uri(&source))
                        .ok()
                        .map(|data| {
                            let bc = bc && super::bc_eligible(&source, data.width(), data.height());
                            GpuImage::from_rgba_maybe_compressed(
                                &device,
                                &queue,
                                data.pixels(),
                                data.width(),
                                data.height(),
                                false,
                                bc,
                                Some(&source),
                            )
                        });
                let _ = tx.send((source, image));
            });
        if spawned.is_err() {
            self.pending.remove(&key);
            self.note_failure(key, fade_duration_ms, bc);
        }
    }

    fn note_failure(&mut self, source: String, fade_duration_ms: u32, bc: bool) {
        let attempts = self.failed.get(&source).map_or(0, |f| f.attempts) + 1;
        self.failed.insert(
            source,
            Failure {
                attempts,
                retry_at: Instant::now() + backoff(attempts),
                fade_duration_ms,
                bc,
            },
        );
    }

    /// Try again the sources whose backoff has run out.
    pub fn retry_due(&mut self) {
        let now = Instant::now();
        let due: Vec<(String, u32, bool)> = self
            .failed
            .iter()
            .filter(|(source, f)| now >= f.retry_at && !self.pending.contains_key(*source))
            .map(|(source, f)| (source.clone(), f.fade_duration_ms, f.bc))
            .collect();
        for (source, fade_duration_ms, bc) in due {
            self.request(&source, fade_duration_ms, bc);
        }
    }

    /// Forget the failures of the sources `matches` picks, so they are tried at once, as after
    /// the file behind them changed.
    pub fn forget_failures(&mut self, matches: impl Fn(&str) -> bool) {
        self.failed.retain(|source, _| !matches(source));
    }

    /// The images that have finished since the last call.
    pub fn take(&mut self) -> Vec<Decoded> {
        let mut out = Vec::new();
        while let Ok((source, image)) = self.rx.try_recv() {
            let (fade_duration_ms, bc) = self.pending.remove(&source).unwrap_or((0, false));
            if image.is_none() {
                tracing::trace!("Failed to load image '{source}'");
                self.note_failure(source.clone(), fade_duration_ms, bc);
            } else {
                self.failed.remove(&source);
            }
            out.push(Decoded {
                source,
                image,
                fade_duration_ms,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_load_waits_longer_each_time_up_to_half_a_minute() {
        let ms = |n| backoff(n).as_millis();
        assert_eq!([ms(1), ms(2), ms(3), ms(4)], [500, 1000, 2000, 4000]);
        assert_eq!(ms(7), 30_000);
        assert_eq!(ms(40), 30_000);
    }
}
