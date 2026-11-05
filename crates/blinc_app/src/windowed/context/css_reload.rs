//! Opt-in CSS file replacement for desktop development.

use super::WindowedContext;
use std::path::Path;

#[cfg(all(
    debug_assertions,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
))]
pub(super) mod watcher;

impl WindowedContext {
    /// Load a named CSS file and watch it for changes in debug desktop builds.
    ///
    /// Requires `css-hot-reload`. Changes use [`Self::set_css`], preserving
    /// component state and the last valid stylesheet on read or parse errors.
    /// Re-registering the same name and path does not create another watcher.
    /// The context owns each watcher until it closes or [`Self::unwatch_css`]
    /// is called. Relative paths use the process's current directory.
    ///
    /// In release builds and on web/mobile this is a no-op: it does not read
    /// files or start threads. Register embedded CSS separately for those builds.
    pub fn watch_css(&mut self, name: &str, path: impl AsRef<Path>) -> crate::Result<()> {
        #[cfg(all(
            debug_assertions,
            any(target_os = "macos", target_os = "linux", target_os = "windows")
        ))]
        {
            use crate::BlincError;
            use watcher::CssWatcher;

            let path = CssWatcher::resolve(path.as_ref()).map_err(|e| {
                BlincError::Other(format!("CSS watch '{}': {e}", path.as_ref().display()))
            })?;
            if self.css_watches.get(name).is_some_and(|w| w.path == path)
                && self.css_overrides.iter().any(|(key, _)| key == name)
            {
                return Ok(());
            }
            let scheduler = self.animations.lock().unwrap().handle();
            let (watch, css) = CssWatcher::new(path.clone(), move || scheduler.request_redraw())
                .map_err(|e| BlincError::Other(format!("CSS watch '{}': {e}", path.display())))?;
            self.set_css(name, &css)
                .map_err(|e| BlincError::Other(format!("CSS parse '{}': {e}", path.display())))?;
            self.css_watches.insert(name.to_string(), watch);
        }
        #[cfg(not(all(
            debug_assertions,
            any(target_os = "macos", target_os = "linux", target_os = "windows")
        )))]
        let _ = (name, path);
        Ok(())
    }

    /// Stop watching a named source, keeping its current CSS in place.
    /// Returns whether a watcher was removed; always false outside debug desktop.
    pub fn unwatch_css(&mut self, name: &str) -> bool {
        #[cfg(all(
            debug_assertions,
            any(target_os = "macos", target_os = "linux", target_os = "windows")
        ))]
        return self.css_watches.remove(name).is_some();
        #[cfg(not(all(
            debug_assertions,
            any(target_os = "macos", target_os = "linux", target_os = "windows")
        )))]
        {
            let _ = name;
            false
        }
    }

    #[cfg(all(
        debug_assertions,
        any(target_os = "macos", target_os = "linux", target_os = "windows")
    ))]
    pub(in crate::windowed) fn reload_changed_css(&mut self) -> bool {
        let updates: Vec<_> = self
            .css_watches
            .iter_mut()
            .filter_map(|(name, watch)| {
                watch
                    .take_update()
                    .map(|update| (name.clone(), watch.path.clone(), update))
            })
            .collect();
        let mut changed = false;
        for (name, path, update) in updates {
            match update {
                Ok(css) => match self.set_css(&name, &css) {
                    Ok(()) => {
                        changed = true;
                        tracing::debug!(source = name, path = %path.display(), "CSS reloaded");
                    }
                    Err(error) => {
                        tracing::warn!(source = name, path = %path.display(), %error, "Keeping previous CSS")
                    }
                },
                Err(error) => {
                    tracing::warn!(source = name, path = %path.display(), %error, "Keeping previous CSS")
                }
            }
        }
        changed
    }
}

#[cfg(test)]
#[path = "tests/css_reload.rs"]
mod tests;
