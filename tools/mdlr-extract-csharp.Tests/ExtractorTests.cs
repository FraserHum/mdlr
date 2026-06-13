using System.Text.Json;
using MdlrExtractCSharp;
using Xunit;

namespace MdlrExtractCSharp.Tests;

public static class Fixture
{
    public static Dictionary<string, FileCacheEntry> Run(string root, int expectedExit = 0)
    {
        var outDir = Directory.CreateTempSubdirectory("mdlr-cs-out").FullName;
        var exit = Program.Run(root, outDir, 7777).GetAwaiter().GetResult();
        Assert.Equal(expectedExit, exit);

        var results = new Dictionary<string, FileCacheEntry>();
        foreach (var file in Directory.EnumerateFiles(outDir, "*.json", SearchOption.AllDirectories))
        {
            var entry = JsonSerializer.Deserialize<FileCacheEntry>(File.ReadAllText(file))!;
            results[entry.SourcePath] = entry;
        }
        return results;
    }

    public static string WriteProject(Dictionary<string, string> files)
    {
        var dir = Directory.CreateTempSubdirectory("mdlr-cs-fixture").FullName;
        foreach (var (name, content) in files)
        {
            var path = Path.Combine(dir, name);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, content);
        }
        return dir;
    }

    public const string LibCsproj = """
        <Project Sdk="Microsoft.NET.Sdk">
          <PropertyGroup>
            <TargetFramework>net8.0</TargetFramework>
            <Nullable>enable</Nullable>
          </PropertyGroup>
        </Project>
        """;
}

/// One shared semantic fixture: a classic .sln with two projects (App
/// references Lib), exercising the breadth of C# constructs.
public sealed class SemanticFixture
{
    public Dictionary<string, FileCacheEntry> Results { get; }

    public SemanticFixture()
    {
        var root = Fixture.WriteProject(new()
        {
            ["App.sln"] = """
                Microsoft Visual Studio Solution File, Format Version 12.00
                Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "Lib", "src\Lib\Lib.csproj", "{11111111-1111-1111-1111-111111111111}"
                EndProject
                Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "App", "src\App\App.csproj", "{22222222-2222-2222-2222-222222222222}"
                EndProject
                Global
                EndGlobal
                """,
            ["src/Lib/Lib.csproj"] = Fixture.LibCsproj,
            ["src/App/App.csproj"] = """
                <Project Sdk="Microsoft.NET.Sdk">
                  <PropertyGroup>
                    <OutputType>Exe</OutputType>
                    <TargetFramework>net8.0</TargetFramework>
                    <Nullable>enable</Nullable>
                  </PropertyGroup>
                  <ItemGroup>
                    <ProjectReference Include="../Lib/Lib.csproj" />
                  </ItemGroup>
                </Project>
                """,
            ["src/Lib/Shapes.cs"] = """
                namespace Lib;

                public interface IShape
                {
                    double Area();
                }

                public class Circle : IShape
                {
                    private double _radius;

                    public Circle(double radius) { _radius = radius; }

                    public double Radius
                    {
                        get => _radius;
                        set { if (value >= 0) _radius = value; }
                    }

                    double IShape.Area() => 3.14159 * _radius * _radius;

                    public string Describe(int precision)
                    {
                        var label = precision switch
                        {
                            0 => "rough",
                            > 0 and < 4 when _radius > 1 => "fine",
                            _ => "exact",
                        };
                        return label;
                    }

                    public async Task<double> AreaAsync()
                    {
                        await Task.Yield();
                        return ((IShape)this).Area();
                    }
                }

                public record Point(double X, double Y)
                {
                    public double Dist() => Math.Sqrt(X * X + Y * Y);
                }
                """,
            ["src/Lib/Stack.cs"] = """
                namespace Lib;

                public class Stack<T>
                {
                    private readonly List<T> _items = new();

                    public void Push(T item) { _items.Add(item); }
                    public void Push(T[] items)
                    {
                        foreach (var item in items) Push(item);
                    }

                    public int CountAbove(Func<T, bool> pred)
                    {
                        var n = 0;
                        var counter = () => { n++; };
                        foreach (var item in _items)
                            if (pred(item)) counter();
                        return Total(n);

                        int Total(int seed)
                        {
                            return seed > 0 ? seed : 0;
                        }
                    }
                }
                """,
            ["src/Lib/Part1.cs"] = """
                namespace Lib;

                public partial class Split
                {
                    public void First() { Second(); }
                }
                """,
            ["src/Lib/Part2.cs"] = """
                namespace Lib;

                public partial class Split
                {
                    public void Second() { }
                }
                """,
            ["src/Lib/Legacy.Designer.cs"] = """
                namespace Lib;
                public class Designed { public void Hidden() { } }
                """,
            ["src/Lib/AutoGen.cs"] = """
                // <auto-generated>
                //     This code was generated by a tool.
                // </auto-generated>
                namespace Lib;
                public class AutoGenerated { public void Hidden() { } }
                """,
            ["src/App/Program.cs"] = """
                using Lib;

                var circle = new Circle(2.0);
                if (args.Length > 0)
                    Console.WriteLine(circle.Describe(1));
                """,
        });
        Results = Fixture.Run(root);
    }
}

