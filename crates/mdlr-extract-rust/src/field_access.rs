//! Field access extraction for Rust code.
//!
//! Extracts field reads and writes from function bodies, tracking which
//! struct fields are accessed via `self.field` patterns.

use tree_sitter::Node;

/// Extract field reads and writes from a function body.
///
/// Returns (reads, writes) where each is a sorted, deduplicated list of field names
/// accessed via `self.field` patterns.
pub fn extract_field_access(
    node: Node,
    source: &str,
) -> (Vec<String>, Vec<String>) {
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    collect_field_access(node, source, &mut reads, &mut writes, false);
    reads.sort();
    reads.dedup();
    writes.sort();
    writes.dedup();
    (reads, writes)
}

/// Recursively collect field accesses from the AST.
fn collect_field_access(
    node: Node,
    source: &str,
    reads: &mut Vec<String>,
    writes: &mut Vec<String>,
    in_assignment_lhs: bool,
) {
    match node.kind() {
        "field_expression" => {
            // Check if this is self.field access
            if let Some(value) = node.child_by_field_name("value") {
                let value_text = node_text(value, source);
                if value_text == "self"
                    || value_text == "&self"
                    || value_text == "&mut self"
                {
                    if let Some(field) = node.child_by_field_name("field") {
                        let field_name = node_text(field, source);
                        if in_assignment_lhs {
                            writes.push(field_name);
                        } else {
                            reads.push(field_name);
                        }
                    }
                }
            }
        }
        "call_expression" => {
            // Handle method calls carefully to distinguish:
            // - self.method() -> NOT a field read (method call)
            // - self.field.method() -> field IS a read (field access chained with method)
            // - self.field.method1().method2() -> still a field read
            if let Some(func) = node.child_by_field_name("function") {
                // Walk down the call chain to find field accesses
                process_call_function(
                    func,
                    source,
                    reads,
                    writes,
                    in_assignment_lhs,
                );
            }
            // Always process arguments
            if let Some(args) = node.child_by_field_name("arguments") {
                collect_field_access(
                    args,
                    source,
                    reads,
                    writes,
                    in_assignment_lhs,
                );
            }
            return; // Don't recurse normally, we handled the relevant children
        }
        "assignment_expression" | "compound_assignment_expr" => {
            // Left side is a write, right side is a read
            if let Some(left) = node.child_by_field_name("left") {
                collect_field_access(left, source, reads, writes, true);
            }
            if let Some(right) = node.child_by_field_name("right") {
                collect_field_access(right, source, reads, writes, false);
            }
            return; // Don't recurse normally, we handled children
        }
        _ => {}
    }

    for child in node.children(&mut node.walk()) {
        collect_field_access(child, source, reads, writes, in_assignment_lhs);
    }
}

/// Walk down the "function" part of a call expression to find self.field accesses.
///
/// Handles arbitrary chains like `self.field.method1().method2()`.
fn process_call_function(
    node: Node,
    source: &str,
    reads: &mut Vec<String>,
    writes: &mut Vec<String>,
    in_assignment_lhs: bool,
) {
    match node.kind() {
        "field_expression" => {
            // This is obj.method - check what obj is
            if let Some(value) = node.child_by_field_name("value") {
                process_call_value(
                    value,
                    source,
                    reads,
                    writes,
                    in_assignment_lhs,
                );
            }
        }
        "generic_function" => {
            // This is obj.method::<T>() with turbofish syntax
            // The function field contains the actual field_expression
            if let Some(func) = node.child_by_field_name("function") {
                process_call_function(
                    func,
                    source,
                    reads,
                    writes,
                    in_assignment_lhs,
                );
            }
        }
        "scoped_identifier" | "identifier" => {
            // Direct function call like foo() - no field access
        }
        _ => {}
    }
}

/// Process the value part of a field_expression in a call chain.
fn process_call_value(
    value: Node,
    source: &str,
    reads: &mut Vec<String>,
    writes: &mut Vec<String>,
    in_assignment_lhs: bool,
) {
    match value.kind() {
        "field_expression" => {
            // obj is itself a field access (e.g., self.field)
            // Process it to capture the field read
            collect_field_access(
                value,
                source,
                reads,
                writes,
                in_assignment_lhs,
            );
        }
        "call_expression" => {
            // obj is a method call (e.g., self.method1())
            // Recurse to find any field access in the call chain
            if let Some(inner_func) = value.child_by_field_name("function") {
                process_call_function(
                    inner_func,
                    source,
                    reads,
                    writes,
                    in_assignment_lhs,
                );
            }
            // Also process arguments of the inner call
            if let Some(args) = value.child_by_field_name("arguments") {
                collect_field_access(
                    args,
                    source,
                    reads,
                    writes,
                    in_assignment_lhs,
                );
            }
        }
        _ => {
            // obj is something else (just "self", a variable, etc.)
            // No field access to capture
        }
    }
}

/// Get the text content of a tree-sitter node.
fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_and_extract(source: &str) -> (Vec<String>, Vec<String>) {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        extract_field_access(tree.root_node(), source)
    }

    #[test]
    fn test_simple_field_read() {
        let source = r#"
impl Foo {
    fn reader(&self) -> i32 {
        self.x + self.y
    }
}
"#;
        let (reads, writes) = parse_and_extract(source);
        assert!(reads.contains(&"x".to_string()));
        assert!(reads.contains(&"y".to_string()));
        assert!(writes.is_empty());
    }

    #[test]
    fn test_field_write() {
        let source = r#"
impl Foo {
    fn writer(&mut self) {
        self.x = 10;
        self.y = self.z;
    }
}
"#;
        let (reads, writes) = parse_and_extract(source);
        assert!(writes.contains(&"x".to_string()));
        assert!(writes.contains(&"y".to_string()));
        assert!(reads.contains(&"z".to_string()));
    }

    #[test]
    fn test_method_calls_not_counted() {
        let source = r#"
impl Foo {
    fn caller(&self) {
        self.do_something();
        self.other_method(self.field);
    }
}
"#;
        let (reads, writes) = parse_and_extract(source);
        assert_eq!(reads, vec!["field".to_string()]);
        assert!(!reads.contains(&"do_something".to_string()));
        assert!(!reads.contains(&"other_method".to_string()));
        assert!(writes.is_empty());
    }

    #[test]
    fn test_chained_field_method_call() {
        let source = r#"
impl Foo {
    fn reader(&self) {
        self.ctx.as_ref();
        self.data.clone();
    }
}
"#;
        let (reads, _) = parse_and_extract(source);
        assert!(reads.contains(&"ctx".to_string()));
        assert!(reads.contains(&"data".to_string()));
        assert!(!reads.contains(&"as_ref".to_string()));
        assert!(!reads.contains(&"clone".to_string()));
    }

    #[test]
    fn test_multi_chained_method_calls() {
        let source = r#"
impl Foo {
    fn reader(&self) {
        self.members.iter().find(|x| x.name == "test");
        self.items.iter().map(|x| x.clone()).collect::<Vec<_>>();
    }
}
"#;
        let (reads, _) = parse_and_extract(source);
        assert!(reads.contains(&"members".to_string()));
        assert!(reads.contains(&"items".to_string()));
        assert!(!reads.contains(&"iter".to_string()));
        assert!(!reads.contains(&"find".to_string()));
        assert!(!reads.contains(&"map".to_string()));
        assert!(!reads.contains(&"collect".to_string()));
    }
}
