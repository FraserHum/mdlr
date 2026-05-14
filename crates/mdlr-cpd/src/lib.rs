pub mod matching;
pub mod metrics;
pub mod tokens;

pub use matching::{ClonePair, find_clones, find_clones_with_progress};
pub use metrics::{DuplicationMetrics, FileDuplication, compute_duplication};
pub use tokens::{FileTokens, NORMALIZED_ID, NORMALIZED_LIT, Token, binary};