public sealed class SemanticTests : IClassFixture<SemanticFixture>
{
    readonly Dictionary<string, FileCacheEntry> _r;

    public SemanticTests(SemanticFixture fixture) => _r = fixture.Results;

    List<Unit> Units(string relPath) => _r[relPath].Units;

    Unit Find(string relPath, string id) =>
        Units(relPath).Single(u => u.Id == relPath + "::" + id);

    [Fact]
    public void GenerationIdIsStamped()
    {
        Assert.All(_r.Values, e => Assert.Equal(7777UL, e.CachedAt));
    }

    [Fact]
    public void NothingIsPartial()
    {
        Assert.All(_r.Values.SelectMany(e => e.Units), u => Assert.False(u.Partial));
    }

    [Fact]
    public void TypesAreStructsWithTags()
    {
        Assert.Equal("Struct", Find("src/Lib/Shapes.cs", "Lib.IShape").Kind);
        Assert.Equal(["interface"], Find("src/Lib/Shapes.cs", "Lib.IShape").Tags);
        Assert.Equal(["class"], Find("src/Lib/Shapes.cs", "Lib.Circle").Tags);
        Assert.Equal(["record"], Find("src/Lib/Shapes.cs", "Lib.Point").Tags);
    }

    [Fact]
    public void MethodsHaveParentLinks()
    {
        var m = Find("src/Lib/Shapes.cs", "Lib.Circle::Describe(int)");
        Assert.Equal("Method", m.Kind);
        Assert.Equal("src/Lib/Shapes.cs::Lib.Circle", m.Parent);
        Assert.Equal(1, m.Params);
    }

    [Fact]
    public void OverloadsGetDistinctIds()
    {
        Find("src/Lib/Stack.cs", "Lib.Stack<T>::Push(T)");
        Find("src/Lib/Stack.cs", "Lib.Stack<T>::Push(T[])");
    }

    [Fact]
    public void ExplicitInterfaceImplementationIsQualified()
    {
        var unit = Units("src/Lib/Shapes.cs")
            .Single(u => u.Id.Contains("IShape.Area", StringComparison.Ordinal));
        Assert.Equal("Method", unit.Kind);
        Assert.Contains("self._radius", unit.Reads);
    }

    [Fact]
    public void AccessorsAreSeparateUnits()
    {
        var getter = Find("src/Lib/Shapes.cs", "Lib.Circle::get_Radius()");
        Assert.Contains("self._radius", getter.Reads);
        var setter = Find("src/Lib/Shapes.cs", "Lib.Circle::set_Radius(double)");
        Assert.Contains("self._radius", setter.Writes);
        Assert.Equal(1, setter.Branches); // if (value >= 0)
    }

