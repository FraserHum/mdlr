package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"golang.org/x/tools/go/packages"
)

// FileCacheEntry matches the Rust FileCacheEntry format.
type FileCacheEntry struct {
	SourcePath string `json:"source_path"`
	Units      []Unit `json:"units"`
	CachedAt   uint64 `json:"cached_at"`
}

// Unit matches the Rust Unit struct.
type Unit struct {
	ID                  string   `json:"id"`
	Kind                string   `json:"kind"`
	File                string   `json:"file"`
	Span                Span     `json:"span"`
	Reads               []string `json:"reads"`
	Writes              []string `json:"writes"`
	Calls               []string `json:"calls"`
	Tags                []string `json:"tags"`
	Params              int      `json:"params"`
	Branches            int      `json:"branches"`
	MaxScopeLines       int      `json:"max_scope_lines"`
	Parent              *string  `json:"parent,omitempty"`
	CognitiveComplexity int      `json:"cognitive_complexity"`
	Partial             bool     `json:"partial,omitempty"`
}

// Span matches the Rust Span struct.
type Span struct {
	StartLine int `json:"start_line"`
	StartCol  int `json:"start_col"`
	EndLine   int `json:"end_line"`
	EndCol    int `json:"end_col"`
}

func main() {
	root := flag.String("root", ".", "Root directory of the Go module")
	output := flag.String("output", "", "Output directory for per-file JSON")
	genID := flag.Uint64("generation-id", 0, "Generation ID for cache filtering")
	flag.Parse()

	if *output == "" {
		fmt.Fprintln(os.Stderr, "mdlr-extract-go: --output is required")
		os.Exit(1)
	}

	absRoot, err := filepath.Abs(*root)
	if err != nil {
		fmt.Fprintf(os.Stderr, "mdlr-extract-go: %v\n", err)
		os.Exit(1)
	}

	timestamp := *genID
	if timestamp == 0 {
		timestamp = uint64(time.Now().Unix())
	}

	if err := run(absRoot, *output, timestamp); err != nil {
		fmt.Fprintf(os.Stderr, "mdlr-extract-go: %v\n", err)
		os.Exit(1)
	}
}

func run(root, outputDir string, timestamp uint64) error {
	cfg := &packages.Config{
		Mode: packages.NeedName |
			packages.NeedFiles |
			packages.NeedSyntax |
			packages.NeedTypes |
			packages.NeedTypesInfo |
			packages.NeedTypesSizes,
		Dir:   root,
		Fset:  token.NewFileSet(),
		Tests: false,
	}

	pkgs, err := packages.Load(cfg, "./...")
	if err != nil {
		return fmt.Errorf("loading packages: %w", err)
	}

	// Collect per-file units across all packages, processing packages in parallel.
	var (
		mu      sync.Mutex
		results = make(map[string][]Unit)
		wg      sync.WaitGroup
	)

	for _, pkg := range pkgs {
		// Skip packages with errors (still try to extract what we can)
		pkg := pkg
		wg.Add(1)
		go func() {
			defer wg.Done()
			localResults := make(map[string][]Unit)

			for _, file := range pkg.Syntax {
				filePath := pkg.Fset.File(file.Pos()).Name()

				// Skip files outside the module root
				if !strings.HasPrefix(filePath, root) {
					continue
				}

				// Skip test files
				if strings.HasSuffix(filePath, "_test.go") {
					continue
				}

				// Skip generated files
				if isGenerated(filePath, file) {
					continue
				}

				relPath, err := filepath.Rel(root, filePath)
				if err != nil {
					continue
				}
				relPath = filepath.ToSlash(relPath)

				units := extractFile(file, pkg, relPath, cfg.Fset, root)
				if len(units) > 0 {
					localResults[relPath] = append(localResults[relPath], units...)
				}
			}

			mu.Lock()
			for k, v := range localResults {
				results[k] = append(results[k], v...)
			}
			mu.Unlock()
		}()
	}
	wg.Wait()

	// Write per-file JSON output and token cache
	for relPath, units := range results {
		entry := FileCacheEntry{
			SourcePath: relPath,
			Units:      units,
			CachedAt:   timestamp,
		}

		outFile := filepath.Join(outputDir, relPath)
		ext := filepath.Ext(outFile)
		outFile = outFile[:len(outFile)-len(ext)] + ".json"

		if err := os.MkdirAll(filepath.Dir(outFile), 0o755); err != nil {
			fmt.Fprintf(os.Stderr, "warning: %v\n", err)
			continue
		}

		data, err := json.MarshalIndent(entry, "", "  ")
		if err != nil {
			fmt.Fprintf(os.Stderr, "warning: marshal %s: %v\n", relPath, err)
			continue
		}
		if err := os.WriteFile(outFile, data, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "warning: write %s: %v\n", outFile, err)
		}

		// Write token cache for CPD
		absFile := filepath.Join(root, relPath)
		source, err := os.ReadFile(absFile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "warning: read %s for tokenization: %v\n", relPath, err)
			continue
		}
		ft := tokenizeGo(source, relPath, timestamp)
		if err := writeTokenFile(outputDir, relPath, ft); err != nil {
			fmt.Fprintf(os.Stderr, "warning: write tokens %s: %v\n", relPath, err)
		}
	}

	return nil
}

