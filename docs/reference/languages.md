# Supported Languages

mdlr supports multiple languages through dedicated extractor binaries that output `FileCacheEntry`-compatible JSON.

## Currently Supported

| Language | Extensions | Status |
|----------|------------|--------|
| Rust | `.rs` | Full support |
| TypeScript | `.ts`, `.tsx` | Full support |
| JavaScript | `.js`, `.jsx` | Full support |
| Go | `.go` | Full support |
| Python | `.py`, `.pyi` | Full support |

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

### Go Extraction

The Go extractor (`mdlr-extract-go`) uses `go/packages` and `go/types` for type-checked analysis and identifies:

- **Functions**: `func` declarations (including `init()` with disambiguation)
- **Structs**: `struct` and `interface` type declarations (both mapped to `Struct`)
- **Methods**: Functions with receivers (value or pointer)
- **Calls**: Function and method invocations resolved via `go/types`
- **Field access**: Receiver field reads/writes normalized to `self.field`
- **Branches**: `if`, `for`, `range`, `switch`, `select`, `&&`, `||`
- **Scopes**: Largest nested block statement

Key capabilities:
- **Type-checked call resolution**: Calls resolved to concrete function/method declarations
- **Interface method resolution**: Interface calls resolve to the interface method declaration
- **Generated code detection**: Skips files with `// Code generated ... DO NOT EDIT.` and `*.pb.go`/`*_gen.go`
- **Test file exclusion**: `_test.go` files are not analyzed

Unit ID format: `<relative_path>::<StructName>::<MethodName>` (e.g., `pkg/server.go::Server::Handle`).

Not extracted: promoted methods from embedded structs, package-level variables/constants, closures (folded into parent function).

Scope: Analyzes the module rooted at `go.mod` (equivalent to `./...`).

Build with: `task build-go` or `go build -o mdlr-extract-go ./tools/mdlr-extract-go`

### Python Extraction

The Python extractor (`mdlr-extract-py`) uses [ruff](https://github.com/astral-sh/ruff)'s Python parser (`ruff_python_parser`) for AST-level analysis and identifies:

- **Functions**: `def` declarations at module level and nested
- **Classes**: `class` declarations (mapped to `Struct`)
- **Methods**: `def` inside a class body
- **Calls**: Function calls (name-based best-effort): `foo()` → `foo`, `self.bar()` → `Self.bar`, `mod.func()` → `mod.func`
- **Field access**: `self.field` reads and `self.field = ...` writes (instance methods only)
- **Branches**: `if`, `elif`, `for`, `while`, `try/except`, `match/case`, `and`/`or`, ternary `x if cond else y`
- **Scopes**: Largest nested scope block

Unit ID format: `<module_path>::<ClassName>.<method_name>` (e.g., `src.mymod.core::Foo.bar`). Module path is derived from the file path with slashes replaced by dots and `.py` stripped.

Project detection: Looks for `pyproject.toml`, `setup.py`, or `setup.cfg` at the workspace root.

File filtering: Respects `.gitignore` and excludes `__pycache__`, `.venv`, `venv`, `.tox`, `build`, `dist`, `.eggs`, `node_modules`.

Not extracted: decorators (v1), type-resolved calls, lambda/comprehension as separate units.

Build with: `cargo install --path tools/mdlr-extract-py`

## Adding Language Support

Language support requires creating a new extractor binary that outputs `FileCacheEntry`-compatible JSON. See the [HIR Extractor](hir-extract.md) for the Rust implementation as a reference.
