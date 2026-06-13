using System.Text.Json;
using Microsoft.Build.Locator;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.MSBuild;
using Microsoft.CodeAnalysis.Text;

namespace MdlrExtractCSharp;

public static class Program
{
    static readonly string[] SkipDirs = ["bin", "obj", "node_modules", ".git", ".mdlr"];

    public static async Task<int> Main(string[] args)
    {
        string root = ".", output = "";
        ulong generationId = 0;
        for (var i = 0; i < args.Length - 1; i++)
        {
            switch (args[i])
            {
                case "--root": root = args[++i]; break;
                case "--output": output = args[++i]; break;
                case "--generation-id": generationId = ulong.Parse(args[++i]); break;
            }
        }
        if (output.Length == 0)
        {
            Console.Error.WriteLine("mdlr-extract-csharp: --output is required");
            return 1;
        }
        root = Path.GetFullPath(root);
        if (generationId == 0)
            generationId = (ulong)DateTimeOffset.UtcNow.ToUnixTimeSeconds();

        try
        {
            return await Run(root, output, generationId);
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"mdlr-extract-csharp: {e.Message}");
            return 1;
        }
    }

    public static async Task<int> Run(string root, string outputDir, ulong generationId)
    {
        // relPath -> units; files extracted semantically are recorded here so
        // the syntax-only sweep at the end picks up only what's left over.
        var results = new Dictionary<string, List<Unit>>();

        var msbuildAvailable = RegisterMsBuild();
        if (msbuildAvailable)
            await ExtractSemantic(root, results);

        // Syntax-only fallback for every analyzable .cs file not covered by a
        // loadable project (or everything, when no .NET SDK is available).
        foreach (var file in FindFiles(root, ".cs"))
        {
            var relPath = RelPath(root, file);
            if (results.ContainsKey(relPath)) continue;
            ExtractSyntaxOnly(file, relPath, results);
        }

        foreach (var (relPath, units) in results)
        {
            if (units.Count == 0) continue;
            WriteEntry(outputDir, root, relPath, units, generationId);
        }
        return 0;
    }

    static bool RegisterMsBuild()
    {
        try
        {
            if (MSBuildLocator.IsRegistered) return true;
            var instances = MSBuildLocator.QueryVisualStudioInstances().ToList();
            if (instances.Count == 0) return false;
            MSBuildLocator.RegisterInstance(instances.OrderByDescending(i => i.Version).First());
            return true;
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"warning: MSBuild registration failed, using syntax-only extraction: {e.Message}");
            return false;
        }
    }

    static async Task ExtractSemantic(string root, Dictionary<string, List<Unit>> results)
    {
        using var workspace = MSBuildWorkspace.Create();
        workspace.WorkspaceFailed += (_, e) =>
        {
            if (e.Diagnostic.Kind == WorkspaceDiagnosticKind.Failure)
                Console.Error.WriteLine($"warning: {e.Diagnostic.Message}");
        };

        var loadedProjects = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var solutions = FindFiles(root, ".sln").Concat(FindFiles(root, ".slnx"));
        foreach (var solutionPath in solutions)
        {
            try
            {
                var solution = await workspace.OpenSolutionAsync(solutionPath);
                foreach (var project in solution.Projects)
                {
                    if (project.FilePath is { } fp) loadedProjects.Add(Path.GetFullPath(fp));
                    await ExtractProject(project, root, results);
                }
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"warning: failed to load {solutionPath}: {e.Message}");
            }
        }

        foreach (var projectPath in FindFiles(root, ".csproj"))
        {
            if (loadedProjects.Contains(Path.GetFullPath(projectPath))) continue;
            try
            {
                var project = await workspace.OpenProjectAsync(projectPath);
                loadedProjects.Add(Path.GetFullPath(projectPath));
                await ExtractProject(project, root, results);
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"warning: failed to load {projectPath}: {e.Message}");
            }
        }
    }

    static async Task ExtractProject(Project project, string root, Dictionary<string, List<Unit>> results)
    {
        var compilation = await project.GetCompilationAsync();
        if (compilation is null) return;

        foreach (var tree in compilation.SyntaxTrees)
        {
            var path = tree.FilePath;
            if (string.IsNullOrEmpty(path) || !File.Exists(path)) continue;
            var rel = Path.GetRelativePath(root, path);
            if (rel.StartsWith("..", StringComparison.Ordinal) || Path.IsPathRooted(rel)) continue;
            var relPath = rel.Replace('\\', '/');
            if (results.ContainsKey(relPath)) continue; // multi-targeting / shared files
            if (IsExcludedPath(relPath)) continue;

            if (await tree.GetRootAsync() is not CompilationUnitSyntax syntaxRoot) continue;
            if (IsGenerated(syntaxRoot)) continue;

            var model = compilation.GetSemanticModel(tree);
            var text = await tree.GetTextAsync();
            results[relPath] = UnitExtractor.Extract(syntaxRoot, model, text, relPath, root);
        }
    }

    static void ExtractSyntaxOnly(string file, string relPath, Dictionary<string, List<Unit>> results)
    {
        try
        {
            var text = SourceText.From(File.ReadAllText(file));
            var tree = CSharpSyntaxTree.ParseText(text, path: file);
            if (tree.GetRoot() is not CompilationUnitSyntax syntaxRoot) return;
            if (IsGenerated(syntaxRoot)) return;
            results[relPath] = UnitExtractor.Extract(syntaxRoot, null, text, relPath, "");
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"warning: failed to parse {relPath}: {e.Message}");
        }
    }

    /// Generated-file detection: conventional suffixes plus the
    /// `<auto-generated>` header comment.
    static bool IsGenerated(CompilationUnitSyntax root)
    {
        var path = root.SyntaxTree.FilePath;
        var name = Path.GetFileName(path);
        if (name.EndsWith(".g.cs") || name.EndsWith(".g.i.cs")
            || name.EndsWith(".generated.cs") || name.EndsWith(".Designer.cs"))
            return true;

        var firstToken = root.GetFirstToken(includeZeroWidth: true);
        foreach (var trivia in firstToken.LeadingTrivia)
        {
            if (!trivia.IsKind(SyntaxKind.SingleLineCommentTrivia)
                && !trivia.IsKind(SyntaxKind.MultiLineCommentTrivia))
                continue;
            var s = trivia.ToFullString();
            if (s.Contains("<auto-generated") || (s.Contains("Code generated") && s.Contains("DO NOT EDIT")))
                return true;
        }
        return false;
    }

    static bool IsExcludedPath(string relPath)
    {
        foreach (var segment in relPath.Split('/'))
            if (SkipDirs.Contains(segment, StringComparer.OrdinalIgnoreCase) || segment.StartsWith('.'))
                return true;
        return false;
    }

    static IEnumerable<string> FindFiles(string root, string extension)
    {
        var stack = new Stack<string>();
        stack.Push(root);
        while (stack.Count > 0)
        {
            var dir = stack.Pop();
            IEnumerable<string> entries;
            try
            {
                entries = Directory.EnumerateFileSystemEntries(dir);
            }
            catch (Exception)
            {
                continue;
            }
            foreach (var entry in entries)
            {
                var name = Path.GetFileName(entry);
                if (Directory.Exists(entry))
                {
                    if (name.StartsWith('.') || SkipDirs.Contains(name, StringComparer.OrdinalIgnoreCase))
                        continue;
                    stack.Push(entry);
                }
                else if (entry.EndsWith(extension, StringComparison.OrdinalIgnoreCase))
                {
                    yield return entry;
                }
            }
        }
    }

    static string RelPath(string root, string file) =>
        Path.GetRelativePath(root, file).Replace('\\', '/');

    static void WriteEntry(string outputDir, string root, string relPath, List<Unit> units, ulong generationId)
    {
        var entry = new FileCacheEntry
        {
            SourcePath = relPath,
            Units = units,
            CachedAt = generationId,
        };

        var outFile = Path.Combine(outputDir, relPath) + ".json";
        Directory.CreateDirectory(Path.GetDirectoryName(outFile)!);
        File.WriteAllText(outFile, JsonSerializer.Serialize(entry, Json.Options));

        try
        {
            var source = SourceText.From(File.ReadAllText(Path.Combine(root, relPath)));
            var tree = CSharpSyntaxTree.ParseText(source);
            var tokens = Tokenizer.Tokenize(source, tree.GetRoot(), relPath, generationId);
            File.WriteAllBytes(Path.Combine(outputDir, relPath) + ".tokens", Tokenizer.Serialize(tokens));
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"warning: tokenize {relPath}: {e.Message}");
        }
    }
}
