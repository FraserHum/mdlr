package main

import (
	"go/ast"
	"go/token"
)

// maxScopeLines measures the largest nested block within a function body,
// excluding the function's own top-level block.
// Closures are folded into the parent.
func maxScopeLines(body *ast.BlockStmt, fset *token.FileSet) int {
	maxLines := 0

	var walk func(ast.Node)
	walk = func(n ast.Node) {
		ast.Inspect(n, func(node ast.Node) bool {
			switch s := node.(type) {
			case *ast.BlockStmt:
				if s == body {
					// Skip the top-level function body itself
					return true
				}
				lines := spanLines(s.Pos(), s.End(), fset)
				if lines > maxLines {
					maxLines = lines
				}
				return true
			case *ast.CaseClause:
				if len(s.Body) > 0 {
					start := s.Pos()
					end := s.Body[len(s.Body)-1].End()
					lines := spanLines(start, end, fset)
					if lines > maxLines {
						maxLines = lines
					}
				}
				return true
			case *ast.CommClause:
				if len(s.Body) > 0 {
					start := s.Pos()
					end := s.Body[len(s.Body)-1].End()
					lines := spanLines(start, end, fset)
					if lines > maxLines {
						maxLines = lines
					}
				}
				return true
			case *ast.FuncLit:
				// Fold closure: walk its body but don't count the
				// closure's block as a separate scope boundary
				if s.Body != nil {
					for _, stmt := range s.Body.List {
						walk(stmt)
					}
				}
				return false
			}
			return true
		})
	}

	// Walk statements inside the body (not the body block itself)
	for _, stmt := range body.List {
		walk(stmt)
	}

	return maxLines
}

func spanLines(start, end token.Pos, fset *token.FileSet) int {
	s := fset.Position(start)
	e := fset.Position(end)
	lines := e.Line - s.Line + 1
	if lines < 0 {
		lines = 0
	}
	return lines
}
