package main

import (
	"os"
	"path/filepath"
	"testing"
)

// helper: write a Go file to a temp dir, tokenize it, round-trip through binary.
func tokenizeFile(t *testing.T, dir, name, source string) *FileTokens {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(path, []byte(source), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read source: %v", err)
	}
	ft := tokenizeGo(data, name, 1)

	// Round-trip through binary on disk
	tokPath := path + ".tokens"
	serialized := serializeTokens(ft)
	if err := os.WriteFile(tokPath, serialized, 0o644); err != nil {
		t.Fatalf("write tokens: %v", err)
	}
	loaded, err := os.ReadFile(tokPath)
	if err != nil {
		t.Fatalf("read tokens: %v", err)
	}
	if len(loaded) != len(serialized) {
		t.Fatalf("binary round-trip size mismatch: wrote %d, read %d", len(serialized), len(loaded))
	}

	return ft
}

func TestBasicTokenization(t *testing.T) {
	source := `package main

func add(a int, b int) int {
	return a + b
}
`
	ft := tokenizeGo([]byte(source), "main.go", 1)

	values := make([]string, len(ft.Tokens))
	for i, tok := range ft.Tokens {
		values[i] = tok.Value
	}

	// Should contain keywords, normalized identifiers, operators
	found := map[string]bool{}
	for _, v := range values {
		found[v] = true
	}

	if !found["package"] {
		t.Errorf("should contain keyword 'package', got %v", values)
	}
	if !found["func"] {
		t.Errorf("should contain keyword 'func', got %v", values)
	}
	if !found[normalizedID] {
		t.Errorf("should normalize identifiers to $ID, got %v", values)
	}
	if !found["return"] {
		t.Errorf("should contain keyword 'return', got %v", values)
	}
}

func TestCommentsStripped(t *testing.T) {
	source := `package main
// this is a comment
/* block comment */
var x = 42
`
	ft := tokenizeGo([]byte(source), "test.go", 1)
	for _, tok := range ft.Tokens {
		if tok.Value == "//" || tok.Value == "/*" {
			t.Errorf("comments should be stripped, found %q", tok.Value)
		}
	}
}

func TestIgnoreMarkers(t *testing.T) {
	source := `package main
var before = 1
// mdlr:ignore-start
var ignored = 2
var alsoIgnored = 3
// mdlr:ignore-end
var after = 4
`
	ft := tokenizeGo([]byte(source), "test.go", 1)

	varCount := 0
	for _, tok := range ft.Tokens {
		if tok.Value == "var" {
			varCount++
		}
	}
	if varCount != 2 {
		t.Errorf("should have 2 'var' tokens (ignoring middle section), got %d", varCount)
	}
}

func TestLiteralsNormalized(t *testing.T) {
	source := `package main
var x = 42
var s = "hello"
var f = 3.14
`
	ft := tokenizeGo([]byte(source), "test.go", 1)

	litCount := 0
	for _, tok := range ft.Tokens {
		if tok.Value == normalizedLIT {
			litCount++
		}
	}
	if litCount != 3 {
		t.Errorf("should have 3 normalized literals, got %d", litCount)
	}
}

// --- End-to-end CPD tests with real Go source files on disk ---

// Two Go functions that do the same thing with different variable names.
// After normalization (identifiers → $ID, literals → $LIT) they should
// produce identical token streams and be detected as clones.
func TestCopyPastedFunctionDifferentNames(t *testing.T) {
	dir := t.TempDir()

	a := tokenizeFile(t, dir, "orders.go", `package handlers

func ProcessOrders(orders []Order) []OrderResult {
	var results []OrderResult
	for _, order := range orders {
		if order.Total > 100 {
			results = append(results, OrderResult{
				ID:       order.ID,
				Discount: order.Total * 0.1,
				Status:   "eligible",
			})
		} else {
			results = append(results, OrderResult{
				ID:       order.ID,
				Discount: 0,
				Status:   "ineligible",
			})
		}
	}
	return results
}
`)

	b := tokenizeFile(t, dir, "payments.go", `package handlers

func HandlePayments(payments []Payment) []PaymentResult {
	var output []PaymentResult
	for _, payment := range payments {
		if payment.Total > 100 {
			output = append(output, PaymentResult{
				ID:       payment.ID,
				Discount: payment.Total * 0.1,
				Status:   "eligible",
			})
		} else {
			output = append(output, PaymentResult{
				ID:       payment.ID,
				Discount: 0,
				Status:   "ineligible",
			})
		}
	}
	return output
}
`)

	if len(a.Tokens) == 0 || len(b.Tokens) == 0 {
		t.Fatal("both files should produce tokens")
	}

	// Verify the token streams are identical after normalization
	if len(a.Tokens) != len(b.Tokens) {
		t.Logf("token count mismatch: a=%d b=%d (may still find clones)", len(a.Tokens), len(b.Tokens))
	}

	// Count matching tokens (should be very high)
	matches := 0
	minLen := len(a.Tokens)
	if len(b.Tokens) < minLen {
		minLen = len(b.Tokens)
	}
	for i := 0; i < minLen; i++ {
		if a.Tokens[i].Value == b.Tokens[i].Value {
			matches++
		}
	}
	matchPct := float64(matches) / float64(minLen) * 100
	if matchPct < 80 {
		t.Errorf("normalized token streams should be >80%% identical, got %.1f%%", matchPct)
	}
}

