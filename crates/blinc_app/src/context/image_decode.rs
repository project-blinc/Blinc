//! Images decoded, compressed and uploaded on a worker thread, so an image entering view does not
//! hold up the frame that first needs it. The frame draws without it and the next paint after it
//! arrives draws it.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use blinc_gpu::GpuImage;
use blinc_layout::ImageRetry;

/// An image that finished on the worker, with the fade-in and retry it was asked for. `None` for
/// one that failed to load.
pub(super) struct Decoded {
    pub source: String,
    pub image: Option<GpuImage>,
    pub fade_duration_ms: u32,
    pub retry: ImageRetry,
}

pub(super) struct ImageDecoder {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Sources underway, with the fade-in and retry each was asked for.
    pending: HashMap<String, (u32, ImageRetry)>,
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
            tx,
            rx,
        }
    }

    /// Whether an image is underway.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Start loading `source` unless it is underway. `bc` says whether the device can sample
    /// block-compressed textures. `false` when no worker could be started.
    pub fn request(
        &mut self,
        source: &str,
        fade_duration_ms: u32,
        retry: ImageRetry,
        bc: bool,
    ) -> bool {
        if self.pending.contains_key(source) {
            return true;
        }
        let (device, queue, tx) = (self.device.clone(), self.queue.clone(), self.tx.clone());
        let owned = source.to_owned();
        let spawned = std::thread::Builder::new()
            .name("blinc-image-decode".into())
            .spawn(move || {
                let image =
                    blinc_image::ImageData::load(blinc_image::ImageSource::from_uri(&owned))
                        .ok()
                        .map(|data| {
                            let bc = bc && super::bc_eligible(&owned, data.width(), data.height());
                            GpuImage::from_rgba_maybe_compressed(
                                &device,
                                &queue,
                                data.pixels(),
                                data.width(),
                                data.height(),
                                false,
                                bc,
                                Some(&owned),
                            )
                        });
                let _ = tx.send((owned, image));
            });
        if spawned.is_ok() {
            self.pending
                .insert(source.to_owned(), (fade_duration_ms, retry));
        }
        spawned.is_ok()
    }

    /// The images that have finished since the last call.
    pub fn take(&mut self) -> Vec<Decoded> {
        let mut out = Vec::new();
        while let Ok((source, image)) = self.rx.try_recv() {
            let (fade_duration_ms, retry) = self
                .pending
                .remove(&source)
                .unwrap_or((0, ImageRetry::default()));
            out.push(Decoded {
                source,
                image,
                fade_duration_ms,
                retry,
            });
        }
        out
    }
}
