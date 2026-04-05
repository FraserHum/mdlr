use std::collections::HashMap;
use std::path::PathBuf;

use crate::tokens::FileTokens;

/// A pair of duplicate code locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClonePair {
    pub file_a: PathBuf,
    pub start_line_a: u32,
    pub end_line_a: u32,
    pub file_b: PathBuf,
    pub start_line_b: u32,
    pub end_line_b: u32,
    /// Number of tokens in the duplicated block.
    pub token_count: usize,
}

/// Rabin-Karp rolling hash parameters.
const HASH_BASE: u64 = 31;
const HASH_MOD: u64 = 1_000_000_007;

/// A single token entry in the flattened token stream, carrying its file index.
#[derive(Clone)]
struct FlatToken {
    /// Index of the normalized token value in the global string-to-id map.
    value_id: u32,
    line: u32,
    file_idx: usize,
}

/// Find all duplicate code blocks across a set of files using Rabin-Karp rolling hash.
///
/// `min_tokens` is the minimum number of tokens for a window to be considered.
/// Returns a list of clone pairs with their locations.
pub fn find_clones(files: &[FileTokens], min_tokens: usize) -> Vec<ClonePair> {
    if min_tokens == 0 || files.is_empty() {
        return Vec::new();
    }

    // Build a global string-to-id mapping for fast hashing
    let mut value_ids: HashMap<&str, u32> = HashMap::new();
    let mut next_id: u32 = 0;
    for file in files {
        for token in &file.tokens {
            if !value_ids.contains_key(token.value.as_str()) {
                value_ids.insert(&token.value, next_id);
                next_id += 1;
            }
        }
    }

    // Flatten all tokens into a single stream with file boundaries
    let mut flat: Vec<FlatToken> = Vec::new();
    let mut file_boundaries: Vec<usize> = Vec::new(); // start index of each file

    for (file_idx, file) in files.iter().enumerate() {
        file_boundaries.push(flat.len());
        for token in &file.tokens {
            flat.push(FlatToken {
                value_id: value_ids[token.value.as_str()],
                line: token.line,
                file_idx,
            });
        }
    }
    // Sentinel for bounds checking
    file_boundaries.push(flat.len());

    if flat.len() < min_tokens {
        return Vec::new();
    }

    // Precompute HASH_BASE^(min_tokens-1) mod HASH_MOD for rolling removal
    let base_power = mod_pow(HASH_BASE, (min_tokens - 1) as u64, HASH_MOD);

    // Hash-to-positions map: hash -> list of starting indices in `flat`
    let mut hash_map: HashMap<u64, Vec<usize>> = HashMap::new();

    // Compute rolling hashes per file segment (don't hash across file boundaries)
    for (file_idx, file) in files.iter().enumerate() {
        let start = file_boundaries[file_idx];
        let end = file_boundaries[file_idx + 1];
        let n = end - start;

        if n < min_tokens {
            continue;
        }

        // Compute initial hash for first window
        let mut hash: u64 = 0;
        for i in 0..min_tokens {
            hash = (hash.wrapping_mul(HASH_BASE)
                + flat[start + i].value_id as u64)
                % HASH_MOD;
        }
        hash_map.entry(hash).or_default().push(start);

        // Roll the hash
        for i in 1..=(n - min_tokens) {
            let out_val = flat[start + i - 1].value_id as u64;
            let in_val = flat[start + i + min_tokens - 1].value_id as u64;

            hash = (hash + HASH_MOD
                - (out_val.wrapping_mul(base_power)) % HASH_MOD)
                % HASH_MOD;
            hash = (hash.wrapping_mul(HASH_BASE) + in_val) % HASH_MOD;
            hash_map.entry(hash).or_default().push(start + i);
        }

        let _ = file; // suppress unused warning
    }

    // For each hash bucket, verify matches and extend them maximally
    let mut clone_pairs: Vec<ClonePair> = Vec::new();
    let mut seen: HashMap<(usize, usize), bool> = HashMap::new(); // (pos_a, pos_b) -> already reported

    for (_hash, positions) in &hash_map {
        if positions.len() < 2 {
            continue;
        }

        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let pos_a = positions[i];
                let pos_b = positions[j];

                // Skip if we've already reported a clone that subsumes this pair
                let key = (pos_a.min(pos_b), pos_a.max(pos_b));
                if seen.contains_key(&key) {
                    continue;
                }

                // Verify the tokens actually match (avoid hash collisions)
                if !tokens_match(&flat, pos_a, pos_b, min_tokens) {
                    continue;
                }

                // Skip if both positions are in the same file and overlap
                if flat[pos_a].file_idx == flat[pos_b].file_idx {
                    let (lo, hi) = if pos_a < pos_b {
                        (pos_a, pos_b)
                    } else {
                        (pos_b, pos_a)
                    };
                    if lo + min_tokens > hi {
                        continue; // overlapping within same file
                    }
                }

                // Extend the match as far as possible
                let match_len = extend_match(
                    &flat,
                    pos_a,
                    pos_b,
                    min_tokens,
                    &file_boundaries,
                );

                seen.insert(key, true);

                let file_a_idx = flat[pos_a].file_idx;
                let file_b_idx = flat[pos_b].file_idx;

                clone_pairs.push(ClonePair {
                    file_a: files[file_a_idx].source_path.clone(),
                    start_line_a: flat[pos_a].line,
                    end_line_a: flat[pos_a + match_len - 1].line,
                    file_b: files[file_b_idx].source_path.clone(),
                    start_line_b: flat[pos_b].line,
                    end_line_b: flat[pos_b + match_len - 1].line,
                    token_count: match_len,
                });
            }
        }
    }

    // Deduplicate: remove clone pairs that are fully subsumed by larger ones
    deduplicate_clones(&mut clone_pairs);

    clone_pairs
}

