package main

import (
	"go/ast"
)

// countBranches counts branch points in a function body for cyclomatic complexity.
// Closures (FuncLit) are folded into the parent — their branches count here.
func countBranches(body *ast.BlockStmt) int {
	count := 0
	ast.Inspect(body, func(n ast.Node) bool {
		switch expr := n.(type) {
		case *ast.IfStmt:
			count++
		case *ast.ForStmt:
			count++
		case *ast.RangeStmt:
			count++
		case *ast.SwitchStmt:
			// Each case is a branch; count cases-1 for baseline
			if expr.Body != nil && len(expr.Body.List) > 1 {
				count += len(expr.Body.List) - 1
			}
		case *ast.TypeSwitchStmt:
			if expr.Body != nil && len(expr.Body.List) > 1 {
				count += len(expr.Body.List) - 1
			}
		case *ast.SelectStmt:
			// Each comm clause is a branch
			if expr.Body != nil && len(expr.Body.List) > 1 {
				count += len(expr.Body.List) - 1
			}
		case *ast.BinaryExpr:
			if expr.Op.String() == "&&" || expr.Op.String() == "||" {
				count++
			}
		case *ast.FuncLit:
			// Fold closure: continue walking its body (don't skip)
			return true
		}
		return true
	})
	return count
}
