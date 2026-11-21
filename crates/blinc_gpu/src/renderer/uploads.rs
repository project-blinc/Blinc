//! Buffer uploads through a reused staging belt, and the renderer's submits.
//!
//! `Queue::write_buffer` allocates and frees a staging buffer per call, and on Metal each
//! of those is a call into the kernel. The renderer's writes go into chunks of a
//! [`wgpu::util::StagingBelt`] instead, and the copies out of them are recorded in a
//! command buffer that [`Uploads::submit`] places ahead of the submitted work. That keeps
//! `write_buffer`'s ordering: every write lands before any command of the next submit,
//! and of two writes to one range the later wins.
//!
//! Between [`Uploads::begin_frame`] and [`Uploads::end_frame`] the renderer's passes share
//! one command encoder: a pass borrows it with [`Uploads::lend`] and returns it with
//! [`Uploads::give_back`], and copies recorded while it is home go into it in order. The
//! driver pays per command buffer, in creating, committing and completing it, so a frame
//! pays once. A submit of other command buffers queues them behind the encoder's work, and
//! the flush hands the whole frame to the queue. Anything that submits on the raw queue
//! flushes first.
//!
//! A pass must make its writes before it borrows the encoder: a write while the encoder is
//! out, or a second borrow, cannot be placed between the passes before and after it. Those
//! are logged, and `BLINC_STRICT_UPLOADS=1` panics on them.
//!
//! A write of the bytes a range already holds from the renderer's last write to it is
//! skipped: no shader writes these buffers, so the range still holds them.
//!
//! A chunk returns to the belt once the GPU has copied out of it and the device is
//! polled, which every submit does, so a steady frame allocates nothing. A write larger
//! than a chunk gets a chunk of its own size, which the belt keeps and reuses, so a large
//! upload made every frame (the text of a scrolled page) allocates nothing either. The
//! belt keeps every chunk it makes, so a write beyond [`LARGEST_WRITE`] goes through
//! `Queue::write_buffer` instead, whose staging is freed after use.

use std::num::NonZeroU64;
use std::sync::Mutex;

use wgpu::util::StagingBelt;

/// Chunk size of the belt.
const CHUNK_BYTES: u64 = 1024 * 1024;
/// The largest write the belt takes.
const LARGEST_WRITE: u64 = 8 * 1024 * 1024;
/// The largest write remembered to skip a repeat of it.
const REMEMBERED_BYTES: usize = 4096;

pub(crate) struct Uploads {
    state: Mutex<State>,
    /// `BLINC_DIRECT_UPLOADS=1` sends every write straight to the queue; safe, for comparison.
    direct: bool,
    /// `BLINC_SUBMIT_EACH=1` submits at every renderer submit instead of once a frame; safe.
    submit_each: bool,
    /// `BLINC_STRICT_UPLOADS=1` panics where a pass breaks the ordering rule; for tests.
    strict: bool,
}

/// The command encoder a pass records into, from [`Uploads::lend`].
pub(crate) struct Lent {
    encoder: Option<wgpu::CommandEncoder>,
    /// Whether this is the frame's shared encoder.
    shared: bool,
}

impl std::ops::Deref for Lent {
    type Target = wgpu::CommandEncoder;
    fn deref(&self) -> &wgpu::CommandEncoder {
        self.encoder.as_ref().expect("a lent encoder")
    }
}

impl std::ops::DerefMut for Lent {
    fn deref_mut(&mut self) -> &mut wgpu::CommandEncoder {
        self.encoder.as_mut().expect("a lent encoder")
    }
}

struct State {
    belt: StagingBelt,
    /// Copies recorded since the last submit and, while batching, the frame's passes.
    pending: Option<wgpu::CommandEncoder>,
    /// Whether `pending` is lent to a pass.
    lent: bool,
    /// Copies recorded while `pending` was lent.
    late: Option<wgpu::CommandEncoder>,
    /// Command buffers queued by submits since the frame began, in order.
    queued: Vec<wgpu::CommandBuffer>,
    batching: bool,
    /// Per buffer, the last small write to it: offset, bytes, and whether a frame since the
    /// last [`Uploads::end_frame`] wrote it, so a buffer no frame writes is let go.
    last: std::collections::HashMap<wgpu::Buffer, (u64, Vec<u8>, bool)>,
}

impl Uploads {
    pub(crate) fn new() -> Self {
        let flag = |name| std::env::var_os(name).is_some_and(|v| v == "1");
        Self {
            state: Mutex::new(State {
                belt: StagingBelt::new(CHUNK_BYTES),
                pending: None,
                lent: false,
                late: None,
                queued: Vec::new(),
                batching: false,
                last: std::collections::HashMap::new(),
            }),
            direct: flag("BLINC_DIRECT_UPLOADS"),
            submit_each: flag("BLINC_SUBMIT_EACH"),
            strict: flag("BLINC_STRICT_UPLOADS"),
        }
    }