// isGenerated checks if a file is auto-generated.
func isGenerated(path string, file *ast.File) bool {
	base := filepath.Base(path)
	if strings.HasSuffix(base, ".pb.go") || strings.HasSuffix(base, "_gen.go") {
		return true
	}
	// Per Go convention, the generated marker must appear before the package clause.
	// Only check comments whose position is before the package keyword.
	for _, cg := range file.Comments {
		if cg.Pos() >= file.Package {
			break
		}
		for _, c := range cg.List {
			if strings.Contains(c.Text, "Code generated") && strings.Contains(c.Text, "DO NOT EDIT") {
				return true
			}
		}
	}
	return false
}

// extractFile extracts all units from a single Go file.
func extractFile(file *ast.File, pkg *packages.Package, relPath string, fset *token.FileSet, root string) []Unit {
	ex := &extractor{
		file:    file,
		pkg:     pkg,
		relPath: relPath,
		fset:    fset,
		info:    pkg.TypesInfo,
		root:    root,
	}
	return ex.extract()
}

type extractor struct {
	file    *ast.File
	pkg     *packages.Package
	relPath string
	fset    *token.FileSet
	info    *types.Info
	root    string
}

func (ex *extractor) extract() []Unit {
	var units []Unit
	initCount := 0

	// Track which type names have struct/interface declarations in this file,
	// so we can associate methods with their receiver types.
	structIDs := make(map[string]string) // typeName -> unit ID

	// First pass: collect structs, interfaces, and top-level functions.
	for _, decl := range ex.file.Decls {
		switch d := decl.(type) {
		case *ast.GenDecl:
			for _, spec := range d.Specs {
				ts, ok := spec.(*ast.TypeSpec)
				if !ok {
					continue
				}
				switch ts.Type.(type) {
				case *ast.StructType, *ast.InterfaceType:
					id := ex.makeID(ts.Name.Name)
					structIDs[ts.Name.Name] = id
					units = append(units, Unit{
						ID:     id,
						Kind:   "Struct",
						File:   ex.relPath,
						Span:   ex.makeSpan(ts.Pos(), ts.End()),
						Reads:  []string{},
						Writes: []string{},
						Calls:  []string{},
						Tags:   []string{},
					})
				}
			}
		case *ast.FuncDecl:
			if d.Recv != nil {
				continue // methods handled in second pass
			}
			name := d.Name.Name
			if name == "init" {
				if initCount > 0 {
					name = fmt.Sprintf("init_%d", initCount)
				}
				initCount++
			}

			unit := ex.extractFuncUnit(name, "Function", d.Type, d.Body, nil, nil)
			units = append(units, unit)
		}
	}

	// Second pass: methods (FuncDecl with receivers).
	for _, decl := range ex.file.Decls {
		fd, ok := decl.(*ast.FuncDecl)
		if !ok || fd.Recv == nil {
			continue
		}

		recvTypeName := receiverTypeName(fd.Recv)
		if recvTypeName == "" {
			continue
		}

		parentID, ok := structIDs[recvTypeName]
		if !ok {
			// The struct might be declared in another file of the same package.
			// Create an implicit struct unit ID.
			parentID = ex.makeID(recvTypeName)
		}

		recvVarName := receiverVarName(fd.Recv)
		unit := ex.extractFuncUnit(fd.Name.Name, "Method", fd.Type, fd.Body, &parentID, &recvVarName)
		// Set the ID with the struct scope
		unit.ID = ex.relPath + "::" + recvTypeName + "::" + fd.Name.Name

		units = append(units, unit)
	}

	return units
}

