using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace MdlrExtractCSharp;

/// Extracts mdlr Units from one C# file. When a SemanticModel is available,
/// IDs come from symbols and call/read/write edges are resolved; without one
/// (syntax fallback), IDs come from syntax and units are marked partial.
public sealed class UnitExtractor
{
    /// Display format for types inside IDs and call targets:
    /// namespace-qualified, generic type parameters included, language
    /// keywords for special types (int, string, ...).
    static readonly SymbolDisplayFormat TypeFormat = new(
        typeQualificationStyle: SymbolDisplayTypeQualificationStyle.NameAndContainingTypesAndNamespaces,
        genericsOptions: SymbolDisplayGenericsOptions.IncludeTypeParameters,
        miscellaneousOptions: SymbolDisplayMiscellaneousOptions.UseSpecialTypes);

    readonly SemanticModel? _model;
    readonly SourceText _text;
    readonly string _relPath;
    readonly string _rootDir;
    readonly List<Unit> _units = [];

    UnitExtractor(SemanticModel? model, SourceText text, string relPath, string rootDir)
    {
        _model = model;
        _text = text;
        _relPath = relPath;
        _rootDir = rootDir;
    }

    public static List<Unit> Extract(
        CompilationUnitSyntax root, SemanticModel? model, SourceText text,
        string relPath, string rootDir)
    {
        var ex = new UnitExtractor(model, text, relPath, rootDir);
        ex.ExtractTopLevelStatements(root);
        foreach (var type in root.DescendantNodes().OfType<BaseTypeDeclarationSyntax>())
            ex.ExtractType(type);
        if (model is null)
            foreach (var u in ex._units) u.Partial = true;
        return ex._units;
    }

    bool Semantic => _model is not null;

    // ---------- types ----------

    void ExtractType(BaseTypeDeclarationSyntax type)
    {
        var typeId = _relPath + "::" + TypeDisplay(type);
        _units.Add(new Unit
        {
            Id = typeId,
            Kind = "Struct",
            File = _relPath,
            Span = MakeSpan(type.Span),
            Tags = [TypeTag(type)],
        });

        if (type is TypeDeclarationSyntax { ParameterList: { } primaryParams } td)
            ExtractPrimaryConstructor(td, typeId, primaryParams);

        if (type is not TypeDeclarationSyntax decl) return; // enums have no member units

        foreach (var member in decl.Members)
        {
            switch (member)
            {
                case MethodDeclarationSyntax m:
                    ExtractCallable(m, typeId, BodyOf(m.Body, m.ExpressionBody), m.ParameterList.Parameters.Count);
                    break;
                case ConstructorDeclarationSyntax c:
                    ExtractCallable(c, typeId, BodyOf(c.Body, c.ExpressionBody), c.ParameterList.Parameters.Count);
                    break;
                case DestructorDeclarationSyntax d:
                    ExtractCallable(d, typeId, BodyOf(d.Body, d.ExpressionBody), 0);
                    break;
                case OperatorDeclarationSyntax op:
                    ExtractCallable(op, typeId, BodyOf(op.Body, op.ExpressionBody), op.ParameterList.Parameters.Count);
                    break;
                case ConversionOperatorDeclarationSyntax conv:
                    ExtractCallable(conv, typeId, BodyOf(conv.Body, conv.ExpressionBody), conv.ParameterList.Parameters.Count);
                    break;
                case BasePropertyDeclarationSyntax prop:
                    ExtractProperty(prop, typeId);
                    break;
            }
        }
    }

    void ExtractPrimaryConstructor(TypeDeclarationSyntax type, string typeId, ParameterListSyntax parameters)
    {
        var id = typeId + "::.ctor" + ParamsDisplayFor(type, parameters);
        _units.Add(new Unit
        {
            Id = id,
            Kind = "Method",
            File = _relPath,
            Span = MakeSpan(parameters.Span),
            Parent = typeId,
            Params = parameters.Parameters.Count,
        });
    }

    /// Properties, indexers, events: each accessor with a body becomes a unit;
    /// an expression-bodied property becomes its getter.
    void ExtractProperty(BasePropertyDeclarationSyntax prop, string typeId)
    {
        var indexerParams = prop is IndexerDeclarationSyntax ix ? ix.ParameterList.Parameters.Count : 0;

        if (prop is PropertyDeclarationSyntax { ExpressionBody: { } arrow } p)
        {
            ExtractAccessorUnit(p, null, typeId, arrow.Expression, 0);
            return;
        }

        if (prop.AccessorList is null) return;
        foreach (var accessor in prop.AccessorList.Accessors)
        {
            SyntaxNode? body = BodyOf(accessor.Body, accessor.ExpressionBody);
            if (body is null) continue; // auto-accessor: no unit
            var extraParams = accessor.Kind() is SyntaxKind.SetAccessorDeclaration
                or SyntaxKind.InitAccessorDeclaration or SyntaxKind.AddAccessorDeclaration
                or SyntaxKind.RemoveAccessorDeclaration ? 1 : 0;
            ExtractAccessorUnit(prop, accessor, typeId, body, indexerParams + extraParams);
        }
    }

