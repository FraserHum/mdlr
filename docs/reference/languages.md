# Language Support

`mdlr` combines linked extractors with sibling binaries. When an extractor is
partial, `mdlr check` still uses any cache entries that were written and marks
the extraction step as partial.

| Language | Files | Extractor type | Required shipped files | Notes |
| --- | --- | --- | --- | --- |
| Rust | `.rs` | Linked Rust crate | `mdlr` | Runs when `Cargo.toml` is present at the project root. |
| TypeScript / JavaScript | `.ts`, `.tsx`, `.js`, `.jsx` | Linked Rust crate | `mdlr` | Runs when `tsconfig.json`, `package.json`, or matching source files are found. |
| Python | `.py`, `.pyi` | Linked Rust crate | `mdlr` | Runs when Python project markers or matching source files are found. |
| Go | `.go` | Sibling Go binary | `mdlr-extract-go` next to `mdlr` | Runs when `go.mod` is present at the project root. |
| C# | `.cs`, `.csproj`, `.sln`, `.slnx` | Sibling POSIX launcher plus framework-dependent .NET app | `mdlr-extract-csharp` next to `mdlr`, plus `libexec/mdlr-extract-csharp/` | Full semantic extraction requires a .NET 8+ SDK. |

## C# Partial Fallback

C# semantic extraction uses Roslyn `MSBuildWorkspace`, so it needs a .NET SDK
with MSBuild discovery support. A runtime-only install is not enough.

If `mdlr` detects a C# project but cannot find `mdlr-extract-csharp`, the C#
step is reported as partial. If the launcher runs but cannot register MSBuild,
or if a solution/project fails to load, the extractor falls back to syntax-only
analysis for affected `.cs` files and exits with status `2`. `mdlr` treats that
as partial rather than fatal and replays the extractor's stderr so the missing
SDK or project-load reason is visible.

Syntax-only C# units still include spans and local structural metrics, but do
not include semantic call/read/write edges.
