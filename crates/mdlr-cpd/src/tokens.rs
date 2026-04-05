use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single normalized token with source location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Normalized value: keywords/operators kept as-is, identifiers become "$ID",
    /// literals become "$LIT".
    pub value: String,
    /// 1-based line number in the source file.
    pub line: u32,
    /// 0-based column offset in the source file.
    pub col: u16,
}

/// All tokens extracted from a single source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTokens {
    /// Relative path to the source file (same as FileCacheEntry.source_path).
    pub source_path: PathBuf,
    /// Normalized, filtered token stream (no comments, no whitespace).
    pub tokens: Vec<Token>,
    /// Generation ID for staleness detection.
    pub cached_at: u64,
}

/// Sentinel values used during normalization.
pub const NORMALIZED_ID: &str = "$ID";
pub const NORMALIZED_LIT: &str = "$LIT";

/// Compact binary format for token streams.
///
/// Layout:
///   Header:
///     string_count: u32
///     for each string: len: u16, bytes: [u8; len]
///   Body:
///     token_count: u32
///     for each token: string_index: u16, line: u32, col: u16
pub mod binary {
    use super::{FileTokens, Token};
    use anyhow::{Result, bail};
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Serialize a FileTokens to compact binary format.
    pub fn serialize(file_tokens: &FileTokens) -> Vec<u8> {
        // Build string table
        let mut string_table: Vec<String> = Vec::new();
        let mut string_index: HashMap<&str, u16> = HashMap::new();

        for token in &file_tokens.tokens {
            if !string_index.contains_key(token.value.as_str()) {
                let idx = string_table.len() as u16;
                string_index.insert(&token.value, idx);
                string_table.push(token.value.clone());
            }
        }

        let mut buf = Vec::new();

        // Write source_path as length-prefixed UTF-8
        let path_bytes = file_tokens.source_path.to_string_lossy();
        let path_bytes = path_bytes.as_bytes();
        buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(path_bytes);

        // Write cached_at
        buf.extend_from_slice(&file_tokens.cached_at.to_le_bytes());

        // Write string table
        buf.extend_from_slice(&(string_table.len() as u32).to_le_bytes());
        for s in &string_table {
            let bytes = s.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(bytes);
        }

        // Write tokens
        buf.extend_from_slice(
            &(file_tokens.tokens.len() as u32).to_le_bytes(),
        );
        for token in &file_tokens.tokens {
            let idx = string_index[token.value.as_str()];
            buf.extend_from_slice(&idx.to_le_bytes());
            buf.extend_from_slice(&token.line.to_le_bytes());
            buf.extend_from_slice(&token.col.to_le_bytes());
        }

        buf
    }

    /// Deserialize a FileTokens from compact binary format.
    pub fn deserialize(data: &[u8]) -> Result<FileTokens> {
        let mut pos = 0;

        // Read source_path
        if data.len() < pos + 4 {
            bail!("truncated: missing path length");
        }
        let path_len =
            u32::from_le_bytes(data[pos..pos + 4].try_into()?) as usize;
        pos += 4;
        if data.len() < pos + path_len {
            bail!("truncated: missing path data");
        }
        let path_str = std::str::from_utf8(&data[pos..pos + path_len])?;
        let source_path = PathBuf::from(path_str);
        pos += path_len;

        // Read cached_at
        if data.len() < pos + 8 {
            bail!("truncated: missing cached_at");
        }
        let cached_at = u64::from_le_bytes(data[pos..pos + 8].try_into()?);
        pos += 8;

        // Read string table
        if data.len() < pos + 4 {
            bail!("truncated: missing string count");
        }
        let string_count =
            u32::from_le_bytes(data[pos..pos + 4].try_into()?) as usize;
        pos += 4;

        let mut string_table = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            if data.len() < pos + 2 {
                bail!("truncated: missing string length");
            }
            let slen =
                u16::from_le_bytes(data[pos..pos + 2].try_into()?) as usize;
            pos += 2;
            if data.len() < pos + slen {
                bail!("truncated: missing string data");
            }
            let s = std::str::from_utf8(&data[pos..pos + slen])?;
            string_table.push(s.to_string());
            pos += slen;
        }