    void ExtractAccessorUnit(
        BasePropertyDeclarationSyntax prop, AccessorDeclarationSyntax? accessor,
        string typeId, SyntaxNode body, int paramCount)
    {
        string memberDisplay;
        var symbol = Semantic
            ? _model!.GetDeclaredSymbol((SyntaxNode?)accessor ?? prop) : null;
        if (symbol is IMethodSymbol ms)
            memberDisplay = MemberDisplay(ms);
        else if (symbol is IPropertySymbol ps && ps.GetMethod is { } getter)
            memberDisplay = MemberDisplay(getter);
        else
            memberDisplay = SyntaxAccessorDisplay(prop, accessor);

        var spanNode = (SyntaxNode?)accessor ?? prop;
        AddCallableUnit(typeId + "::" + memberDisplay, typeId, spanNode.Span, body, paramCount);
    }

    void ExtractCallable(MemberDeclarationSyntax decl, string typeId, SyntaxNode? body, int paramCount)
    {
        string memberDisplay;
        if (Semantic && _model!.GetDeclaredSymbol(decl) is IMethodSymbol symbol)
            memberDisplay = MemberDisplay(symbol);
        else
            memberDisplay = SyntaxMemberDisplay(decl);

        AddCallableUnit(typeId + "::" + memberDisplay, typeId, decl.Span, body, paramCount);
    }

    void AddCallableUnit(string id, string? parent, TextSpan declSpan, SyntaxNode? body, int paramCount)
    {
        var unit = new Unit
        {
            Id = id,
            Kind = parent is null ? "Function" : "Method",
            File = _relPath,
            Span = MakeSpan(declSpan),
            Parent = parent,
            Params = paramCount,
            Branches = body is null ? 0 : MemberMetrics.CountBranches(body),
            CognitiveComplexity = body is null ? 0 : MemberMetrics.ComputeCognitive(body),
            MaxScopeLines = body is null ? 0 : MemberMetrics.MaxScopeLines(body, _text),
        };
        if (body is not null && Semantic)
        {
            unit.Calls.AddRange(ExtractCalls(body));
            var (reads, writes) = ExtractFieldAccess(body);
            unit.Reads.AddRange(reads);
            unit.Writes.AddRange(writes);
        }
        _units.Add(unit);
        if (body is not null) ExtractLocalFunctions(body, id);
    }

    void ExtractLocalFunctions(SyntaxNode body, string parentId)
    {
        foreach (var local in body.DescendantNodes(n => n is not LocalFunctionStatementSyntax || n == body)
                     .OfType<LocalFunctionStatementSyntax>())
        {
            string display;
            if (Semantic && _model!.GetDeclaredSymbol(local) is IMethodSymbol sym)
                display = MemberDisplay(sym);
            else
                display = local.Identifier.Text + SyntaxTypeParams(local.TypeParameterList)
                    + SyntaxParams(local.ParameterList);

            var localBody = BodyOf(local.Body, local.ExpressionBody);
            var unit = new Unit
            {
                Id = parentId + "::" + display,
                Kind = "Function",
                File = _relPath,
                Span = MakeSpan(local.Span),
                Parent = parentId,
                Params = local.ParameterList.Parameters.Count,
                Branches = localBody is null ? 0 : MemberMetrics.CountBranches(localBody),
                CognitiveComplexity = localBody is null ? 0 : MemberMetrics.ComputeCognitive(localBody),
                MaxScopeLines = localBody is null ? 0 : MemberMetrics.MaxScopeLines(localBody, _text),
            };
            if (localBody is not null && Semantic)
            {
                unit.Calls.AddRange(ExtractCalls(localBody));
                var (reads, writes) = ExtractFieldAccess(localBody);
                unit.Reads.AddRange(reads);
                unit.Writes.AddRange(writes);
            }
            _units.Add(unit);
            if (localBody is not null) ExtractLocalFunctions(localBody, unit.Id);
        }
    }

    // ---------- top-level statements ----------

