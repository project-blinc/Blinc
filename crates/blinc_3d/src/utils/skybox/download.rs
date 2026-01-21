//! Skybox asset download helper
//!
//! Provides utilities for downloading HDRI skybox assets from external sources.

use std::path::{Path, PathBuf};

/// Known HDRI asset sources
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdriSource {
    /// Poly Haven (polyhaven.com) - CC0 licensed HDRIs
    PolyHaven,
    /// HDRI Haven (legacy, redirects to Poly Haven)
    HdriHaven,
    /// ambientCG - CC0 licensed assets
    AmbientCG,
    /// Custom URL
    Custom,
}

impl HdriSource {
    /// Get the base URL for this source
    pub fn base_url(&self) -> Option<&'static str> {
        match self {
            HdriSource::PolyHaven => Some("https://dl.polyhaven.org/file/ph-assets/HDRIs"),
            HdriSource::HdriHaven => Some("https://dl.polyhaven.org/file/ph-assets/HDRIs"),
            HdriSource::AmbientCG => Some("https://ambientcg.com/get"),
            HdriSource::Custom => None,
        }
    }

    /// Get the license type for this source
    pub fn license(&self) -> &'static str {
        match self {
            HdriSource::PolyHaven => "CC0 1.0 Universal",
            HdriSource::HdriHaven => "CC0 1.0 Universal",
            HdriSource::AmbientCG => "CC0 1.0 Universal",
            HdriSource::Custom => "Unknown",
        }
    }

    /// Get attribution text for this source
    pub fn attribution(&self) -> &'static str {
        match self {
            HdriSource::PolyHaven => "HDRI from Poly Haven (polyhaven.com)",
            HdriSource::HdriHaven => "HDRI from Poly Haven (polyhaven.com)",
            HdriSource::AmbientCG => "HDRI from ambientCG (ambientcg.com)",
            HdriSource::Custom => "",
        }
    }
}

/// HDRI resolution options
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdriResolution {
    /// 1K resolution (1024x512)
    Res1K,
    /// 2K resolution (2048x1024)
    Res2K,
    /// 4K resolution (4096x2048)
    Res4K,
    /// 8K resolution (8192x4096)
    Res8K,
}

impl HdriResolution {
    /// Get the resolution suffix for URLs
    pub fn suffix(&self) -> &'static str {
        match self {
            HdriResolution::Res1K => "1k",
            HdriResolution::Res2K => "2k",
            HdriResolution::Res4K => "4k",
            HdriResolution::Res8K => "8k",
        }
    }

    /// Get the approximate file size in MB
    pub fn approx_size_mb(&self) -> f32 {
        match self {
            HdriResolution::Res1K => 0.5,
            HdriResolution::Res2K => 2.0,
            HdriResolution::Res4K => 8.0,
            HdriResolution::Res8K => 32.0,
        }
    }

    /// Get dimensions (width, height)
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            HdriResolution::Res1K => (1024, 512),
            HdriResolution::Res2K => (2048, 1024),
            HdriResolution::Res4K => (4096, 2048),
            HdriResolution::Res8K => (8192, 4096),
        }
    }
}

/// A downloadable HDRI asset
#[derive(Clone, Debug)]
pub struct HdriAsset {
    /// Asset name/identifier
    pub name: String,
    /// Display name
    pub display_name: String,
    /// Source of the asset
    pub source: HdriSource,
    /// Available resolutions
    pub resolutions: Vec<HdriResolution>,
    /// Tags/categories
    pub tags: Vec<String>,
}

impl HdriAsset {
    /// Create a new HDRI asset
    pub fn new(name: impl Into<String>, source: HdriSource) -> Self {
        let name = name.into();
        Self {
            display_name: name.replace('_', " "),
            name,
            source,
            resolutions: vec![
                HdriResolution::Res1K,
                HdriResolution::Res2K,
                HdriResolution::Res4K,
            ],
            tags: Vec::new(),
        }
    }

