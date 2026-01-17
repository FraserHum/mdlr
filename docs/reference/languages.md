# Supported Languages

mdlr uses tree-sitter for parsing, enabling accurate extraction of code structure.

## Currently Supported

| Language | Extensions | Status |
|----------|------------|--------|
| Rust | `.rs` | Full support |

### Rust Extraction

The Rust extractor identifies:

- **Functions**: `fn` declarations
- **Structs**: `struct` declarations
- **Traits**: `trait` declarations
- **Impl blocks**: `impl` blocks (with or without traits)
- **Calls**: Function and method invocations

Module paths are tracked, so a function `foo` inside `mod bar` gets ID `bar::foo`.

## Planned

| Language | Extensions | Status |
|----------|------------|--------|
| TypeScript | `.ts`, `.tsx` | Planned |
| Go | `.go` | Planned |
| Python | `.py` | Planned |

## Adding Language Support

Language support requires implementing the `Extractor` trait:

```rust
pub trait Extractor: Send + Sync {
    fn language(&self) -> &'static str;
    fn extract(&self, source: &str, path: &Path) -> Result<Vec<Unit>>;
}
```

Each extractor:
1. Parses source using the appropriate tree-sitter grammar
2. Walks the AST to identify units (functions, classes, etc.)
3. Extracts relationships (calls, imports, etc.)
4. Returns a list of `Unit` structs
