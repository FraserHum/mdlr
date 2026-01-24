use crate::graph::Unit;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Metadata for a single source file used for staleness detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileMetadata {
    pub mtime: u64,
    pub size: u64,
}

/// Cached extraction data for a single source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheEntry {
    pub source_path: PathBuf,
    pub mtime: u64,
    pub size: u64,
    pub units: Vec<Unit>,
    pub cached_at: u64,
}

/// Project-wide index tracking all known files and their metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub version: u32,
    pub files: HashMap<PathBuf, FileMetadata>,
    pub last_scan: u64,
}

impl Default for ProjectIndex {
    fn default() -> Self {
        Self {
            version: 1,
            files: HashMap::new(),
            last_scan: 0,
        }
    }
}

/// User-defined semantic tags stored separately from extracted units.
/// This allows tags to persist across re-extraction when source files change.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticTags {
    pub version: u32,
    pub tags: HashMap<String, Vec<String>>, // unit_id -> tags
}

impl SemanticTags {
    pub fn new() -> Self {
        Self {
            version: 1,
            tags: HashMap::new(),
        }
    }

    /// Add a tag to a unit. Validates the tag format (namespace:value).
    pub fn add_tag(&mut self, unit_id: &str, tag: &str) -> Result<()> {
        validate_tag(tag)?;
        let entry = self.tags.entry(unit_id.to_string()).or_default();
        if !entry.contains(&tag.to_string()) {
            entry.push(tag.to_string());
        }
        Ok(())
    }

    /// Remove a tag from a unit.
    pub fn remove_tag(&mut self, unit_id: &str, tag: &str) -> bool {
        if let Some(entry) = self.tags.get_mut(unit_id) {
            if let Some(pos) = entry.iter().position(|t| t == tag) {
                entry.remove(pos);
                if entry.is_empty() {
                    self.tags.remove(unit_id);
                }
                return true;
            }
        }
        false
    }

    /// Clear all tags from a unit.
    pub fn clear_tags(&mut self, unit_id: &str) -> bool {
        self.tags.remove(unit_id).is_some()
    }

    /// Get tags for a unit.
    pub fn get_tags(&self, unit_id: &str) -> &[String] {
        self.tags.get(unit_id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Validate that a tag has the format namespace:value.
pub fn validate_tag(tag: &str) -> Result<()> {
    let parts: Vec<&str> = tag.split(':').collect();
    if parts.len() != 2 {
        bail!("Invalid tag format '{}': must be 'namespace:value' (exactly one colon)", tag);
    }
    if parts[0].is_empty() || parts[1].is_empty() {
        bail!("Invalid tag format '{}': namespace and value must not be empty", tag);
    }
    Ok(())
}