    void ExtractTopLevelStatements(CompilationUnitSyntax root)
    {
        var globals = root.Members.OfType<GlobalStatementSyntax>().ToList();
        if (globals.Count == 0) return;

        var span = TextSpan.FromBounds(globals[0].SpanStart, globals[^1].Span.End);
        var unit = new Unit
        {
            Id = _relPath + "::<Main>$",
            Kind = "Function",
            File = _relPath,
            Span = MakeSpan(span),
            Branches = globals.Sum(g => MemberMetrics.CountBranches(g)),
            CognitiveComplexity = globals.Sum(g => MemberMetrics.ComputeCognitive(g)),
            MaxScopeLines = globals.Max(g => MemberMetrics.MaxScopeLines(g, _text)),
        };
        if (Semantic)
        {
            var calls = new List<string>();
            var seen = new HashSet<string>();
            foreach (var g in globals)
                foreach (var c in ExtractCalls(g))
                    if (seen.Add(c)) calls.Add(c);
            unit.Calls.AddRange(calls);
        }
        _units.Add(unit);
        foreach (var g in globals)
            ExtractLocalFunctions(g, unit.Id);
    }

    // ---------- calls ----------

    /// Type-resolved call targets, deduplicated in order of first occurrence.
    /// Targets declared under the root use the same format as unit IDs
    /// (relPath::Type::Member(...)), so the graph builder matches them
    /// exactly; external targets fall back to a dotted name.
    List<string> ExtractCalls(SyntaxNode body)
    {
        var calls = new List<string>();
        var seen = new HashSet<string>();

        foreach (var node in body.DescendantNodesAndSelf(n => n is not LocalFunctionStatementSyntax || n == body))
        {
            ExpressionSyntax? callNode = node switch
            {
                InvocationExpressionSyntax inv => inv,
                BaseObjectCreationExpressionSyntax create => create,
                _ => null,
            };
            string? target = null;
            if (callNode is not null)
                target = ResolveCallTarget(callNode);
            else if (node is ConstructorInitializerSyntax init)
                target = TargetFor(_model!.GetSymbolInfo(init).Symbol as IMethodSymbol);

            if (target is { Length: > 0 } && seen.Add(target))
                calls.Add(target);
        }
        return calls;
    }

    string? ResolveCallTarget(ExpressionSyntax callNode)
    {
        var info = _model!.GetSymbolInfo(callNode);
        var symbol = (info.Symbol ?? info.CandidateSymbols.FirstOrDefault()) as IMethodSymbol;
        if (symbol is null)
        {
            // Unresolved: fall back to the textual callee name, like Go's
            // selectorToString fallback.
            return callNode is InvocationExpressionSyntax inv ? CalleeText(inv.Expression) : null;
        }
        if (symbol.MethodKind == MethodKind.DelegateInvoke) return null;
        return TargetFor(symbol);
    }

    string? TargetFor(IMethodSymbol? symbol)
    {
        if (symbol is null) return null;
        symbol = symbol.ReducedFrom ?? symbol;
        symbol = symbol.PartialImplementationPart ?? symbol;
        var original = symbol.OriginalDefinition;

        var declRelPath = DeclarationRelPath(original);
        // Local functions nest under their containing method(s) in unit IDs.
        var member = MemberDisplay(original);
        for (var s = original.ContainingSymbol; s is IMethodSymbol m; s = m.ContainingSymbol)
            member = MemberDisplay(m) + "::" + member;
        var type = original.ContainingType is { } ct ? ct.OriginalDefinition.ToDisplayString(TypeFormat) : null;

        if (declRelPath is not null && type is not null)
            return declRelPath + "::" + type + "::" + member;

        // External (BCL/NuGet) target: dotted fallback.
        return type is not null ? type + "." + original.Name : original.Name;
    }

    string? DeclarationRelPath(IMethodSymbol symbol)
    {
        foreach (var sr in symbol.DeclaringSyntaxReferences)
        {
            var path = sr.SyntaxTree.FilePath;
            if (string.IsNullOrEmpty(path)) continue;
            var rel = Path.GetRelativePath(_rootDir, path);
            if (!rel.StartsWith("..", StringComparison.Ordinal) && !Path.IsPathRooted(rel))
                return rel.Replace('\\', '/');
        }
        return null;
    }

    static string CalleeText(ExpressionSyntax expr) => expr switch
    {
        IdentifierNameSyntax id => id.Identifier.Text,
        GenericNameSyntax g => g.Identifier.Text,
        MemberAccessExpressionSyntax ma => CalleeText(ma.Expression) + "." + ma.Name.Identifier.Text,
        MemberBindingExpressionSyntax mb => mb.Name.Identifier.Text,
        ThisExpressionSyntax => "this",
        BaseExpressionSyntax => "base",
        _ => "",
    };

