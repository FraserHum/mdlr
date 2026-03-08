use swc_ecma_ast::*;
use swc_ecma_visit::{Visit, VisitWith};

/// Extract `this.field` reads and writes from a block statement.
pub fn extract_field_access_block(
    block: &BlockStmt,
) -> (Vec<String>, Vec<String>) {
    let mut visitor =
        FieldAccessVisitor { reads: Vec::new(), writes: Vec::new() };
    block.visit_with(&mut visitor);
    (visitor.reads, visitor.writes)
}

/// Extract `this.field` reads and writes from a single expression.
pub fn extract_field_access_expr(expr: &Expr) -> (Vec<String>, Vec<String>) {
    let mut visitor =
        FieldAccessVisitor { reads: Vec::new(), writes: Vec::new() };
    expr.visit_with(&mut visitor);
    (visitor.reads, visitor.writes)
}

struct FieldAccessVisitor {
    reads: Vec<String>,
    writes: Vec<String>,
}

impl FieldAccessVisitor {
    fn record_read(&mut self, name: String) {
        if !self.reads.contains(&name) {
            self.reads.push(name);
        }
    }

    fn record_write(&mut self, name: String) {
        if !self.writes.contains(&name) {
            self.writes.push(name);
        }
    }

    /// Check if an expression is `this.field` and return the field name.
    fn this_field_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Member(member) => {
                if matches!(&*member.obj, Expr::This(_)) {
                    match &member.prop {
                        MemberProp::Ident(ident) => {
                            Some(ident.sym.to_string())
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            // this?.field
            Expr::OptChain(opt) => match &*opt.base {
                OptChainBase::Member(member) => {
                    if matches!(&*member.obj, Expr::This(_)) {
                        match &member.prop {
                            MemberProp::Ident(ident) => {
                                Some(ident.sym.to_string())
                            }
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Walk an expression collecting this.field reads, but skip the
    /// immediate this.field at the top (it was already recorded as write).
    fn walk_read_context(&mut self, expr: &Expr) {
        expr.visit_with(self);
    }

    /// Check if expr is a call like `this.method(...)`. In that case
    /// `method` is NOT a field read.
    fn is_this_method_call(expr: &Expr) -> bool {
        match expr {
            Expr::Member(member) => matches!(&*member.obj, Expr::This(_)),
            _ => false,
        }
    }
}

impl Visit for FieldAccessVisitor {
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        // LHS: check for this.field write
        match &n.left {
            AssignTarget::Simple(simple) => match simple {
                SimpleAssignTarget::Member(member) => {
                    if let Some(name) =
                        Self::this_field_name(&Expr::Member(member.clone()))
                    {
                        self.record_write(name);
                        // Don't recurse into LHS (already handled)
                        self.walk_read_context(&n.right);
                        return;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        // Default recursion
        n.visit_children_with(self);
    }

    fn visit_update_expr(&mut self, n: &UpdateExpr) {
        // this.field++ / ++this.field → write
        if let Some(name) = Self::this_field_name(&n.arg) {
            self.record_write(name);
            return;
        }
        n.visit_children_with(self);
    }

    fn visit_member_expr(&mut self, n: &MemberExpr) {
        if matches!(&*n.obj, Expr::This(_)) {
            if let MemberProp::Ident(ident) = &n.prop {
                // Only record as read — writes are handled in visit_assign_expr
                self.record_read(ident.sym.to_string());
                return;
            }
        }
        // For chained access like this.field.subfield, the inner `this.field`
        // IS a read even if the outer is a method call receiver
        n.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, n: &CallExpr) {
        // this.method() — `method` is NOT a field read, it's a call.
        // But this.field.method() — `field` IS a read.
        if let Callee::Expr(callee) = &n.callee {
            if Self::is_this_method_call(callee) {
                // Skip recording `method` as a field read — just recurse args
                for arg in &n.args {
                    arg.expr.visit_with(self);
                }
                return;
            }
        }
        n.visit_children_with(self);
    }

    // Do NOT descend into nested functions/arrows
    fn visit_arrow_expr(&mut self, _n: &ArrowExpr) {}
    fn visit_fn_expr(&mut self, _n: &FnExpr) {}
    fn visit_fn_decl(&mut self, _n: &FnDecl) {}
}