    fn misordered(&self, what: &str) {
        if self.strict {
            panic!("renderer upload order: {what}");
        }
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| tracing::error!("renderer upload order: {what}"));
    }

    fn encoder(device: &wgpu::Device) -> wgpu::CommandEncoder {
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Renderer"),
        })
    }

    /// An encoder to record a pass into: the frame's shared one while batching, else a new
    /// one. Writes for the pass come before this.
    pub(crate) fn lend(&self, device: &wgpu::Device) -> Lent {
        let mut state = self.lock();
        if !state.batching {
            return Lent {
                encoder: Some(Self::encoder(device)),
                shared: false,
            };
        }
        if state.lent {
            drop(state);
            self.misordered("a pass borrowed the encoder while another had it");
            return Lent {
                encoder: Some(Self::encoder(device)),
                shared: false,
            };
        }
        state.lent = true;
        let encoder = state
            .pending
            .take()
            .unwrap_or_else(|| Self::encoder(device));
        Lent {
            encoder: Some(encoder),
            shared: true,
        }
    }

    /// Returns an encoder from [`Uploads::lend`], submitting it unless the frame is batching.
    pub(crate) fn give_back(&self, queue: &wgpu::Queue, mut lent: Lent) {
        let encoder = lent.encoder.take().expect("a lent encoder");
        if !lent.shared {
            self.submit(queue, std::iter::once(encoder.finish()));
            return;
        }
        let mut state = self.lock();
        state.lent = false;
        if let Some(late) = state.late.take() {
            drop(state);
            self.misordered("a pass wrote a buffer while it had the encoder");
            let mut state = self.lock();
            state.queued.push(late.finish());
            state.queued.push(encoder.finish());
            return;
        }
        state.pending = Some(encoder);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Writes `data` into `target` at `offset` before the next [`Uploads::submit`].
    /// Same rules as `Queue::write_buffer`: offset and length are multiples of 4.
    pub(crate) fn write(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Buffer,
        offset: u64,
        data: &[u8],
    ) {
        let Some(size) = NonZeroU64::new(data.len() as u64) else {
            return;
        };
        if self.direct || size.get() > LARGEST_WRITE {
            // The queue's own writes run ahead of every command buffer of a submission, so
            // everything recorded before this write is submitted first.
            self.flush(queue);
            self.lock().last.remove(target);
            queue.write_buffer(target, offset, data);
            return;
        }
        let mut state = self.lock();
        let State {
            belt,
            pending,
            lent,
            late,
            last,
            ..
        } = &mut *state;
        if data.len() <= REMEMBERED_BYTES {
            match last.get_mut(target) {
                Some((at, bytes, written)) if *at == offset && bytes.as_slice() == data => {
                    *written = true;
                    return;
                }
                Some((at, bytes, written)) => {
                    *at = offset;
                    bytes.clear();
                    bytes.extend_from_slice(data);
                    *written = true;
                }
                None => {
                    last.insert(target.clone(), (offset, data.to_vec(), true));
                }
            }
        } else {
            last.remove(target);
        }
        let encoder = if *lent {
            late.get_or_insert_with(|| Self::encoder(device))
        } else {
            pending.get_or_insert_with(|| Self::encoder(device))
        };
        belt.write_buffer(encoder, target, offset, size, device)
            .copy_from_slice(data);
    }

    /// Submits `commands` after the uploads recorded since the last submit, or queues
    /// them both while a frame is batching.
    pub(crate) fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(
        &self,
        queue: &wgpu::Queue,
        commands: I,
    ) {
        if self.lock().lent {
            self.misordered("a pass submitted while another had the encoder");
        }
        let mut state = self.lock();
        // The copies' source chunks stay mapped until the flush; a command buffer may be
        // finished while they are, only submitted once they are not.
        if let Some(encoder) = state.pending.take() {
            state.queued.push(encoder.finish());
        }
        state.queued.extend(commands);
        if !state.batching {
            Self::submit_queued(&mut state, queue);
        }
    }

    /// Queues the renderer's submits until [`Uploads::end_frame`].
    pub(crate) fn begin_frame(&self) {
        if !self.submit_each {
            self.lock().batching = true;
        }
    }

    /// Submits everything recorded and queued so far. Batching, if on, carries on; a
    /// caller about to use the raw queue flushes so its work lands after the renderer's.
    pub(crate) fn flush(&self, queue: &wgpu::Queue) {
        if self.lock().lent {
            self.misordered("the queue was used while a pass had the encoder");
        }
        let mut state = self.lock();
        if let Some(encoder) = state.pending.take() {
            state.queued.push(encoder.finish());
        }
        Self::submit_queued(&mut state, queue);
    }

    /// Flushes and ends batching.
    pub(crate) fn end_frame(&self, queue: &wgpu::Queue) {
        self.flush(queue);
        let mut state = self.lock();
        state.batching = false;
        state
            .last
            .retain(|_, (_, _, written)| std::mem::take(written));
    }

    fn submit_queued(state: &mut State, queue: &wgpu::Queue) {
        if state.queued.is_empty() {
            return;
        }
        state.belt.finish();
        queue.submit(state.queued.drain(..));
        state.belt.recall();
    }
}
