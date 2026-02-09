# HIR Extractor

`mdlr-extract-rust` is a `RUSTC_WRAPPER` binary that uses the Rust compiler's HIR (High-level Intermediate Representation) to extract code units with fully-resolved type information.

## Requirements

- Nightly Rust toolchain with `rustc-dev` and `llvm-tools` components
- The crate's `rust-toolchain.toml` handles this automatically when building from its directory

## Usage

The binary is used as a `RUSTC_WRAPPER`. The orchestrating CLI sets environment variables and runs `cargo +nightly check`:

```bash
RUSTC_WRAPPER=path/to/mdlr-extract-rust \
MDLR_HIR_MAPPING=mapping.json \
MDLR_HIR_CRATE=mdlr-core \
  cargo +nightly check -p mdlr-core
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `MDLR_HIR_MAPPING` | Path to a JSON file mapping source file paths to output destinations |
| `MDLR_HIR_CRATE` | Cargo package name of the crate to extract from |

### Mapping File Format

```json
{
  "crates/mdlr-core/src/graph/types.rs": ".mdlr/cache/crates/mdlr-core/src/graph/types.json",
  "crates/mdlr-core/src/graph/builder.rs": ".mdlr/cache/crates/mdlr-core/src/graph/builder.json"
}
```

For non-target crates the wrapper passes through to real `rustc`. For the target crate it runs `rustc` with callbacks that extract HIR after type checking, writes output files, and stops before codegen.

## Output Format

Each output file contains a `FileCacheEntry`-compatible JSON object:

```json
{
  "source_path": "crates/mdlr-core/src/graph/types.rs",
  "units": [
    {
      "id": "graph::types::Span",
      "kind": "Struct",
      "file": "crates/mdlr-core/src/graph/types.rs",
      "span": { "start_line": 5, "start_col": 0, "end_line": 10, "end_col": 1 },
      "reads": [],
      "writes": [],
      "calls": [],
      "tags": [],
      "params": 0,
      "branches": 0
    }
  ],
  "cached_at": 1769900625
}
```

## Building

```bash
cd crates/mdlr-extract-rust
cargo build
```

The `rust-toolchain.toml` ensures the correct nightly toolchain is used automatically.

## ID Format

Unit IDs are module-relative within the crate (matching `def_path_str` output), e.g. `graph::types::Span`.
