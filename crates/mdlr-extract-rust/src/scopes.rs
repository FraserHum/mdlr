use rustc_hir as hir;
use rustc_middle::ty::TyCtxt;

/// Find the largest single scope block within a function body.
///
/// Measures the line count of each scope-creating expression:
/// - `if` then/else bodies
/// - `match` arm bodies
/// - `loop`/`while`/`for` bodies
/// - Block expressions (`{}`)
/// - Closures
///
/// The function's own top-level block is excluded (that's `function_size`).
/// Returns 0 for functions with no nested scope blocks.
pub fn max_scope_lines(tcx: TyCtxt<'_>, body: &hir::Body<'_>) -> usize {
    let mut max = 0;
    // Walk into the top-level block's contents without measuring
    // the block itself (which would duplicate function_size).
    if let hir::ExprKind::Block(block, _) = &body.value.kind {
        for stmt in block.stmts {
            visit_stmt_for_scopes(tcx, stmt, &mut max);
        }
        if let Some(expr) = block.expr {
            visit_expr_for_scopes(tcx, expr, &mut max);
        }
    } else {
        // Expression-bodied function (e.g. closure) — walk directly
        visit_expr_for_scopes(tcx, body.value, &mut max);
    }
    max
}

/// Compute the line count of a span, returning 0 for macro-expanded or dummy spans.
fn span_lines(tcx: TyCtxt<'_>, span: rustc_span::Span) -> usize {
    if span.from_expansion() || span.is_dummy() {
        return 0;
    }
    let sm = tcx.sess.source_map();
    let lo = sm.lookup_char_pos(span.lo());
    let hi = sm.lookup_char_pos(span.hi());
    hi.line.saturating_sub(lo.line) + 1
}

/// Record a scope span, updating max if it's larger.
fn record_scope(tcx: TyCtxt<'_>, span: rustc_span::Span, max: &mut usize) {
    let lines = span_lines(tcx, span);
    if lines > *max {
        *max = lines;
    }
}

fn visit_expr_for_scopes(
    tcx: TyCtxt<'_>,
    expr: &hir::Expr<'_>,
    max: &mut usize,
) {
    match &expr.kind {
        hir::ExprKind::If(cond, then_branch, else_branch) => {
            // Measure then/else bodies as scopes
            record_scope(tcx, then_branch.span, max);
            visit_expr_for_scopes(tcx, cond, max);
            visit_expr_for_scopes(tcx, then_branch, max);
            if let Some(else_br) = else_branch {
                record_scope(tcx, else_br.span, max);
                visit_expr_for_scopes(tcx, else_br, max);
            }
        }

        hir::ExprKind::Match(scrutinee, arms, _) => {
            visit_expr_for_scopes(tcx, scrutinee, max);
            for arm in arms.iter() {
                // Each arm body is a scope
                record_scope(tcx, arm.body.span, max);
                if let Some(guard) = &arm.guard {
                    visit_expr_for_scopes(tcx, guard, max);
                }
                visit_expr_for_scopes(tcx, arm.body, max);
            }
        }

        hir::ExprKind::Loop(block, _, _, _) => {
            // The loop body block is a scope
            record_scope(tcx, block.span, max);
            for stmt in block.stmts {
                visit_stmt_for_scopes(tcx, stmt, max);
            }
            if let Some(expr) = block.expr {
                visit_expr_for_scopes(tcx, expr, max);
            }
        }

        hir::ExprKind::Block(block, _) => {
            // Bare block expression is a scope
            record_scope(tcx, block.span, max);
            for stmt in block.stmts {
                visit_stmt_for_scopes(tcx, stmt, max);
            }
            if let Some(expr) = block.expr {
                visit_expr_for_scopes(tcx, expr, max);
            }
        }

        hir::ExprKind::Closure(closure) => {
            let body = tcx.hir_body(closure.body);
            record_scope(tcx, body.value.span, max);
            visit_expr_for_scopes(tcx, body.value, max);
        }

        // Recurse into sub-expressions
        hir::ExprKind::Call(func, args) => {
            visit_expr_for_scopes(tcx, func, max);
            for arg in args.iter() {
                visit_expr_for_scopes(tcx, arg, max);
            }
        }

        hir::ExprKind::MethodCall(_, receiver, args, _) => {
            visit_expr_for_scopes(tcx, receiver, max);
            for arg in args.iter() {
                visit_expr_for_scopes(tcx, arg, max);
            }
        }

        hir::ExprKind::Binary(_, lhs, rhs) => {
            visit_expr_for_scopes(tcx, lhs, max);
            visit_expr_for_scopes(tcx, rhs, max);
        }

        hir::ExprKind::Assign(lhs, rhs, _) => {
            visit_expr_for_scopes(tcx, lhs, max);
            visit_expr_for_scopes(tcx, rhs, max);
        }

        hir::ExprKind::AssignOp(_, lhs, rhs) => {
            visit_expr_for_scopes(tcx, lhs, max);
            visit_expr_for_scopes(tcx, rhs, max);
        }

        hir::ExprKind::Field(base, _) => {
            visit_expr_for_scopes(tcx, base, max);
        }

        hir::ExprKind::Index(base, idx, _) => {
            visit_expr_for_scopes(tcx, base, max);
            visit_expr_for_scopes(tcx, idx, max);
        }

        hir::ExprKind::Unary(_, operand) => {
            visit_expr_for_scopes(tcx, operand, max);
        }

        hir::ExprKind::AddrOf(_, _, operand) => {
            visit_expr_for_scopes(tcx, operand, max);
        }

        hir::ExprKind::Ret(Some(expr)) => {
            visit_expr_for_scopes(tcx, expr, max);
        }

        hir::ExprKind::Break(_, Some(expr)) => {
            visit_expr_for_scopes(tcx, expr, max);
        }

        hir::ExprKind::Struct(_, fields, base) => {
            for field in fields.iter() {
                visit_expr_for_scopes(tcx, field.expr, max);
            }
            if let hir::StructTailExpr::Base(base) = base {
                visit_expr_for_scopes(tcx, base, max);
            }
        }

        hir::ExprKind::Tup(exprs) | hir::ExprKind::Array(exprs) => {
            for e in exprs.iter() {
                visit_expr_for_scopes(tcx, e, max);
            }
        }

        hir::ExprKind::Repeat(expr, _) => {
            visit_expr_for_scopes(tcx, expr, max);
        }

        hir::ExprKind::Cast(expr, _) | hir::ExprKind::Type(expr, _) => {
            visit_expr_for_scopes(tcx, expr, max);
        }

        hir::ExprKind::Let(let_expr) => {
            visit_expr_for_scopes(tcx, let_expr.init, max);
        }

        hir::ExprKind::DropTemps(expr) => {
            visit_expr_for_scopes(tcx, expr, max);
        }

        hir::ExprKind::Yield(expr, _) => {
            visit_expr_for_scopes(tcx, expr, max);
        }

        // Leaf expressions
        _ => {}
    }
}

fn visit_stmt_for_scopes(
    tcx: TyCtxt<'_>,
    stmt: &hir::Stmt<'_>,
    max: &mut usize,
) {
    match &stmt.kind {
        hir::StmtKind::Let(local) => {
            if let Some(init) = local.init {
                visit_expr_for_scopes(tcx, init, max);
            }
        }
        hir::StmtKind::Expr(expr) | hir::StmtKind::Semi(expr) => {
            visit_expr_for_scopes(tcx, expr, max);
        }
        hir::StmtKind::Item(_) => {}
    }
}
