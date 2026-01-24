use crate::extract::types::Extractor;
use crate::graph::{Span, Unit, UnitKind};
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct RustExtractor;

impl RustExtractor {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl Default for RustExtractor {
    fn default() -> Self {
        Self::new().expect("Failed to create Rust parser")
    }
}

impl Extractor for RustExtractor {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn extract(&self, source: &str, path: &Path) -> Result<Vec<Unit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse source"))?;

        let mut units = Vec::new();
        let mut context = ExtractionContext {
            source,
            path,
            module_path: Vec::new(),
        };

        extract_from_node(tree.root_node(), &mut context, &mut units);

        Ok(units)
    }
}

struct ExtractionContext<'a> {
    source: &'a str,
    path: &'a Path,
    module_path: Vec<String>,
}

impl<'a> ExtractionContext<'a> {
    fn qualified_name(&self, name: &str) -> String {
        if self.module_path.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.module_path.join("::"), name)
        }
    }
}

fn extract_from_node(node: Node, ctx: &mut ExtractionContext, units: &mut Vec<Unit>) {
    match node.kind() {
        "function_item" => {
            if let Some(unit) = extract_function(node, ctx) {
                units.push(unit);
            }
        }
        "struct_item" => {
            if let Some(unit) = extract_struct(node, ctx) {
                units.push(unit);
            }
        }
        "trait_item" => {
            if let Some(unit) = extract_trait(node, ctx) {
                units.push(unit);
            }
        }
        "impl_item" => {
            if let Some(unit) = extract_impl(node, ctx) {
                units.push(unit);
            }
        }
        "mod_item" => {
            if let Some(name) = get_node_name(node, ctx.source) {
                ctx.module_path.push(name);
                for child in node.children(&mut node.walk()) {
                    extract_from_node(child, ctx, units);
                }
                ctx.module_path.pop();
                return;
            }
        }
        _ => {}
    }

    for child in node.children(&mut node.walk()) {
        extract_from_node(child, ctx, units);
    }
}

fn extract_function(node: Node, ctx: &ExtractionContext) -> Option<Unit> {
    let name = get_node_name(node, ctx.source)?;
    let calls = extract_calls(node, ctx.source);

    Some(Unit {
        id: ctx.qualified_name(&name),
        kind: UnitKind::Function,
        file: ctx.path.to_path_buf(),
        span: node_span(node),
        reads: Vec::new(),
        writes: Vec::new(),
        calls,
        tags: Vec::new(),
    })
}

fn extract_struct(node: Node, ctx: &ExtractionContext) -> Option<Unit> {
    let name = get_node_name(node, ctx.source)?;

    Some(Unit {
        id: ctx.qualified_name(&name),
        kind: UnitKind::Struct,
        file: ctx.path.to_path_buf(),
        span: node_span(node),
        reads: Vec::new(),
        writes: Vec::new(),
        calls: Vec::new(),
        tags: Vec::new(),
    })
}

fn extract_trait(node: Node, ctx: &ExtractionContext) -> Option<Unit> {
    let name = get_node_name(node, ctx.source)?;

    Some(Unit {
        id: ctx.qualified_name(&name),
        kind: UnitKind::Trait,
        file: ctx.path.to_path_buf(),
        span: node_span(node),
        reads: Vec::new(),
        writes: Vec::new(),
        calls: Vec::new(),
        tags: Vec::new(),
    })
}

fn extract_impl(node: Node, ctx: &ExtractionContext) -> Option<Unit> {
    let type_node = node.child_by_field_name("type")?;
    let type_name = node_text(type_node, ctx.source);

    let trait_name = node
        .child_by_field_name("trait")
        .map(|n| node_text(n, ctx.source));

    let id = match trait_name {
        Some(trait_name) => ctx.qualified_name(&format!("impl {} for {}", trait_name, type_name)),
        None => ctx.qualified_name(&format!("impl {}", type_name)),
    };

    Some(Unit {
        id,
        kind: UnitKind::Impl,
        file: ctx.path.to_path_buf(),
        span: node_span(node),
        reads: Vec::new(),
        writes: Vec::new(),
        calls: Vec::new(),
        tags: Vec::new(),
    })
}

fn extract_calls(node: Node, source: &str) -> Vec<String> {
    let mut calls = Vec::new();
    collect_calls(node, source, &mut calls);
    calls.sort();
    calls.dedup();
    calls
}