    [Fact]
    public void ConstructorIsExtracted()
    {
        var ctor = Find("src/Lib/Shapes.cs", "Lib.Circle::.ctor(double)");
        Assert.Contains("self._radius", ctor.Writes);
    }

    [Fact]
    public void RecordPrimaryConstructorIsExtracted()
    {
        Find("src/Lib/Shapes.cs", "Lib.Point::.ctor(double,double)");
    }

    [Fact]
    public void SwitchExpressionAndWhenCountBranches()
    {
        var m = Find("src/Lib/Shapes.cs", "Lib.Circle::Describe(int)");
        // switch with 3 arms (+2) and one `when` guard (+1)
        Assert.Equal(3, m.Branches);
    }

    [Fact]
    public void AsyncMethodCallsResolveThroughInterface()
    {
        var m = Find("src/Lib/Shapes.cs", "Lib.Circle::AreaAsync()");
        // `((IShape)this).Area()` binds to the interface member's unit.
        Assert.Contains("src/Lib/Shapes.cs::Lib.IShape::Area()", m.Calls);
    }

    [Fact]
    public void LocalFunctionIsSeparateUnitAndLambdaFoldsIntoParent()
    {
        // The Func<T, bool> parameter display depends on whether the fixture
        // project is restored, so match the stable prefix/suffix only.
        var m = Units("src/Lib/Stack.cs").Single(u =>
            u.Id.Contains("Lib.Stack<T>::CountAbove(") && !u.Id.EndsWith("::Total(int)"));
        // foreach (+1) + if (+1); the local function's ternary is NOT folded in
        Assert.Equal(2, m.Branches);

        var local = Units("src/Lib/Stack.cs").Single(u => u.Id.EndsWith("::Total(int)"));
        Assert.Equal("Function", local.Kind);
        Assert.Equal(1, local.Branches); // ternary
        Assert.Equal(m.Id, local.Parent);

        // Calls from inside the lambda and to the local function attach to the parent
        Assert.Contains("src/Lib/Stack.cs::Lib.Stack<T>::Push(T)", Find("src/Lib/Stack.cs", "Lib.Stack<T>::Push(T[])").Calls);
        Assert.Contains(m.Calls, c => c.EndsWith("::Total(int)", StringComparison.Ordinal));
    }

    [Fact]
    public void CrossFileCallsUseDeclarationFile()
    {
        var first = Find("src/Lib/Part1.cs", "Lib.Split::First()");
        Assert.Contains("src/Lib/Part2.cs::Lib.Split::Second()", first.Calls);
    }

    [Fact]
    public void CrossProjectCallsResolve()
    {
        var main = _r["src/App/Program.cs"].Units.Single(u => u.Id.EndsWith("<Main>$", StringComparison.Ordinal));
        Assert.Equal("Function", main.Kind);
        Assert.Equal(1, main.Branches); // if (args.Length > 0)
        Assert.Contains("src/Lib/Shapes.cs::Lib.Circle::.ctor(double)", main.Calls);
        Assert.Contains("src/Lib/Shapes.cs::Lib.Circle::Describe(int)", main.Calls);
    }

    [Fact]
    public void GeneratedFilesAreExcluded()
    {
        Assert.DoesNotContain("src/Lib/Legacy.Designer.cs", _r.Keys);
        Assert.DoesNotContain("src/Lib/AutoGen.cs", _r.Keys);
    }

    [Fact]
    public void SpansAreOneBasedAndOrdered()
    {
        foreach (var u in _r.Values.SelectMany(e => e.Units))
        {
            Assert.True(u.Span.StartLine >= 1, u.Id);
            Assert.True(u.Span.EndLine >= u.Span.StartLine, u.Id);
        }
    }

