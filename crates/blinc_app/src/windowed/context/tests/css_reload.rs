use super::*;

#[cfg(all(
    debug_assertions,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
))]
mod desktop {
    use super::*;
    use blinc_layout::{div::div, renderer::RenderTree};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    struct Files(PathBuf);

    impl Files {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "blinc-css-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, css: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, css).unwrap();
            path
        }
    }

    impl Drop for Files {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn watch(ctx: &mut WindowedContext, path: &Path) -> mpsc::Receiver<()> {
        let (tx, rx) = mpsc::channel();
        ctx.animations.lock().unwrap().set_wake_callback(move || {
            let _ = tx.send(());
        });
        ctx.watch_css("app", path).unwrap();
        rx
    }

    fn reload(ctx: &mut WindowedContext, wake: &mpsc::Receiver<()>) -> bool {
        wake.recv_timeout(Duration::from_secs(5))
            .expect("file save must wake an idle app");
        ctx.reload_changed_css()
    }

    fn source<'a>(ctx: &'a WindowedContext, name: &str) -> &'a str {
        &ctx.css_overrides
            .iter()
            .find(|(key, _)| key == name)
            .unwrap()
            .1
    }

    #[test]
    fn saves_replace_rules_and_keep_signals_and_node_identity() {
        let files = Files::new();
        let path = files.write("app.css", ".card { width: 120px; height: 40px; }");
        let mut ctx = WindowedContext::headless(400.0, 300.0);
        ctx.add_css(":root { --card-width: 180px; } .card { height: 60px; }");
        let count = ctx.use_signal_keyed("count", || 0);
        count.set(7);
        let wake = watch(&mut ctx, &path);
        let view = div().class("card");
        let mut tree = RenderTree::from_element(&view);
        let root = tree.root().unwrap();
        let stable = tree.stable_id(root);
        tree.set_stylesheet_arc(ctx.stylesheet.clone().unwrap());
        tree.apply_all_stylesheet_styles();
        tree.compute_layout(400.0, 300.0);
        assert_eq!(tree.layout_tree.get_layout(root).unwrap().size.width, 120.0);

        files.write("app.css", ".card { width: var(--card-width); }");
        assert!(reload(&mut ctx, &wake));
        tree.set_stylesheet_arc(ctx.stylesheet.clone().unwrap());
        tree.incremental_update(&view);
        tree.apply_all_stylesheet_styles();
        tree.compute_layout(400.0, 300.0);
        assert_eq!(tree.layout_tree.get_layout(root).unwrap().size.width, 180.0);
        assert_eq!(tree.layout_tree.get_layout(root).unwrap().size.height, 60.0);
        assert_eq!(tree.stable_id(root), stable);
        assert_eq!(ctx.use_signal_keyed("count", || 0).get(), 7);
        assert_eq!(ctx.css_sources.len(), 1);
        assert_eq!(ctx.css_overrides.len(), 1);
    }

    #[test]
    fn invalid_and_missing_saves_keep_last_valid_css_and_recover() {
        let files = Files::new();
        let valid = ".card { width: 120px; }";
        let path = files.write("app.css", valid);
        let mut ctx = WindowedContext::headless(400.0, 300.0);
        let wake = watch(&mut ctx, &path);
        let sheet = ctx.stylesheet.clone().unwrap();
        files.write("app.css", ".card { background:");
        assert!(!reload(&mut ctx, &wake));
        assert_eq!(source(&ctx, "app"), valid);
        assert!(Arc::ptr_eq(&sheet, ctx.stylesheet.as_ref().unwrap()));
        std::fs::remove_file(&path).unwrap();
        assert!(!reload(&mut ctx, &wake));
        assert!(Arc::ptr_eq(&sheet, ctx.stylesheet.as_ref().unwrap()));
        let fixed = ".card { width: 180px; }";
        files.write("app.css", fixed);
        assert!(reload(&mut ctx, &wake));
        assert_eq!(source(&ctx, "app"), fixed);
    }

    #[test]
    fn atomic_saves_empty_files_and_repeated_registration_work() {
        let files = Files::new();
        let path = files.write("app.css", ".card { width: 120px; }");
        let mut ctx = WindowedContext::headless(400.0, 300.0);
        let wake = watch(&mut ctx, &path);
        for _ in 0..10 {
            ctx.watch_css("app", &path).unwrap();
        }
        assert_eq!(ctx.css_watches.len(), 1);
        let saved = files.write("saved.css", ".card { width: 200px; }");
        std::fs::rename(saved, &path).unwrap();
        assert!(reload(&mut ctx, &wake));
        assert_eq!(source(&ctx, "app"), ".card { width: 200px; }");
        files.write("app.css", "");
        assert!(reload(&mut ctx, &wake));
        assert_eq!(source(&ctx, "app"), "");
        assert!(ctx.unwatch_css("app"));
        assert!(!ctx.unwatch_css("app"));
        files.write("app.css", ".card { width: 300px; }");
        assert!(wake.recv_timeout(Duration::from_millis(250)).is_err());
        assert!(!ctx.reload_changed_css());
    }

    #[test]
    fn bursts_coalesce_and_unrelated_files_do_not_trigger_reloads() {
        let files = Files::new();
        let path = files.write("app.css", ".card { width: 120px; }");
        let mut ctx = WindowedContext::headless(400.0, 300.0);
        let wake = watch(&mut ctx, &path);
        files.write("other.css", ".other { width: 90px; }");
        assert!(wake.recv_timeout(Duration::from_millis(300)).is_err());
        for width in 130..140 {
            files.write("app.css", &format!(".card {{ width: {width}px; }}"));
        }
        assert!(reload(&mut ctx, &wake));
        assert_eq!(source(&ctx, "app"), ".card { width: 139px; }");
        files.write("app.css", ".card { width: 139px; }");
        assert!(wake.recv_timeout(Duration::from_millis(250)).is_err());
        assert!(!ctx.reload_changed_css());
        assert_eq!(ctx.css_overrides.len(), 1);
    }

    #[test]
    fn named_sources_and_contexts_are_isolated_and_drop_stops_watchers() {
        let files = Files::new();
        let a = files.write("a.css", ".a { width: 120px; }");
        let b = files.write("b.css", ".b { width: 80px; }");
        let mut first = WindowedContext::headless(400.0, 300.0);
        let mut second = WindowedContext::headless(400.0, 300.0);
        let wake = watch(&mut first, &a);
        second.watch_css("app", &b).unwrap();
        first.watch_css("other", &b).unwrap();
        files.write("a.css", ".a { width: 150px; }");
        assert!(reload(&mut first, &wake));
        assert_eq!(source(&first, "other"), ".b { width: 80px; }");
        assert!(!second.reload_changed_css());
        assert_eq!(source(&second, "app"), ".b { width: 80px; }");
        let start = Instant::now();
        drop(first);
        drop(second);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn failed_registration_keeps_previous_source_and_watch() {
        let files = Files::new();
        let path = files.write("app.css", ".card { width: 120px; }");
        let invalid = files.write("invalid.css", ".card { background:");
        let mut ctx = WindowedContext::headless(400.0, 300.0);
        let wake = watch(&mut ctx, &path);
        assert!(ctx.watch_css("app", &invalid).is_err());
        assert!(ctx.watch_css("app", files.0.join("missing.css")).is_err());
        assert_eq!(ctx.css_watches.len(), 1);
        assert_eq!(source(&ctx, "app"), ".card { width: 120px; }");
        files.write("app.css", ".card { width: 160px; }");
        assert!(reload(&mut ctx, &wake));
        assert_eq!(source(&ctx, "app"), ".card { width: 160px; }");
    }
}

#[cfg(not(all(
    debug_assertions,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
)))]
#[test]
fn watching_is_a_noop_without_debug_desktop_support() {
    let mut ctx = WindowedContext::headless(400.0, 300.0);
    ctx.set_css("app", ".card { width: 120px; }").unwrap();
    let sheet = ctx.stylesheet.clone().unwrap();
    ctx.watch_css("app", "/missing-directory/no-file.css")
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &sheet,
        ctx.stylesheet.as_ref().unwrap()
    ));
    assert!(!ctx.unwatch_css("app"));
}
