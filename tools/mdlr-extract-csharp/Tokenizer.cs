using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;

namespace MdlrExtractCSharp;

public readonly record struct CpdToken(string Value, uint Line, ushort Col);

public sealed class FileTokens
{
    public required string SourcePath { get; init; }
    public required List<CpdToken> Tokens { get; init; }
    public required ulong CachedAt { get; init; }
}

/// CPD tokenizer matching the Go extractor's normalization:
/// identifiers -> "$ID", literals -> "$LIT", keywords/operators kept as-is,
/// comments/whitespace stripped, mdlr:ignore-start/end markers respected.
public static class Tokenizer
{
    const string NormalizedId = "$ID";
    const string NormalizedLit = "$LIT";

    public static FileTokens Tokenize(SourceText text, SyntaxNode root, string relPath, ulong generationId)
    {
        var tokens = new List<CpdToken>();
        var ignoring = false;

        foreach (var token in root.DescendantTokens(descendIntoTrivia: false))
        {
            if (token.RawKind == (int)SyntaxKind.EndOfFileToken) break;

            foreach (var trivia in token.LeadingTrivia)
                ScanIgnoreMarker(trivia, ref ignoring);

            if (!ignoring && token.Span.Length > 0)
            {
                var pos = text.Lines.GetLinePosition(token.SpanStart);
                tokens.Add(new CpdToken(Normalize(token), (uint)(pos.Line + 1), (ushort)pos.Character));
            }

            foreach (var trivia in token.TrailingTrivia)
                ScanIgnoreMarker(trivia, ref ignoring);
        }

        return new FileTokens { SourcePath = relPath, Tokens = tokens, CachedAt = generationId };
    }

    static void ScanIgnoreMarker(SyntaxTrivia trivia, ref bool ignoring)
    {
        if (!trivia.IsKind(SyntaxKind.SingleLineCommentTrivia)
            && !trivia.IsKind(SyntaxKind.MultiLineCommentTrivia)
            && !trivia.IsKind(SyntaxKind.SingleLineDocumentationCommentTrivia)
            && !trivia.IsKind(SyntaxKind.MultiLineDocumentationCommentTrivia))
            return;
        var s = trivia.ToFullString();
        if (s.Contains("mdlr:ignore-start")) ignoring = true;
        else if (s.Contains("mdlr:ignore-end")) ignoring = false;
    }

    static string Normalize(SyntaxToken token)
    {
        var kind = token.Kind();
        if (kind == SyntaxKind.IdentifierToken)
            return NormalizedId;
        switch (kind)
        {
            case SyntaxKind.NumericLiteralToken:
            case SyntaxKind.StringLiteralToken:
            case SyntaxKind.CharacterLiteralToken:
            case SyntaxKind.InterpolatedStringTextToken:
            case SyntaxKind.SingleLineRawStringLiteralToken:
            case SyntaxKind.MultiLineRawStringLiteralToken:
            case SyntaxKind.Utf8StringLiteralToken:
            case SyntaxKind.Utf8SingleLineRawStringLiteralToken:
            case SyntaxKind.Utf8MultiLineRawStringLiteralToken:
                return NormalizedLit;
            default:
                return token.Text;
        }
    }

    /// Serialize to the compact binary format read by mdlr-cpd::binary::deserialize.
    /// Layout (little-endian):
    ///   path_len: u32, path_bytes; cached_at: u64;
    ///   string_count: u32, each { len: u16, bytes };
    ///   token_count: u32, each { string_index: u16, line: u32, col: u16 }.
    public static byte[] Serialize(FileTokens ft)
    {
        var stringTable = new List<string>();
        var stringIndex = new Dictionary<string, ushort>();
        foreach (var tok in ft.Tokens)
        {
            if (!stringIndex.ContainsKey(tok.Value))
            {
                stringIndex[tok.Value] = (ushort)stringTable.Count;
                stringTable.Add(tok.Value);
            }
        }

        using var ms = new MemoryStream();
        using var w = new BinaryWriter(ms); // BinaryWriter is little-endian

        var pathBytes = System.Text.Encoding.UTF8.GetBytes(ft.SourcePath);
        w.Write((uint)pathBytes.Length);
        w.Write(pathBytes);
        w.Write(ft.CachedAt);

        w.Write((uint)stringTable.Count);
        foreach (var s in stringTable)
        {
            var b = System.Text.Encoding.UTF8.GetBytes(s);
            w.Write((ushort)b.Length);
            w.Write(b);
        }

        w.Write((uint)ft.Tokens.Count);
        foreach (var tok in ft.Tokens)
        {
            w.Write(stringIndex[tok.Value]);
            w.Write(tok.Line);
            w.Write(tok.Col);
        }

        w.Flush();
        return ms.ToArray();
    }
}
