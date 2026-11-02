//! Flatten a parsed module into the contract's packed AST: every node in
//! pre-order, each with its parent, subtree end and the strings it carries.

use std::collections::HashMap;

use ferrolint_core::{LineIndex, Range};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef, ExprContext as Ctx, HasNodeIndex, NodeIndex};
use ruff_text_size::Ranged;

use crate::wit::{self, ExprContext, NodeKind};

/// Words per node in the packed list `check` receives.
const WORDS: usize = 14;
/// The word that marks an absent name or detail.
const ABSENT: u32 = u32::MAX;

/// A byte span of the string table.
pub type Span = (u32, u32);

/// One node; its id is its position in [`Flat::nodes`].
#[derive(Clone, Debug)]
pub struct FlatNode {
    pub parent: Option<u32>,
    pub kind: NodeKind,
    pub range: Range,
    /// The id of the last node of its subtree.
    pub last: u32,
    pub context: ExprContext,
    pub name: Option<Span>,
    pub detail: Option<Span>,
}

/// A file's nodes in pre-order and the string table their names point into.
pub struct Flat {
    pub nodes: Vec<FlatNode>,
    pub strings: String,
}

impl Flat {
    /// The text of a name or detail span.
    pub fn string(&self, span: Option<Span>) -> Option<&str> {
        let (start, end) = span?;
        self.strings.get(start as usize..end as usize)
    }

    /// The contract's packed form, built once and handed to every plugin.
    pub fn pack(&self) -> wit::Ast {
        let mut words = Vec::with_capacity(self.nodes.len() * WORDS);
        for node in &self.nodes {
            let (name_start, name_end) = node.name.unwrap_or((ABSENT, ABSENT));
            let (detail_start, detail_end) = node.detail.unwrap_or((ABSENT, ABSENT));
            words.extend([
                node.parent.map_or(0, |parent| parent + 1),
                node.kind as u32,
                node.range.start.offset,
                node.range.start.line,
                node.range.start.column,
                node.range.end.offset,
                node.range.end.line,
                node.range.end.column,
                node.last,
                node.context as u32,
                name_start,
                name_end,
                detail_start,
                detail_end,
            ]);
        }
        wit::Ast {
            nodes: words,
            strings: self.strings.clone(),
        }
    }
}

/// The flat node list of `module`. Each emitted node's `node_index` is set
/// to its id, so a later pass over the same tree can name it.
pub fn flatten(source: &str, index: &LineIndex, module: &ast::ModModule) -> Flat {
    let mut flattener = Flattener {
        source,
        index,
        nodes: Vec::new(),
        stack: Vec::new(),
        strings: String::new(),
        interned: HashMap::new(),
    };
    flattener.enter_node(AnyNodeRef::from(module));
    for stmt in &module.body {
        flattener.visit_stmt(stmt);
    }
    flattener.leave_node(AnyNodeRef::from(module));
    Flat {
        nodes: flattener.nodes,
        strings: flattener.strings,
    }
}

struct Flattener<'s> {
    source: &'s str,
    index: &'s LineIndex,
    nodes: Vec<FlatNode>,
    /// One entry per entered node: its id, or none for a node left out.
    stack: Vec<Option<u32>>,
    strings: String,
    interned: HashMap<String, Span>,
}

impl Flattener<'_> {
    fn parent(&self) -> Option<u32> {
        self.stack.iter().rev().find_map(|entry| *entry)
    }

    fn parent_kind(&self) -> Option<NodeKind> {
        self.parent().map(|id| self.nodes[id as usize].kind)
    }

    fn text(&self, node: AnyNodeRef<'_>) -> String {
        self.source[node.range()].to_owned()
    }

    /// The table span of a string, adding it the first time.
    fn intern(&mut self, value: Option<String>) -> Option<Span> {
        let value = value?;
        if let Some(&span) = self.interned.get(&value) {
            return Some(span);
        }
        let start = u32::try_from(self.strings.len()).unwrap_or(ABSENT);
        self.strings.push_str(&value);
        let span = (start, u32::try_from(self.strings.len()).unwrap_or(ABSENT));
        self.interned.insert(value, span);
        Some(span)
    }
}

