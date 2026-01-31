mod extractor;
pub mod resolve;

pub use extractor::RustExtractor;
pub use resolve::{CargoWorkspace, ResolutionContext};

/// Get all supported file extensions.
pub fn supported_extensions() -> &'static [&'static str] {
    &["rs"]
}