    // ---------- reads/writes ----------

    /// Instance field/property accesses on the containing type, normalized to
    /// "self.<name>" (mirrors the Go extractor's receiver-field extraction).
    (List<string> reads, List<string> writes) ExtractFieldAccess(SyntaxNode body)
    {
        var reads = new List<string>();
        var writes = new List<string>();
        var readSet = new HashSet<string>();
        var writeSet = new HashSet<string>();

        foreach (var node in body.DescendantNodesAndSelf(n => n is not LocalFunctionStatementSyntax || n == body))
        {
            if (node is not SimpleNameSyntax name) continue;
            if (!IsSelfMemberAccess(name)) continue;

            var symbol = _model!.GetSymbolInfo(name).Symbol;
            if (symbol is not (IFieldSymbol or IPropertySymbol) || symbol.IsStatic) continue;

            var entry = "self." + symbol.Name;
            var accessNode = name.Parent is MemberAccessExpressionSyntax ma && ma.Name == name
                ? (ExpressionSyntax)ma : name;
            if (IsWriteContext(accessNode))
            {
                if (writeSet.Add(entry)) writes.Add(entry);
            }
            else
            {
                if (readSet.Add(entry)) reads.Add(entry);
            }
        }
        return (reads, writes);
    }

    /// True when `name` refers to a member of the current instance: either a
    /// bare identifier (implicit this) or `this.name`.
    static bool IsSelfMemberAccess(SimpleNameSyntax name)
    {
        if (name.Parent is MemberAccessExpressionSyntax ma && ma.Name == name)
            return ma.Expression is ThisExpressionSyntax;
        // Bare identifier (implicit this) — exclude type-name positions; the
        // field/property symbol filter at the call site excludes the rest.
        return name.Parent is not (QualifiedNameSyntax or NameEqualsSyntax or NameColonSyntax);
    }

    static bool IsWriteContext(ExpressionSyntax node)
    {
        var inner = node;
        while (inner.Parent is ParenthesizedExpressionSyntax p) inner = p;
        return inner.Parent switch
        {
            AssignmentExpressionSyntax assign when assign.Left == inner => true,
            PrefixUnaryExpressionSyntax pre when
                pre.IsKind(SyntaxKind.PreIncrementExpression) ||
                pre.IsKind(SyntaxKind.PreDecrementExpression) => true,
            PostfixUnaryExpressionSyntax post when
                post.IsKind(SyntaxKind.PostIncrementExpression) ||
                post.IsKind(SyntaxKind.PostDecrementExpression) => true,
            _ => false,
        };
    }

    // ---------- display helpers ----------

    string TypeDisplay(BaseTypeDeclarationSyntax type)
    {
        if (Semantic && _model!.GetDeclaredSymbol(type) is INamedTypeSymbol symbol)
            return symbol.ToDisplayString(TypeFormat);
        return SyntaxTypeDisplay(type);
    }

    /// Member display: symbol name (get_X, .ctor, op_Addition,
    /// interface-qualified for explicit implementations), generic type
    /// parameters, and the parenthesized parameter list with ref/out/in and
    /// params modifiers — overload-stable.
    public static string MemberDisplay(IMethodSymbol symbol)
    {
        var name = symbol.Name;
        if (symbol.IsGenericMethod)
            name += "<" + string.Join(",", symbol.TypeParameters.Select(t => t.Name)) + ">";
        var ps = symbol.Parameters.Select(p =>
        {
            var prefix = p.RefKind switch
            {
                RefKind.Ref => "ref ",
                RefKind.Out => "out ",
                RefKind.In => "in ",
                _ => "",
            };
            if (p.IsParams) prefix = "params " + prefix;
            return prefix + p.Type.ToDisplayString(TypeFormat);
        });
        return name + "(" + string.Join(",", ps) + ")";
    }

    static string TypeTag(BaseTypeDeclarationSyntax type) => type switch
    {
        ClassDeclarationSyntax => "class",
        StructDeclarationSyntax => "struct",
        InterfaceDeclarationSyntax => "interface",
        EnumDeclarationSyntax => "enum",
        RecordDeclarationSyntax r =>
            r.ClassOrStructKeyword.IsKind(SyntaxKind.StructKeyword) ? "record-struct" : "record",
        _ => "type",
    };

    // ---------- syntax-fallback display ----------