    /// Set display name
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    /// Set available resolutions
    pub fn with_resolutions(mut self, resolutions: Vec<HdriResolution>) -> Self {
        self.resolutions = resolutions;
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Get the download URL for a specific resolution
    pub fn download_url(&self, resolution: HdriResolution) -> Option<String> {
        match self.source {
            HdriSource::PolyHaven | HdriSource::HdriHaven => {
                Some(format!(
                    "{}/hdr/{}/{}.hdr",
                    self.source.base_url()?,
                    resolution.suffix(),
                    self.name
                ))
            }
            HdriSource::AmbientCG => {
                Some(format!(
                    "{}?file={}_{}-HDR.exr",
                    self.source.base_url()?,
                    self.name,
                    resolution.suffix().to_uppercase()
                ))
            }
            HdriSource::Custom => None,
        }
    }

    /// Get the preview/thumbnail URL
    pub fn preview_url(&self) -> Option<String> {
        match self.source {
            HdriSource::PolyHaven | HdriSource::HdriHaven => {
                Some(format!(
                    "https://cdn.polyhaven.com/asset_img/primary/{}.png?width=256",
                    self.name
                ))
            }
            HdriSource::AmbientCG => {
                Some(format!(
                    "https://ambientcg.com/get?file={}_preview.png",
                    self.name
                ))
            }
            HdriSource::Custom => None,
        }
    }
}

/// Popular HDRI assets from Poly Haven
pub fn polyhaven_popular() -> Vec<HdriAsset> {
    vec![
        HdriAsset::new("abandoned_parking", HdriSource::PolyHaven)
            .with_display_name("Abandoned Parking")
            .with_tags(vec!["urban".into(), "outdoor".into()]),
        HdriAsset::new("adams_place_bridge", HdriSource::PolyHaven)
            .with_display_name("Adams Place Bridge")
            .with_tags(vec!["urban".into(), "outdoor".into()]),
        HdriAsset::new("autumn_forest", HdriSource::PolyHaven)
            .with_display_name("Autumn Forest")
            .with_tags(vec!["nature".into(), "forest".into()]),
        HdriAsset::new("blue_lagoon_night", HdriSource::PolyHaven)
            .with_display_name("Blue Lagoon Night")
            .with_tags(vec!["night".into(), "water".into()]),
        HdriAsset::new("chinese_garden", HdriSource::PolyHaven)
            .with_display_name("Chinese Garden")
            .with_tags(vec!["garden".into(), "outdoor".into()]),
        HdriAsset::new("clarens_midday", HdriSource::PolyHaven)
            .with_display_name("Clarens Midday")
            .with_tags(vec!["outdoor".into(), "bright".into()]),
        HdriAsset::new("drakensberg_solitary_mountain", HdriSource::PolyHaven)
            .with_display_name("Drakensberg Mountain")
            .with_tags(vec!["mountain".into(), "outdoor".into()]),
        HdriAsset::new("evening_road", HdriSource::PolyHaven)
            .with_display_name("Evening Road")
            .with_tags(vec!["sunset".into(), "outdoor".into()]),
        HdriAsset::new("fouriesburg_mountain_lookout", HdriSource::PolyHaven)
            .with_display_name("Mountain Lookout")
            .with_tags(vec!["mountain".into(), "outdoor".into()]),
        HdriAsset::new("goegap", HdriSource::PolyHaven)
            .with_display_name("Goegap")
            .with_tags(vec!["desert".into(), "outdoor".into()]),
        HdriAsset::new("kloppenheim", HdriSource::PolyHaven)
            .with_display_name("Kloppenheim")
            .with_tags(vec!["outdoor".into(), "rural".into()]),
        HdriAsset::new("metro_noord", HdriSource::PolyHaven)
            .with_display_name("Metro Noord")
            .with_tags(vec!["urban".into(), "outdoor".into()]),
        HdriAsset::new("museum_of_ethnography", HdriSource::PolyHaven)
            .with_display_name("Museum Interior")
            .with_tags(vec!["indoor".into(), "studio".into()]),
        HdriAsset::new("photo_studio_01", HdriSource::PolyHaven)
            .with_display_name("Photo Studio")
            .with_tags(vec!["studio".into(), "indoor".into()]),
        HdriAsset::new("san_giuseppe_bridge", HdriSource::PolyHaven)
            .with_display_name("San Giuseppe Bridge")
            .with_tags(vec!["urban".into(), "outdoor".into()]),
        HdriAsset::new("snowy_park", HdriSource::PolyHaven)
            .with_display_name("Snowy Park")
            .with_tags(vec!["winter".into(), "snow".into()]),
        HdriAsset::new("spruit_sunrise", HdriSource::PolyHaven)
            .with_display_name("Spruit Sunrise")
            .with_tags(vec!["sunrise".into(), "outdoor".into()]),
        HdriAsset::new("studio_small_03", HdriSource::PolyHaven)
            .with_display_name("Small Studio")
            .with_tags(vec!["studio".into(), "indoor".into()]),
        HdriAsset::new("sunset_fairway", HdriSource::PolyHaven)
            .with_display_name("Sunset Fairway")
            .with_tags(vec!["sunset".into(), "outdoor".into()]),
        HdriAsset::new("urban_street_01", HdriSource::PolyHaven)
            .with_display_name("Urban Street")
            .with_tags(vec!["urban".into(), "outdoor".into()]),
    ]
}

/// Download configuration
#[derive(Clone, Debug)]
pub struct DownloadConfig {
    /// Target directory for downloaded files
    pub target_dir: PathBuf,
    /// Whether to overwrite existing files
    pub overwrite: bool,
    /// Maximum concurrent downloads
    pub max_concurrent: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            target_dir: PathBuf::from("assets/hdri"),
            overwrite: false,
            max_concurrent: 2,
        }
    }
}

