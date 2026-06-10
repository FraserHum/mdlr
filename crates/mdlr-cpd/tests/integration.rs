use mdlr_cpd::tokens::{NORMALIZED_ID, NORMALIZED_LIT, Token};
use mdlr_cpd::{
    self, FileTokens, UnitSpan, binary, compute_duplication, find_clones,
};
use std::path::PathBuf;

/// Helper: create FileTokens with tokens on sequential lines.
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

/// Helper: a unit spanning one whole test file (tokens sit one per line).
fn whole_file_unit(file: &FileTokens) -> UnitSpan {
    UnitSpan {
        id: format!("{}::f", file.source_path.display()),
        file: file.source_path.clone(),
        start_line: 1,
        end_line: file.tokens.len() as u32,
    }
}

/// Helper: generate a realistic-looking Rust function token sequence
fn rust_function_tokens(size: usize) -> Vec<&'static str> {
    let pattern = [
        "fn",
        NORMALIZED_ID,
        "(",
        NORMALIZED_ID,
        ":",
        NORMALIZED_ID,
        ")",
        "{",
        "let",
        NORMALIZED_ID,
        "=",
        NORMALIZED_LIT,
        ";",
        "if",
        NORMALIZED_ID,
        ">",
        NORMALIZED_LIT,
        "{",
        "return",
        NORMALIZED_ID,
        "+",
        NORMALIZED_LIT,
        ";",
        "}",
        "}",
    ];
    let mut result = Vec::new();
    while result.len() < size {
        for tok in &pattern {
            result.push(*tok);
            if result.len() >= size {
                break;
            }
        }
    }
    result
}

// === End-to-end: binary serialization + matching + metrics ===

#[test]
fn test_end_to_end_binary_then_match() {
    // Simulate: extract tokens, serialize to binary, deserialize, run matching
    let tokens = rust_function_tokens(80);
    let file_a = make_file("src/a.rs", &tokens);
    let file_b = make_file("src/b.rs", &tokens);

    // Serialize both to binary
    let bytes_a = binary::serialize(&file_a);
    let bytes_b = binary::serialize(&file_b);

    // Deserialize
    let loaded_a = binary::deserialize(&bytes_a).unwrap();
    let loaded_b = binary::deserialize(&bytes_b).unwrap();

    // Find clones
    let clones = find_clones(&[loaded_a, loaded_b], 50);
    assert!(!clones.is_empty(), "should find clones after binary round-trip");
    assert!(clones[0].token_count >= 50);
}

#[test]
fn test_end_to_end_metrics_computation() {
    let tokens = rust_function_tokens(100);
    let file_a = make_file("src/a.rs", &tokens);
    let file_b = make_file("src/b.rs", &tokens);

    let files = vec![file_a, file_b];
    let units: Vec<UnitSpan> = files.iter().map(whole_file_unit).collect();
    let clones = find_clones(&files, 50);
    let metrics = compute_duplication(&clones, &units);

    assert!(metrics.clone_count > 0);
    assert!(metrics.max > 0.0);
    assert!(!metrics.distribution.is_empty());
    // Both units should show high duplication
    for (id, pct) in &metrics.distribution {
        assert!(
            *pct > 50,
            "unit {id} should show >50% duplication, got {pct}"
        );
    }
}

// === Matching edge cases ===

#[test]
fn test_different_structure_no_false_positive() {
    // Two files with completely different token structures
    let file_a = make_file(
        "a.rs",
        &[
            "fn",
            NORMALIZED_ID,
            "(",
            ")",
            "{",
            "for",
            NORMALIZED_ID,
            "in",
            NORMALIZED_ID,
            "{",
            NORMALIZED_ID,
            ".",
            NORMALIZED_ID,
            "(",
            NORMALIZED_ID,
            ")",
            ";",
            "}",
            "}",
            // Repeat to hit min_tokens
            "fn",
            NORMALIZED_ID,
            "(",
            ")",
            "{",
            "for",
            NORMALIZED_ID,
            "in",
            NORMALIZED_ID,
            "{",
            NORMALIZED_ID,
            ".",
            NORMALIZED_ID,
            "(",
            NORMALIZED_ID,
            ")",
            ";",
            "}",
            "}",
            "fn",
            NORMALIZED_ID,
            "(",
            ")",
            "{",
            "for",
            NORMALIZED_ID,
            "in",
            NORMALIZED_ID,
            "{",
            NORMALIZED_ID,
            ".",
            NORMALIZED_ID,
            "(",
            NORMALIZED_ID,
            ")",
            ";",
            "}",
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
            "pub",
            NORMALIZED_ID,
            ":",
            NORMALIZED_ID,
            ",",
            "}",
            "impl",
            NORMALIZED_ID,
            "{",
            "pub",
            "fn",
            NORMALIZED_ID,
            "(",
            "&",
            NORMALIZED_ID,
            ")",
            "->",
            NORMALIZED_ID,
            "{",
            NORMALIZED_ID,
            ".",
            NORMALIZED_ID,
            ".",
            NORMALIZED_ID,
            "(",
            ")",
            "}",
            "}",
            "struct",
            NORMALIZED_ID,
            "{",
            "pub",
            NORMALIZED_ID,
            ":",
            NORMALIZED_ID,
            ",",
            "pub",
            NORMALIZED_ID,
            ":",
            NORMALIZED_ID,
            ",",
            "}",
        ],
    );

    let clones = find_clones(&[file_a, file_b], 20);
    // These files have different structure, so any matches should be small
    for clone in &clones {
        assert!(
            clone.token_count < 25,
            "false positive: found large clone of {} tokens between different-structure files",
            clone.token_count
        );
    }
}

