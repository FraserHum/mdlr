use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};

mod cache;
mod check;
mod cli;
mod config;
mod extraction;
mod ignore_commands;
mod json_output;
mod metrics_commands;
mod metrics_rows;
mod symbol_commands;
mod timing;
mod walk;

use cli::{Cli, Command};
use symbol_commands::{handle_get, handle_ls};

/// Walk up from `start_dir` and find the highest directory with both `.mdlr` and `.git`.
/// Falls back to `start_dir` if none found.
pub fn find_project_root(start_dir: &Path) -> PathBuf {
    let start =
        start_dir.canonicalize().unwrap_or_else(|_| start_dir.to_path_buf());
    let mut current = start.as_path();
    let mut highest: Option<&Path> = None;

    loop {
        if current.join(".mdlr").exists() && current.join(".git").exists() {
            highest = Some(current);
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    highest.map(|p| p.to_path_buf()).unwrap_or(start)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { target, k, pretty, format, timing } => {
            check::handle_check(target.as_deref(), k, pretty, format, timing)
        }
        Command::Metrics { command } => {
            metrics_commands::handle_metrics(command)
        }
        Command::Prompt => handle_prompt(),
        Command::Ls { path, kind, format } => handle_ls(&path, kind, format),
        Command::Get { symbol, format } => handle_get(&symbol, format),
        Command::Ignore { metric, symbol, remove, list } => {
            ignore_commands::handle_ignore(metric, symbol, remove, list)
        }
    }
}

fn handle_prompt() -> Result<()> {
    print!("{}", include_str!("prompt.md"));
    Ok(())
}