    [Fact]
    public async Task TokensFilesAreWrittenForEveryEntry()
    {
        // Re-run against a tiny project to inspect the .tokens binary layout.
        var root = Fixture.WriteProject(new()
        {
            ["Lib.csproj"] = Fixture.LibCsproj,
            ["A.cs"] = "namespace N; // mdlr:ignore-start\n// mdlr:ignore-end\npublic class A { int F() { return 42; } }",
        });
        var outDir = Directory.CreateTempSubdirectory("mdlr-cs-tok").FullName;
        Assert.Equal(0, await Program.Run(root, outDir, 42));

        var data = File.ReadAllBytes(Path.Combine(outDir, "A.cs.tokens"));
        var (path, cachedAt, tokens) = ParseTokens(data);
        Assert.Equal("A.cs", path);
        Assert.Equal(42UL, cachedAt);
        Assert.Contains(("namespace", 1u), tokens.Select(t => (t.value, t.line)));
        Assert.Contains("$ID", tokens.Select(t => t.value));
        Assert.Contains("$LIT", tokens.Select(t => t.value));
        Assert.DoesNotContain(tokens, t => t.value.StartsWith("//"));
    }

    static (string path, ulong cachedAt, List<(string value, uint line, ushort col)> tokens) ParseTokens(byte[] data)
    {
        using var r = new BinaryReader(new MemoryStream(data));
        var path = System.Text.Encoding.UTF8.GetString(r.ReadBytes((int)r.ReadUInt32()));
        var cachedAt = r.ReadUInt64();
        var strings = new List<string>();
        var stringCount = r.ReadUInt32();
        for (var i = 0; i < stringCount; i++)
            strings.Add(System.Text.Encoding.UTF8.GetString(r.ReadBytes(r.ReadUInt16())));
        var tokens = new List<(string, uint, ushort)>();
        var tokenCount = r.ReadUInt32();
        for (var i = 0; i < tokenCount; i++)
            tokens.Add((strings[r.ReadUInt16()], r.ReadUInt32(), r.ReadUInt16()));
        return (path, cachedAt, tokens);
    }
}

public sealed class FallbackAndSolutionTests
{
    [Fact]
    public void NoProjectFallsBackToSyntaxOnlyWithPartialUnits()
    {
        var root = Fixture.WriteProject(new()
        {
            ["Loose.cs"] = """
                namespace Loose;
                public class Thing
                {
                    public int Twice(int x)
                    {
                        if (x > 0) return x * 2;
                        return Helper(x);
                    }
                    int Helper(int v) => v;
                }
                """,
        });
        var results = Fixture.Run(root, expectedExit: 2);

        var units = results["Loose.cs"].Units;
        Assert.All(units, u => Assert.True(u.Partial));
        var twice = units.Single(u => u.Id == "Loose.cs::Loose.Thing::Twice(int)");
        Assert.Equal(1, twice.Branches);
        Assert.Empty(twice.Calls); // no semantic edges in fallback mode
    }

    [Fact]
    public async Task SlnxSolutionIsAnalyzed()
    {
        var root = Fixture.WriteProject(new()
        {
            ["App.slnx"] = """
                <Solution>
                  <Project Path="src/Lib/Lib.csproj" />
                </Solution>
                """,
            ["src/Lib/Lib.csproj"] = Fixture.LibCsproj,
            ["src/Lib/A.cs"] = """
                namespace N;
                public class A
                {
                    public void Go() { Stop(); }
                    public void Stop() { }
                }
                """,
        });
        var outDir = Directory.CreateTempSubdirectory("mdlr-cs-out").FullName;
        using var stderr = new StringWriter();
        var originalError = Console.Error;
        Console.SetError(stderr);
        try
        {
            var exit = await Program.Run(root, outDir, 7777);
            Assert.Equal(0, exit);
        }
        finally
        {
            Console.SetError(originalError);
        }

        Assert.DoesNotContain("No file format header found", stderr.ToString());

        var results = new Dictionary<string, FileCacheEntry>();
        foreach (var file in Directory.EnumerateFiles(outDir, "*.json", SearchOption.AllDirectories))
        {
            var entry = JsonSerializer.Deserialize<FileCacheEntry>(File.ReadAllText(file))!;
            results[entry.SourcePath] = entry;
        }

        var go = results["src/Lib/A.cs"].Units.Single(u => u.Id.EndsWith("::Go()", StringComparison.Ordinal));
        Assert.False(go.Partial);
        Assert.Contains("src/Lib/A.cs::N.A::Stop()", go.Calls);
    }

