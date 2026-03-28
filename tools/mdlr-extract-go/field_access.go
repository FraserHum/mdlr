package main

import (
	"go/ast"
)

// extractFieldAccess extracts receiver field reads and writes from a method body.
// All receiver field accesses are normalized to "self.field" for LCOM computation.
// Closures are folded into the parent.
func extractFieldAccess(body *ast.BlockStmt, recvName string) (reads []string, writes []string) {
	readSet := make(map[string]bool)
	writeSet := make(map[string]bool)

	recordRead := func(field string) {
		name := "self." + field
		if !readSet[name] {
			readSet[name] = true
			reads = append(reads, name)
		}
	}

	recordWrite := func(field string) {
		name := "self." + field
		if !writeSet[name] {
			writeSet[name] = true
			writes = append(writes, name)
		}
	}

	ast.Inspect(body, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.AssignStmt:
			// Check LHS for receiver field writes
			for _, lhs := range node.Lhs {
				if field := recvFieldName(lhs, recvName); field != "" {
					recordWrite(field)
				}
			}
			// Check RHS for receiver field reads
			for _, rhs := range node.Rhs {
				inspectReads(rhs, recvName, recordRead)
			}
			return false // we handled children

		case *ast.IncDecStmt:
			// recv.field++ or recv.field--
			if field := recvFieldName(node.X, recvName); field != "" {
				recordWrite(field)
				return false
			}

		case *ast.SelectorExpr:
			// recv.field (read context)
			if field := recvFieldName(node, recvName); field != "" {
				recordRead(field)
				return false
			}

		case *ast.FuncLit:
			// Fold closure: walk its body
			return true
		}
		return true
	})

	return reads, writes
}

// recvFieldName checks if expr is recv.field and returns the field name.
func recvFieldName(expr ast.Expr, recvName string) string {
	sel, ok := expr.(*ast.SelectorExpr)
	if !ok {
		return ""
	}
	ident, ok := sel.X.(*ast.Ident)
	if !ok {
		return ""
	}
	if ident.Name == recvName {
		return sel.Sel.Name
	}
	return ""
}

// inspectReads walks an expression tree looking for receiver field reads.
func inspectReads(expr ast.Expr, recvName string, recordRead func(string)) {
	ast.Inspect(expr, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.SelectorExpr:
			if field := recvFieldName(node, recvName); field != "" {
				recordRead(field)
				return false
			}
		case *ast.CallExpr:
			// For recv.Method() — Method is a call, not a field read.
			// But arguments may contain field reads, so walk args only.
			if sel, ok := node.Fun.(*ast.SelectorExpr); ok {
				if ident, ok := sel.X.(*ast.Ident); ok && ident.Name == recvName {
					// Skip the selector (it's a method call), walk args
					for _, arg := range node.Args {
						inspectReads(arg, recvName, recordRead)
					}
					return false
				}
			}
			return true
		case *ast.FuncLit:
			// Fold closure
			return true
		}
		return true
	})
}
