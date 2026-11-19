use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use winit::{event_loop::ActiveEventLoop, window::Window};

/// Work queued by a native accessibility client for the UI thread.
pub struct AccessibilityRequest {
    pub generation: u64,
    pub active: bool,
    pub actions: Vec<ActionRequest>,
}

struct Requests {
    window: Weak<Window>,
    active: AtomicBool,
    pending: AtomicBool,
    generation: AtomicU64,
    actions: Mutex<Vec<ActionRequest>>,
}

impl Requests {
    fn wake(&self) {
        self.pending.store(true, Ordering::Release);
        if let Some(window) = self.window.upgrade() {
            window.request_redraw();
        }
    }

    fn take(&self) -> Option<AccessibilityRequest> {
        if !self.pending.load(Ordering::Acquire) || !self.pending.swap(false, Ordering::AcqRel) {
            return None;
        }
        Some(AccessibilityRequest {
            generation: self.generation.load(Ordering::Acquire),
            active: self.active.load(Ordering::Acquire),
            actions: std::mem::take(&mut *self.actions.lock().unwrap()),
        })
    }
}

#[derive(Clone)]
struct Handler(Arc<Requests>);

impl ActivationHandler for Handler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.active.store(true, Ordering::Release);
        self.0.generation.fetch_add(1, Ordering::AcqRel);
        self.0.wake();
        None
    }
}

impl ActionHandler for Handler {
    fn do_action(&mut self, action: ActionRequest) {
        self.0.actions.lock().unwrap().push(action);
        self.0.wake();
    }
}

impl DeactivationHandler for Handler {
    fn deactivate_accessibility(&mut self) {
        self.0.active.store(false, Ordering::Release);
        *self.0.actions.lock().unwrap() = Vec::new();
        self.0.wake();
    }
}

pub(crate) struct DesktopAccessibility {
    adapter: std::cell::RefCell<accesskit_winit::Adapter>,
    requests: Arc<Requests>,
}

impl DesktopAccessibility {
    pub(crate) fn new(event_loop: &ActiveEventLoop, window: &Arc<Window>) -> Self {
        let requests = Arc::new(Requests {
            window: Arc::downgrade(window),
            active: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            actions: Mutex::new(Vec::new()),
        });
        let handler = Handler(requests.clone());
        let adapter = accesskit_winit::Adapter::with_direct_handlers(
            event_loop,
            window,
            handler.clone(),
            handler.clone(),
            handler,
        );
        Self {
            adapter: std::cell::RefCell::new(adapter),
            requests,
        }
    }

    pub(crate) fn process_event(&self, window: &Window, event: &winit::event::WindowEvent) {
        self.adapter.borrow_mut().process_event(window, event);
    }

    pub(crate) fn is_active(&self) -> bool {
        self.requests.active.load(Ordering::Acquire)
    }
    pub(crate) fn is_pending(&self) -> bool {
        self.requests.pending.load(Ordering::Acquire)
    }
    pub(crate) fn generation(&self) -> u64 {
        self.requests.generation.load(Ordering::Acquire)
    }
    pub(crate) fn take(&self) -> Option<AccessibilityRequest> {
        self.requests.take()
    }
    pub(crate) fn update(&self, update: impl FnOnce() -> TreeUpdate) {
        self.adapter.borrow_mut().update_if_active(update);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_actions_and_deactivation_wake_without_a_window() {
        let state = Arc::new(Requests {
            window: Weak::new(),
            active: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            actions: Mutex::new(Vec::new()),
        });
        let mut handler = Handler(state.clone());
        assert!(state.take().is_none());
        assert!(handler.request_initial_tree().is_none());
        let activated = state.take().unwrap();
        assert!(activated.active);
        assert_eq!(activated.generation, 1);
        handler.do_action(ActionRequest {
            action: accesskit::Action::Focus,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(7),
            data: None,
        });
        assert_eq!(
            state.take().unwrap().actions[0].target_node,
            accesskit::NodeId(7)
        );
        handler.deactivate_accessibility();
        assert!(!state.take().unwrap().active);
        assert!(state.take().is_none());
        handler.request_initial_tree();
        assert_eq!(state.take().unwrap().generation, 2);
    }
}