func (ex *extractor) makeID(name string) string {
	return ex.relPath + "::" + name
}

func (ex *extractor) makeSpan(start, end token.Pos) Span {
	s := ex.fset.Position(start)
	e := ex.fset.Position(end)
	return Span{
		StartLine: s.Line,
		StartCol:  s.Column - 1, // 0-based
		EndLine:   e.Line,
		EndCol:    e.Column - 1,
	}
}

func (ex *extractor) extractFuncUnit(name, kind string, funcType *ast.FuncType, body *ast.BlockStmt, parent *string, recvVarName *string) Unit {
	id := ex.makeID(name)
	span := ex.makeSpan(funcType.Pos(), funcType.End())
	if body != nil {
		span = ex.makeSpan(funcType.Pos(), body.End())
	}

	params := 0
	if funcType.Params != nil {
		for _, field := range funcType.Params.List {
			if len(field.Names) == 0 {
				params++ // unnamed param
			} else {
				params += len(field.Names)
			}
		}
	}

	var calls []string
	var reads, writes []string
	var branches int
	var maxScope int
	var cognitive int

	if body != nil {
		rawCalls := extractCalls(body, ex.info, ex.fset, ex.relPath)
		for _, c := range rawCalls {
			calls = append(calls, makeCallTargetRelative(c, ex.root))
		}
		branches = countBranches(body)
		maxScope = maxScopeLines(body, ex.fset)
		cognitive = computeCognitive(body)

		if recvVarName != nil && *recvVarName != "" {
			reads, writes = extractFieldAccess(body, *recvVarName)
		}
	}

	if calls == nil {
		calls = []string{}
	}
	if reads == nil {
		reads = []string{}
	}
	if writes == nil {
		writes = []string{}
	}

	return Unit{
		ID:                  id,
		Kind:                kind,
		File:                ex.relPath,
		Span:                span,
		Reads:               reads,
		Writes:              writes,
		Calls:               calls,
		Tags:                []string{},
		Params:              params,
		Branches:            branches,
		MaxScopeLines:       maxScope,
		Parent:              parent,
		CognitiveComplexity: cognitive,
	}
}

// receiverTypeName extracts the type name from a method receiver.
func receiverTypeName(recv *ast.FieldList) string {
	if recv == nil || len(recv.List) == 0 {
		return ""
	}
	t := recv.List[0].Type
	// Unwrap pointer receiver
	if star, ok := t.(*ast.StarExpr); ok {
		t = star.X
	}
	if ident, ok := t.(*ast.Ident); ok {
		return ident.Name
	}
	// Generic receivers: T[P] -> IndexExpr or IndexListExpr
	if idx, ok := t.(*ast.IndexExpr); ok {
		if ident, ok := idx.X.(*ast.Ident); ok {
			return ident.Name
		}
	}
	return ""
}

// receiverVarName extracts the variable name of the receiver.
func receiverVarName(recv *ast.FieldList) string {
	if recv == nil || len(recv.List) == 0 {
		return ""
	}
	if len(recv.List[0].Names) == 0 {
		return ""
	}
	return recv.List[0].Names[0].Name
}
