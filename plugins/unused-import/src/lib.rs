//! A port of Pylint's `unused-import` checker (W0611), the sample plugin of
//! the ferrolint SDK.
//!
//! Like Pylint, an import counts as used when any name read in the scope
//! it binds into, or in a scope nested under it, finds that name there:
//! the check is by name, not by binding, so `import json; json = 1;
//! print(json)` uses the import. On top of name reads it counts `del`,
//! the target of an augmented assignment, names listed in a module's
//! `__all__`, and names inside string annotations. It leaves alone what Pylint leaves alone with its default
//! options: `__future__` imports, imports bound to `_`, imports in a class
//! body, imports under `if TYPE_CHECKING:`, and every import of an
//! `__init__.py`.

use std::collections::{HashMap, HashSet};

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "IMP001";

#[derive(Default)]
struct UnusedImport {
    imports: Vec<Import>,
    /// (scope, name) pairs some read reached.
    used: HashSet<(u32, String)>,
    /// The bindings visible in each scope, fetched once per scope for the
    /// string annotations in it.
    visible: HashMap<u32, Vec<Binding>>,
}

/// An import alias that binds a name.
struct Import {
    statement: u32,
    alias: u32,
    scope: u32,
    name: String,
    message: String,
}

impl Checker for UnusedImport {
    fn info() -> Info {
        Info::new("unused-import", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(CODE, "unused-import", "An imported name is never used.")
                .pylint("W0611", "unused-import"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        match node.kind {
            NodeKind::Alias => self.collect(node, cx),
            NodeKind::Name if matches!(node.context, ExprContext::Load | ExprContext::Del) => {
                if let (Some(name), Some(binding)) = (node.name, cx.resolve(node)) {
                    self.used.insert((binding.scope, name.to_owned()));
                }
            }
            // Pylint reads the target of `name += ...` as a use of `name`.
            NodeKind::Name
                if node.context == ExprContext::Store
                    && cx
                        .parent(node)
                        .is_some_and(|parent| parent.kind == NodeKind::AugAssign) =>
            {
                let name = node.name.unwrap_or_default();
                if let Some(binding) = cx
                    .visible_bindings(node)
                    .into_iter()
                    .find(|binding| binding.name == name)
                {
                    self.used.insert((binding.scope, name.to_owned()));
                }
            }
            NodeKind::StringLiteral if in_annotation(node, cx) => {
                let scope = cx.scope(node).id;
                let visible = self
                    .visible
                    .entry(scope)
                    .or_insert_with(|| cx.visible_bindings(node));
                for name in annotation_names(cx.text(node)) {
                    if let Some(binding) = visible.iter().find(|binding| binding.name == name) {
                        self.used.insert((binding.scope, name));
                    }
                }
            }
            NodeKind::Assign if cx.scope(node).kind == ScopeKind::Module => {
                for name in dunder_all(node, cx) {
                    self.used.insert((0, name));
                }
            }
            _ => {}
        }
    }

    fn end_file(&mut self, cx: &mut Context<'_>) {
        if cx.path() == "__init__.py" || cx.path().ends_with("/__init__.py") {
            return;
        }
        for import in &self.imports {
            if self.used.contains(&(import.scope, import.name.clone())) {
                continue;
            }
            let (Some(statement), Some(alias)) = (cx.node(import.statement), cx.node(import.alias))
            else {
                continue;
            };
            let fix = removal(statement, alias, cx);
            cx.report(Diagnostic::new(CODE, &import.message, statement.range).fix(fix));
        }
    }
}

impl UnusedImport {
    fn collect(&mut self, alias: &Node<'_>, cx: &Context<'_>) {
        let Some(statement) = cx.parent(alias) else {
            return;
        };
        let dotted = alias.name.unwrap_or_default();
        let asname = alias.detail;
        let module = match statement.kind {
            NodeKind::Import => None,
            NodeKind::ImportFrom => Some(statement.detail.unwrap_or("")),
            _ => return,
        };
        if module == Some("__future__") || dotted == "*" || asname == Some("_") {
            return;
        }
        let scope = cx.scope(alias);
        if scope.kind == ScopeKind::Class || type_checking_only(statement, cx) {
            return;
        }
        let Some(binding) = cx.resolve(alias) else {
            return;
        };
        // Pylint words a relative import with no module ("from . import x")
        // as "import x" in a module and as "x imported from " in a function.
        let imported = match module.map(|module| module.trim_start_matches('.')) {
            Some(module) if !module.is_empty() || scope.kind == ScopeKind::Function => match asname
            {
                None => format!("{dotted} imported from {module}"),
                Some(asname) => format!("{dotted} imported from {module} as {asname}"),
            },
            _ => match asname {
                None => format!("import {dotted}"),
                Some(asname) => format!("{dotted} imported as {asname}"),
            },
        };
        self.imports.push(Import {
            statement: statement.id,
            alias: alias.id,
            scope: binding.scope,
            name: binding.name,
            message: format!("Unused {imported}"),
        });
    }
}

/// Whether a string literal is (part of) an annotation: of a parameter, of
/// a function's return, or of an annotated assignment.
fn in_annotation(node: &Node<'_>, cx: &Context<'_>) -> bool {
    let mut child = node;
    for parent in cx.ancestors(node) {
        match parent.kind {
            NodeKind::Parameter => return true,
            NodeKind::FunctionDef => {
                return !Context::is_statement(child)
                    && !matches!(
                        child.kind,
                        NodeKind::Decorator | NodeKind::TypeParam | NodeKind::Parameters
                    );
            }
            NodeKind::AnnAssign => {
                return cx.children(parent).nth(1).map(|n| n.id) == Some(child.id);
            }
            NodeKind::Lambda => return false,
            _ if Context::is_statement(parent) => return false,
            _ => child = parent,
        }
    }
    false
}

/// The first component of every dotted name in a string annotation's
/// source text, string prefixes left out.
fn annotation_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let after_dot = start > 0 && bytes[start - 1] == b'.';
            let prefix = string_prefix(&text[start..index])
                && (start == 0 || bytes[start - 1].is_ascii_whitespace())
                && matches!(bytes.get(index), Some(b'"' | b'\''));
            if !after_dot && !prefix {
                names.push(text[start..index].to_owned());
            }
        } else {
            index += 1;
        }
    }
    names
}

