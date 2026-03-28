package main

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"path/filepath"
	"strings"
)

// extractCalls extracts type-resolved call targets from a function body.
// Closures are folded into the parent — their calls count toward the enclosing function.
func extractCalls(body *ast.BlockStmt, info *types.Info, fset *token.FileSet, relPath string) []string {
	var calls []string
	seen := make(map[string]bool)

	record := func(name string) {
		if !seen[name] {
			seen[name] = true
			calls = append(calls, name)
		}
	}

	var walk func(ast.Node)
	walk = func(n ast.Node) {
		ast.Inspect(n, func(node ast.Node) bool {
			switch expr := node.(type) {
			case *ast.CallExpr:
				if target := resolveCallTarget(expr, info, fset, relPath); target != "" {
					record(target)
				}
				return true // recurse into args, including closure bodies

			case *ast.GoStmt:
				// go foo() — treat as a call
				if target := resolveCallTarget(expr.Call, info, fset, relPath); target != "" {
					record(target)
				}
				// Also walk the call args for nested calls
				for _, arg := range expr.Call.Args {
					walk(arg)
				}
				return false // we handled children

			case *ast.DeferStmt:
				// defer foo() — treat as a call
				if target := resolveCallTarget(expr.Call, info, fset, relPath); target != "" {
					record(target)
				}
				for _, arg := range expr.Call.Args {
					walk(arg)
				}
				return false

			case *ast.FuncLit:
				// Fold closure body into parent — walk it
				if expr.Body != nil {
					walk(expr.Body)
				}
				return false // we handle recursion ourselves
			}
			return true
		})
	}

	walk(body)
	return calls
}

// resolveCallTarget resolves a call expression to a target string using type info.
func resolveCallTarget(call *ast.CallExpr, info *types.Info, fset *token.FileSet, relPath string) string {
	switch fn := call.Fun.(type) {
	case *ast.Ident:
		// Simple call: foo()
		return resolveIdent(fn, info, fset, relPath)

	case *ast.SelectorExpr:
		// Method or qualified call: obj.Method() or pkg.Func()
		return resolveSelector(fn, info, fset, relPath)

	case *ast.FuncLit:
		// Immediately invoked closure: func() { ... }()
		// The closure body is walked by the caller; no call target to record.
		return ""

	case *ast.IndexExpr:
		// Generic function instantiation: foo[T]()
		if ident, ok := fn.X.(*ast.Ident); ok {
			return resolveIdent(ident, info, fset, relPath)
		}
		if sel, ok := fn.X.(*ast.SelectorExpr); ok {
			return resolveSelector(sel, info, fset, relPath)
		}
	}

	return ""
}

// resolveIdent resolves a simple function call identifier.
func resolveIdent(ident *ast.Ident, info *types.Info, fset *token.FileSet, relPath string) string {
	if info == nil {
		return ident.Name
	}

	obj := info.Uses[ident]
	if obj == nil {
		// Could be a builtin or unresolved
		return ident.Name
	}

	return objectToTarget(obj, fset, relPath)
}

// resolveSelector resolves a selector expression (obj.Method or pkg.Func).
func resolveSelector(sel *ast.SelectorExpr, info *types.Info, fset *token.FileSet, relPath string) string {
	if info == nil {
		return selectorToString(sel)
	}

	// Check if this is a method call on a typed value
	selection := info.Selections[sel]
	if selection != nil {
		// Method call: resolve the method
		obj := selection.Obj()
		if obj != nil {
			return objectToTarget(obj, fset, relPath)
		}
	}

	// Check Uses for qualified identifier (pkg.Func)
	obj := info.Uses[sel.Sel]
	if obj != nil {
		return objectToTarget(obj, fset, relPath)
	}

	return selectorToString(sel)
}

// objectToTarget converts a types.Object to a call target string.
func objectToTarget(obj types.Object, fset *token.FileSet, relPath string) string {
	if obj == nil {
		return ""
	}

	switch o := obj.(type) {
	case *types.Func:
		sig := o.Type().(*types.Signature)
		recv := sig.Recv()

		if recv != nil {
			// Method: build file::Type::Method target
			typeName := receiverTypeNameFromType(recv.Type())
			if typeName != "" {
				targetFile := objectRelPath(o, fset)
				if targetFile != "" {
					return targetFile + "::" + typeName + "::" + o.Name()
				}
				return typeName + "." + o.Name()
			}
		}

		// Package-level function
		targetFile := objectRelPath(o, fset)
		if targetFile != "" {
			return targetFile + "::" + o.Name()
		}
		return o.Name()

	case *types.Builtin:
		return o.Name()

	default:
		return obj.Name()
	}
}

// receiverTypeNameFromType extracts the base type name from a receiver type.
func receiverTypeNameFromType(t types.Type) string {
	// Unwrap pointer
	if ptr, ok := t.(*types.Pointer); ok {
		t = ptr.Elem()
	}
	if named, ok := t.(*types.Named); ok {
		return named.Obj().Name()
	}
	return ""
}

// objectRelPath returns the relative file path for an object's declaration.
func objectRelPath(obj types.Object, fset *token.FileSet) string {
	if obj.Pos() == token.NoPos {
		return ""
	}
	pos := fset.Position(obj.Pos())
	if pos.Filename == "" {
		return ""
	}
	// Try to make it relative to the module root.
	// We use the file path from the position and convert to forward slashes.
	return filepath.ToSlash(pos.Filename)
}

// selectorToString converts a selector expression to a dotted string fallback.
func selectorToString(sel *ast.SelectorExpr) string {
	switch x := sel.X.(type) {
	case *ast.Ident:
		return fmt.Sprintf("%s.%s", x.Name, sel.Sel.Name)
	case *ast.SelectorExpr:
		return fmt.Sprintf("%s.%s", selectorToString(x), sel.Sel.Name)
	}
	return sel.Sel.Name
}

// makeCallTargetRelative converts absolute file paths in call targets to relative paths.
func makeCallTargetRelative(target string, root string) string {
	// Call targets may contain absolute paths from type resolution.
	// Convert them to relative paths matching unit IDs.
	parts := strings.SplitN(target, "::", 2)
	if len(parts) < 2 {
		return target
	}
	filePart := parts[0]
	if filepath.IsAbs(filePart) {
		if rel, err := filepath.Rel(root, filePart); err == nil {
			return filepath.ToSlash(rel) + "::" + parts[1]
		}
	}
	return target
}
