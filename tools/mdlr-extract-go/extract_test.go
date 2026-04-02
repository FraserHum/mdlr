package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// setupTestProject creates a temporary Go project with the given source files
// and runs the extractor, returning the resulting units grouped by file.
func setupTestProject(t *testing.T, files map[string]string) map[string][]Unit {
	t.Helper()

	dir := t.TempDir()

	// Write go.mod
	gomod := "module testmod\n\ngo 1.22\n"
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte(gomod), 0o644); err != nil {
		t.Fatal(err)
	}

	// Write source files
	for name, content := range files {
		path := filepath.Join(dir, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	// Run extractor
	outDir := t.TempDir()
	if err := run(dir, outDir, 12345); err != nil {
		t.Fatal(err)
	}

	// Read output
	results := make(map[string][]Unit)
	err := filepath.Walk(outDir, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() || filepath.Ext(path) != ".json" {
			return err
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		var entry FileCacheEntry
		if err := json.Unmarshal(data, &entry); err != nil {
			return err
		}
		results[entry.SourcePath] = entry.Units
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return results
}

func findUnit(units []Unit, id string) *Unit {
	for i := range units {
		if units[i].ID == id {
			return &units[i]
		}
	}
	return nil
}

func TestExtractFunction(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"main.go": `package main

func add(a, b int) int {
	return a + b
}

func main() {
	add(1, 2)
}
`,
	})

	units := results["main.go"]
	if len(units) == 0 {
		t.Fatal("expected units for main.go")
	}

	add := findUnit(units, "main.go::add")
	if add == nil {
		t.Fatal("expected unit main.go::add")
	}
	if add.Kind != "Function" {
		t.Errorf("expected Function, got %s", add.Kind)
	}
	if add.Params != 2 {
		t.Errorf("expected 2 params, got %d", add.Params)
	}

	main := findUnit(units, "main.go::main")
	if main == nil {
		t.Fatal("expected unit main.go::main")
	}
	if len(main.Calls) == 0 {
		t.Error("expected main to have calls")
	}
}

func TestExtractStructAndMethod(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"server.go": `package main

type Server struct {
	port int
	host string
}

func (s *Server) Start() error {
	return nil
}

func (s *Server) Stop() {
	s.port = 0
}
`,
	})

	units := results["server.go"]
	if len(units) == 0 {
		t.Fatal("expected units for server.go")
	}

	srv := findUnit(units, "server.go::Server")
	if srv == nil {
		t.Fatal("expected unit server.go::Server")
	}
	if srv.Kind != "Struct" {
		t.Errorf("expected Struct, got %s", srv.Kind)
	}

	start := findUnit(units, "server.go::Server::Start")
	if start == nil {
		t.Fatal("expected unit server.go::Server::Start")
	}
	if start.Kind != "Method" {
		t.Errorf("expected Method, got %s", start.Kind)
	}
	if start.Parent == nil || *start.Parent != "server.go::Server" {
		t.Errorf("expected parent server.go::Server, got %v", start.Parent)
	}

	stop := findUnit(units, "server.go::Server::Stop")
	if stop == nil {
		t.Fatal("expected unit server.go::Server::Stop")
	}
	// Stop writes s.port = 0
	if len(stop.Writes) == 0 {
		t.Error("expected Stop to have writes")
	}
}

func TestExtractInterface(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"iface.go": `package main

type Reader interface {
	Read(p []byte) (n int, err error)
}
`,
	})

	units := results["iface.go"]
	reader := findUnit(units, "iface.go::Reader")
	if reader == nil {
		t.Fatal("expected unit iface.go::Reader")
	}
	if reader.Kind != "Struct" {
		t.Errorf("expected Struct (interfaces map to Struct), got %s", reader.Kind)
	}
}

func TestBranches(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"branchy.go": `package main

func branchy(x int) string {
	if x > 0 {
		return "positive"
	} else if x < 0 {
		return "negative"
	}

	for i := 0; i < 10; i++ {
		if i%2 == 0 {
			continue
		}
	}

	switch x {
	case 1:
		return "one"
	case 2:
		return "two"
	case 3:
		return "three"
	}

	return "zero"
}
`,
	})

	units := results["branchy.go"]
	fn := findUnit(units, "branchy.go::branchy")
	if fn == nil {
		t.Fatal("expected unit branchy.go::branchy")
	}
	// if + for + nested if + switch(3 cases - 1 = 2)
	if fn.Branches < 4 {
		t.Errorf("expected at least 4 branches, got %d", fn.Branches)
	}
}