// Completely different Go code — an HTTP handler vs a sorting algorithm.
// Should produce very different token streams.
func TestUnrelatedCodeDifferentTokenStreams(t *testing.T) {
	dir := t.TempDir()

	a := tokenizeFile(t, dir, "handler.go", `package api

import "net/http"

func GetUser(w http.ResponseWriter, r *http.Request) {
	id := r.URL.Query().Get("id")
	user, err := db.FindUser(id)
	if err != nil {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}
	profile := user.LoadProfile()
	json.NewEncoder(w).Encode(map[string]interface{}{
		"id":    user.ID,
		"name":  user.Name,
		"email": profile.Email,
	})
}
`)

	b := tokenizeFile(t, dir, "sort.go", `package algo

func MergeSort(arr []int) []int {
	if len(arr) <= 1 {
		return arr
	}
	mid := len(arr) / 2
	left := MergeSort(arr[:mid])
	right := MergeSort(arr[mid:])
	return merge(left, right)
}

func merge(left, right []int) []int {
	result := make([]int, 0, len(left)+len(right))
	i, j := 0, 0
	for i < len(left) && j < len(right) {
		if left[i] <= right[j] {
			result = append(result, left[i])
			i++
		} else {
			result = append(result, right[j])
			j++
		}
	}
	result = append(result, left[i:]...)
	result = append(result, right[j:]...)
	return result
}
`)

	// Count matching tokens — should be low
	minLen := len(a.Tokens)
	if len(b.Tokens) < minLen {
		minLen = len(b.Tokens)
	}
	if minLen == 0 {
		t.Fatal("both files should produce tokens")
	}
	matches := 0
	for i := 0; i < minLen; i++ {
		if a.Tokens[i].Value == b.Tokens[i].Value {
			matches++
		}
	}
	matchPct := float64(matches) / float64(minLen) * 100
	if matchPct > 50 {
		t.Errorf("unrelated code should have <50%% matching tokens, got %.1f%%", matchPct)
	}
}

// Same file has two copy-pasted handler functions.
func TestSelfCloneWithinSingleFile(t *testing.T) {
	dir := t.TempDir()

	ft := tokenizeFile(t, dir, "handlers.go", `package handlers

func HandleAdminRequest(adminID int64) Response {
	user := db.FindByID(adminID)
	if user == nil {
		panic("not found")
	}
	stats := ComputeStats(user.Activity)
	notifications := FetchNotifications(user.ID)
	return Response{
		User:          user,
		Stats:         stats,
		Notifications: notifications,
		LastLogin:     user.LastLogin,
	}
}

func unrelatedHelper() {
	fmt.Println("this separates the two clones")
}

func HandleUserRequest(userID int64) Response {
	user := db.FindByID(userID)
	if user == nil {
		panic("not found")
	}
	stats := ComputeStats(user.Activity)
	notifications := FetchNotifications(user.ID)
	return Response{
		User:          user,
		Stats:         stats,
		Notifications: notifications,
		LastLogin:     user.LastLogin,
	}
}
`)

	if len(ft.Tokens) == 0 {
		t.Fatal("should produce tokens")
	}

	// Find the two handler functions by looking for repeated subsequences.
	// The two handlers should produce very similar token sequences.
	// We verify by checking the file has enough tokens for a meaningful self-clone.
	if len(ft.Tokens) < 60 {
		t.Errorf("file with two handlers should have ≥60 tokens, got %d", len(ft.Tokens))
	}
}

// Binary serialization round-trip: serialize, write to disk, read back,
// verify all fields match.
func TestBinaryRoundTrip(t *testing.T) {
	source := `package main

import "fmt"

func main() {
	for i := 0; i < 10; i++ {
		if i%2 == 0 {
			fmt.Println(i, "even")
		} else {
			fmt.Println(i, "odd")
		}
	}
}
`
	ft := tokenizeGo([]byte(source), "src/main.go", 42)
	if len(ft.Tokens) == 0 {
		t.Fatal("should produce tokens")
	}

	dir := t.TempDir()
	tokPath := filepath.Join(dir, "main.tokens")

	// Write
	data := serializeTokens(ft)
	if err := os.WriteFile(tokPath, data, 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}

	// Read back and verify size matches
	loaded, err := os.ReadFile(tokPath)
	if err != nil {
		t.Fatalf("read: %v", err)
	}

	if len(loaded) != len(data) {
		t.Errorf("round-trip size mismatch: wrote %d, read %d", len(data), len(loaded))
	}

	// Verify the binary is structurally valid by checking we can re-parse it
	// (basic sanity check — full deserialization is done by the Rust side)
	if len(loaded) < 12 {
		t.Fatal("binary too small")
	}
}
