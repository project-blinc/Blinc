//! Blinc CLI
//!
//! Build, run, and hot-reload Blinc applications.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod config;
mod doctor;
mod project;

use config::BlincConfig;

#[derive(Parser)]
#[command(name = "blinc")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Blinc UI Framework CLI", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a Blinc application
    Build {
        /// Source file or directory
        #[arg(default_value = ".")]
        source: String,

        /// Target platform (desktop, android, ios, macos, windows, linux)
        #[arg(short, long, default_value = "desktop")]
        target: String,

        /// Build in release mode
        #[arg(short, long)]
        release: bool,

        /// Output path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Run a Blinc application with hot-reload (development mode)
    Dev {
        /// Source file or directory
        #[arg(default_value = ".")]
        source: String,

        /// Target platform
        #[arg(short, long, default_value = "desktop")]
        target: String,

        /// Port for hot-reload server
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Device to run on (for mobile targets)
        #[arg(long)]
        device: Option<String>,
    },

    /// Run a compiled Blinc application
    Run {
        /// Compiled binary or source file
        #[arg(default_value = ".")]
        source: String,
    },

    /// Build a ZRTL plugin
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Create a new Blinc project
    New {
        /// Project name
        name: String,

        /// Template to use (default, minimal, counter)
        #[arg(short, long, default_value = "default")]
        template: String,
    },

    /// Initialize a Blinc project in the current directory
    Init {
        /// Template to use
        #[arg(short, long, default_value = "default")]
        template: String,
    },

    /// Check a Blinc project for errors
    Check {
        /// Source file or directory
        #[arg(default_value = ".")]
        source: String,
    },

    /// Show toolchain and target information
    Info,

    /// Check platform setup and dependencies
    Doctor,
}

#[derive(Subcommand)]
enum PluginCommands {
    /// Build a plugin
    Build {
        /// Plugin directory
        #[arg(default_value = ".")]
        path: String,

        /// Plugin mode (dynamic or static)
        #[arg(short, long, default_value = "dynamic")]
        mode: String,
    },

