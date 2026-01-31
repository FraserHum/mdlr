use crate::extract::types::Extractor;
use crate::graph::{Span, Unit, UnitKind};
use crate::resolve::ResolutionContext;
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

    fn extract(
        &self,
        source: &str,
        path: &Path,
        resolution_ctx: Option<&ResolutionContext>,
    ) -> Result<Vec<Unit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse source"))?;

        // Get the crate name and module path for this file if resolution context is available
        let (crate_name, crate_module_path) =
            resolution_ctx.and_then(|ctx| ctx.file_to_module(path)).unzip();

        let mut units = Vec::new();
        let mut context = ExtractionContext {
            source,
            path,
            module_path: Vec::new(),
            current_impl: None,
            resolution_ctx,
            crate_name,
            crate_module_path,
        };

        extract_from_node(tree.root_node(), &mut context, &mut units);

        Ok(units)
    }
}

struct ExtractionContext<'a> {
    source: &'a str,
    path: &'a Path,
    module_path: Vec<String>,
    /// Current impl block ID (if inside an impl)
    current_impl: Option<String>,
    /// Resolution context for resolving calls to fully qualified names
    resolution_ctx: Option<&'a ResolutionContext>,
    /// The crate name this file belongs to (if resolution context is available)
    crate_name: Option<String>,
    /// The module path within the crate (if resolution context is available)
    crate_module_path: Option<Vec<String>>,
}

impl<'a> ExtractionContext<'a> {
    /// Generate a qualified name for a unit.
    ///
    /// When resolution context is available, uses crate-based naming:
    ///   "my_crate::module::impl Foo::method"
    ///
    /// Without resolution context, uses file-based naming:
    ///   "src/foo.rs::module::impl Foo::method"
    fn qualified_name(&self, name: &str) -> String {
        let mut parts = Vec::new();

        // Add module path if present (from inline mod declarations)
        if !self.module_path.is_empty() {
            parts.push(self.module_path.join("::"));
        }

        // Add parent impl block if inside one (for methods)
        if let Some(ref impl_name) = self.current_impl {
            // Extract just the impl part without prefix
            // e.g., "my_crate::foo::impl Foo" -> "impl Foo"
            // or "src/foo.rs::impl Foo" -> "impl Foo"
            if let Some(idx) = impl_name.rfind("::impl ") {
                let impl_local = &impl_name[idx + 2..];
                parts.push(impl_local.to_string());
            } else if let Some(idx) = impl_name.find("::") {
                let impl_local = &impl_name[idx + 2..];
                if impl_local.starts_with("impl ") {
                    parts.push(impl_local.to_string());
                }
            }
        }

        parts.push(name.to_string());

        let local_name = parts.join("::");

        // Use crate-based naming if resolution context is available
        if let (Some(crate_name), Some(crate_module)) =
            (&self.crate_name, &self.crate_module_path)
        {
            // Build the full crate path: crate_name::module::local_name
            // Skip "crate" from module path since we use the actual crate name
            let module_parts: Vec<&str> = crate_module
                .iter()
                .filter(|s| *s != "crate")
                .map(|s| s.as_str())
                .collect();

            if module_parts.is_empty() {
                format!("{}::{}", crate_name, local_name)
            } else {
                format!(
                    "{}::{}::{}",
                    crate_name,
                    module_parts.join("::"),
                    local_name
                )
            }
        } else {
            // Fall back to file-based naming
            format!("{}::{}", self.path.display(), local_name)
        }
    }

    /// Resolve a call expression to a fully qualified name.
    ///
    /// Returns the resolved crate path if resolution succeeds,
    /// otherwise returns the original call name.
    fn resolve_call(&self, call: &str) -> String {
        if let Some(ctx) = self.resolution_ctx {
            if let Some(resolved) = ctx.resolve_call(call, self.path) {
                return resolved;
            }
        }
        call.to_string()
    }
}