#[test]
fn test_clone_extends_maximally() {
    // Two files that share a 100-token block — the clone should extend to cover all of it
    let shared = rust_function_tokens(100);

    let mut a: Vec<&str> = vec!["use"; 10];
    a.extend_from_slice(&shared);
    a.extend_from_slice(&vec!["mod"; 10]);

    let mut b: Vec<&str> = vec!["pub"; 10];
    b.extend_from_slice(&shared);
    b.extend_from_slice(&vec!["impl"; 10]);

    let file_a = make_file("a.rs", &a);
    let file_b = make_file("b.rs", &b);

    let clones = find_clones(&[file_a, file_b], 50);
    assert!(!clones.is_empty(), "should find at least one clone");
    // The largest clone should span the full shared block
    let max_clone = clones.iter().max_by_key(|c| c.token_count).unwrap();
    assert!(
        max_clone.token_count >= 100,
        "largest clone should extend to full shared block, got {} tokens",
        max_clone.token_count
    );
}

// === Unit attribution ===

#[test]
fn test_attribution_covers_both_clone_sides() {
    let tokens = rust_function_tokens(80);
    let files = vec![
        make_file("src/changed.rs", &tokens),
        make_file("src/unchanged.rs", &tokens),
    ];
    let units: Vec<UnitSpan> = files.iter().map(whole_file_unit).collect();

    let clones = find_clones(&files, 50);
    assert!(!clones.is_empty());

    let metrics = compute_duplication(&clones, &units);
    // Both sides of the clone have a unit, so both get a row.
    assert_eq!(metrics.distribution.len(), 2);
}

#[test]
fn test_clones_outside_any_unit_drop() {
    let tokens = rust_function_tokens(80);
    let files =
        vec![make_file("src/a.rs", &tokens), make_file("src/b.rs", &tokens)];

    let clones = find_clones(&files, 50);
    assert!(!clones.is_empty());

    // Only a unit in an unrelated file — clone lines attribute to nothing.
    let units = vec![UnitSpan {
        id: "src/unrelated.rs::f".to_string(),
        file: PathBuf::from("src/unrelated.rs"),
        start_line: 1,
        end_line: 100,
    }];
    let metrics = compute_duplication(&clones, &units);
    assert!(metrics.distribution.is_empty());
}

// === Binary format edge cases ===

#[test]
fn test_binary_large_file() {
    // File with many tokens — stress test the binary format
    let ft = FileTokens {
        source_path: PathBuf::from("large.rs"),
        tokens: (0..10000)
            .map(|i| Token {
                value: if i % 3 == 0 {
                    NORMALIZED_ID.to_string()
                } else {
                    ";".to_string()
                },
                line: (i / 10) as u32 + 1,
                col: (i % 10) as u16,
            })
            .collect(),
        cached_at: 99999,
    };

    let bytes = binary::serialize(&ft);
    let restored = binary::deserialize(&bytes).unwrap();
    assert_eq!(restored.tokens.len(), 10000);
    assert_eq!(restored.source_path, ft.source_path);
    assert_eq!(restored.cached_at, ft.cached_at);
}

#[test]
fn test_binary_unicode_path() {
    let ft = FileTokens {
        source_path: PathBuf::from("src/日本語/テスト.rs"),
        tokens: vec![Token { value: "fn".to_string(), line: 1, col: 0 }],
        cached_at: 1,
    };

    let bytes = binary::serialize(&ft);
    let restored = binary::deserialize(&bytes).unwrap();
    assert_eq!(restored.source_path, ft.source_path);
}

// === Metrics edge cases ===

#[test]
fn test_metrics_p90_both_units_duplicated() {
    let tokens = rust_function_tokens(80);
    let files = vec![make_file("a.rs", &tokens), make_file("b.rs", &tokens)];
    let units: Vec<UnitSpan> = files.iter().map(whole_file_unit).collect();
    let clones = find_clones(&files, 50);
    let metrics = compute_duplication(&clones, &units);

    // With 2 units both duplicated, p90 should be meaningful
    assert!(metrics.p90 > 0.0);
    assert!(metrics.p90 <= 100.0);
}

#[test]
fn test_metrics_units_without_duplication_not_in_distribution() {
    let tokens = rust_function_tokens(80);
    let unique: Vec<&str> = vec!["struct"; 80];

    let files = vec![
        make_file("a.rs", &tokens),
        make_file("b.rs", &tokens),
        make_file("c.rs", &unique), // no duplication
    ];
    let units: Vec<UnitSpan> = files.iter().map(whole_file_unit).collect();

    let clones = find_clones(&files, 50);
    let metrics = compute_duplication(&clones, &units);

    // distribution should only contain units with duplication
    let dist_units: Vec<&str> =
        metrics.distribution.iter().map(|(id, _)| id.as_str()).collect();
    assert!(
        !dist_units.contains(&"c.rs::f"),
        "unit without duplication should not be in distribution"
    );
}

// === Self-clones (within same file) ===

#[test]
fn test_self_clone_metrics() {
    // File with a duplicated block within itself
    let block = rust_function_tokens(30);
    let mut tokens: Vec<&str> = block.clone();
    tokens.extend_from_slice(&["---", "===", "***", "~~~", "!!!"]);
    tokens.extend_from_slice(&block);

    let files = vec![make_file("a.rs", &tokens)];
    let units: Vec<UnitSpan> = files.iter().map(whole_file_unit).collect();
    let clones = find_clones(&files, 20);

    if !clones.is_empty() {
        let metrics = compute_duplication(&clones, &units);
        // The duplicated lines should be counted once (deduplicated)
        let (_, pct) = &metrics.distribution[0];
        assert!(*pct <= 100, "dedup percentage should be <= 100");
    }
}
