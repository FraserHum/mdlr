using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace MdlrExtractCSharp;

/// Syntax-only metrics for a member body. Lambdas and anonymous methods are
/// folded into the containing member; local functions are excluded (they are
/// extracted as separate units).
public static class MemberMetrics
{
    /// Branch points for cyclomatic complexity (cyclomatic = branches + 1).
    /// Counted: if, loops, catch, catch-filter `when`, case-guard `when`,
    /// switch sections/arms (n-1), ternary, &&, ||.
    /// Not counted: ??, ??=, ?. (mirrors the TS extractor, which skips
    /// nullish coalescing and optional chaining).
    public static int CountBranches(SyntaxNode body)
    {
        var count = 0;
        foreach (var node in Folded(body))
        {
            switch (node)
            {
                case IfStatementSyntax:
                case ForStatementSyntax:
                case ForEachStatementSyntax:
                case ForEachVariableStatementSyntax:
                case WhileStatementSyntax:
                case DoStatementSyntax:
                case ConditionalExpressionSyntax:
                case CatchClauseSyntax:
                case CatchFilterClauseSyntax:
                case WhenClauseSyntax:
                    count++;
                    break;
                case SwitchStatementSyntax sw when sw.Sections.Count > 1:
                    count += sw.Sections.Count - 1;
                    break;
                case SwitchExpressionSyntax swe when swe.Arms.Count > 1:
                    count += swe.Arms.Count - 1;
                    break;
                case BinaryExpressionSyntax bin when
                    bin.IsKind(SyntaxKind.LogicalAndExpression) ||
                    bin.IsKind(SyntaxKind.LogicalOrExpression):
                    count++;
                    break;
            }
        }
        return count;
    }

    /// Descendants of `body`, skipping local function subtrees but descending
    /// into lambdas/anonymous methods (folded into the parent).
    static IEnumerable<SyntaxNode> Folded(SyntaxNode body) =>
        body.DescendantNodesAndSelf(n => n is not LocalFunctionStatementSyntax);

    /// Cognitive complexity (SonarSource formulation, mirroring the Go
    /// extractor): nesting-incrementing constructs cost 1 + nesting, plain
    /// else costs 1, else-if chains stay at the chain's nesting level,
    /// && and || cost 1 each, lambdas fold at the current nesting.
    public static int ComputeCognitive(SyntaxNode body)
    {
        var v = new CognitiveVisitor();
        v.WalkRoot(body);
        return v.Score;
    }

    sealed class CognitiveVisitor
    {
        public int Score;

        public void WalkRoot(SyntaxNode body) => WalkNode(body, 0);

        void Walk(SyntaxNode node, int nesting)
        {
            foreach (var child in node.ChildNodes())
                WalkNode(child, nesting);
        }

        void WalkNode(SyntaxNode node, int nesting)
        {
            switch (node)
            {
                case LocalFunctionStatementSyntax:
                    return; // separate unit
                case IfStatementSyntax ifStmt:
                    WalkIf(ifStmt, nesting);
                    return;
                case ForStatementSyntax f:
                    Score += 1 + nesting;
                    foreach (var part in f.ChildNodes())
                        WalkNode(part, part == f.Statement ? nesting + 1 : nesting);
                    return;
                case ForEachStatementSyntax fe:
                    Score += 1 + nesting;
                    WalkNode(fe.Expression, nesting);
                    WalkNode(fe.Statement, nesting + 1);
                    return;
                case ForEachVariableStatementSyntax fev:
                    Score += 1 + nesting;
                    WalkNode(fev.Expression, nesting);
                    WalkNode(fev.Statement, nesting + 1);
                    return;
                case WhileStatementSyntax w:
                    Score += 1 + nesting;
                    WalkNode(w.Condition, nesting);
                    WalkNode(w.Statement, nesting + 1);
                    return;
                case DoStatementSyntax d:
                    Score += 1 + nesting;
                    WalkNode(d.Statement, nesting + 1);
                    WalkNode(d.Condition, nesting);
                    return;
                case SwitchStatementSyntax sw:
                    Score += 1 + nesting;
                    WalkNode(sw.Expression, nesting);
                    foreach (var section in sw.Sections)
                        Walk(section, nesting + 1);
                    return;
                case SwitchExpressionSyntax swe:
                    Score += 1 + nesting;
                    WalkNode(swe.GoverningExpression, nesting);
                    foreach (var arm in swe.Arms)
                        Walk(arm, nesting + 1);
                    return;
                case CatchClauseSyntax c:
                    Score += 1 + nesting;
                    Walk(c, nesting + 1);
                    return;
                case ConditionalExpressionSyntax cond:
                    Score += 1 + nesting;
                    Walk(cond, nesting);
                    return;
                case BinaryExpressionSyntax bin when
                    bin.IsKind(SyntaxKind.LogicalAndExpression) ||
                    bin.IsKind(SyntaxKind.LogicalOrExpression):
                    Score += 1;
                    Walk(bin, nesting);
                    return;
                case AnonymousFunctionExpressionSyntax lambda:
                    // Fold at the current nesting, mirroring Go's FuncLit handling.
                    if (lambda.Body is SyntaxNode lb)
                        WalkNode(lb, nesting);
                    return;
                default:
                    Walk(node, nesting);
                    return;
            }
        }

        void WalkIf(IfStatementSyntax ifStmt, int nesting)
        {
            Score += 1 + nesting;
            WalkNode(ifStmt.Condition, nesting);
            WalkNode(ifStmt.Statement, nesting + 1);

            if (ifStmt.Else is not { } elseClause) return;
            if (elseClause.Statement is IfStatementSyntax elseIf)
            {
                // else-if: same nesting; the chained if adds its own +1.
                WalkIf(elseIf, nesting);
            }
            else
            {
                Score += 1; // plain else: +1, no nesting penalty
                WalkNode(elseClause.Statement, nesting + 1);
            }
        }
    }

    /// Largest nested scope in lines, excluding the member's own top-level
    /// body block. Lambda bodies fold into the parent: the lambda's immediate
    /// block is not a scope boundary, but blocks inside it are.
    public static int MaxScopeLines(SyntaxNode body, SourceText text)
    {
        var max = 0;
        var lambdaBodies = new HashSet<SyntaxNode>();

        foreach (var node in Folded(body))
        {
            switch (node)
            {
                case AnonymousFunctionExpressionSyntax lambda:
                    if (lambda.Body is BlockSyntax lambdaBlock)
                        lambdaBodies.Add(lambdaBlock);
                    break;
                case BlockSyntax block when block != body && !lambdaBodies.Contains(block):
                    max = Math.Max(max, SpanLines(block.Span, text));
                    break;
                case SwitchSectionSyntax section when section.Statements.Count > 0:
                    var span = TextSpan.FromBounds(
                        section.SpanStart, section.Statements.Last().Span.End);
                    max = Math.Max(max, SpanLines(span, text));
                    break;
            }
        }
        return max;
    }

    static int SpanLines(TextSpan span, SourceText text)
    {
        var start = text.Lines.GetLinePosition(span.Start).Line;
        var end = text.Lines.GetLinePosition(span.End).Line;
        return Math.Max(0, end - start + 1);
    }
}