impl<'a> SourceOrderVisitor<'a> for Flattener<'_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        let Some((kind, name, detail, context)) = describe(self, node) else {
            self.stack.push(None);
            return TraversalSignal::Traverse;
        };
        let id = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);
        node.node_index().set(NodeIndex::from(id));
        let (name, detail) = (self.intern(name), self.intern(detail));
        let range = node.range();
        let range = self
            .index
            .range(self.source, range.start().to_u32(), range.end().to_u32());
        self.nodes.push(FlatNode {
            parent: self.parent(),
            kind,
            range,
            last: id,
            context,
            name,
            detail,
        });
        self.stack.push(Some(id));
        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, _node: AnyNodeRef<'a>) {
        if let Some(Some(id)) = self.stack.pop() {
            let last = u32::try_from(self.nodes.len() - 1).unwrap_or(u32::MAX);
            self.nodes[id as usize].last = last;
        }
    }
}

type Description = (NodeKind, Option<String>, Option<String>, ExprContext);

/// The kind and strings of a node, or none for a node the list leaves out
/// (its children still appear, under its nearest kept ancestor).
// One arm per node type: splitting the match would only scatter it.
#[allow(clippy::too_many_lines)]
fn describe(flattener: &Flattener<'_>, node: AnyNodeRef<'_>) -> Option<Description> {
    statement(flattener, node)
        .or_else(|| expression(flattener, node))
        .or_else(|| pattern(node))
        .or_else(|| other(flattener, node))
}

fn plain(kind: NodeKind) -> Description {
    (kind, None, None, ExprContext::None)
}

fn named(kind: NodeKind, name: &str) -> Description {
    (kind, Some(name.to_owned()), None, ExprContext::None)
}

fn detailed(kind: NodeKind, detail: String) -> Description {
    (kind, None, Some(detail), ExprContext::None)
}

fn flag(kind: NodeKind, set: bool, what: &str) -> Description {
    (kind, None, set.then(|| what.to_owned()), ExprContext::None)
}

/// The description of a statement node.
fn statement(flattener: &Flattener<'_>, node: AnyNodeRef<'_>) -> Option<Description> {
    use NodeKind as K;
    match node {
        AnyNodeRef::ModModule(_) => Some(plain(K::Module)),
        AnyNodeRef::StmtFunctionDef(def) => Some((
            K::FunctionDef,
            Some(def.name.to_string()),
            def.is_async.then(|| "async".to_owned()),
            ExprContext::None,
        )),
        AnyNodeRef::StmtClassDef(def) => Some(named(K::ClassDef, &def.name)),
        AnyNodeRef::StmtReturn(_) => Some(plain(K::Return)),
        AnyNodeRef::StmtDelete(_) => Some(plain(K::Delete)),
        AnyNodeRef::StmtTypeAlias(_) => Some(plain(K::TypeAlias)),
        AnyNodeRef::StmtAssign(_) => Some(plain(K::Assign)),
        AnyNodeRef::StmtAugAssign(assign) => {
            Some(detailed(K::AugAssign, format!("{}=", assign.op.as_str())))
        }
        AnyNodeRef::StmtAnnAssign(_) => Some(plain(K::AnnAssign)),
        AnyNodeRef::StmtFor(stmt) => Some(flag(K::For, stmt.is_async, "async")),
        AnyNodeRef::StmtWhile(_) => Some(plain(K::While)),
        AnyNodeRef::StmtIf(_) => Some(plain(K::If)),
        AnyNodeRef::StmtWith(stmt) => Some(flag(K::With, stmt.is_async, "async")),
        AnyNodeRef::StmtMatch(_) => Some(plain(K::Match)),
        AnyNodeRef::StmtRaise(_) => Some(plain(K::Raise)),
        AnyNodeRef::StmtTry(stmt) => Some(flag(K::Try, stmt.is_star, "star")),
        AnyNodeRef::StmtAssert(_) => Some(plain(K::Assert)),
        AnyNodeRef::StmtImport(_) => Some(plain(K::Import)),
        AnyNodeRef::StmtImportFrom(import) => {
            let module = import.module.as_ref().map_or("", |module| module.as_str());
            Some(detailed(
                K::ImportFrom,
                format!("{}{module}", ".".repeat(import.level as usize)),
            ))
        }
        AnyNodeRef::StmtGlobal(_) => Some(plain(K::Global)),
        AnyNodeRef::StmtNonlocal(_) => Some(plain(K::Nonlocal)),
        AnyNodeRef::StmtExpr(_) => Some(plain(K::Expr)),
        // `checks::parse` parses a bare `raise;` as `pass `: the source still says `raise`.
        AnyNodeRef::StmtPass(pass) => Some(plain(
            if flattener.source[pass.start().to_usize()..].starts_with("raise") {
                K::Raise
            } else {
                K::Pass
            },
        )),
        AnyNodeRef::StmtBreak(_) => Some(plain(K::Break)),
        AnyNodeRef::StmtContinue(_) => Some(plain(K::Continue)),
        _ => None,
    }
}

