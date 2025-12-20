//! Blinc configuration file handling
//!
//! Blinc uses two configuration files:
//! - `.blincproj` - Project configuration (metadata, dependencies, targets)
//! - `blinc.toml` - Workspace configuration (build settings, dev server)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// =============================================================================
// .blincproj - Project Configuration
// =============================================================================

/// Project configuration stored in .blincproj
#[derive(Debug, Deserialize, Serialize)]
pub struct BlincProject {
    pub project: ProjectMetadata,
    #[serde(default)]
    pub dependencies: DependenciesConfig,
    #[serde(default)]
    pub platforms: PlatformsConfig,
}

/// Project metadata
#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectMetadata {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// Dependencies configuration
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct DependenciesConfig {
    /// Local plugins (path-based)
    #[serde(default)]
    pub plugins: Vec<PluginDependency>,
    /// External dependencies (future: registry-based)
    #[serde(default)]
    pub external: Vec<ExternalDependency>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginDependency {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExternalDependency {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
}

/// Platform-specific configurations
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PlatformsConfig {
    #[serde(default)]
    pub android: Option<AndroidPlatformConfig>,
    #[serde(default)]
    pub ios: Option<IosPlatformConfig>,
    #[serde(default)]
    pub macos: Option<MacosPlatformConfig>,
    #[serde(default)]
    pub windows: Option<WindowsPlatformConfig>,
    #[serde(default)]
    pub linux: Option<LinuxPlatformConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AndroidPlatformConfig {
    /// Android package name (e.g., com.example.app)
    pub package: String,
    /// Minimum SDK version
    #[serde(default = "default_min_sdk")]
    pub min_sdk: u32,
    /// Target SDK version
    #[serde(default = "default_target_sdk")]
    pub target_sdk: u32,
    /// Version code for Play Store
    #[serde(default = "default_version_code")]
    pub version_code: u32,
}

fn default_min_sdk() -> u32 {
    24
}

fn default_target_sdk() -> u32 {
    35
}

fn default_version_code() -> u32 {
    1
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IosPlatformConfig {
    /// iOS bundle identifier
    pub bundle_id: String,
    /// Minimum iOS version
    #[serde(default = "default_ios_target")]
    pub deployment_target: String,
    /// Team ID for signing
    #[serde(default)]
    pub team_id: Option<String>,
}

fn default_ios_target() -> String {
    "15.0".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MacosPlatformConfig {
    /// macOS bundle identifier
    pub bundle_id: String,
    /// Minimum macOS version
    #[serde(default = "default_macos_target")]
    pub deployment_target: String,
    /// App category
    #[serde(default)]
    pub category: Option<String>,
}

fn default_macos_target() -> String {
    "12.0".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WindowsPlatformConfig {
    /// Product name
    #[serde(default)]
    pub product_name: Option<String>,
    /// Company name
    #[serde(default)]
    pub company: Option<String>,
    /// File description
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LinuxPlatformConfig {
    /// Desktop entry name
    #[serde(default)]
    pub desktop_name: Option<String>,
    /// Desktop entry categories
    #[serde(default)]
    pub categories: Vec<String>,
}

impl BlincProject {
    /// Load project configuration from .blincproj
    pub fn load_from_dir(path: &Path) -> Result<Self> {
        let config_path = path.join(".blincproj");

        if !config_path.exists() {
            anyhow::bail!(
                "No .blincproj found in {}. Run `blinc init` to create one.",
                path.display()
            );
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;

        let config: BlincProject = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;

        Ok(config)
    }

    /// Create a new project configuration
    pub fn new(name: &str) -> Self {
        Self {
            project: ProjectMetadata {
                name: name.to_string(),
                version: default_version(),
                description: None,
                authors: Vec::new(),
                license: None,
                repository: None,
            },
            dependencies: DependenciesConfig::default(),
            platforms: PlatformsConfig::default(),
        }
    }

    /// Create with all platforms enabled
    pub fn with_all_platforms(mut self, name: &str) -> Self {
        let package_name = name.replace('-', "_").replace(' ', "_").to_lowercase();

        self.platforms = PlatformsConfig {
            android: Some(AndroidPlatformConfig {
                package: format!("com.example.{}", package_name),
                min_sdk: default_min_sdk(),
                target_sdk: default_target_sdk(),
                version_code: default_version_code(),
            }),
            ios: Some(IosPlatformConfig {
                bundle_id: format!("com.example.{}", package_name),
                deployment_target: default_ios_target(),
                team_id: None,
            }),
            macos: Some(MacosPlatformConfig {
                bundle_id: format!("com.example.{}", package_name),
                deployment_target: default_macos_target(),
                category: None,
            }),
            windows: Some(WindowsPlatformConfig {
                product_name: Some(name.to_string()),
                company: None,
                description: None,
            }),
            linux: Some(LinuxPlatformConfig {
                desktop_name: Some(name.to_string()),
                categories: vec!["Utility".to_string()],
            }),
        };

        self
    }

    /// Serialize to TOML string
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize project config")
    }
}

// =============================================================================
// blinc.toml - Workspace Configuration (Legacy/Backward Compatibility)
// =============================================================================

/// Workspace-level Blinc configuration (blinc.toml)
/// This is for build settings and dev server configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct BlincConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub dev: DevConfig,
    #[serde(default)]
    pub targets: TargetsConfig,
}

/// Project metadata (legacy format)
#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
}

/// Build configuration
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BuildConfig {
    /// Entry point file (relative to project root)
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Output directory
    #[serde(default = "default_output")]
    pub output: String,
    /// Additional source directories to include
    #[serde(default)]
    pub include: Vec<String>,
    /// Files/patterns to exclude
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_entry() -> String {
    "src/main.blinc".to_string()
}

fn default_output() -> String {
    "target".to_string()
}

/// Development server configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct DevConfig {
    /// Hot-reload port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Enable hot-reload
    #[serde(default = "default_true")]
    pub hot_reload: bool,
    /// Watch additional directories
    #[serde(default)]
    pub watch: Vec<String>,
}

fn default_port() -> u16 {
    3000
}

fn default_true() -> bool {
    true
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            hot_reload: true,
            watch: Vec::new(),
        }
    }
}

/// Target-specific configuration (legacy)
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TargetsConfig {
    #[serde(default)]
    pub desktop: Option<DesktopConfig>,
    #[serde(default)]
    pub android: Option<AndroidConfig>,
    #[serde(default)]
    pub ios: Option<IosConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DesktopConfig {
    #[serde(default)]
    pub window_title: Option<String>,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub resizable: bool,
}

fn default_width() -> u32 {
    800
}

fn default_height() -> u32 {
    600
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AndroidConfig {
    pub package: String,
    #[serde(default = "default_min_sdk")]
    pub min_sdk: u32,
    #[serde(default = "default_target_sdk")]
    pub target_sdk: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IosConfig {
    pub bundle_id: String,
    #[serde(default = "default_ios_target")]
    pub deployment_target: String,
}

impl BlincConfig {
    /// Load configuration from a directory
    /// Checks for .blincproj first, falls back to blinc.toml
    pub fn load_from_dir(path: &Path) -> Result<Self> {
        let blincproj_path = path.join(".blincproj");
        let blinc_toml_path = path.join("blinc.toml");

        // Try .blincproj first (new format)
        if blincproj_path.exists() {
            let project = BlincProject::load_from_dir(path)?;
            return Ok(Self::from_project(&project));
        }

        // Fall back to blinc.toml (legacy format)
        if blinc_toml_path.exists() {
            let content = fs::read_to_string(&blinc_toml_path)
                .with_context(|| format!("Failed to read {}", blinc_toml_path.display()))?;

            let config: BlincConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse {}", blinc_toml_path.display()))?;

            return Ok(config);
        }

        anyhow::bail!(
            "No .blincproj or blinc.toml found in {}. Run `blinc init` to create one.",
            path.display()
        );
    }

    /// Convert from BlincProject to BlincConfig
    fn from_project(project: &BlincProject) -> Self {
        Self {
            project: ProjectConfig {
                name: project.project.name.clone(),
                version: project.project.version.clone(),
                description: project.project.description.clone(),
                authors: project.project.authors.clone(),
            },
            build: BuildConfig::default(),
            dev: DevConfig::default(),
            targets: TargetsConfig {
                desktop: None,
                android: project.platforms.android.as_ref().map(|a| AndroidConfig {
                    package: a.package.clone(),
                    min_sdk: a.min_sdk,
                    target_sdk: a.target_sdk,
                }),
                ios: project.platforms.ios.as_ref().map(|i| IosConfig {
                    bundle_id: i.bundle_id.clone(),
                    deployment_target: i.deployment_target.clone(),
                }),
            },
        }
    }

    /// Create a new configuration with the given project name
    pub fn new(name: &str) -> Self {
        Self {
            project: ProjectConfig {
                name: name.to_string(),
                version: default_version(),
                description: None,
                authors: Vec::new(),
            },
            build: BuildConfig::default(),
            dev: DevConfig::default(),
            targets: TargetsConfig::default(),
        }
    }

    /// Serialize to TOML string
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize config")
    }
}
