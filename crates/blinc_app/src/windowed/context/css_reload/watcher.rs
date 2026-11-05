use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const SETTLE_TIME: Duration = Duration::from_millis(100);

pub(in crate::windowed::context) struct CssWatcher {
    pub path: PathBuf,
    pending: Arc<Mutex<Option<io::Result<String>>>>,
    watcher: Option<RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
}

impl CssWatcher {
    pub fn resolve(path: &Path) -> io::Result<PathBuf> {
        let absolute = std::path::absolute(path)?;
        let parent = absolute
            .parent()
            .ok_or_else(|| io::Error::other("expected a CSS file"))?;
        let name = absolute
            .file_name()
            .ok_or_else(|| io::Error::other("expected a CSS file"))?;
        Ok(parent.canonicalize()?.join(name))
    }

    pub fn new(path: PathBuf, wake: impl Fn() + Send + 'static) -> io::Result<(Self, String)> {
        let pending = Arc::new(Mutex::new(None));
        // A save needs one notification, even if the editor emits many events.
        let (tx, rx) = mpsc::sync_channel(1);
        let target = path.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) && event.paths.iter().any(|path| path == &target) =>
                {
                    let _ = tx.try_send(());
                }
                Err(error) => tracing::warn!(%error, "CSS watcher error"),
                _ => {}
            })
            .map_err(io::Error::other)?;
        // Watching the directory also catches editors that replace the file on save.
        watcher
            .watch(path.parent().unwrap(), RecursiveMode::NonRecursive)
            .map_err(io::Error::other)?;
        let css = std::fs::read_to_string(&path)?;
        let mut source = Some(css.clone());
        let output = Arc::clone(&pending);
        let file = path.clone();
        let thread = thread::Builder::new()
            .name("blinc-css".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    loop {
                        match rx.recv_timeout(SETTLE_TIME) {
                            Ok(()) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                        }
                    }
                    let update = std::fs::read_to_string(&file);
                    if let Ok(css) = &update {
                        if source.as_ref() == Some(css) {
                            continue;
                        }
                        source = Some(css.clone());
                    } else {
                        source = None;
                    }
                    *output.lock().unwrap() = Some(update);
                    wake();
                }
            })?;
        Ok((
            Self {
                path,
                pending,
                watcher: Some(watcher),
                thread: Some(thread),
            },
            css,
        ))
    }

    pub fn take_update(&mut self) -> Option<io::Result<String>> {
        self.pending.lock().unwrap().take()
    }
}

impl Drop for CssWatcher {
    fn drop(&mut self) {
        drop(self.watcher.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