func TestCognitiveComplexity(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"complex.go": `package main

func complex(items []int) int {
	total := 0
	for _, item := range items {
		if item > 0 {
			if item > 100 {
				total += item * 2
			} else {
				total += item
			}
		}
	}
	return total
}
`,
	})

	units := results["complex.go"]
	fn := findUnit(units, "complex.go::complex")
	if fn == nil {
		t.Fatal("expected unit complex.go::complex")
	}
	// for: +1(0), if: +1+1(1), if: +1+2(2), else: +1 = 6
	if fn.CognitiveComplexity < 5 {
		t.Errorf("expected cognitive >= 5, got %d", fn.CognitiveComplexity)
	}
}

func TestFieldAccess(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"fields.go": `package main

type Cache struct {
	data  map[string]string
	count int
}

func (c *Cache) Set(key, val string) {
	c.data[key] = val
	c.count++
}

func (c *Cache) Get(key string) string {
	return c.data[key]
}
`,
	})

	units := results["fields.go"]

	set := findUnit(units, "fields.go::Cache::Set")
	if set == nil {
		t.Fatal("expected unit fields.go::Cache::Set")
	}
	if len(set.Writes) == 0 {
		t.Error("Set should have writes (c.count++)")
	}

	get := findUnit(units, "fields.go::Cache::Get")
	if get == nil {
		t.Fatal("expected unit fields.go::Cache::Get")
	}
	if len(get.Reads) == 0 {
		t.Error("Get should have reads (c.data)")
	}
}

func TestMaxScopeLines(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"scope.go": `package main

func scoped(x int) {
	if x > 0 {
		a := 1
		b := 2
		c := 3
		_ = a + b + c
	}
	y := x
	_ = y
}
`,
	})

	units := results["scope.go"]
	fn := findUnit(units, "scope.go::scoped")
	if fn == nil {
		t.Fatal("expected unit scope.go::scoped")
	}
	// The if block spans ~5 lines
	if fn.MaxScopeLines < 4 {
		t.Errorf("expected max_scope_lines >= 4, got %d", fn.MaxScopeLines)
	}
}

func TestInitDisambiguation(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"inits.go": `package main

func init() {
	println("first")
}

func init() {
	println("second")
}
`,
	})

	units := results["inits.go"]
	init0 := findUnit(units, "inits.go::init")
	init1 := findUnit(units, "inits.go::init_1")
	if init0 == nil {
		t.Error("expected unit inits.go::init")
	}
	if init1 == nil {
		t.Error("expected unit inits.go::init_1")
	}
}

func TestTestFilesExcluded(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"lib.go": `package main

func Lib() {}
`,
		"lib_test.go": `package main

import "testing"

func TestLib(t *testing.T) {}
`,
	})

	if _, ok := results["lib_test.go"]; ok {
		t.Error("test files should be excluded")
	}
	if _, ok := results["lib.go"]; !ok {
		t.Error("lib.go should be present")
	}
}

func TestGeneratedFileExcluded(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"real.go": `package main

func Real() {}
`,
		"gen.pb.go": `package main

func Generated() {}
`,
	})

	if _, ok := results["gen.pb.go"]; ok {
		t.Error(".pb.go files should be excluded")
	}
}

func TestLogicalOperatorBranches(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"logic.go": `package main

func logic(a, b, c bool) bool {
	if a && b || c {
		return true
	}
	return false
}
`,
	})

	units := results["logic.go"]
	fn := findUnit(units, "logic.go::logic")
	if fn == nil {
		t.Fatal("expected unit logic.go::logic")
	}
	// if + && + ||
	if fn.Branches < 3 {
		t.Errorf("expected at least 3 branches, got %d", fn.Branches)
	}
}

func TestCallResolution(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"caller.go": `package main

import "fmt"

func helper() {}

func caller() {
	helper()
	fmt.Println("hello")
}
`,
	})

	units := results["caller.go"]
	fn := findUnit(units, "caller.go::caller")
	if fn == nil {
		t.Fatal("expected unit caller.go::caller")
	}
	if len(fn.Calls) < 2 {
		t.Errorf("expected at least 2 calls, got %d: %v", len(fn.Calls), fn.Calls)
	}
}

func TestDeferAndGoStmtCalls(t *testing.T) {
	results := setupTestProject(t, map[string]string{
		"async.go": `package main

func cleanup() {}
func work() {}

func asyncFunc() {
	defer cleanup()
	go work()
}
`,
	})

	units := results["async.go"]
	fn := findUnit(units, "async.go::asyncFunc")
	if fn == nil {
		t.Fatal("expected unit async.go::asyncFunc")
	}
	if len(fn.Calls) < 2 {
		t.Errorf("expected at least 2 calls (defer+go), got %d: %v", len(fn.Calls), fn.Calls)
	}
}