/// Verify that tokens at two positions actually match for `len` tokens.
fn tokens_match(flat: &[FlatToken], a: usize, b: usize, len: usize) -> bool {
    for i in 0..len {
        if flat[a + i].value_id != flat[b + i].value_id {
            return false;
        }
    }
    true
}

/// Extend a verified match beyond min_tokens as far as tokens continue to match,
/// without crossing file boundaries.
fn extend_match(
    flat: &[FlatToken],
    pos_a: usize,
    pos_b: usize,
    min_tokens: usize,
    file_boundaries: &[usize],
) -> usize {
    let file_a = flat[pos_a].file_idx;
    let file_b = flat[pos_b].file_idx;
    let end_a = file_boundaries[file_a + 1];
    let end_b = file_boundaries[file_b + 1];

    let mut len = min_tokens;
    loop {
        let next_a = pos_a + len;
        let next_b = pos_b + len;

        if next_a >= end_a || next_b >= end_b {
            break;
        }

        // For same-file clones, stop if they would overlap
        if file_a == file_b {
            let (lo, hi) =
                if pos_a < pos_b { (pos_a, pos_b) } else { (pos_b, pos_a) };
            if lo + len >= hi {
                break;
            }
        }

        if flat[next_a].value_id != flat[next_b].value_id {
            break;
        }

        len += 1;
    }

    len
}

