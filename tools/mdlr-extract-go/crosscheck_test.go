package main

import (
	"encoding/binary"
	"os"
	"path/filepath"
	"testing"
)

// TestBinaryFormatCrossCheck verifies the binary format written by Go
// can be parsed field-by-field, matching the Rust mdlr-cpd deserializer's
// expectations. This is a structural validation without needing the Rust
// binary.
func TestBinaryFormatCrossCheck(t *testing.T) {
	source := `package main

func hello() string {
	return "world"
}
`
	ft := tokenizeGo([]byte(source), "src/main.go", 42)
	data := serializeTokens(ft)

	dir := t.TempDir()
	tokPath := filepath.Join(dir, "main.tokens")
	if err := os.WriteFile(tokPath, data, 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}

	// Parse the binary manually to validate structure
	loaded, err := os.ReadFile(tokPath)
	if err != nil {
		t.Fatalf("read: %v", err)
	}

	pos := 0

	// 1. Read source_path: u32 length + bytes
	if len(loaded) < pos+4 {
		t.Fatal("truncated: path length")
	}
	pathLen := binary.LittleEndian.Uint32(loaded[pos : pos+4])
	pos += 4
	if len(loaded) < pos+int(pathLen) {
		t.Fatal("truncated: path bytes")
	}
	path := string(loaded[pos : pos+int(pathLen)])
	pos += int(pathLen)
	if path != "src/main.go" {
		t.Errorf("path = %q, want %q", path, "src/main.go")
	}

	// 2. Read cached_at: u64
	if len(loaded) < pos+8 {
		t.Fatal("truncated: cached_at")
	}
	cachedAt := binary.LittleEndian.Uint64(loaded[pos : pos+8])
	pos += 8
	if cachedAt != 42 {
		t.Errorf("cached_at = %d, want 42", cachedAt)
	}

	// 3. Read string table: u32 count, then for each: u16 len + bytes
	if len(loaded) < pos+4 {
		t.Fatal("truncated: string count")
	}
	stringCount := binary.LittleEndian.Uint32(loaded[pos : pos+4])
	pos += 4
	if stringCount == 0 {
		t.Error("string table should not be empty")
	}

	strings := make([]string, stringCount)
	for i := 0; i < int(stringCount); i++ {
		if len(loaded) < pos+2 {
			t.Fatalf("truncated: string[%d] length", i)
		}
		sLen := binary.LittleEndian.Uint16(loaded[pos : pos+2])
		pos += 2
		if len(loaded) < pos+int(sLen) {
			t.Fatalf("truncated: string[%d] bytes", i)
		}
		strings[i] = string(loaded[pos : pos+int(sLen)])
		pos += int(sLen)
	}

	// Verify string table contains expected values
	foundID := false
	foundFunc := false
	for _, s := range strings {
		if s == "$ID" {
			foundID = true
		}
		if s == "func" {
			foundFunc = true
		}
	}
	if !foundID {
		t.Errorf("string table should contain $ID, got %v", strings)
	}
	if !foundFunc {
		t.Errorf("string table should contain 'func', got %v", strings)
	}

	// 4. Read tokens: u32 count, then for each: u16 string_index + u32 line + u16 col
	if len(loaded) < pos+4 {
		t.Fatal("truncated: token count")
	}
	tokenCount := binary.LittleEndian.Uint32(loaded[pos : pos+4])
	pos += 4
	if tokenCount == 0 {
		t.Error("should have tokens")
	}
	if int(tokenCount) != len(ft.Tokens) {
		t.Errorf("token count = %d, want %d", tokenCount, len(ft.Tokens))
	}

	for i := 0; i < int(tokenCount); i++ {
		if len(loaded) < pos+8 {
			t.Fatalf("truncated: token[%d]", i)
		}
		strIdx := binary.LittleEndian.Uint16(loaded[pos : pos+2])
		pos += 2
		line := binary.LittleEndian.Uint32(loaded[pos : pos+4])
		pos += 4
		col := binary.LittleEndian.Uint16(loaded[pos : pos+2])
		pos += 2

		if int(strIdx) >= len(strings) {
			t.Fatalf("token[%d] string index %d out of range (table has %d)", i, strIdx, len(strings))
		}
		_ = line
		_ = col
	}

	if pos != len(loaded) {
		t.Errorf("trailing bytes: consumed %d of %d", pos, len(loaded))
	}
}