/// Whether a word is a string literal prefix such as `r` or `rb`.
fn string_prefix(word: &str) -> bool {
    word.len() <= 2 && word.chars().all(|c| "rRbBuUfFtT".contains(c))
}

/// The names a module-level `__all__ = [...]` (or tuple) lists.
fn dunder_all(assign: &Node<'_>, cx: &Context<'_>) -> Vec<String> {
    let children: Vec<&Node> = cx.children(assign).collect();
    let [target, value] = children.as_slice() else {
        return Vec::new();
    };
    if target.name != Some("__all__") || !matches!(value.kind, NodeKind::List | NodeKind::Tuple) {
        return Vec::new();
    }
    cx.children(value)
        .filter(|element| element.kind == NodeKind::StringLiteral)
        .map(|element| {
            cx.text(element)
                .trim_matches(|c| c == '"' || c == '\'')
                .to_owned()
        })
        .collect()
}

/// Whether the import sits in the body of an `if TYPE_CHECKING:` or
/// `elif TYPE_CHECKING:` block.
fn type_checking_only(statement: &Node<'_>, cx: &Context<'_>) -> bool {
    let mut child = statement;
    for parent in cx.ancestors(statement) {
        if matches!(parent.kind, NodeKind::If | NodeKind::ElifElseClause)
            && Context::is_statement(child)
        {
            let guard = cx.children(parent).next();
            if guard.is_some_and(|test| test.id != child.id && test.name == Some("TYPE_CHECKING")) {
                return true;
            }
        }
        child = parent;
    }
    false
}