fn collect_calls(node: Node, source: &str, calls: &mut Vec<String>) {
    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let call_name = extract_callable_name(func, source);
            if !call_name.is_empty() {
                calls.push(call_name);
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        collect_calls(child, source, calls);
    }
}

/// Extract just the function/method name from a call's function node.
/// Handles:
/// - Simple calls: `foo()` -> "foo"
/// - Path calls: `foo::bar()` -> "foo::bar"
/// - Method calls: `obj.method()` -> "obj.method"
/// - Chained calls: `foo().bar()` -> "bar" (the method being called)
fn extract_callable_name(node: Node, source: &str) -> String {
    match node.kind() {
        "identifier" | "scoped_identifier" => node_text(node, source),
        "field_expression" => {
            // obj.method - extract object and field
            let field = node
                .child_by_field_name("field")
                .map(|n| node_text(n, source))
                .unwrap_or_default();

            if let Some(value) = node.child_by_field_name("value") {
                // If the value is a call_expression, just return the field name
                // e.g., foo().bar() -> "bar"
                if value.kind() == "call_expression" {
                    return field;
                }
                // Otherwise build "value.field"
                let value_name = extract_callable_name(value, source);
                if value_name.is_empty() {
                    field
                } else {
                    format!("{}.{}", value_name, field)
                }
            } else {
                field
            }
        }
        _ => String::new(),
    }
}

fn get_node_name(node: Node, source: &str) -> Option<String> {
    let name_node = node.child_by_field_name("name")?;
    Some(node_text(name_node, source))
}

fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

fn node_span(node: Node) -> Span {
    let start = node.start_position();
    let end = node.end_position();
    Span {
        start_line: start.row + 1,
        start_col: start.column,
        end_line: end.row + 1,
        end_col: end.column,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_function() {
        let extractor = RustExtractor::new().unwrap();
        let source = r#"
fn hello() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let units = extractor
            .extract(source, &PathBuf::from("test.rs"))
            .unwrap();

        assert_eq!(units.len(), 2);
        assert_eq!(units[0].id, "hello");
        assert_eq!(units[0].kind, UnitKind::Function);
        assert_eq!(units[1].id, "add");
    }

    #[test]
    fn test_extract_struct() {
        let extractor = RustExtractor::new().unwrap();
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}
"#;
        let units = extractor
            .extract(source, &PathBuf::from("test.rs"))
            .unwrap();

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].id, "Point");
        assert_eq!(units[0].kind, UnitKind::Struct);
    }

    #[test]
    fn test_extract_calls() {
        let extractor = RustExtractor::new().unwrap();
        let source = r#"
fn caller() {
    foo();
    bar();
    baz::qux();
}
"#;
        let units = extractor
            .extract(source, &PathBuf::from("test.rs"))
            .unwrap();

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].calls, vec!["bar", "baz::qux", "foo"]);
    }

    #[test]
    fn test_extract_module() {
        let extractor = RustExtractor::new().unwrap();
        let source = r#"
mod inner {
    fn nested() {}
}
"#;
        let units = extractor
            .extract(source, &PathBuf::from("test.rs"))
            .unwrap();

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].id, "inner::nested");
    }

    #[test]
    fn test_extract_method_chains() {
        let extractor = RustExtractor::new().unwrap();
        let source = r#"
fn chained() {
    obj.method();
    foo().bar();
    fs::write("path", "content").unwrap();
    TempDir::new().unwrap();
    some.long.chain().of().calls();
}
"#;
        let units = extractor
            .extract(source, &PathBuf::from("test.rs"))
            .unwrap();

        assert_eq!(units.len(), 1);
        let calls = &units[0].calls;
        assert!(calls.contains(&"obj.method".to_string()));
        assert!(calls.contains(&"foo".to_string()));
        assert!(calls.contains(&"bar".to_string()));
        assert!(calls.contains(&"fs::write".to_string()));
        assert!(calls.contains(&"unwrap".to_string()));
        assert!(calls.contains(&"TempDir::new".to_string()));
        assert!(calls.contains(&"of".to_string()));
        assert!(calls.contains(&"calls".to_string()));
        // Should NOT contain the full multi-line expression
        assert!(!calls.iter().any(|c| c.contains("content")));
    }
}
