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

        /// Target platforms (e.g., linux/amd64, linux/arm64)
        /// Can be specified multiple times or as a comma-separated list
        #[arg(long, value_delimiter = ',')]
        platform: Option<Vec<String>>,

        /// Skip pushing the image to the registry after building
        #[arg(long)]
        no_push: bool,

        /// Tag to apply to the image (e.g., latest, v1.0.0)
        /// If not specified, only pushes by digest
        #[arg(long)]
        tag: Option<String>,

        /// Repository prefix (e.g., ghcr.io/username)
        #[arg(env = "KRUST_REPO")]
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

    /// Resolve krust:// references in YAML files
    Resolve {
        /// Path to YAML file or directory containing YAML files
        #[arg(short = 'f', long = "filename", required = true)]
        filenames: Vec<PathBuf>,

        /// Target platforms (e.g., linux/amd64, linux/arm64)
        #[arg(long, value_delimiter = ',')]
        platform: Option<Vec<String>>,

        /// Repository prefix (e.g., ghcr.io/username)
        #[arg(env = "KRUST_REPO")]
        repo: Option<String>,

        /// Tag to apply to the images (e.g., latest, v1.0.0)
        #[arg(long)]
        tag: Option<String>,
    },

    /// Build images and apply resolved YAML with kubectl
    Apply {
        /// Path to YAML file or directory containing YAML files
        #[arg(short = 'f', long = "filename", required = true)]
        filenames: Vec<PathBuf>,

        /// Target platforms (e.g., linux/amd64, linux/arm64)
        #[arg(long, value_delimiter = ',')]
        platform: Option<Vec<String>>,

        /// Repository prefix (e.g., ghcr.io/username)
        #[arg(env = "KRUST_REPO")]
        repo: Option<String>,

        /// Tag to apply to the images (e.g., latest, v1.0.0)
        #[arg(long)]
        tag: Option<String>,
    },

    /// Show version information
    Version,
}
