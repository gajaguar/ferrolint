//! A port of Pylint's `unused-import` checker (W0611), the sample plugin of
//! the ferrolint SDK.
//!
//! Like Pylint, an import counts as used when any name read in the scope
//! it binds into, or in a scope nested under it, finds that name there:
//! the check is by name, not by binding, so `import json; json = 1;
//! print(json)` uses the import. On top of name reads it counts `del`,
//! names listed in a module's `__all__`, and names inside string
//! annotations. It leaves alone what Pylint leaves alone with its default
//! options: `__future__` imports, imports bound to `_`, imports in a class
//! body, imports under `if TYPE_CHECKING:`, and every import of an
//! `__init__.py`.

use std::collections::HashSet;

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "IMP001";

#[derive(Default)]
struct UnusedImport {
    imports: Vec<Import>,
    /// (scope, name) pairs some read reached.
    used: HashSet<(u32, String)>,
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

    fn visit(&mut self, node: &Node, cx: &mut Context<'_>) {
        match node.kind {
            NodeKind::Alias => self.collect(node, cx),
            NodeKind::Name if matches!(node.context, ExprContext::Load | ExprContext::Del) => {
                if let (Some(name), Some(binding)) = (&node.name, cx.resolve(node)) {
                    self.used.insert((binding.scope, name.clone()));
                }
            }
            NodeKind::StringLiteral if in_annotation(node, cx) => {
                let visible = cx.visible_bindings(node);
                for name in annotation_names(cx.text(node)) {
                    if let Some(binding) = visible.iter().find(|binding| binding.name == name) {
                        self.used.insert((binding.scope, name));
                    }
                }
            }
            NodeKind::Assign if node.parent == Some(0) => {
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
    fn collect(&mut self, alias: &Node, cx: &Context<'_>) {
        let Some(statement) = cx.parent(alias) else {
            return;
        };
        let dotted = alias.name.clone().unwrap_or_default();
        let asname = alias.detail.as_deref();
        let module = match statement.kind {
            NodeKind::Import => None,
            NodeKind::ImportFrom => Some(statement.detail.as_deref().unwrap_or("")),
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
        let imported = match module.map(|module| module.trim_start_matches('.')) {
            None | Some("") => match asname {
                None => format!("import {dotted}"),
                Some(asname) => format!("{dotted} imported as {asname}"),
            },
            Some(module) => match asname {
                None => format!("{dotted} imported from {module}"),
                Some(asname) => format!("{dotted} imported from {module} as {asname}"),
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
fn in_annotation(node: &Node, cx: &Context<'_>) -> bool {
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
            let prefix = matches!(bytes.get(index), Some(b'"' | b'\''));
            if !after_dot && !prefix {
                names.push(text[start..index].to_owned());
            }
        } else {
            index += 1;
        }
    }
    names
}

/// The names a module-level `__all__ = [...]` (or tuple) lists.
fn dunder_all(assign: &Node, cx: &Context<'_>) -> Vec<String> {
    let children: Vec<&Node> = cx.children(assign).collect();
    let [target, value] = children.as_slice() else {
        return Vec::new();
    };
    if target.name.as_deref() != Some("__all__")
        || !matches!(value.kind, NodeKind::List | NodeKind::Tuple)
    {
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

/// Whether the import sits in the body of an `if TYPE_CHECKING:` block.
fn type_checking_only(statement: &Node, cx: &Context<'_>) -> bool {
    let mut child = statement;
    for parent in cx.ancestors(statement) {
        if parent.kind == NodeKind::If && Context::is_statement(child) {
            let guard = cx.children(parent).next();
            if guard.is_some_and(|test| test.name.as_deref() == Some("TYPE_CHECKING")) {
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
fn removal(statement: &Node, alias: &Node, cx: &Context<'_>) -> Fix {
    let name = alias
        .detail
        .as_deref()
        .or(alias.name.as_deref())
        .unwrap_or("");
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

fn span(start: Position, end: Position) -> Range {
    Range { start, end }
}

/// Whether deleting the statement would leave its block empty.
fn only_statement(statement: &Node, cx: &Context<'_>) -> bool {
    let Some(parent) = cx.parent(statement) else {
        return false;
    };
    let single_body = matches!(
        parent.kind,
        NodeKind::Module
            | NodeKind::FunctionDef
            | NodeKind::ClassDef
            | NodeKind::If
            | NodeKind::ElifElseClause
            | NodeKind::With
            | NodeKind::ExceptHandler
            | NodeKind::MatchCase
    );
    if !single_body {
        return true;
    }
    parent.kind != NodeKind::Module
        && cx
            .children(parent)
            .filter(|child| Context::is_statement(child))
            .count()
            == 1
}

/// The statement's range grown to whole lines when nothing else shares
/// them, so deleting it leaves no blank line behind.
fn whole_lines(statement: &Node, source: &str) -> Range {
    let range = statement.range;
    let (start, end) = (range.start.offset as usize, range.end.offset as usize);
    let line_start = source[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let rest = &source[end..];
    let line_end = rest
        .find('\n')
        .map_or(source.len(), |newline| end + newline + 1);
    let before_blank = source[line_start..start].trim().is_empty();
    let after = source[end..line_end].trim();
    if !before_blank || !(after.is_empty() || after.starts_with('#')) {
        return range;
    }
    let shift = |position: Position, offset: usize, line: u32| Position {
        offset: u32::try_from(offset).unwrap_or(position.offset),
        line,
        column: 1,
    };
    let lines_after = u32::from(line_end > end && source.as_bytes()[line_end - 1] == b'\n');
    Range {
        start: shift(range.start, line_start, range.start.line),
        end: shift(range.end, line_end, range.end.line + lines_after),
    }
}

export_checker!(UnusedImport);