    /// Create a new plugin project
    New {
        /// Plugin name
        name: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    match cli.command {
        Commands::Build {
            source,
            target,
            release,
            output,
        } => cmd_build(&source, &target, release, output.as_deref()),

        Commands::Dev {
            source,
            target,
            port,
            device,
        } => cmd_dev(&source, &target, port, device.as_deref()),

        Commands::Run { source } => cmd_run(&source),

        Commands::Plugin { command } => match command {
            PluginCommands::Build { path, mode } => cmd_plugin_build(&path, &mode),
            PluginCommands::New { name } => cmd_plugin_new(&name),
        },

        Commands::New { name, template } => cmd_new(&name, &template),

        Commands::Init { template } => cmd_init(&template),

        Commands::Check { source } => cmd_check(&source),

        Commands::Info => cmd_info(),

        Commands::Doctor => cmd_doctor(),
    }
}

fn cmd_build(source: &str, target: &str, release: bool, output: Option<&str>) -> Result<()> {
    let path = PathBuf::from(source);
    let config = BlincConfig::load_from_dir(&path)?;

    info!(
        "Building {} for {} ({})",
        config.project.name,
        target,
        if release { "release" } else { "debug" }
    );

    // Validate target
    let valid_targets = ["desktop", "android", "ios", "macos", "windows", "linux"];
    if !valid_targets.contains(&target) {
        anyhow::bail!(
            "Invalid target '{}'. Valid targets: {:?}",
            target,
            valid_targets
        );
    }

    // TODO: When Zyntax Grammar2 is ready:
    // 1. Parse .blinc files
    // 2. Generate Rust code
    // 3. Compile with cargo

    warn!("Build not yet implemented - waiting for Zyntax Grammar2");

    if let Some(out) = output {
        info!("Output will be written to: {}", out);
    }

    Ok(())
}

fn cmd_dev(source: &str, target: &str, port: u16, device: Option<&str>) -> Result<()> {
    let path = PathBuf::from(source);
    let config = BlincConfig::load_from_dir(&path)?;

    info!(
        "Starting dev server for {} on port {} targeting {}",
        config.project.name, port, target
    );

    if let Some(dev) = device {
        info!("Running on device: {}", dev);
    }

    // TODO: When Zyntax Grammar2 is ready:
    // 1. Start file watcher
    // 2. Compile on change (using JIT)
    // 3. Push updates to running app

    warn!("Dev server not yet implemented - waiting for Zyntax Runtime2");

    Ok(())
}

fn cmd_run(source: &str) -> Result<()> {
    info!("Running {}", source);

    // TODO: Execute compiled binary or interpret source
    warn!("Run not yet implemented - waiting for Zyntax Runtime2");

    Ok(())
}

fn cmd_plugin_build(path: &str, mode: &str) -> Result<()> {
    info!("Building plugin at {} (mode: {})", path, mode);

    let valid_modes = ["dynamic", "static"];
    if !valid_modes.contains(&mode) {
        anyhow::bail!("Invalid mode '{}'. Valid modes: {:?}", mode, valid_modes);
    }

    // TODO: Build the plugin crate with appropriate flags
    warn!("Plugin build not yet implemented");

    Ok(())
}

fn cmd_plugin_new(name: &str) -> Result<()> {
    info!("Creating new plugin: {}", name);

    let path = PathBuf::from(name);
    if path.exists() {
        anyhow::bail!("Directory '{}' already exists", name);
    }

    fs::create_dir_all(&path)?;
    project::create_plugin_project(&path, name)?;

    info!("Plugin created at {}/", name);
    Ok(())
}

fn cmd_new(name: &str, template: &str) -> Result<()> {
    info!("Creating new project: {} (template: {})", name, template);

    let path = PathBuf::from(name);
    if path.exists() {
        anyhow::bail!("Directory '{}' already exists", name);
    }

    fs::create_dir_all(&path)?;
    project::create_project(&path, name, template)?;

    info!("Project created at {}/", name);
    info!("To get started:");
    info!("  cd {}", name);
    info!("  blinc dev");

    Ok(())
}

fn cmd_init(template: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("blinc_app");

    info!(
        "Initializing Blinc project in current directory (template: {})",
        template
    );

    // Check if already initialized
    if cwd.join(".blincproj").exists() {
        anyhow::bail!("This directory already contains a .blincproj");
    }
    if cwd.join("blinc.toml").exists() {
        anyhow::bail!("This directory already contains a blinc.toml (legacy format)");
    }

    project::create_project(&cwd, name, template)?;

    info!("Project initialized!");
    info!("Run `blinc dev` to start development");

    Ok(())
}

fn cmd_check(source: &str) -> Result<()> {
    let path = PathBuf::from(source);
    let config = BlincConfig::load_from_dir(&path)?;

    info!("Checking project: {}", config.project.name);

    // TODO: Parse and validate all .blinc files
    warn!("Check not yet implemented - waiting for Zyntax Grammar2");

    Ok(())
}

fn cmd_info() -> Result<()> {
    println!("Blinc UI Framework");
    println!("==================");
    println!();
    let git_hash = option_env!("BLINC_GIT_HASH").unwrap_or("unknown");
    println!("Version: {} ({})", env!("CARGO_PKG_VERSION"), git_hash);
    println!();
    println!("Supported targets:");
    println!("  - desktop (native window)");
    println!("  - macos");
    println!("  - windows");
    println!("  - linux");
    println!("  - android");
    println!("  - ios");
    println!();
    println!("Build modes:");
    println!("  - JIT (development, hot-reload) - requires Zyntax Runtime2");
    println!("  - AOT (production) - requires Zyntax Grammar2");
    println!();
    println!("Status:");
    println!("  - Core reactive system: Ready");
    println!("  - FSM runtime: Ready");
    println!("  - Animation system: Ready");
    println!("  - Zyntax integration: Pending Grammar2/Runtime2");

    Ok(())
}

fn cmd_doctor() -> Result<()> {
    let categories = doctor::run_doctor();
    doctor::print_doctor_results(&categories);

    // Return error if there are critical issues
    let has_errors = categories
        .iter()
        .any(|c| c.status() == doctor::CheckStatus::Error);

    if has_errors {
        std::process::exit(1);
    }

    Ok(())
}
