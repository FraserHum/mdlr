# Supported Languages

mdlr uses the Rust compiler's HIR (High-level Intermediate Representation) for extraction, providing fully-resolved type information.

## Currently Supported

| Language | Extensions | Status |
|----------|------------|--------|
| Rust | `.rs` | Full support |

### Rust Extraction

The Rust extractor (`mdlr-extract-rust`) uses the compiler's HIR and identifies:

- **Functions**: `fn` declarations
- **Structs**: `struct` declarations
- **Traits**: `trait` declarations
- **Impl blocks**: `impl` blocks (with or without traits)
- **Calls**: Function and method invocations (fully resolved via `typeck`)

Key capabilities:
- **Fully-qualified call resolution**: Trait method calls resolved to concrete implementations
- **Accurate type inference**: Full compiler type information
- **Macro expansion**: Fully expanded (not just surface syntax)
- **Compiler-guaranteed correctness**: Uses the same type information as `rustc`

Requires a nightly Rust toolchain with `rustc-dev` and `llvm-tools` components.

See [HIR Extractor](hir-extract.md) for implementation details.

## Planned

| Language | Extensions | Status |
|----------|------------|--------|
| TypeScript | `.ts`, `.tsx` | Planned |
| Go | `.go` | Planned |
| Python | `.py` | Planned |

## Adding Language Support

Language support requires creating a new extractor binary that outputs `FileCacheEntry`-compatible JSON. See the [HIR Extractor](hir-extract.md) for the Rust implementation as a reference.