/// The fix: drop the alias from a statement importing several names, drop
/// the whole statement when it imports one, or put `pass` in its place
/// when it is the only statement of its block.
fn removal(statement: &Node, alias: &Node<'_>, cx: &Context<'_>) -> Fix {
    let name = alias.detail.or(alias.name).unwrap_or("");
    let description = format!("Remove the unused import of {name}");
    let aliases: Vec<&Node> = cx
        .children(statement)
        .filter(|child| child.kind == NodeKind::Alias)
        .collect();
    if aliases.len() > 1 {
        let position = aliases
            .iter()
            .position(|other| other.id == alias.id)
            .unwrap_or(0);
        let range = match aliases.get(position + 1) {
            Some(next) => span(alias.range.start, next.range.start),
            None => span(aliases[position - 1].range.end, alias.range.end),
        };
        return Fix::delete(range, description);
    }
    if only_statement(statement, cx) {
        return Fix::replace(statement.range, "pass", description);
    }
    Fix::delete(whole_lines(statement, cx.source()), description)
}

/// A range over two byte offsets; the host recomputes lines and columns
/// from the offsets, so only those need to be right.
fn offsets(range: Range, start: usize, end: usize) -> Range {
    let at = |position: Position, offset: usize| Position {
        offset: u32::try_from(offset).unwrap_or(position.offset),
        ..position
    };
    Range {
        start: at(range.start, start),
        end: at(range.end, end),
    }
}

fn span(start: Position, end: Position) -> Range {
    Range { start, end }
}

/// Whether deleting the statement would leave its block empty. Siblings
/// belong to the same block unless an `else`, `elif`, `except` or `finally`
/// line separates them (the bodies of `for`, `while` and `try` share a
/// parent node).
fn only_statement(statement: &Node<'_>, cx: &Context<'_>) -> bool {
    let Some(parent) = cx.parent(statement) else {
        return false;
    };
    if parent.kind == NodeKind::Module {
        return false;
    }
    let siblings: Vec<&Node> = cx
        .children(parent)
        .filter(|child| Context::is_statement(child))
        .collect();
    let Some(position) = siblings
        .iter()
        .position(|sibling| sibling.id == statement.id)
    else {
        return true;
    };
    let same_block = |a: &Node, b: &Node| {
        let gap = cx
            .source()
            .get(a.range.end.offset as usize..b.range.start.offset as usize)
            .unwrap_or("");
        !gap.lines().any(|line| {
            let line = line.trim_start();
            ["else", "elif", "except", "finally"].iter().any(|keyword| {
                line.strip_prefix(keyword)
                    .is_some_and(|rest| rest.starts_with([':', ' ', '(', '*']))
            })
        })
    };
    let before = position > 0 && same_block(siblings[position - 1], statement);
    let after = siblings
        .get(position + 1)
        .is_some_and(|next| same_block(statement, next));
    !before && !after
}

/// The statement's range grown to whole lines when nothing else shares
/// them, so deleting it leaves no blank line behind.
fn whole_lines(statement: &Node<'_>, source: &str) -> Range {
    let range = statement.range;
    let (start, end) = (range.start.offset as usize, range.end.offset as usize);
    let line_start = source[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let rest = &source[end..];
    let line_end = rest
        .find('\n')
        .map_or(source.len(), |newline| end + newline + 1);
    let before = source[line_start..start].trim_end();
    let after = source[end..line_end].trim();
    if let Some(rest) = source[end..line_end].trim_start().strip_prefix(';') {
        // `import os; x = 1`: take the semicolon and the space after it.
        let gap = source[end..line_end].len() - rest.len() + (rest.len() - rest.trim_start().len());
        return offsets(range, start, end + gap);
    }
    if before.ends_with(';') {
        // `x = 1; import os`: take the semicolon before it.
        return offsets(range, line_start + before.len() - 1, end);
    }
    if !before.trim().is_empty() || !(after.is_empty() || after.starts_with('#')) {
        return range;
    }
    offsets(range, line_start, line_end)
}

export_checker!(UnusedImport);
