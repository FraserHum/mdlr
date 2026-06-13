using System.Text.Json;
using System.Text.Json.Serialization;

namespace MdlrExtractCSharp;

/// Matches the Rust FileCacheEntry format (crates/mdlr-core/src/lib.rs).
public sealed class FileCacheEntry
{
    [JsonPropertyName("source_path")] public required string SourcePath { get; init; }
    [JsonPropertyName("units")] public required List<Unit> Units { get; init; }
    [JsonPropertyName("cached_at")] public required ulong CachedAt { get; init; }
}

/// Matches the Rust Unit struct (crates/mdlr-core/src/graph/types.rs).
public sealed class Unit
{
    [JsonPropertyName("id")] public required string Id { get; set; }
    [JsonPropertyName("kind")] public required string Kind { get; init; }
    [JsonPropertyName("file")] public required string File { get; init; }
    [JsonPropertyName("span")] public required Span Span { get; init; }
    [JsonPropertyName("reads")] public List<string> Reads { get; init; } = [];
    [JsonPropertyName("writes")] public List<string> Writes { get; init; } = [];
    [JsonPropertyName("calls")] public List<string> Calls { get; init; } = [];
    [JsonPropertyName("tags")] public List<string> Tags { get; init; } = [];
    [JsonPropertyName("params")] public int Params { get; init; }
    [JsonPropertyName("branches")] public int Branches { get; init; }
    [JsonPropertyName("max_scope_lines")] public int MaxScopeLines { get; init; }

    [JsonPropertyName("parent")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? Parent { get; set; }

    [JsonPropertyName("cognitive_complexity")] public int CognitiveComplexity { get; init; }

    // Rust side: skip_serializing_if Not — omitted when false.
    [JsonPropertyName("partial")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)]
    public bool Partial { get; set; }
}

public sealed class Span
{
    [JsonPropertyName("start_line")] public int StartLine { get; init; }
    [JsonPropertyName("start_col")] public int StartCol { get; init; }
    [JsonPropertyName("end_line")] public int EndLine { get; init; }
    [JsonPropertyName("end_col")] public int EndCol { get; init; }
}

public static class Json
{
    public static readonly JsonSerializerOptions Options = new()
    {
        WriteIndented = true,
        Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };
}