        // Read tokens
        if data.len() < pos + 4 {
            bail!("truncated: missing token count");
        }
        let token_count =
            u32::from_le_bytes(data[pos..pos + 4].try_into()?) as usize;
        pos += 4;

        let mut tokens = Vec::with_capacity(token_count);
        for _ in 0..token_count {
            if data.len() < pos + 8 {
                bail!("truncated: missing token data");
            }
            let str_idx =
                u16::from_le_bytes(data[pos..pos + 2].try_into()?) as usize;
            pos += 2;
            let line = u32::from_le_bytes(data[pos..pos + 4].try_into()?);
            pos += 4;
            let col = u16::from_le_bytes(data[pos..pos + 2].try_into()?);
            pos += 2;

            if str_idx >= string_table.len() {
                bail!(
                    "invalid string index {} (table has {})",
                    str_idx,
                    string_table.len()
                );
            }

            tokens.push(Token {
                value: string_table[str_idx].clone(),
                line,
                col,
            });
        }

        Ok(FileTokens { source_path, tokens, cached_at })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_file_tokens() -> FileTokens {
        FileTokens {
            source_path: PathBuf::from("src/main.rs"),
            tokens: vec![
                Token { value: "fn".to_string(), line: 1, col: 0 },
                Token { value: NORMALIZED_ID.to_string(), line: 1, col: 3 },
                Token { value: "(".to_string(), line: 1, col: 7 },
                Token { value: ")".to_string(), line: 1, col: 8 },
                Token { value: "{".to_string(), line: 1, col: 10 },
                Token { value: "let".to_string(), line: 2, col: 4 },
                Token { value: NORMALIZED_ID.to_string(), line: 2, col: 8 },
                Token { value: "=".to_string(), line: 2, col: 10 },
                Token { value: NORMALIZED_LIT.to_string(), line: 2, col: 12 },
                Token { value: ";".to_string(), line: 2, col: 14 },
                Token { value: "}".to_string(), line: 3, col: 0 },
            ],
            cached_at: 1000,
        }
    }

    #[test]
    fn test_binary_round_trip() {
        let original = sample_file_tokens();
        let bytes = binary::serialize(&original);
        let restored = binary::deserialize(&bytes).unwrap();

        assert_eq!(restored.source_path, original.source_path);
        assert_eq!(restored.cached_at, original.cached_at);
        assert_eq!(restored.tokens.len(), original.tokens.len());
        for (a, b) in restored.tokens.iter().zip(original.tokens.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn test_binary_empty_tokens() {
        let ft = FileTokens {
            source_path: PathBuf::from("empty.rs"),
            tokens: vec![],
            cached_at: 42,
        };
        let bytes = binary::serialize(&ft);
        let restored = binary::deserialize(&bytes).unwrap();
        assert_eq!(restored.tokens.len(), 0);
        assert_eq!(restored.source_path, ft.source_path);
    }

    #[test]
    fn test_binary_string_table_dedup() {
        // Many tokens with same value should share string table entry
        let ft = FileTokens {
            source_path: PathBuf::from("test.rs"),
            tokens: (0..100)
                .map(|i| Token {
                    value: NORMALIZED_ID.to_string(),
                    line: i,
                    col: 0,
                })
                .collect(),
            cached_at: 1,
        };
        let bytes = binary::serialize(&ft);
        // String table should have exactly 1 entry
        // Verify by round-tripping
        let restored = binary::deserialize(&bytes).unwrap();
        assert_eq!(restored.tokens.len(), 100);
        // Binary size should be much smaller than 100 * full string
        assert!(bytes.len() < 1200); // ~8 bytes per token + overhead
    }

    #[test]
    fn test_binary_truncated_data() {
        let ft = sample_file_tokens();
        let bytes = binary::serialize(&ft);

        // Truncate at various points
        assert!(binary::deserialize(&bytes[..2]).is_err());
        assert!(binary::deserialize(&bytes[..10]).is_err());
        assert!(binary::deserialize(&[]).is_err());
    }
}