/// Remove clone pairs that are fully subsumed by a larger clone pair.
fn deduplicate_clones(clones: &mut Vec<ClonePair>) {
    if clones.len() <= 1 {
        return;
    }

    // Sort by token_count descending so larger clones come first
    clones.sort_by(|a, b| b.token_count.cmp(&a.token_count));

    let mut keep = vec![true; clones.len()];

    for i in 0..clones.len() {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..clones.len() {
            if !keep[j] {
                continue;
            }
            if is_subsumed(&clones[j], &clones[i]) {
                keep[j] = false;
            }
        }
    }

    let mut idx = 0;
    clones.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

/// Check if `small` clone is fully contained within `large` clone.
fn is_subsumed(small: &ClonePair, large: &ClonePair) -> bool {
    // Check if small's A side is within large's A side and small's B side is within large's B side
    let a_in_a = small.file_a == large.file_a
        && small.start_line_a >= large.start_line_a
        && small.end_line_a <= large.end_line_a;
    let b_in_b = small.file_b == large.file_b
        && small.start_line_b >= large.start_line_b
        && small.end_line_b <= large.end_line_b;

    if a_in_a && b_in_b {
        return true;
    }

    // Check the flipped case (small's A in large's B and vice versa)
    let a_in_b = small.file_a == large.file_b
        && small.start_line_a >= large.start_line_b
        && small.end_line_a <= large.end_line_b;
    let b_in_a = small.file_b == large.file_a
        && small.start_line_b >= large.start_line_a
        && small.end_line_b <= large.end_line_a;

    a_in_b && b_in_a
}

/// Modular exponentiation: base^exp mod modulus
fn mod_pow(mut base: u64, mut exp: u64, modulus: u64) -> u64 {
    let mut result = 1u64;
    base %= modulus;
    while exp > 0 {
        if exp % 2 == 1 {
            result = result.wrapping_mul(base) % modulus;
        }
        exp >>= 1;
        base = base.wrapping_mul(base) % modulus;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{NORMALIZED_ID, NORMALIZED_LIT, Token};

    /// Helper to create a FileTokens with tokens on sequential lines.
    fn make_file(path: &str, token_values: &[&str]) -> FileTokens {
        FileTokens {
            source_path: PathBuf::from(path),
            tokens: token_values
                .iter()
                .enumerate()
                .map(|(i, v)| Token {
                    value: v.to_string(),
                    line: (i + 1) as u32,
                    col: 0,
                })
                .collect(),
            cached_at: 1,
        }
    }

    #[test]
    fn test_exact_duplicate_two_files() {
        // Two files with identical token sequences
        let tokens: Vec<&str> = (0..60)
            .map(|i| {
                if i % 3 == 0 {
                    "fn"
                } else if i % 3 == 1 {
                    NORMALIZED_ID
                } else {
                    ";"
                }
            })
            .collect();

        let file_a = make_file("a.rs", &tokens);
        let file_b = make_file("b.rs", &tokens);

        let clones = find_clones(&[file_a, file_b], 50);
        assert!(!clones.is_empty(), "should find at least one clone");
        assert_eq!(clones[0].file_a, PathBuf::from("a.rs"));
        assert_eq!(clones[0].file_b, PathBuf::from("b.rs"));
        assert!(clones[0].token_count >= 50);
    }

    #[test]
    fn test_no_duplicate() {
        let file_a = make_file(
            "a.rs",
            &[
                "fn",
                NORMALIZED_ID,
                "(",
                ")",
                "{",
                "let",
                NORMALIZED_ID,
                "=",
                NORMALIZED_LIT,
                ";",
                "}",
            ],
        );
        let file_b = make_file(
            "b.rs",
            &[
                "struct",
                NORMALIZED_ID,
                "{",
                "pub",
                NORMALIZED_ID,
                ":",
                NORMALIZED_ID,
                ",",
                "}",
            ],
        );

        let clones = find_clones(&[file_a, file_b], 5);
        // These are very different — should have no clones at min_tokens=5
        assert!(
            clones.is_empty(),
            "should find no clones between different code"
        );
    }

    #[test]
    fn test_min_tokens_boundary() {
        // Exactly min_tokens matching tokens should be found
        let shared: Vec<&str> = vec!["fn"; 10];
        let file_a = make_file("a.rs", &shared);
        let file_b = make_file("b.rs", &shared);

        let clones_at_10 = find_clones(&[file_a.clone(), file_b.clone()], 10);
        assert!(!clones_at_10.is_empty(), "exact min_tokens should match");

        let clones_at_11 = find_clones(&[file_a, file_b], 11);
        assert!(clones_at_11.is_empty(), "above min_tokens should not match");
    }

    #[test]
    fn test_normalized_duplicates() {
        // Two files with same structure but different original identifiers
        // After normalization they both use $ID and $LIT
        let tokens: Vec<&str> = (0..60)
            .map(|i| match i % 5 {
                0 => "fn",
                1 => NORMALIZED_ID,
                2 => "(",
                3 => NORMALIZED_LIT,
                4 => ")",
                _ => unreachable!(),
            })
            .collect();

        let file_a = make_file("a.rs", &tokens);
        let file_b = make_file("b.rs", &tokens);

        let clones = find_clones(&[file_a, file_b], 50);
        assert!(!clones.is_empty(), "normalized tokens should match");
    }

    #[test]
    fn test_self_clone_same_file() {
        // A file with the same block duplicated within itself
        let block: Vec<&str> = (0..30)
            .map(|i| if i % 2 == 0 { "let" } else { NORMALIZED_ID })
            .collect();

        let mut tokens = block.clone();
        // Add separator
        tokens.extend_from_slice(&["---", "===", "***"]);
        tokens.extend_from_slice(&block);

        let file = make_file("a.rs", &tokens);
        let clones = find_clones(&[file], 20);
        assert!(!clones.is_empty(), "should detect self-clone within a file");
        assert_eq!(clones[0].file_a, clones[0].file_b);
    }

    #[test]
    fn test_empty_input() {
        assert!(find_clones(&[], 50).is_empty());

        let empty = make_file("empty.rs", &[]);
        assert!(find_clones(&[empty], 50).is_empty());
    }

    #[test]
    fn test_file_below_min_tokens() {
        let small = make_file("small.rs", &["fn", NORMALIZED_ID, "(", ")"]);
        let clones = find_clones(&[small], 50);
        assert!(clones.is_empty());
    }

    #[test]
    fn test_partial_overlap_different_files() {
        // Files share a 60-token block but have different prefixes/suffixes
        let shared: Vec<&str> = (0..60)
            .map(|i| {
                if i % 3 == 0 {
                    "fn"
                } else if i % 3 == 1 {
                    NORMALIZED_ID
                } else {
                    ";"
                }
            })
            .collect();

        let mut tokens_a: Vec<&str> = vec!["use"; 10];
        tokens_a.extend_from_slice(&shared);
        tokens_a.extend_from_slice(&vec!["mod"; 10]);

        let mut tokens_b: Vec<&str> = vec!["pub"; 5];
        tokens_b.extend_from_slice(&shared);
        tokens_b.extend_from_slice(&vec!["impl"; 15]);

        let file_a = make_file("a.rs", &tokens_a);
        let file_b = make_file("b.rs", &tokens_b);

        let clones = find_clones(&[file_a, file_b], 50);
        assert!(!clones.is_empty());
        // The clone should span the shared 60-token block
        assert!(clones[0].token_count >= 50);
    }

    #[test]
    fn test_multiple_clone_pairs() {
        // Three files with the same block
        let tokens: Vec<&str> = vec!["fn"; 60];

        let file_a = make_file("a.rs", &tokens);
        let file_b = make_file("b.rs", &tokens);
        let file_c = make_file("c.rs", &tokens);

        let clones = find_clones(&[file_a, file_b, file_c], 50);
        // Should find clones between a-b, a-c, b-c
        assert!(
            clones.len() >= 3,
            "should find clone pairs between all 3 files, got {}",
            clones.len()
        );
    }

    #[test]
    fn test_mod_pow() {
        assert_eq!(mod_pow(2, 10, 1000), 24);
        assert_eq!(mod_pow(31, 0, 100), 1);
        assert_eq!(mod_pow(31, 1, 100), 31);
    }

    #[test]
    fn test_deduplication_removes_subsumed() {
        let mut clones = vec![
            ClonePair {
                file_a: PathBuf::from("a.rs"),
                start_line_a: 1,
                end_line_a: 20,
                file_b: PathBuf::from("b.rs"),
                start_line_b: 1,
                end_line_b: 20,
                token_count: 60,
            },
            ClonePair {
                file_a: PathBuf::from("a.rs"),
                start_line_a: 5,
                end_line_a: 15,
                file_b: PathBuf::from("b.rs"),
                start_line_b: 5,
                end_line_b: 15,
                token_count: 30,
            },
        ];

        deduplicate_clones(&mut clones);
        assert_eq!(clones.len(), 1, "smaller clone should be removed");
        assert_eq!(clones[0].token_count, 60);
    }
}
