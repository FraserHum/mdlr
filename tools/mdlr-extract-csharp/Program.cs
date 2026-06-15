using System.Text.Json;
using System.Xml.Linq;
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
    static readonly string[] TestPackageReferences =
    [
        "Microsoft.NET.Test.Sdk",
        "xunit",
        "xunit.v3",
        "NUnit",
        "NUnit3TestAdapter",
        "MSTest.TestFramework",
        "MSTest.TestAdapter",
    ];
    static readonly TimeSpan DefaultSemanticTimeout = TimeSpan.FromSeconds(15);

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
        // relPath -> units; physical files extracted semantically are recorded
        // here so the syntax-only sweep picks up only genuinely uncovered files.
        var results = new Dictionary<string, List<Unit>>();
        var extractedFiles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var projectFacts = ReadProjectFacts(root);
        var msbuildAvailable = RegisterMsBuild();
        var semanticTimeout = GetSemanticTimeout();
        if (msbuildAvailable && semanticTimeout > TimeSpan.Zero)
        {
            try
            {
                await ExtractSemantic(root, results, extractedFiles, projectFacts, semanticTimeout);
            }
            catch (OperationCanceledException)
            {
                Console.Error.WriteLine(
                    $"warning: C# semantic extraction exceeded {semanticTimeout.TotalSeconds:0.#}s, using syntax-only extraction for remaining files");
            }
        }
        else if (msbuildAvailable)
        {
            Console.Error.WriteLine("warning: C# semantic extraction disabled, using syntax-only extraction");
        }

        // Syntax-only fallback for every analyzable .cs file not covered by a
        // loadable project (or everything, when no .NET SDK is available).
        foreach (var file in FindFiles(root, ".cs"))
        {
            var relPath = RelPath(root, file);
            if (results.ContainsKey(relPath)) continue;
            if (!extractedFiles.Add(CanonicalPath(file))) continue;
            ExtractSyntaxOnly(file, relPath, results);
        }

        foreach (var (relPath, units) in results)
        {
            if (units.Count == 0) continue;
            WriteEntry(outputDir, root, relPath, units, generationId);
        }
        MarkExecutableReachability(projectFacts);
        WriteProjectFacts(outputDir, projectFacts, generationId);
        return results.Values.SelectMany(units => units).Any(unit => unit.Partial) ? 2 : 0;
    }

    static TimeSpan GetSemanticTimeout()
    {
        var value = Environment.GetEnvironmentVariable("MDLR_CSHARP_SEMANTIC_TIMEOUT_SECONDS");
        if (string.IsNullOrWhiteSpace(value)) return DefaultSemanticTimeout;
        if (double.TryParse(value, out var seconds) && seconds >= 0)
            return TimeSpan.FromSeconds(seconds);
        Console.Error.WriteLine(
            $"warning: invalid MDLR_CSHARP_SEMANTIC_TIMEOUT_SECONDS value '{value}', using {DefaultSemanticTimeout.TotalSeconds:0.#}s");
        return DefaultSemanticTimeout;
    }

    static bool RegisterMsBuild()
    {
        try
        {
            if (MSBuildLocator.IsRegistered) return true;
            MSBuildLocator.RegisterDefaults();
            return true;
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"warning: MSBuild registration failed, using syntax-only extraction: {e.Message}");
            return false;
        }
    }

    static async Task ExtractSemantic(
        string root,
        Dictionary<string, List<Unit>> results,
        HashSet<string> extractedFiles,
        Dictionary<string, CSharpProjectFacts> projectFacts,
        TimeSpan semanticTimeout)
    {
        using var cancellation = new CancellationTokenSource(semanticTimeout);
        var workspace = MSBuildWorkspace.Create();
        var disposeWorkspace = true;
        try
        {
        workspace.WorkspaceFailed += (_, e) =>
        {
            if (e.Diagnostic.Kind == WorkspaceDiagnosticKind.Failure)
                Console.Error.WriteLine($"warning: {e.Diagnostic.Message}");
        };

        var loadedProjects = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var solutionPath in FindFiles(root, ".sln"))
        {
            try
            {
                var solution = await AwaitSemantic(
                    () => workspace.OpenSolutionAsync(solutionPath, progress: null, cancellation.Token),
                    cancellation.Token);
            await ExtractWorkspaceProjects(solution, root, results, extractedFiles, loadedProjects, projectFacts, cancellation.Token);
            }
            catch (OperationCanceledException) { throw; }
            catch (Exception e)
            {
                Console.Error.WriteLine($"warning: failed to load {solutionPath}: {e.Message}");
            }
        }

        foreach (var solutionPath in FindFiles(root, ".slnx"))
            await ExtractSlnxProjects(workspace, solutionPath, root, results, extractedFiles, loadedProjects, projectFacts, cancellation.Token);

        foreach (var projectPath in FindFiles(root, ".csproj"))
        {
            cancellation.Token.ThrowIfCancellationRequested();
            if (loadedProjects.Contains(Path.GetFullPath(projectPath))) continue;
            try
            {
                await AwaitSemantic(
                    () => workspace.OpenProjectAsync(projectPath, progress: null, cancellation.Token),
                    cancellation.Token);
                await ExtractWorkspaceProjects(workspace.CurrentSolution, root, results, extractedFiles, loadedProjects, projectFacts, cancellation.Token);
            }
            catch (OperationCanceledException) { throw; }
            catch (Exception e)
            {
                Console.Error.WriteLine($"warning: failed to load {projectPath}: {e.Message}");
            }
        }
        }
        catch (OperationCanceledException)
        {
            disposeWorkspace = false;
            throw;
        }
        finally
        {
            if (disposeWorkspace)
                workspace.Dispose();
        }
    }

    static async Task ExtractSlnxProjects(
        MSBuildWorkspace workspace,
        string solutionPath,
        string root,
        Dictionary<string, List<Unit>> results,
        HashSet<string> extractedFiles,
        HashSet<string> loadedProjects,
        Dictionary<string, CSharpProjectFacts> projectFacts,
        CancellationToken cancellationToken)
    {
        string[] projectPaths;
        try
        {
            projectPaths = ReadSlnxProjectPaths(solutionPath).ToArray();
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"warning: failed to read {solutionPath}: {e.Message}");
            return;
        }

        foreach (var projectPath in projectPaths)
        {
            if (loadedProjects.Contains(projectPath)) continue;
            try
            {
                await AwaitSemantic(
                    () => workspace.OpenProjectAsync(projectPath, progress: null, cancellationToken),
                    cancellationToken);
                await ExtractWorkspaceProjects(workspace.CurrentSolution, root, results, extractedFiles, loadedProjects, projectFacts, cancellationToken);
            }
            catch (OperationCanceledException) { throw; }
            catch (Exception e)
            {
                Console.Error.WriteLine($"warning: failed to load {projectPath} from {solutionPath}: {e.Message}");
            }
        }
    }

    static IEnumerable<string> ReadSlnxProjectPaths(string solutionPath)
    {
        var baseDir = Path.GetDirectoryName(solutionPath) ?? ".";
        var doc = XDocument.Load(solutionPath);
        foreach (var element in doc.Descendants().Where(e => e.Name.LocalName == "Project"))
        {
            var path = element.Attribute("Path")?.Value;
            if (string.IsNullOrWhiteSpace(path)) continue;

            var projectPath = path.Replace('\\', Path.DirectorySeparatorChar);
            var fullPath = Path.GetFullPath(Path.Combine(baseDir, projectPath));
            if (fullPath.EndsWith(".csproj", StringComparison.OrdinalIgnoreCase))
                yield return fullPath;
        }
    }

    static async Task ExtractWorkspaceProjects(
        Solution solution,
        string root,
        Dictionary<string, List<Unit>> results,
        HashSet<string> extractedFiles,
        HashSet<string> loadedProjects,
        Dictionary<string, CSharpProjectFacts> projectFacts,
        CancellationToken cancellationToken)
    {
        foreach (var project in solution.Projects)
        {
            if (project.FilePath is not { } fp) continue;
            if (!loadedProjects.Add(Path.GetFullPath(fp))) continue;
            RecordProjectDocuments(project, root, projectFacts);
            await ExtractProject(project, root, results, extractedFiles, projectFacts, cancellationToken);
        }
    }

    static Dictionary<string, CSharpProjectFacts> ReadProjectFacts(string root)
    {
        var facts = new Dictionary<string, CSharpProjectFacts>(StringComparer.OrdinalIgnoreCase);
        foreach (var projectPath in FindFiles(root, ".csproj"))
        {
            var relPath = RelPath(root, projectPath);
            facts[relPath] = ReadProjectFacts(root, projectPath, relPath);
        }
        return facts;
    }

    static CSharpProjectFacts ReadProjectFacts(string root, string projectPath, string relPath)
    {
        var fact = new CSharpProjectFacts { ProjectPath = relPath };
        try
        {
            var doc = XDocument.Load(projectPath);
            fact.OutputType = NormalizeOutputType(
                doc.Descendants()
                    .FirstOrDefault(e => e.Name.LocalName == "OutputType")
                    ?.Value);
            fact.IsTestProject = ParseBool(
                doc.Descendants()
                    .FirstOrDefault(e => e.Name.LocalName == "IsTestProject")
                    ?.Value);
            fact.TestPackageReferences = doc
                .Descendants()
                .Where(e => e.Name.LocalName == "PackageReference")
                .Select(e => e.Attribute("Include")?.Value ?? e.Attribute("Update")?.Value)
                .Where(value => !string.IsNullOrWhiteSpace(value))
                .Where(value => TestPackageReferences.Contains(
                    value!,
                    StringComparer.OrdinalIgnoreCase))
                .Select(value => value!)
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .OrderBy(value => value, StringComparer.OrdinalIgnoreCase)
                .ToList();
            fact.HasMicrosoftNetTestSdk = fact.TestPackageReferences.Any(
                value => string.Equals(
                    value,
                    "Microsoft.NET.Test.Sdk",
                    StringComparison.OrdinalIgnoreCase));
            fact.ExplicitTestProject =
                fact.IsTestProject == true || fact.TestPackageReferences.Count > 0;
            fact.ProjectReferences = doc
                .Descendants()
                .Where(e => e.Name.LocalName == "ProjectReference")
                .Select(e => e.Attribute("Include")?.Value)
                .Where(value => !string.IsNullOrWhiteSpace(value))
                .Select(value => ResolveProjectReference(root, projectPath, value!))
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .OrderBy(value => value, StringComparer.OrdinalIgnoreCase)
                .ToList();
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"warning: failed to read project facts for {relPath}: {e.Message}");
        }

        fact.OutputType ??= "Library";
        return fact;
    }

    static string ResolveProjectReference(string root, string projectPath, string reference)
    {
        var normalizedReference = reference
            .Replace('\\', Path.DirectorySeparatorChar)
            .Replace('/', Path.DirectorySeparatorChar);
        return RelPath(
            root,
            Path.GetFullPath(Path.Combine(
                Path.GetDirectoryName(projectPath) ?? root,
                normalizedReference)));
    }

    static void RecordProjectDocuments(
        Project project,
        string root,
        Dictionary<string, CSharpProjectFacts> projectFacts)
    {
        if (project.FilePath is not { } projectPath) return;
        var relProjectPath = RelPath(root, projectPath);
        if (!projectFacts.TryGetValue(relProjectPath, out var fact))
        {
            fact = new CSharpProjectFacts
            {
                ProjectPath = relProjectPath,
                OutputType = NormalizeOutputTypeFromKind(project.CompilationOptions?.OutputKind),
            };
            projectFacts[relProjectPath] = fact;
        }
        fact.OutputType ??= NormalizeOutputTypeFromKind(
            project.CompilationOptions?.OutputKind);
        fact.SourceFiles = project.Documents
            .Select(doc => doc.FilePath)
            .Where(path => !string.IsNullOrWhiteSpace(path) && File.Exists(path))
            .Select(path => Path.GetRelativePath(root, path!))
            .Where(rel => !rel.StartsWith("..", StringComparison.Ordinal) && !Path.IsPathRooted(rel))
            .Select(rel => rel.Replace('\\', '/'))
            .Where(rel => !IsExcludedPath(rel))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .OrderBy(value => value, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    static bool? ParseBool(string? value)
    {
        if (string.IsNullOrWhiteSpace(value)) return null;
        if (bool.TryParse(value.Trim(), out var parsed)) return parsed;
        return null;
    }

    static string NormalizeOutputType(string? value)
    {
        if (string.IsNullOrWhiteSpace(value)) return "Library";
        return value.Trim() switch
        {
            var v when string.Equals(v, "Exe", StringComparison.OrdinalIgnoreCase) => "Exe",
            var v when string.Equals(v, "WinExe", StringComparison.OrdinalIgnoreCase) => "WinExe",
            var v when string.Equals(v, "Library", StringComparison.OrdinalIgnoreCase) => "Library",
            var v => v,
        };
    }

    static string NormalizeOutputTypeFromKind(OutputKind? kind) =>
        kind switch
        {
            OutputKind.ConsoleApplication => "Exe",
            OutputKind.WindowsApplication => "WinExe",
            OutputKind.DynamicallyLinkedLibrary => "Library",
            _ => "Library",
        };

    static void MarkExecutableReachability(Dictionary<string, CSharpProjectFacts> projectFacts)
    {
        var executableRoots = projectFacts
            .Values
            .Where(f => !f.ExplicitTestProject && (f.OutputType == "Exe" || f.OutputType == "WinExe"))
            .Select(f => f.ProjectPath)
            .ToList();
        var stack = new Stack<string>(executableRoots);
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        while (stack.Count > 0)
        {
            var projectPath = stack.Pop();
            if (!seen.Add(projectPath)) continue;
            if (!projectFacts.TryGetValue(projectPath, out var fact)) continue;
            if (fact.ExplicitTestProject) continue;

            fact.ReachableFromExecutable = true;
            foreach (var reference in fact.ProjectReferences)
                stack.Push(reference);
        }
    }

    static void WriteProjectFacts(
        string outputDir,
        Dictionary<string, CSharpProjectFacts> projectFacts,
        ulong generationId)
    {
        var facts = new CSharpProjectFactsFile
        {
            CachedAt = generationId,
            Projects = projectFacts
                .Values
                .OrderBy(f => f.ProjectPath, StringComparer.OrdinalIgnoreCase)
                .ToList(),
        };
        File.WriteAllText(
            Path.Combine(outputDir, "csharp-projects.json"),
            JsonSerializer.Serialize(facts, Json.Options));
    }

    static async Task ExtractProject(
        Project project,
        string root,
        Dictionary<string, List<Unit>> results,
        HashSet<string> extractedFiles,
        Dictionary<string, CSharpProjectFacts> projectFacts,
        CancellationToken cancellationToken)
    {
        var compilation = await AwaitSemantic(() => project.GetCompilationAsync(cancellationToken), cancellationToken);
        if (compilation is null) return;
        RecordCompilationOutputKind(project, root, projectFacts, compilation);

        foreach (var tree in compilation.SyntaxTrees)
        {
            var path = tree.FilePath;
            if (string.IsNullOrEmpty(path) || !File.Exists(path)) continue;
            var rel = Path.GetRelativePath(root, path);
            if (rel.StartsWith("..", StringComparison.Ordinal) || Path.IsPathRooted(rel)) continue;
            var relPath = rel.Replace('\\', '/');
            if (results.ContainsKey(relPath)) continue; // multi-targeting / shared files
            if (!extractedFiles.Add(CanonicalPath(path))) continue; // symlinked/shared files
            if (IsExcludedPath(relPath)) continue;

            if (await tree.GetRootAsync(cancellationToken) is not CompilationUnitSyntax syntaxRoot) continue;
            if (IsGenerated(syntaxRoot)) continue;

            var model = compilation.GetSemanticModel(tree);
            var text = await tree.GetTextAsync(cancellationToken);
            results[relPath] = UnitExtractor.Extract(syntaxRoot, model, text, relPath, root);
        }
    }

    static void RecordCompilationOutputKind(
        Project project,
        string root,
        Dictionary<string, CSharpProjectFacts> projectFacts,
        Compilation compilation)
    {
        if (project.FilePath is not { } projectPath) return;
        var relProjectPath = RelPath(root, projectPath);
        if (!projectFacts.TryGetValue(relProjectPath, out var fact))
        {
            fact = new CSharpProjectFacts { ProjectPath = relProjectPath };
            projectFacts[relProjectPath] = fact;
        }
        fact.OutputType = NormalizeOutputTypeFromKind(compilation.Options.OutputKind);
    }

    static async Task<T> AwaitSemantic<T>(Func<Task<T>> operation, CancellationToken cancellationToken)
    {
        var task = Task.Run(operation);
        var delay = Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
        var completed = await Task.WhenAny(task, delay);
        if (completed == task)
            return await task;
        throw new OperationCanceledException(cancellationToken);
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
        if (name.EndsWith(".g.cs", StringComparison.OrdinalIgnoreCase)
            || name.EndsWith(".g.i.cs", StringComparison.OrdinalIgnoreCase)
            || name.EndsWith(".generated.cs", StringComparison.OrdinalIgnoreCase)
            || name.EndsWith(".Designer.cs", StringComparison.OrdinalIgnoreCase))
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
                    if (File.GetAttributes(entry).HasFlag(FileAttributes.ReparsePoint))
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

    static string CanonicalPath(string path) => CanonicalPath(path, depth: 0);

    static string CanonicalPath(string path, int depth)
    {
        var fullPath = Path.GetFullPath(path);
        try
        {
            if (depth > 32) return fullPath;

            var root = Path.GetPathRoot(fullPath);
            if (string.IsNullOrEmpty(root)) return fullPath;

            var relative = Path.GetRelativePath(root, fullPath);
            var segments = relative.Split(
                new[] { Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar },
                StringSplitOptions.RemoveEmptyEntries);
            var current = root;
            for (var i = 0; i < segments.Length; i++)
            {
                var segment = segments[i];
                if (segment == ".") continue;
                current = Path.Combine(current, segment);

                var attrs = File.GetAttributes(current);
                if ((attrs & FileAttributes.ReparsePoint) == 0) continue;

                var target = (attrs & FileAttributes.Directory) != 0
                    ? new DirectoryInfo(current).ResolveLinkTarget(returnFinalTarget: true)?.FullName
                    : new FileInfo(current).ResolveLinkTarget(returnFinalTarget: true)?.FullName;
                if (target is null) continue;

                var resolvedTarget = Path.IsPathRooted(target)
                    ? Path.GetFullPath(target)
                    : Path.GetFullPath(Path.Combine(Path.GetDirectoryName(current) ?? root, target));

                var resolvedPath = resolvedTarget;
                foreach (var remaining in segments.Skip(i + 1))
                    resolvedPath = Path.Combine(resolvedPath, remaining);
                return CanonicalPath(resolvedPath, depth + 1);
            }
            return Path.GetFullPath(current);
        }
        catch (Exception)
        {
            return fullPath;
        }
    }

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