/// The description of an expression node.
fn expression(flattener: &Flattener<'_>, node: AnyNodeRef<'_>) -> Option<Description> {
    use NodeKind as K;
    match node {
        AnyNodeRef::ExprBoolOp(expr) => Some(detailed(K::BoolOp, expr.op.as_str().to_owned())),
        AnyNodeRef::ExprNamed(_) => Some(plain(K::Named)),
        AnyNodeRef::ExprBinOp(expr) => Some(detailed(K::BinOp, expr.op.as_str().to_owned())),
        AnyNodeRef::ExprUnaryOp(expr) => Some(detailed(K::UnaryOp, expr.op.as_str().to_owned())),
        AnyNodeRef::ExprLambda(_) => Some(plain(K::Lambda)),
        AnyNodeRef::ExprIf(_) => Some(plain(K::IfExp)),
        AnyNodeRef::ExprDict(_) => Some(plain(K::Dict)),
        AnyNodeRef::ExprSet(_) => Some(plain(K::Set)),
        AnyNodeRef::ExprListComp(_) => Some(plain(K::ListComp)),
        AnyNodeRef::ExprSetComp(_) => Some(plain(K::SetComp)),
        AnyNodeRef::ExprDictComp(_) => Some(plain(K::DictComp)),
        AnyNodeRef::ExprGenerator(_) => Some(plain(K::Generator)),
        AnyNodeRef::ExprAwait(_) => Some(plain(K::Await)),
        AnyNodeRef::ExprYield(_) => Some(plain(K::Yield)),
        AnyNodeRef::ExprYieldFrom(_) => Some(plain(K::YieldFrom)),
        AnyNodeRef::ExprCompare(expr) => Some(detailed(
            K::Compare,
            expr.ops
                .iter()
                .map(ast::CmpOp::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        )),
        AnyNodeRef::ExprCall(_) => Some(plain(K::Call)),
        AnyNodeRef::ExprFString(_) => Some(plain(K::FString)),
        AnyNodeRef::ExprTString(_) => Some(plain(K::TString)),
        AnyNodeRef::ExprStringLiteral(_) => Some(plain(K::StringLiteral)),
        AnyNodeRef::ExprBytesLiteral(_) => Some(plain(K::BytesLiteral)),
        AnyNodeRef::ExprNumberLiteral(_) => Some(detailed(K::NumberLiteral, flattener.text(node))),
        AnyNodeRef::ExprBooleanLiteral(_) => {
            Some(detailed(K::BooleanLiteral, flattener.text(node)))
        }
        AnyNodeRef::ExprNoneLiteral(_) => Some(plain(K::NoneLiteral)),
        AnyNodeRef::ExprEllipsisLiteral(_) => Some(plain(K::EllipsisLiteral)),
        AnyNodeRef::ExprAttribute(expr) => Some((
            K::Attribute,
            Some(expr.attr.to_string()),
            None,
            context(expr.ctx),
        )),
        AnyNodeRef::ExprSubscript(expr) => Some((K::Subscript, None, None, context(expr.ctx))),
        AnyNodeRef::ExprStarred(expr) => Some((K::Starred, None, None, context(expr.ctx))),
        AnyNodeRef::ExprName(expr) => {
            Some((K::Name, Some(expr.id.to_string()), None, context(expr.ctx)))
        }
        AnyNodeRef::ExprList(expr) => Some((K::List, None, None, context(expr.ctx))),
        AnyNodeRef::ExprTuple(expr) => Some((K::Tuple, None, None, context(expr.ctx))),
        AnyNodeRef::ExprSlice(_) => Some(plain(K::Slice)),
        _ => None,
    }
}

/// The description of a pattern node.
fn pattern(node: AnyNodeRef<'_>) -> Option<Description> {
    use NodeKind as K;
    match node {
        AnyNodeRef::PatternMatchValue(_) => Some(detailed(K::Pattern, "value".into())),
        AnyNodeRef::PatternMatchSingleton(_) => Some(detailed(K::Pattern, "singleton".into())),
        AnyNodeRef::PatternMatchSequence(_) => Some(detailed(K::Pattern, "sequence".into())),
        AnyNodeRef::PatternMatchMapping(pattern) => Some((
            K::Pattern,
            pattern.rest.as_ref().map(ToString::to_string),
            Some("mapping".into()),
            ExprContext::None,
        )),
        AnyNodeRef::PatternMatchClass(_) => Some(detailed(K::Pattern, "class".into())),
        AnyNodeRef::PatternMatchStar(pattern) => Some((
            K::Pattern,
            pattern.name.as_ref().map(ToString::to_string),
            Some("star".into()),
            ExprContext::None,
        )),
        AnyNodeRef::PatternMatchAs(pattern) => Some((
            K::Pattern,
            pattern.name.as_ref().map(ToString::to_string),
            Some("as".into()),
            ExprContext::None,
        )),
        AnyNodeRef::PatternMatchOr(_) => Some(detailed(K::Pattern, "or".into())),
        _ => None,
    }
}

/// The description of the other nodes (clauses, parameters, aliases), and the nodes that have none.
fn other(flattener: &Flattener<'_>, node: AnyNodeRef<'_>) -> Option<Description> {
    use NodeKind as K;
    match node {
        AnyNodeRef::ExceptHandlerExceptHandler(handler) => Some((
            K::ExceptHandler,
            handler.name.as_ref().map(ToString::to_string),
            None,
            ExprContext::None,
        )),
        AnyNodeRef::PatternKeyword(keyword) => Some(named(K::Keyword, &keyword.attr)),
        AnyNodeRef::TypeParamTypeVar(param) => Some(named(K::TypeParam, &param.name)),
        AnyNodeRef::TypeParamTypeVarTuple(param) => Some(named(K::TypeParam, &param.name)),
        AnyNodeRef::TypeParamParamSpec(param) => Some(named(K::TypeParam, &param.name)),
        AnyNodeRef::Comprehension(comprehension) => {
            Some(flag(K::Comprehension, comprehension.is_async, "async"))
        }
        AnyNodeRef::Arguments(_) => Some(plain(K::Arguments)),
        AnyNodeRef::Parameters(_) => Some(plain(K::Parameters)),
        AnyNodeRef::Parameter(parameter) => Some(named(K::Parameter, &parameter.name)),
        AnyNodeRef::Keyword(keyword) => Some((
            K::Keyword,
            keyword.arg.as_ref().map(ToString::to_string),
            None,
            ExprContext::None,
        )),
        AnyNodeRef::Alias(alias) => Some((
            K::Alias,
            Some(alias.name.to_string()),
            alias.asname.as_ref().map(ToString::to_string),
            ExprContext::None,
        )),
        AnyNodeRef::WithItem(_) => Some(plain(K::WithItem)),
        AnyNodeRef::MatchCase(_) => Some(plain(K::MatchCase)),
        AnyNodeRef::Decorator(_) => Some(plain(K::Decorator)),
        AnyNodeRef::ElifElseClause(_) => Some(plain(K::ElifElseClause)),
        AnyNodeRef::Identifier(identifier) => matches!(
            flattener.parent_kind(),
            Some(NodeKind::Global | NodeKind::Nonlocal)
        )
        .then(|| {
            (
                K::Identifier,
                Some(identifier.to_string()),
                None,
                ExprContext::None,
            )
        }),
        AnyNodeRef::StmtIpyEscapeCommand(_) | AnyNodeRef::ExprIpyEscapeCommand(_) => {
            Some(plain(K::Other))
        }
        // The other nodes (f-string parts, string and bytes literals, type parameter
        // lists, parameters with defaults) have no description of their own.
        _ => None,
    }
}

fn context(ctx: Ctx) -> ExprContext {
    match ctx {
        Ctx::Load => ExprContext::Load,
        Ctx::Store => ExprContext::Store,
        Ctx::Del => ExprContext::Del,
        Ctx::Invalid => ExprContext::None,
    }
}
