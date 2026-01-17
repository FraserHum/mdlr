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
    /// Manage analysis sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Manage analysis targets
    Target {
        #[command(subcommand)]
        action: TargetAction,
    },
    /// Run analysis on a session
    Analyze {
        /// Session name
        #[arg(long)]
        session: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// Export the graph from a session
    Export {
        /// Session name
        #[arg(long)]
        session: String,
        /// Output format
        #[arg(long, default_value = "json")]
        format: OutputFormat,
    },
}

#[derive(Subcommand)]
pub enum SessionAction {
    /// Create a new session
    New {
        /// Session name
        name: String,
    },
    /// List all sessions
    List,
    /// Delete a session
    Delete {
        /// Session name
        name: String,
    },
    /// Show session details
    Show {
        /// Session name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum TargetAction {
    /// Add a target to a session
    Add {
        /// Path or object reference to add
        path: String,
        /// Session name
        #[arg(long)]
        session: String,
    },
    /// List targets in a session
    List {
        /// Session name
        #[arg(long)]
        session: String,
    },
    /// Clear all targets from a session
    Clear {
        /// Session name
        #[arg(long)]
        session: String,
    },
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

pub fn parse_target(input: &str) -> crate::session::Target {
    if let Some((file, name)) = input.split_once("::") {
        crate::session::Target::Object {
            file: PathBuf::from(file),
            name: name.to_string(),
        }
    } else {
        let path = PathBuf::from(input);
        if path.is_dir() {
            crate::session::Target::Directory(path)
        } else {
            crate::session::Target::File(path)
        }
    }
}
