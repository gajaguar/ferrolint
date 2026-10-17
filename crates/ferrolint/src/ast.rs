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
    use NodeKind as K;
    let plain = |kind| Some((kind, None, None, ExprContext::None));
    let named = |kind, name: &str| Some((kind, Some(name.to_owned()), None, ExprContext::None));
    let detailed = |kind, detail: String| Some((kind, None, Some(detail), ExprContext::None));
    let flag = |kind, set: bool, what: &str| {
        Some((kind, None, set.then(|| what.to_owned()), ExprContext::None))
    };
    match node {
        AnyNodeRef::ModModule(_) => plain(K::Module),
        AnyNodeRef::StmtFunctionDef(def) => Some((
            K::FunctionDef,
            Some(def.name.to_string()),
            def.is_async.then(|| "async".to_owned()),
            ExprContext::None,
        )),
        AnyNodeRef::StmtClassDef(def) => named(K::ClassDef, &def.name),
        AnyNodeRef::StmtReturn(_) => plain(K::Return),
        AnyNodeRef::StmtDelete(_) => plain(K::Delete),
        AnyNodeRef::StmtTypeAlias(_) => plain(K::TypeAlias),
        AnyNodeRef::StmtAssign(_) => plain(K::Assign),
        AnyNodeRef::StmtAugAssign(assign) => {
            detailed(K::AugAssign, format!("{}=", assign.op.as_str()))
        }
        AnyNodeRef::StmtAnnAssign(_) => plain(K::AnnAssign),
        AnyNodeRef::StmtFor(stmt) => flag(K::For, stmt.is_async, "async"),
        AnyNodeRef::StmtWhile(_) => plain(K::While),
        AnyNodeRef::StmtIf(_) => plain(K::If),
        AnyNodeRef::StmtWith(stmt) => flag(K::With, stmt.is_async, "async"),
        AnyNodeRef::StmtMatch(_) => plain(K::Match),
        AnyNodeRef::StmtRaise(_) => plain(K::Raise),
        AnyNodeRef::StmtTry(stmt) => flag(K::Try, stmt.is_star, "star"),
        AnyNodeRef::StmtAssert(_) => plain(K::Assert),
        AnyNodeRef::StmtImport(_) => plain(K::Import),
        AnyNodeRef::StmtImportFrom(import) => {
            let module = import.module.as_ref().map_or("", |module| module.as_str());
            detailed(
                K::ImportFrom,
                format!("{}{module}", ".".repeat(import.level as usize)),
            )
        }
        AnyNodeRef::StmtGlobal(_) => plain(K::Global),
        AnyNodeRef::StmtNonlocal(_) => plain(K::Nonlocal),
        AnyNodeRef::StmtExpr(_) => plain(K::Expr),
        AnyNodeRef::StmtPass(_) => plain(K::Pass),
        AnyNodeRef::StmtBreak(_) => plain(K::Break),
        AnyNodeRef::StmtContinue(_) => plain(K::Continue),
        AnyNodeRef::ExprBoolOp(expr) => detailed(K::BoolOp, expr.op.as_str().to_owned()),
        AnyNodeRef::ExprNamed(_) => plain(K::Named),
        AnyNodeRef::ExprBinOp(expr) => detailed(K::BinOp, expr.op.as_str().to_owned()),
        AnyNodeRef::ExprUnaryOp(expr) => detailed(K::UnaryOp, expr.op.as_str().to_owned()),
        AnyNodeRef::ExprLambda(_) => plain(K::Lambda),
        AnyNodeRef::ExprIf(_) => plain(K::IfExp),
        AnyNodeRef::ExprDict(_) => plain(K::Dict),
        AnyNodeRef::ExprSet(_) => plain(K::Set),
        AnyNodeRef::ExprListComp(_) => plain(K::ListComp),
        AnyNodeRef::ExprSetComp(_) => plain(K::SetComp),
        AnyNodeRef::ExprDictComp(_) => plain(K::DictComp),
        AnyNodeRef::ExprGenerator(_) => plain(K::Generator),
        AnyNodeRef::ExprAwait(_) => plain(K::Await),
        AnyNodeRef::ExprYield(_) => plain(K::Yield),
        AnyNodeRef::ExprYieldFrom(_) => plain(K::YieldFrom),
        AnyNodeRef::ExprCompare(expr) => detailed(
            K::Compare,
            expr.ops
                .iter()
                .map(ast::CmpOp::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        AnyNodeRef::ExprCall(_) => plain(K::Call),
        AnyNodeRef::ExprFString(_) => plain(K::FString),
        AnyNodeRef::ExprTString(_) => plain(K::TString),
        AnyNodeRef::ExprStringLiteral(_) => plain(K::StringLiteral),
        AnyNodeRef::ExprBytesLiteral(_) => plain(K::BytesLiteral),
        AnyNodeRef::ExprNumberLiteral(_) => detailed(K::NumberLiteral, flattener.text(node)),
        AnyNodeRef::ExprBooleanLiteral(_) => detailed(K::BooleanLiteral, flattener.text(node)),
        AnyNodeRef::ExprNoneLiteral(_) => plain(K::NoneLiteral),
        AnyNodeRef::ExprEllipsisLiteral(_) => plain(K::EllipsisLiteral),
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
        AnyNodeRef::ExprSlice(_) => plain(K::Slice),
        AnyNodeRef::ExceptHandlerExceptHandler(handler) => Some((
            K::ExceptHandler,
            handler.name.as_ref().map(ToString::to_string),
            None,
            ExprContext::None,
        )),
        AnyNodeRef::PatternMatchValue(_) => detailed(K::Pattern, "value".into()),
        AnyNodeRef::PatternMatchSingleton(_) => detailed(K::Pattern, "singleton".into()),
        AnyNodeRef::PatternMatchSequence(_) => detailed(K::Pattern, "sequence".into()),
        AnyNodeRef::PatternMatchMapping(pattern) => Some((
            K::Pattern,
            pattern.rest.as_ref().map(ToString::to_string),
            Some("mapping".into()),
            ExprContext::None,
        )),
        AnyNodeRef::PatternMatchClass(_) => detailed(K::Pattern, "class".into()),
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
        AnyNodeRef::PatternMatchOr(_) => detailed(K::Pattern, "or".into()),
        AnyNodeRef::PatternKeyword(keyword) => named(K::Keyword, &keyword.attr),
        AnyNodeRef::TypeParamTypeVar(param) => named(K::TypeParam, &param.name),
        AnyNodeRef::TypeParamTypeVarTuple(param) => named(K::TypeParam, &param.name),
        AnyNodeRef::TypeParamParamSpec(param) => named(K::TypeParam, &param.name),
        AnyNodeRef::Comprehension(comprehension) => {
            flag(K::Comprehension, comprehension.is_async, "async")
        }
        AnyNodeRef::Arguments(_) => plain(K::Arguments),
        AnyNodeRef::Parameters(_) => plain(K::Parameters),
        AnyNodeRef::Parameter(parameter) => named(K::Parameter, &parameter.name),
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
        AnyNodeRef::WithItem(_) => plain(K::WithItem),
        AnyNodeRef::MatchCase(_) => plain(K::MatchCase),
        AnyNodeRef::Decorator(_) => plain(K::Decorator),
        AnyNodeRef::ElifElseClause(_) => plain(K::ElifElseClause),
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
            plain(K::Other)
        }
        AnyNodeRef::ModExpression(_)
        | AnyNodeRef::InterpolatedElement(_)
        | AnyNodeRef::InterpolatedStringLiteralElement(_)
        | AnyNodeRef::InterpolatedStringFormatSpec(_)
        | AnyNodeRef::PatternArguments(_)
        | AnyNodeRef::ParameterWithDefault(_)
        | AnyNodeRef::TypeParams(_)
        | AnyNodeRef::FString(_)
        | AnyNodeRef::TString(_)
        | AnyNodeRef::StringLiteral(_)
        | AnyNodeRef::BytesLiteral(_) => None,
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
