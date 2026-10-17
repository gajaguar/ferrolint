//! What a plugin declares and reports, with builders.

use crate::bindings::ferrolint::plugin::types as raw;
use crate::{ExprContext, NodeKind, Position, Range, Severity};

/// One AST node, its strings borrowed from the file's string table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node<'a> {
    pub id: u32,
    pub parent: Option<u32>,
    pub kind: NodeKind,
    pub range: Range,
    /// The id of the last node in this node's subtree.
    pub last: u32,
    pub context: ExprContext,
    /// A definition's name, a name's id, an attribute's attribute, an
    /// alias's dotted name, a keyword's argument, a parameter's name.
    pub name: Option<&'a str>,
    /// An alias's `as` name, an `import-from`'s module, an operator, a
    /// number or boolean literal's text.
    pub detail: Option<&'a str>,
}

/// Words per node in the packed AST.
const WORDS: usize = 14;
/// The word that marks an absent name or detail.
const ABSENT: u32 = u32::MAX;

/// Every `node-kind` case, in the contract's order: the packed AST names a
/// kind by its index here.
const KINDS: [NodeKind; 72] = {
    use NodeKind as K;
    [
        K::Module,
        K::FunctionDef,
        K::ClassDef,
        K::Return,
        K::Delete,
        K::Assign,
        K::AugAssign,
        K::AnnAssign,
        K::TypeAlias,
        K::For,
        K::While,
        K::If,
        K::With,
        K::Match,
        K::Raise,
        K::Try,
        K::Assert,
        K::Import,
        K::ImportFrom,
        K::Global,
        K::Nonlocal,
        K::Expr,
        K::Pass,
        K::Break,
        K::Continue,
        K::BoolOp,
        K::Named,
        K::BinOp,
        K::UnaryOp,
        K::Lambda,
        K::IfExp,
        K::Dict,
        K::Set,
        K::ListComp,
        K::SetComp,
        K::DictComp,
        K::Generator,
        K::Await,
        K::Yield,
        K::YieldFrom,
        K::Compare,
        K::Call,
        K::FString,
        K::TString,
        K::StringLiteral,
        K::BytesLiteral,
        K::NumberLiteral,
        K::BooleanLiteral,
        K::NoneLiteral,
        K::EllipsisLiteral,
        K::Attribute,
        K::Subscript,
        K::Starred,
        K::Name,
        K::List,
        K::Tuple,
        K::Slice,
        K::Alias,
        K::Arguments,
        K::Keyword,
        K::Parameters,
        K::Parameter,
        K::Decorator,
        K::ExceptHandler,
        K::Comprehension,
        K::WithItem,
        K::ElifElseClause,
        K::MatchCase,
        K::Pattern,
        K::TypeParam,
        K::Identifier,
        K::Other,
    ]
};

/// Every `expr-context` case, in the contract's order.
const CONTEXTS: [ExprContext; 4] = [
    ExprContext::None,
    ExprContext::Load,
    ExprContext::Store,
    ExprContext::Del,
];

impl<'a> Node<'a> {
    /// Unpack the AST the host sends. A malformed node (an index out of
    /// range) decodes as kind `other` with no strings.
    pub fn decode(ast: &'a raw::Ast) -> Vec<Self> {
        let string = |start: u32, end: u32| {
            (start != ABSENT)
                .then(|| ast.strings.get(start as usize..end as usize))
                .flatten()
        };
        ast.nodes
            .as_chunks::<WORDS>()
            .0
            .iter()
            .zip(0..)
            .map(|(words, id)| {
                let position = |at: usize| Position {
                    offset: words[at],
                    line: words[at + 1],
                    column: words[at + 2],
                };
                Self {
                    id,
                    parent: words[0].checked_sub(1),
                    kind: KINDS
                        .get(words[1] as usize)
                        .copied()
                        .unwrap_or(NodeKind::Other),
                    range: Range {
                        start: position(2),
                        end: position(5),
                    },
                    last: words[8],
                    context: CONTEXTS
                        .get(words[9] as usize)
                        .copied()
                        .unwrap_or(ExprContext::None),
                    name: string(words[10], words[11]),
                    detail: string(words[12], words[13]),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_table_follows_the_contract_order() {
        for (index, kind) in KINDS.iter().enumerate() {
            assert_eq!(*kind as usize, index, "{kind:?}");
        }
        assert_eq!(NodeKind::Other as usize + 1, KINDS.len());
        for (index, context) in CONTEXTS.iter().enumerate() {
            assert_eq!(*context as usize, index);
        }
    }
}

/// A plugin's name, version and rules.
#[derive(Clone, Debug)]
pub struct Info {
    pub name: String,
    pub version: String,
    pub rules: Vec<Rule>,
}

impl Info {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            rules: Vec::new(),
        }
    }

    /// Add a rule.
    #[must_use]
    pub fn rule(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    pub(crate) fn into_raw(self) -> raw::PluginInfo {
        raw::PluginInfo {
            name: self.name,
            version: self.version,
            rules: self.rules.into_iter().map(Rule::into_raw).collect(),
        }
    }
}

/// A rule a plugin reports, with the Pylint msg-id and symbol it ports.
#[derive(Clone, Debug)]
pub struct Rule {
    pub code: String,
    pub name: String,
    pub summary: String,
    pub severity: Severity,
    pub pylint_id: Option<String>,
    pub pylint_symbol: Option<String>,
}

impl Rule {
    /// A warning rule with no Pylint aliases.
    pub fn new(
        code: impl Into<String>,
        name: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
            summary: summary.into(),
            severity: Severity::Warning,
            pylint_id: None,
            pylint_symbol: None,
        }
    }

    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// The Pylint msg-id (`W0611`) and symbol (`unused-import`) this rule
    /// ports; `select`, `ignore` and suppressions accept either.
    #[must_use]
    pub fn pylint(mut self, msg_id: impl Into<String>, symbol: impl Into<String>) -> Self {
        self.pylint_id = Some(msg_id.into());
        self.pylint_symbol = Some(symbol.into());
        self
    }

    fn into_raw(self) -> raw::Rule {
        raw::Rule {
            code: self.code,
            name: self.name,
            summary: self.summary,
            severity: self.severity,
            pylint_id: self.pylint_id,
            pylint_symbol: self.pylint_symbol,
        }
    }
}

/// A suggested edit: replace `range` with `replacement`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fix {
    pub description: String,
    pub replacement: String,
    pub range: Range,
}

impl Fix {
    pub fn replace(
        range: Range,
        replacement: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            description: description.into(),
            replacement: replacement.into(),
            range,
        }
    }

    pub fn delete(range: Range, description: impl Into<String>) -> Self {
        Self::replace(range, "", description)
    }
}

/// A diagnostic to report. The severity defaults to the rule's own when
/// left unset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub range: Range,
    pub severity: Option<Severity>,
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>, range: Range) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            range,
            severity: None,
            fix: None,
        }
    }

    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
    pub fn fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    pub(crate) fn into_raw(self) -> raw::Diagnostic {
        raw::Diagnostic {
            code: self.code,
            message: self.message,
            range: self.range,
            severity: self.severity.unwrap_or(Severity::Warning),
            fix: self.fix.map(|fix| raw::Fix {
                description: fix.description,
                replacement: fix.replacement,
                range: fix.range,
            }),
        }
    }
}