    string SyntaxTypeDisplay(BaseTypeDeclarationSyntax type)
    {
        var parts = new List<string>();
        for (SyntaxNode? node = type; node is not null; node = node.Parent)
        {
            switch (node)
            {
                case BaseTypeDeclarationSyntax t:
                    var name = t.Identifier.Text;
                    if (t is TypeDeclarationSyntax { TypeParameterList: { } tps })
                        name += SyntaxTypeParams(tps);
                    parts.Insert(0, name);
                    break;
                case BaseNamespaceDeclarationSyntax ns:
                    parts.Insert(0, ns.Name.ToString());
                    break;
            }
        }
        return string.Join(".", parts);
    }

    static string SyntaxTypeParams(TypeParameterListSyntax? tps) =>
        tps is null ? "" : "<" + string.Join(",", tps.Parameters.Select(p => p.Identifier.Text)) + ">";

    static string SyntaxParams(ParameterListSyntax? parameters) =>
        "(" + string.Join(",", (parameters?.Parameters ?? default).Select(p =>
        {
            var mods = string.Concat(p.Modifiers
                .Where(m => m.IsKind(SyntaxKind.RefKeyword) || m.IsKind(SyntaxKind.OutKeyword)
                    || m.IsKind(SyntaxKind.InKeyword) || m.IsKind(SyntaxKind.ParamsKeyword))
                .Select(m => m.Text + " "));
            return mods + (p.Type?.ToString() ?? "");
        })) + ")";

    static string SyntaxMemberDisplay(MemberDeclarationSyntax decl) => decl switch
    {
        MethodDeclarationSyntax m =>
            (m.ExplicitInterfaceSpecifier is { } eis ? eis.Name + "." : "")
            + m.Identifier.Text + SyntaxTypeParams(m.TypeParameterList) + SyntaxParams(m.ParameterList),
        ConstructorDeclarationSyntax c =>
            (c.Modifiers.Any(SyntaxKind.StaticKeyword) ? ".cctor" : ".ctor") + SyntaxParams(c.ParameterList),
        DestructorDeclarationSyntax => "Finalize()",
        OperatorDeclarationSyntax op => "op_" + op.OperatorToken.Text + SyntaxParams(op.ParameterList),
        ConversionOperatorDeclarationSyntax conv =>
            (conv.ImplicitOrExplicitKeyword.IsKind(SyntaxKind.ImplicitKeyword) ? "op_Implicit" : "op_Explicit")
            + SyntaxParams(conv.ParameterList),
        _ => decl.ToString(),
    };

    static string SyntaxAccessorDisplay(BasePropertyDeclarationSyntax prop, AccessorDeclarationSyntax? accessor)
    {
        var propName = prop switch
        {
            PropertyDeclarationSyntax p => p.Identifier.Text,
            IndexerDeclarationSyntax => "Item",
            EventDeclarationSyntax e => e.Identifier.Text,
            _ => "?",
        };
        var prefix = accessor?.Kind() switch
        {
            SyntaxKind.GetAccessorDeclaration or null => "get_",
            SyntaxKind.SetAccessorDeclaration => "set_",
            SyntaxKind.InitAccessorDeclaration => "set_",
            SyntaxKind.AddAccessorDeclaration => "add_",
            SyntaxKind.RemoveAccessorDeclaration => "remove_",
            _ => "",
        };
        var ps = prop is IndexerDeclarationSyntax ix
            ? "(" + string.Join(",", ix.ParameterList.Parameters.Select(p => p.Type?.ToString() ?? "")) + ")"
            : "()";
        return prefix + propName + ps;
    }

    string ParamsDisplayFor(TypeDeclarationSyntax type, ParameterListSyntax parameters)
    {
        if (Semantic && _model!.GetDeclaredSymbol(type) is INamedTypeSymbol sym)
        {
            var primaryCtor = sym.InstanceConstructors.FirstOrDefault(c =>
                c.DeclaringSyntaxReferences.Any(r => r.GetSyntax() == type));
            if (primaryCtor is not null)
            {
                var display = MemberDisplay(primaryCtor);
                return display[display.IndexOf('(')..];
            }
        }
        return SyntaxParams(parameters);
    }

    // ---------- spans ----------

    static SyntaxNode? BodyOf(BlockSyntax? block, ArrowExpressionClauseSyntax? arrow) =>
        (SyntaxNode?)block ?? arrow?.Expression;

    Span MakeSpan(TextSpan span)
    {
        var start = _text.Lines.GetLinePosition(span.Start);
        var end = _text.Lines.GetLinePosition(span.End);
        return new Span
        {
            StartLine = start.Line + 1,
            StartCol = start.Character,
            EndLine = end.Line + 1,
            EndCol = end.Character,
        };
    }

}