impl DownloadConfig {
    /// Create with a specific target directory
    pub fn with_target_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.target_dir = dir.into();
        self
    }

    /// Set overwrite behavior
    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Get the full path for a downloaded asset
    pub fn asset_path(&self, asset: &HdriAsset, resolution: HdriResolution) -> PathBuf {
        self.target_dir.join(format!(
            "{}_{}.hdr",
            asset.name,
            resolution.suffix()
        ))
    }

    /// Check if an asset is already downloaded
    pub fn is_downloaded(&self, asset: &HdriAsset, resolution: HdriResolution) -> bool {
        self.asset_path(asset, resolution).exists()
    }
}

/// Download status for tracking progress
#[derive(Clone, Debug)]
pub enum DownloadStatus {
    /// Not started
    Pending,
    /// Currently downloading
    InProgress {
        /// Bytes downloaded
        bytes_downloaded: u64,
        /// Total bytes (if known)
        total_bytes: Option<u64>,
    },
    /// Download completed
    Completed {
        /// Path to downloaded file
        path: PathBuf,
    },
    /// Download failed
    Failed {
        /// Error message
        error: String,
    },
    /// Skipped (already exists)
    Skipped,
}

/// Download request for an HDRI asset
#[derive(Clone, Debug)]
pub struct DownloadRequest {
    /// The asset to download
    pub asset: HdriAsset,
    /// Desired resolution
    pub resolution: HdriResolution,
    /// Current status
    pub status: DownloadStatus,
}

impl DownloadRequest {
    /// Create a new download request
    pub fn new(asset: HdriAsset, resolution: HdriResolution) -> Self {
        Self {
            asset,
            resolution,
            status: DownloadStatus::Pending,
        }
    }

    /// Get the download URL
    pub fn url(&self) -> Option<String> {
        self.asset.download_url(self.resolution)
    }

    /// Get the expected file size in bytes
    pub fn expected_size(&self) -> u64 {
        (self.resolution.approx_size_mb() * 1024.0 * 1024.0) as u64
    }
}

/// Asset download helper
///
/// Note: This is a configuration/helper struct. Actual downloading
/// requires async runtime support which should be handled by the
/// application layer using libraries like `reqwest`.
///
/// # Example
///
/// ```ignore
/// let helper = AssetDownloadHelper::new(DownloadConfig::default());
///
/// // Get popular assets
/// let assets = polyhaven_popular();
///
/// // Create download request
/// let request = DownloadRequest::new(assets[0].clone(), HdriResolution::Res2K);
///
/// // Get URL for manual download
/// if let Some(url) = request.url() {
///     println!("Download from: {}", url);
/// }
///
/// // Or implement actual download with reqwest/ureq in your app
/// ```
pub struct AssetDownloadHelper {
    config: DownloadConfig,
}

impl AssetDownloadHelper {
    /// Create a new download helper
    pub fn new(config: DownloadConfig) -> Self {
        Self { config }
    }

    /// Get the configuration
    pub fn config(&self) -> &DownloadConfig {
        &self.config
    }

    /// Create a download request
    pub fn request(&self, asset: HdriAsset, resolution: HdriResolution) -> DownloadRequest {
        let mut request = DownloadRequest::new(asset, resolution);

        // Check if already downloaded
        if !self.config.overwrite && self.config.is_downloaded(&request.asset, request.resolution) {
            request.status = DownloadStatus::Skipped;
        }

        request
    }

    /// Get the target path for a request
    pub fn target_path(&self, request: &DownloadRequest) -> PathBuf {
        self.config.asset_path(&request.asset, request.resolution)
    }

    /// Ensure target directory exists
    pub fn ensure_target_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config.target_dir)
    }

    /// List downloaded assets in the target directory
    pub fn list_downloaded(&self) -> std::io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if self.config.target_dir.exists() {
            for entry in std::fs::read_dir(&self.config.target_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "hdr" || ext == "exr" {
                            files.push(path);
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Get disk usage of downloaded assets in bytes
    pub fn disk_usage(&self) -> std::io::Result<u64> {
        let mut total = 0u64;

        for path in self.list_downloaded()? {
            if let Ok(metadata) = std::fs::metadata(&path) {
                total += metadata.len();
            }
        }

        Ok(total)
    }

    /// Delete a downloaded asset
    pub fn delete_asset(&self, asset: &HdriAsset, resolution: HdriResolution) -> std::io::Result<()> {
        let path = self.config.asset_path(asset, resolution);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Clear all downloaded assets
    pub fn clear_all(&self) -> std::io::Result<()> {
        if self.config.target_dir.exists() {
            std::fs::remove_dir_all(&self.config.target_dir)?;
        }
        Ok(())
    }
}

/// Search for HDRI assets by tag
pub fn search_by_tag<'a>(assets: &'a [HdriAsset], tag: &str) -> Vec<&'a HdriAsset> {
    let tag_lower = tag.to_lowercase();
    assets
        .iter()
        .filter(|a| a.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)))
        .collect()
}

/// Filter assets by source
pub fn filter_by_source<'a>(assets: &'a [HdriAsset], source: HdriSource) -> Vec<&'a HdriAsset> {
    assets.iter().filter(|a| a.source == source).collect()
}
