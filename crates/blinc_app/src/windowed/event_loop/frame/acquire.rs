//! Taking the swapchain image for a frame that is going to be drawn.
//!
//! The image is taken only after the frame is known to be drawn, and the caller presents every
//! image it receives: an image that is acquired and neither presented nor returned stays held.

use blinc_platform::ControlFlow;

/// Proof that the frame passed the draw decision; `acquire_for_draw` needs one.
pub(super) struct DrawToken(());

pub(super) enum Decision {
    /// The frame is not drawn; leave with this flow.
    Skip(ControlFlow),
    Draw(DrawToken),
}

impl Decision {
    /// `skip` is what the rebuild, build and animation phases returned: `Some` ends the frame.
    pub(super) fn after_phases(skip: Option<ControlFlow>) -> Self {
        match skip {
            Some(flow) => Decision::Skip(flow),
            None => Decision::Draw(DrawToken(())),
        }
    }
}

/// A swapchain the frame loop draws into.
pub(super) trait Swapchain {
    type Image;
    fn acquire(&mut self) -> Result<Self::Image, AcquireError>;
    fn reconfigure(&mut self);
}

/// How the loop asks for a frame again.
pub(super) trait FrameSignals {
    /// The next frame has something to draw even if no input arrives.
    fn mark_dirty(&mut self);
    fn request_redraw(&mut self);
}

pub(super) enum AcquireError {
    Lost,
    Outdated,
    Timeout,
    OutOfMemory,
    Other,
}

pub(super) enum Acquired<I> {
    Image(I),
    /// No image this frame; a new frame has been asked for.
    Retry,
    Exit,
}

pub(super) fn acquire_for_draw<S: Swapchain, G: FrameSignals>(
    _decided: DrawToken,
    sc: &mut S,
    signals: &mut G,
) -> Acquired<S::Image> {
    match sc.acquire() {
        Ok(image) => Acquired::Image(image),
        Err(AcquireError::OutOfMemory) => Acquired::Exit,
        Err(e) => {
            if !matches!(e, AcquireError::Other) {
                sc.reconfigure();
            }
            // The dirty flags of this frame are already consumed.
            signals.mark_dirty();
            signals.request_redraw();
            Acquired::Retry
        }
    }
}

/// The window's wgpu surface.
pub(super) struct SurfaceSwapchain<'a> {
    pub surface: &'a wgpu::Surface<'static>,
    pub device: &'a wgpu::Device,
    pub config: &'a wgpu::SurfaceConfiguration,
}

impl Swapchain for SurfaceSwapchain<'_> {
    type Image = wgpu::SurfaceTexture;

    fn acquire(&mut self) -> Result<Self::Image, AcquireError> {
        self.surface.get_current_texture().map_err(|e| match e {
            wgpu::SurfaceError::Lost => AcquireError::Lost,
            wgpu::SurfaceError::Outdated => AcquireError::Outdated,
            wgpu::SurfaceError::Timeout => AcquireError::Timeout,
            wgpu::SurfaceError::OutOfMemory => AcquireError::OutOfMemory,
            _ => AcquireError::Other,
        })
    }

    fn reconfigure(&mut self) {
        self.surface.configure(self.device, self.config);
    }
}

/// The window's redraw request and the loop's dirty flag.
pub(super) struct WindowSignals<'a> {
    pub window: &'a blinc_platform_desktop::DesktopWindow,
    pub dirty: &'a std::sync::atomic::AtomicBool,
}

impl FrameSignals for WindowSignals<'_> {
    fn mark_dirty(&mut self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Release);
    }

    fn request_redraw(&mut self) {
        use blinc_platform::Window;
        self.window.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        next: Vec<Result<u32, AcquireError>>,
        acquired: u32,
        reconfigured: u32,
    }

    impl Swapchain for Fake {
        type Image = u32;
        fn acquire(&mut self) -> Result<u32, AcquireError> {
            let r = self.next.remove(0);
            if r.is_ok() {
                self.acquired += 1;
            }
            r
        }
        fn reconfigure(&mut self) {
            self.reconfigured += 1;
        }
    }

    #[derive(Default)]
    struct Signals {
        dirty: u32,
        redraws: u32,
    }

    impl FrameSignals for Signals {
        fn mark_dirty(&mut self) {
            self.dirty += 1;
        }
        fn request_redraw(&mut self) {
            self.redraws += 1;
        }
    }

    /// One frame as the loop runs it: the decision comes first and the only way to an acquire.
    fn frame(sc: &mut Fake, sig: &mut Signals, skip: Option<ControlFlow>, presented: &mut u32) {
        let Decision::Draw(token) = Decision::after_phases(skip) else {
            return;
        };
        if let Acquired::Image(_) = acquire_for_draw(token, sc, sig) {
            *presented += 1;
        }
    }

    #[test]
    fn skipped_frames_take_no_image() {
        let mut sc = Fake {
            next: vec![Ok(1), Ok(2)],
            ..Default::default()
        };
        let mut sig = Signals::default();
        let mut presented = 0;
        for _ in 0..50 {
            frame(
                &mut sc,
                &mut sig,
                Some(ControlFlow::Continue),
                &mut presented,
            );
        }
        frame(&mut sc, &mut sig, None, &mut presented);
        for _ in 0..50 {
            frame(
                &mut sc,
                &mut sig,
                Some(ControlFlow::Continue),
                &mut presented,
            );
        }
        frame(&mut sc, &mut sig, None, &mut presented);
        assert_eq!(sc.acquired, 2);
        assert_eq!(presented, sc.acquired);
        assert_eq!(sig.redraws, 0);
    }

    #[test]
    fn a_skip_keeps_its_flow() {
        assert!(matches!(
            Decision::after_phases(Some(ControlFlow::Exit)),
            Decision::Skip(ControlFlow::Exit)
        ));
        assert!(matches!(Decision::after_phases(None), Decision::Draw(_)));
    }

    #[test]
    fn a_failed_acquire_does_not_lose_the_frame() {
        for (err, reconfigures) in [
            (AcquireError::Timeout, 1),
            (AcquireError::Lost, 1),
            (AcquireError::Outdated, 1),
            (AcquireError::Other, 0),
        ] {
            let mut sc = Fake {
                next: vec![Err(err)],
                ..Default::default()
            };
            let mut sig = Signals::default();
            let Decision::Draw(token) = Decision::after_phases(None) else {
                unreachable!()
            };
            assert!(matches!(
                acquire_for_draw(token, &mut sc, &mut sig),
                Acquired::Retry
            ));
            assert_eq!(sc.reconfigured, reconfigures);
            assert_eq!((sig.dirty, sig.redraws), (1, 1));
        }
    }

    #[test]
    fn out_of_memory_exits() {
        let mut sc = Fake {
            next: vec![Err(AcquireError::OutOfMemory)],
            ..Default::default()
        };
        let mut sig = Signals::default();
        let Decision::Draw(token) = Decision::after_phases(None) else {
            unreachable!()
        };
        assert!(matches!(
            acquire_for_draw(token, &mut sc, &mut sig),
            Acquired::Exit
        ));
        assert_eq!(sig.redraws, 0);
    }
}
