//! Golden images: a frame compared with a recorded one.

use std::path::{Path, PathBuf};

use super::framebuffer::{CapturedFrame, ScreenshotExporter};

/// How a frame differs from a golden.
#[derive(Clone, Copy, Debug)]
pub struct Difference {
    pub size_mismatch: bool,
    /// Pixels off by more than the tolerance in some channel.
    pub differing: usize,
    pub total: usize,
    /// The largest channel difference anywhere.
    pub max_channel: u8,
}

impl Difference {
    pub fn ratio(&self) -> f64 {
        self.differing as f64 / self.total.max(1) as f64
    }
}

fn worst(a: &[u8], b: &[u8]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

/// How `frame` differs from `other`, counting pixels off by more than
/// `tolerance` in any channel.
pub fn difference(frame: &CapturedFrame, other: &CapturedFrame, tolerance: u8) -> Difference {
    if (frame.width, frame.height) != (other.width, other.height) {
        let total = frame.pixel_count().max(other.pixel_count());
        return Difference {
            size_mismatch: true,
            differing: total,
            total,
            max_channel: 255,
        };
    }
    let (mut differing, mut max_channel) = (0, 0u8);
    for (a, b) in frame.data.chunks_exact(4).zip(other.data.chunks_exact(4)) {
        let w = worst(a, b);
        max_channel = max_channel.max(w);
        if w > tolerance {
            differing += 1;
        }
    }
    Difference {
        size_mismatch: false,
        differing,
        total: frame.pixel_count(),
        max_channel,
    }
}

/// A picture of where `frame` differs from `other`: matching pixels dimmed,
/// differing ones red.
pub fn diff_image(frame: &CapturedFrame, other: &CapturedFrame, tolerance: u8) -> CapturedFrame {
    let mut data = Vec::with_capacity(frame.data.len());
    for (a, b) in frame.data.chunks_exact(4).zip(other.data.chunks_exact(4)) {
        if worst(a, b) > tolerance {
            data.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            data.extend_from_slice(&[a[0] / 3, a[1] / 3, a[2] / 3, 255]);
        }
    }
    CapturedFrame::new(data, frame.width, frame.height)
}

/// Read an RGBA8 PNG.
pub fn load_png(path: &Path) -> std::io::Result<CapturedFrame> {
    let file = std::fs::File::open(path)?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(std::io::Error::other)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(std::io::Error::other)?;
    buf.truncate(info.buffer_size());
    let data = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => {
            return Err(std::io::Error::other(format!("unsupported PNG {other:?}")));
        }
    };
    Ok(CapturedFrame::new(data, info.width, info.height))
}

/// The workspace root above the crate under test: the nearest ancestor whose
/// `Cargo.toml` declares a `[workspace]`, else the crate's own directory.
fn workspace_root() -> PathBuf {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    manifest
        .ancestors()
        .find(|dir| {
            std::fs::read_to_string(dir.join("Cargo.toml"))
                .is_ok_and(|toml| toml.contains("[workspace]"))
        })
        .unwrap_or(&manifest)
        .to_path_buf()
}

/// Golden images in a directory.
pub struct Golden {
    dir: PathBuf,
    out_dir: PathBuf,
    tolerance: u8,
    max_ratio: f64,
}

impl Golden {
    /// `.blinc/snapshots/<crate>/<os>/` at the workspace root, for the crate
    /// whose tests are running (`BLINC_SNAPSHOT_DIR` replaces
    /// `.blinc/snapshots`). The directory is per machine and not tracked:
    /// adapters differ in how they antialias, so each platform keeps its own
    /// set. Failed frames and their diffs go to `failed/` beside the goldens.
    pub fn in_crate() -> Self {
        let root = std::env::var_os("BLINC_SNAPSHOT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace_root().join(".blinc").join("snapshots"));
        let krate = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "blinc".into());
        let dir = root.join(krate).join(std::env::consts::OS);
        Self {
            out_dir: dir.join("failed"),
            dir,
            tolerance: 2,
            max_ratio: 0.0005,
        }
    }

    /// Whether comparisons are skipped: `BLINC_SNAPSHOT=off`, or no goldens
    /// have been recorded for this platform (unless blessing).
    pub fn skipped(&self) -> bool {
        match std::env::var("BLINC_SNAPSHOT").as_deref() {
            Ok("off") => true,
            Ok("bless") => false,
            _ => !self.dir.is_dir(),
        }
    }

    /// Channel difference allowed per pixel (default 2).
    pub fn tolerance(mut self, tolerance: u8) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Share of pixels allowed to differ (default 0.05%).
    pub fn max_ratio(mut self, ratio: f64) -> Self {
        self.max_ratio = ratio;
        self
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.png"))
    }

    fn save(frame: &CapturedFrame, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        ScreenshotExporter::save_png(frame, path).expect("write png");
    }

    /// Compare `frame` with the golden `name`, or write it when
    /// `BLINC_SNAPSHOT=bless`. A missing golden fails. With
    /// `BLINC_SNAPSHOT=report` a difference is logged and its images written,
    /// and the test goes on, to see every change in one run.
    pub fn check(&self, name: &str, frame: &CapturedFrame) {
        let path = self.path(name);
        if std::env::var("BLINC_SNAPSHOT").is_ok_and(|v| v == "bless") {
            Self::save(frame, &path);
            return;
        }
        let actual = self.out_dir.join(format!("{name}.actual.png"));
        let Ok(want) = load_png(&path) else {
            Self::save(frame, &actual);
            panic!(
                "no golden for `{name}` at {}\nthe frame is at {}\nrun with BLINC_SNAPSHOT=bless to record it",
                path.display(),
                actual.display()
            );
        };
        let diff = difference(frame, &want, self.tolerance);
        if !diff.size_mismatch && diff.ratio() <= self.max_ratio {
            return;
        }
        let diff_path = self.out_dir.join(format!("{name}.diff.png"));
        Self::save(frame, &actual);
        if !diff.size_mismatch {
            Self::save(&diff_image(frame, &want, self.tolerance), &diff_path);
        }
        if std::env::var("BLINC_SNAPSHOT").is_ok_and(|v| v == "report") {
            eprintln!(
                "SNAPSHOT `{name}`: {} of {} pixels differ (largest channel difference {}){}",
                diff.differing,
                diff.total,
                diff.max_channel,
                if diff.size_mismatch {
                    ", size changed"
                } else {
                    ""
                },
            );
            return;
        }
        panic!(
            "`{name}` no longer looks as recorded: {} of {} pixels differ by more than {} (largest channel difference {}){}\nactual: {}\ndiff:   {}\ngolden: {}",
            diff.differing,
            diff.total,
            self.tolerance,
            diff.max_channel,
            if diff.size_mismatch {
                ", and the size changed"
            } else {
                ""
            },
            actual.display(),
            diff_path.display(),
            path.display(),
        );
    }
}
