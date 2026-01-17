pub mod rust;
pub mod types;

pub use rust::RustExtractor;
pub use types::Extractor;

use std::path::Path;

pub fn extractor_for_path(path: &Path) -> Option<Box<dyn Extractor>> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(Box::new(RustExtractor::new().ok()?)),
        _ => None,
    }
}

pub fn supported_extensions() -> &'static [&'static str] {
    &["rs"]
}
