use crate::graph::Unit;
use anyhow::Result;
use std::path::Path;

pub trait Extractor: Send + Sync {
    fn language(&self) -> &'static str;
    fn extract(&self, source: &str, path: &Path) -> Result<Vec<Unit>>;
}
