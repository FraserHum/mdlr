# Supported Languages

mdlr supports multiple languages through dedicated extractor binaries that output `FileCacheEntry`-compatible JSON.

## Currently Supported

| Language | Extensions | Status |
|----------|------------|--------|
| Rust | `.rs` | Full support |
| TypeScript | `.ts`, `.tsx` | Full support |
| JavaScript | `.js`, `.jsx` | Full support |

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

### TypeScript/JavaScript Extraction

The TypeScript extractor (`mdlr-extract-ts`) uses [SWC](https://swc.rs/) for parsing and identifies:

- **Functions**: `function` declarations, `const foo = () => {}`, `const foo = function() {}`
- **Classes**: `class` declarations (mapped to `Struct`)
- **Methods**: class methods, constructors, getters (`get_x`), setters (`set_x`)
- **Calls**: function calls, method calls, `new` expressions
- **Field access**: `this.field` reads and writes
- **Branches**: `if`, `switch`, `for`, `while`, `do-while`, `&&`, `||`, ternary `?:`
- **Scopes**: largest nested block statement

Unit ID format: `<relative_path>::<scope>::<name>` (e.g., `src/utils.ts::Calculator::add`).

Not extracted: interfaces, type aliases, enums, namespaces (no runtime function bodies).

Build with: `cargo install --path tools/mdlr-extract-ts`

## Planned

| Language | Extensions | Status |
|----------|------------|--------|
| Go | `.go` | Planned |
| Python | `.py` | Planned |

## Adding Language Support

Language support requires creating a new extractor binary that outputs `FileCacheEntry`-compatible JSON. See the [HIR Extractor](hir-extract.md) for the Rust implementation as a reference.
