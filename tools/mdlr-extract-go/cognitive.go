package main

import (
	"go/ast"
	"go/token"
)

// computeCognitive computes cognitive complexity (SonarSource formulation)
// for a function body. Penalizes nesting depth.
// Closures are folded into the parent.
func computeCognitive(body *ast.BlockStmt) int {
	v := &cognitiveVisitor{}
	v.walkBlock(body)
	return v.score
}

type cognitiveVisitor struct {
	score   int
	nesting int
}

func (v *cognitiveVisitor) walkBlock(block *ast.BlockStmt) {
	if block == nil {
		return
	}
	for _, stmt := range block.List {
		v.walkStmt(stmt)
	}
}

func (v *cognitiveVisitor) walkStmt(stmt ast.Stmt) {
	switch s := stmt.(type) {
	case *ast.IfStmt:
		v.walkIf(s)
	case *ast.ForStmt:
		v.score += 1 + v.nesting
		v.walkExpr(s.Cond)
		v.nesting++
		v.walkBlock(s.Body)
		v.nesting--
	case *ast.RangeStmt:
		v.score += 1 + v.nesting
		v.nesting++
		v.walkBlock(s.Body)
		v.nesting--
	case *ast.SwitchStmt:
		v.score += 1 + v.nesting
		v.walkExpr(s.Tag)
		v.nesting++
		if s.Body != nil {
			for _, clause := range s.Body.List {
				if cc, ok := clause.(*ast.CaseClause); ok {
					for _, st := range cc.Body {
						v.walkStmt(st)
					}
				}
			}
		}
		v.nesting--
	case *ast.TypeSwitchStmt:
		v.score += 1 + v.nesting
		v.nesting++
		if s.Body != nil {
			for _, clause := range s.Body.List {
				if cc, ok := clause.(*ast.CaseClause); ok {
					for _, st := range cc.Body {
						v.walkStmt(st)
					}
				}
			}
		}
		v.nesting--
	case *ast.SelectStmt:
		v.score += 1 + v.nesting
		v.nesting++
		if s.Body != nil {
			for _, clause := range s.Body.List {
				if cc, ok := clause.(*ast.CommClause); ok {
					for _, st := range cc.Body {
						v.walkStmt(st)
					}
				}
			}
		}
		v.nesting--
	case *ast.BlockStmt:
		v.walkBlock(s)
	case *ast.ExprStmt:
		v.walkExpr(s.X)
	case *ast.AssignStmt:
		for _, expr := range s.Rhs {
			v.walkExpr(expr)
		}
	case *ast.ReturnStmt:
		for _, expr := range s.Results {
			v.walkExpr(expr)
		}
	case *ast.DeclStmt:
		// Walk any func literals in variable declarations
		if gd, ok := s.Decl.(*ast.GenDecl); ok {
			for _, spec := range gd.Specs {
				if vs, ok := spec.(*ast.ValueSpec); ok {
					for _, val := range vs.Values {
						v.walkExpr(val)
					}
				}
			}
		}
	case *ast.GoStmt:
		v.walkCallExpr(s.Call)
	case *ast.DeferStmt:
		v.walkCallExpr(s.Call)
	case *ast.SendStmt:
		v.walkExpr(s.Value)
	case *ast.LabeledStmt:
		v.walkStmt(s.Stmt)
	}
}

func (v *cognitiveVisitor) walkIf(s *ast.IfStmt) {
	// +1 inherent + nesting penalty
	v.score += 1 + v.nesting
	v.walkExpr(s.Cond)

	// Visit body at increased nesting
	v.nesting++
	v.walkBlock(s.Body)
	v.nesting--

	// Handle else
	if s.Else != nil {
		if elseIf, ok := s.Else.(*ast.IfStmt); ok {
			// else if — walk at same nesting (the nested if adds its own +1)
			v.walkIf(elseIf)
		} else {
			// plain else: +1 inherent, no nesting penalty
			v.score += 1
			v.nesting++
			v.walkStmt(s.Else)
			v.nesting--
		}
	}
}

func (v *cognitiveVisitor) walkExpr(expr ast.Expr) {
	if expr == nil {
		return
	}
	ast.Inspect(expr, func(n ast.Node) bool {
		switch e := n.(type) {
		case *ast.BinaryExpr:
			if e.Op == token.LAND || e.Op == token.LOR {
				v.score += 1 // logical operators: +1, no nesting penalty
			}
		case *ast.FuncLit:
			// Fold closure into parent: walk its body at current nesting
			if e.Body != nil {
				v.walkBlock(e.Body)
			}
			return false
		}
		return true
	})
}

func (v *cognitiveVisitor) walkCallExpr(call *ast.CallExpr) {
	if call == nil {
		return
	}
	for _, arg := range call.Args {
		v.walkExpr(arg)
	}
}
