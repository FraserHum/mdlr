use rustc_hir as hir;
use rustc_middle::ty::TyCtxt;

use crate::walk::ExprVisitor;

/// Compute cognitive complexity (SonarSource formulation) for a function body.
///
/// Unlike cyclomatic complexity, cognitive complexity penalizes nesting depth:
/// each control structure adds `1 + current_nesting_depth` to the score.
pub fn compute_cognitive_complexity<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &hir::Body<'tcx>,
) -> usize {
    let mut visitor = CognitiveVisitor { tcx, score: 0 };
    visitor.walk_expr(body.value, 0);
    visitor.score
}

struct CognitiveVisitor<'tcx> {
    tcx: TyCtxt<'tcx>,
    score: usize,
}

impl<'tcx> ExprVisitor<'tcx> for CognitiveVisitor<'tcx> {
    /// Nesting depth as context.
    type Ctx = usize;

    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_if(
        &mut self,
        cond: &hir::Expr<'tcx>,
        then_branch: &hir::Expr<'tcx>,
        else_branch: Option<&hir::Expr<'tcx>>,
        nesting: usize,
    ) {
        // +1 inherent + nesting penalty
        self.score += 1 + nesting;
        self.walk_expr(cond, nesting);
        self.walk_expr(then_branch, nesting + 1);
        if let Some(else_br) = else_branch {
            // `else if` in HIR is If(cond, then, Some(If(...)))
            // Walk else-if at same nesting (no nesting increment), just +1 for the else-if
            if matches!(&else_br.kind, hir::ExprKind::If(..)) {
                self.walk_expr(else_br, nesting);
            } else {
                // Plain `else`: +1 inherent, no nesting penalty
                self.score += 1;
                self.walk_expr(else_br, nesting + 1);
            }
        }
    }

    fn visit_match(
        &mut self,
        scrutinee: &hir::Expr<'tcx>,
        arms: &'tcx [hir::Arm<'tcx>],
        source: hir::MatchSource,
        nesting: usize,
    ) {
        // Only count user-written match expressions, not desugared ones
        if source == hir::MatchSource::Normal {
            self.score += 1 + nesting;
        }
        self.walk_expr(scrutinee, nesting);
        let arm_nesting = if source == hir::MatchSource::Normal {
            nesting + 1
        } else {
            nesting
        };
        for arm in arms.iter() {
            if let Some(guard) = &arm.guard {
                self.walk_expr(guard, arm_nesting);
            }
            self.walk_expr(arm.body, arm_nesting);
        }
    }

    fn visit_loop(&mut self, block: &hir::Block<'tcx>, nesting: usize) {
        // +1 inherent + nesting penalty for loop/while/for
        self.score += 1 + nesting;
        // Walk the block's statements and tail expr at increased nesting
        for stmt in block.stmts {
            self.walk_stmt_with_ctx(stmt, nesting + 1);
        }
        if let Some(expr) = block.expr {
            self.walk_expr(expr, nesting + 1);
        }
    }

    fn visit_binary(
        &mut self,
        op: hir::BinOp,
        lhs: &hir::Expr<'tcx>,
        rhs: &hir::Expr<'tcx>,
        nesting: usize,
    ) {
        // +1 for && or || (no nesting penalty for boolean operators)
        match op.node {
            hir::BinOpKind::And | hir::BinOpKind::Or => {
                self.score += 1;
            }
            _ => {}
        }
        self.walk_expr(lhs, nesting);
        self.walk_expr(rhs, nesting);
    }

    fn visit_closure(
        &mut self,
        closure: &'tcx hir::Closure<'tcx>,
        nesting: usize,
    ) {
        // Closures increase nesting but don't add to the score
        let body = self.tcx().hir_body(closure.body);
        self.walk_expr(body.value, nesting + 1);
    }

    fn visit_block_expr(&mut self, block: &hir::Block<'tcx>, nesting: usize) {
        // Pass nesting through to block contents
        for stmt in block.stmts {
            self.walk_stmt_with_ctx(stmt, nesting);
        }
        if let Some(expr) = block.expr {
            self.walk_expr(expr, nesting);
        }
    }
}

impl<'tcx> CognitiveVisitor<'tcx> {
    /// Walk a statement passing the current nesting context through.
    fn walk_stmt_with_ctx(&mut self, stmt: &hir::Stmt<'tcx>, nesting: usize) {
        match &stmt.kind {
            hir::StmtKind::Let(local) => {
                if let Some(init) = local.init {
                    self.walk_expr(init, nesting);
                }
            }
            hir::StmtKind::Expr(expr) | hir::StmtKind::Semi(expr) => {
                self.walk_expr(expr, nesting);
            }
            hir::StmtKind::Item(_) => {}
        }
    }
}