fn extract_from_node(
    node: Node,
    ctx: &mut ExtractionContext,
    units: &mut Vec<Unit>,
) {
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
                let impl_id = unit.id.clone();
                units.push(unit);
                // Extract methods inside this impl
                let old_impl = ctx.current_impl.take();
                ctx.current_impl = Some(impl_id);
                for child in node.children(&mut node.walk()) {
                    extract_from_node(child, ctx, units);
                }
                ctx.current_impl = old_impl;
                return;
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
    let raw_calls = extract_calls(node, ctx.source);
    let params = count_parameters(node);
    let branches = count_branches(node);
    let (reads, writes) = extract_field_access(node, ctx.source);

    // Resolve calls to fully qualified names
    let calls: Vec<String> =
        raw_calls.into_iter().map(|call| ctx.resolve_call(&call)).collect();

    Some(Unit {
        id: ctx.qualified_name(&name),
        kind: UnitKind::Function,
        file: ctx.path.to_path_buf(),
        span: node_span(node),
        reads,
        writes,
        calls,
        tags: Vec::new(),
        params,
        branches,
        parent: ctx.current_impl.clone(),
        impl_trait: None,
        impl_type: None,
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
        params: 0,
        branches: 0,
        parent: None,
        impl_trait: None,
        impl_type: None,
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
        params: 0,
        branches: 0,
        parent: None,
        impl_trait: None,
        impl_type: None,
    })
}

fn extract_impl(node: Node, ctx: &ExtractionContext) -> Option<Unit> {
    let type_node = node.child_by_field_name("type")?;
    let type_name = node_text(type_node, ctx.source);

    let trait_name =
        node.child_by_field_name("trait").map(|n| node_text(n, ctx.source));

    let id = match &trait_name {
        Some(t) => {
            ctx.qualified_name(&format!("impl {} for {}", t, type_name))
        }
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
        params: 0,
        branches: 0,
        parent: None,
        impl_trait: trait_name,
        impl_type: Some(type_name),
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

/// Count the number of parameters in a function
fn count_parameters(node: Node) -> usize {
    let Some(params_node) = node.child_by_field_name("parameters") else {
        return 0;
    };

    let mut count = 0;
    for child in params_node.children(&mut params_node.walk()) {
        // Count parameter nodes (excluding self parameters for methods)
        if child.kind() == "parameter" {
            count += 1;
        } else if child.kind() == "self_parameter" {
            // Don't count self/&self/&mut self as a parameter
        }
    }
    count
}

/// Count branch points for cyclomatic complexity
/// Counts: if, else if, match arms, while, for, loop, && and ||
fn count_branches(node: Node) -> usize {
    let mut count = 0;
    count_branches_recursive(node, &mut count);
    count
}

fn count_branches_recursive(node: Node, count: &mut usize) {
    match node.kind() {
        "if_expression" => {
            // Count the if itself
            *count += 1;
        }
        "match_expression" => {
            // Count each match arm (minus 1 since one path is the default)
            let mut arm_count = 0;
            for child in node.children(&mut node.walk()) {
                if child.kind() == "match_block" {
                    for arm in child.children(&mut child.walk()) {
                        if arm.kind() == "match_arm" {
                            arm_count += 1;
                        }
                    }
                }
            }
            // Each arm beyond the first adds a branch
            if arm_count > 0 {
                *count += arm_count - 1;
            }
        }
        "while_expression" | "for_expression" | "loop_expression" => {
            *count += 1;
        }
        "binary_expression" => {
            // Check for && or || operators
            for child in node.children(&mut node.walk()) {
                if child.kind() == "&&" || child.kind() == "||" {
                    *count += 1;
                }
            }
        }
        _ => {}
    }

    for child in node.children(&mut node.walk()) {
        count_branches_recursive(child, count);
    }
}

/// Extract field reads and writes from a function body
/// Returns (reads, writes) where each is a list of field names
fn extract_field_access(
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
#[path = "rust_tests.rs"]
mod tests;
