package main

import (
	"encoding/binary"
	"go/scanner"
	"go/token"
	"os"
	"path/filepath"
	"strings"
)

const (
	normalizedID  = "$ID"
	normalizedLIT = "$LIT"
)

// CpdToken is a single normalized token with source location.
type CpdToken struct {
	Value string
	Line  uint32
	Col   uint16
}

// FileTokens holds all CPD tokens for a single source file.
type FileTokens struct {
	SourcePath string
	Tokens     []CpdToken
	CachedAt   uint64
}

// tokenizeGo tokenizes a Go source file for CPD analysis.
//
//   - Strips comments and whitespace
//   - Normalizes identifiers to $ID and literals to $LIT
//   - Respects mdlr:ignore-start / mdlr:ignore-end markers in comments
func tokenizeGo(source []byte, sourcePath string, generationID uint64) *FileTokens {
	fset := token.NewFileSet()
	file := fset.AddFile(sourcePath, -1, len(source))

	var s scanner.Scanner
	s.Init(file, source, nil, scanner.ScanComments)

	var tokens []CpdToken
	ignoring := false

	for {
		pos, tok, lit := s.Scan()
		if tok == token.EOF {
			break
		}

		position := fset.Position(pos)

		// Handle comments — check for ignore markers, then skip
		if tok == token.COMMENT {
			if strings.Contains(lit, "mdlr:ignore-start") {
				ignoring = true
			} else if strings.Contains(lit, "mdlr:ignore-end") {
				ignoring = false
			}
			continue
		}

		// Skip semicolons (auto-inserted by Go scanner)
		if tok == token.SEMICOLON {
			continue
		}

		if ignoring {
			continue
		}

		var value string
		if tok.IsLiteral() && tok != token.IDENT {
			// INT, FLOAT, IMAG, CHAR, STRING
			value = normalizedLIT
		} else if tok == token.IDENT {
			value = normalizedID
		} else if tok.IsKeyword() {
			value = tok.String()
		} else if tok.IsOperator() {
			value = tok.String()
		} else {
			value = tok.String()
		}

		tokens = append(tokens, CpdToken{
			Value: value,
			Line:  uint32(position.Line),
			Col:   uint16(position.Column - 1), // Go columns are 1-based
		})
	}

	return &FileTokens{
		SourcePath: sourcePath,
		Tokens:     tokens,
		CachedAt:   generationID,
	}
}

// serializeTokens writes a FileTokens to the compact binary format
// compatible with mdlr-cpd's binary::deserialize.
//
// Layout (all little-endian):
//
//	path_len: u32, path_bytes: [u8; path_len]
//	cached_at: u64
//	string_count: u32
//	  for each: str_len: u16, str_bytes: [u8; str_len]
//	token_count: u32
//	  for each: string_index: u16, line: u32, col: u16
func serializeTokens(ft *FileTokens) []byte {
	// Build string table
	stringTable := []string{}
	stringIndex := map[string]uint16{}
	for _, tok := range ft.Tokens {
		if _, ok := stringIndex[tok.Value]; !ok {
			stringIndex[tok.Value] = uint16(len(stringTable))
			stringTable = append(stringTable, tok.Value)
		}
	}

	buf := make([]byte, 0, 1024)

	// Helper for little-endian writes
	appendU16 := func(v uint16) { b := make([]byte, 2); binary.LittleEndian.PutUint16(b, v); buf = append(buf, b...) }
	appendU32 := func(v uint32) { b := make([]byte, 4); binary.LittleEndian.PutUint32(b, v); buf = append(buf, b...) }
	appendU64 := func(v uint64) { b := make([]byte, 8); binary.LittleEndian.PutUint64(b, v); buf = append(buf, b...) }

	// Write source_path
	pathBytes := []byte(ft.SourcePath)
	appendU32(uint32(len(pathBytes)))
	buf = append(buf, pathBytes...)

	// Write cached_at
	appendU64(ft.CachedAt)

	// Write string table
	appendU32(uint32(len(stringTable)))
	for _, s := range stringTable {
		sb := []byte(s)
		appendU16(uint16(len(sb)))
		buf = append(buf, sb...)
	}

	// Write tokens
	appendU32(uint32(len(ft.Tokens)))
	for _, tok := range ft.Tokens {
		appendU16(stringIndex[tok.Value])
		appendU32(tok.Line)
		appendU16(tok.Col)
	}

	return buf
}

// writeTokenFile writes a .tokens binary file alongside the .json cache file.
func writeTokenFile(outputDir, relPath string, ft *FileTokens) error {
	outFile := filepath.Join(outputDir, relPath) + ".tokens"

	if err := os.MkdirAll(filepath.Dir(outFile), 0o755); err != nil {
		return err
	}

	data := serializeTokens(ft)
	return os.WriteFile(outFile, data, 0o644)
}
