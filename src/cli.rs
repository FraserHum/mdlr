use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mdlr")]
#[command(about = "Modularity analyzer for code")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run analysis and display metrics
    Check {
        /// Path to analyze (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Save extraction results to cache (by default, check is read-only)
        #[arg(long)]
        save: bool,
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// List symbols (units) in a file or directory
    Ls {
        /// Path to list symbols from (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Filter by unit kind (function, struct, trait, impl, module)
        #[arg(long)]
        kind: Option<String>,
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// Get the content of a symbol
    Get {
        /// Symbol ID to retrieve
        symbol: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// Manage semantic tags on symbols
    Tag {
        /// Symbol ID to tag (required unless --list is used)
        symbol: Option<String>,
        /// Add tags to the symbol (can be used multiple times)
        #[arg(long)]
        add: Vec<String>,
        /// Remove a tag from the symbol
        #[arg(long)]
        remove: Option<String>,
        /// Clear all tags from the symbol
        #[arg(long)]
        clear: bool,
        /// List all semantic tags in the project
        #[arg(long)]
        list: bool,
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}