    [Fact]
    public void ProjectReferencesLoadedByCsprojSweepAreAnalyzed()
    {
        var root = Fixture.WriteProject(new()
        {
            ["App.csproj"] = """
                <Project Sdk="Microsoft.NET.Sdk">
                  <PropertyGroup>
                    <OutputType>Exe</OutputType>
                    <TargetFramework>net8.0</TargetFramework>
                    <Nullable>enable</Nullable>
                  </PropertyGroup>
                  <ItemGroup>
                    <ProjectReference Include="src/Lib/Lib.csproj" />
                  </ItemGroup>
                </Project>
                """,
            ["src/Lib/Lib.csproj"] = Fixture.LibCsproj,
            ["src/Lib/Service.cs"] = """
                namespace Lib;
                public class Service
                {
                    public int Value() => 42;
                }
                """,
            ["Program.cs"] = """
                using Lib;

                var service = new Service();
                Console.WriteLine(service.Value());
                """,
        });
        var results = Fixture.Run(root);

        Assert.All(results.Values.SelectMany(e => e.Units), u => Assert.False(u.Partial));
        var main = results["Program.cs"].Units.Single(u => u.Id.EndsWith("<Main>$", StringComparison.Ordinal));
        Assert.Contains("src/Lib/Service.cs::Lib.Service::Value()", main.Calls);
    }

    [Fact]
    public async Task SymlinkedProjectFilesDoNotEmitFallbackDuplicates()
    {
        var root = Fixture.WriteProject(new()
        {
            ["App.slnx"] = """
                <Solution>
                  <Project Path="game/Game.csproj" />
                </Solution>
                """,
            ["game/Game.csproj"] = """
                <Project Sdk="Microsoft.NET.Sdk">
                  <PropertyGroup>
                    <TargetFramework>net8.0</TargetFramework>
                    <Nullable>enable</Nullable>
                    <EnableDefaultCompileItems>false</EnableDefaultCompileItems>
                  </PropertyGroup>
                  <ItemGroup>
                    <Compile Include="addons/genode/Linked.cs" />
                  </ItemGroup>
                </Project>
                """,
            ["addons/genode/Linked.cs"] = """
                namespace N;
                public class Linked
                {
                    public void Go() { Stop(); }
                    public void Stop() { }
                }
                """,
        });

        Directory.CreateDirectory(Path.Combine(root, "game", "addons"));
        Directory.CreateSymbolicLink(
            Path.Combine(root, "game", "addons", "genode"),
            Path.Combine(root, "addons", "genode"));

        var outDir = Directory.CreateTempSubdirectory("mdlr-cs-out").FullName;
        Assert.Equal(0, await Program.Run(root, outDir, 7777));

        var results = new Dictionary<string, FileCacheEntry>();
        foreach (var file in Directory.EnumerateFiles(outDir, "*.json", SearchOption.AllDirectories))
        {
            var entry = JsonSerializer.Deserialize<FileCacheEntry>(File.ReadAllText(file))!;
            results[entry.SourcePath] = entry;
        }

        Assert.Contains("game/addons/genode/Linked.cs", results.Keys);
        Assert.DoesNotContain("addons/genode/Linked.cs", results.Keys);
        Assert.All(results.Values.SelectMany(e => e.Units), u => Assert.False(u.Partial));

        var go = results["game/addons/genode/Linked.cs"].Units.Single(u => u.Id.EndsWith("::Go()", StringComparison.Ordinal));
        Assert.Contains("game/addons/genode/Linked.cs::N.Linked::Stop()", go.Calls);
    }
}
