use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "krust")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Build a container image from a Rust application
    Build {
        /// Path to the Rust project directory
        #[arg(value_name = "DIRECTORY")]
        path: Option<PathBuf>,

        /// Target image reference (overrides KRUST_REPO)
        #[arg(short, long, env = "KRUST_IMAGE")]
        image: Option<String>,

        /// Target platforms (e.g., linux/amd64, linux/arm64)
        /// Can be specified multiple times or as a comma-separated list
        #[arg(long, value_delimiter = ',')]
        platform: Option<Vec<String>>,

        /// Skip pushing the image to the registry after building
        #[arg(long)]
        no_push: bool,

        /// Tag to apply to the manifest list (e.g., latest, v1.0.0)
        #[arg(long)]
        tag: Option<String>,

        /// Repository prefix (e.g., ghcr.io/username)
        #[arg(long, env = "KRUST_REPO")]
        repo: Option<String>,

        /// Additional cargo build arguments
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },

    /// Push a built image to a container registry
    Push {
        /// Image reference to push
        image: String,
    },

    /// Show version information
    Version,
}
