//! Import rules: `C0415` import-outside-toplevel, `W0404` reimported,
//! `W0407` preferred-module and `W0416` shadowed-import.
//!
use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Alias, ExceptHandler, Stmt};
use ruff_text_size::Ranged;

use super::classes::py_repr;
use super::{Checker, Ctx, rule};

const OUTSIDE: &str = "C0415";
const REIMPORTED: &str = "W0404";
const PREFERRED: &str = "W0407";
const SHADOWED: &str = "W0416";

pub struct Imports;

impl Checker for Imports {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                OUTSIDE,
                "import-outside-toplevel",
                "An import statement outside the module's top level.",
            ),
            rule(
                REIMPORTED,
                "reimported",
                "A module or name imported more than once.",
            ),
            rule(
                PREFERRED,
                "preferred-module",
                "A module that `preferred-modules` replaces.",
            ),
            rule(
                SHADOWED,
                "shadowed-import",
                "A module aliased with the name of another import.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![OUTSIDE, REIMPORTED, PREFERRED, SHADOWED]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let mut walk = Walk {
            imports: Vec::new(),
            frames: 1,
            frame: 0,
            path: Vec::new(),
            base: 0,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
        for import in &mut walk.imports {
            import.line = line_of(ctx, import.stmt);
        }
        let preferred = preferred(ctx);
        let allowed = ctx
            .option("allow-any-import-level")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .collect::<BTreeSet<_>>();
        for import in &walk.imports {
            if enabled.contains(OUTSIDE) && import.frame != 0 {
                outside(ctx, import, &allowed, out);
            }
            if enabled.contains(PREFERRED) {
                preferred_module(ctx, import, &preferred, out);
            }
            if enabled.contains(REIMPORTED) {
                same_line(ctx, import, out);
            }
            if enabled.contains(REIMPORTED) || enabled.contains(SHADOWED) {
                reimport(ctx, &walk, import, enabled, out);
            }
        }
    }
}

fn line_of(ctx: &Ctx<'_>, stmt: &Stmt) -> u32 {
    let start = stmt.range().start().to_u32();
    ctx.index.range(ctx.source, start, start).start.line
}

/// One compound statement on the way to a statement, and the branch of it
/// the way takes: what Pylint's `are_exclusive` compares.
#[derive(PartialEq, Eq, Clone, Copy)]
struct Step {
    id: u32,
    slot: usize,
    kind: StepKind,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum StepKind {
    If,
    Try,
    Other,
}

const HANDLERS: usize = 1000;

/// Whether two statements sit in branches of an `if` or a `try` that never
/// both run (Pylint's `are_exclusive` without exception names).
fn exclusive(a: &[Step], b: &[Step]) -> bool {
    let Some(at) = a.iter().zip(b).position(|(x, y)| x != y) else {
        return false;
    };
    let (x, y) = (a[at], b[at]);
    if x.id != y.id {
        return false;
    }
    match x.kind {
        StepKind::If => true,
        StepKind::Try => {
            let handler = |s: usize| (1..HANDLERS).contains(&s);
            // A handler excludes the body, the `else` block and each other
            // handler; the `finally` block excludes nothing.
            (handler(x.slot) || handler(y.slot)) && x.slot != HANDLERS + 1 && y.slot != HANDLERS + 1
        }
        StepKind::Other => false,
    }
}

struct Import<'a> {
    stmt: &'a Stmt,
    frame: usize,
    /// Written directly in the body of its frame.
    direct: bool,
    path: Vec<Step>,
    line: u32,
}

struct Walk<'a> {
    imports: Vec<Import<'a>>,
    /// How many frames (module, functions and classes) the walk has opened.
    frames: usize,
    frame: usize,
    path: Vec<Step>,
    /// How long `path` was when the current frame started.
    base: usize,
}

impl<'a> Walk<'a> {
    fn branch(&mut self, stmt: &Stmt, slot: usize, kind: StepKind, body: &'a [Stmt]) {
        self.path.push(Step {
            id: stmt.range().start().to_u32(),
            slot,
            kind,
        });
        self.visit_body(body);
        self.path.pop();
    }
}

impl<'a> Visitor<'a> for Walk<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(_) | Stmt::ImportFrom(_) => self.imports.push(Import {
                stmt,
                frame: self.frame,
                direct: self.path.len() == self.base,
                path: self.path.clone(),
                line: 0,
            }),
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                let (frame, base) = (self.frame, self.base);
                self.frame = self.frames;
                self.frames += 1;
                self.path.push(Step {
                    id: stmt.range().start().to_u32(),
                    slot: 0,
                    kind: StepKind::Other,
                });
                self.base = self.path.len();
                visitor::walk_stmt(self, stmt);
                self.path.pop();
                (self.frame, self.base) = (frame, base);
            }
            Stmt::If(s) => {
                self.branch(stmt, 0, StepKind::If, &s.body);
                for (i, clause) in s.elif_else_clauses.iter().enumerate() {
                    self.branch(stmt, i + 1, StepKind::If, &clause.body);
                }
            }
            Stmt::Try(s) => {
                self.branch(stmt, 0, StepKind::Try, &s.body);
                for (i, handler) in s.handlers.iter().enumerate() {
                    let ExceptHandler::ExceptHandler(h) = handler;
                    self.branch(stmt, i + 1, StepKind::Try, &h.body);
                }
                self.branch(stmt, HANDLERS, StepKind::Try, &s.orelse);
                self.branch(stmt, HANDLERS + 1, StepKind::Try, &s.finalbody);
            }
            Stmt::For(s) => {
                self.branch(stmt, 0, StepKind::Other, &s.body);
                self.branch(stmt, 1, StepKind::Other, &s.orelse);
            }
            Stmt::While(s) => {
                self.branch(stmt, 0, StepKind::Other, &s.body);
                self.branch(stmt, 1, StepKind::Other, &s.orelse);
            }
            Stmt::With(s) => self.branch(stmt, 0, StepKind::Other, &s.body),
            Stmt::Match(s) => {
                for (i, case) in s.cases.iter().enumerate() {
                    self.branch(stmt, i, StepKind::Other, &case.body);
                }
            }
            _ => {}
        }
    }
}

/// An alias as Pylint lists it: the dotted name and its `as` name.
fn names(stmt: &Stmt) -> Vec<(String, Option<String>)> {
    let aliases: &[Alias] = match stmt {
        Stmt::Import(s) => &s.names,
        Stmt::ImportFrom(s) => &s.names,
        _ => return Vec::new(),
    };
    aliases
        .iter()
        .map(|a| {
            (
                a.name.to_string(),
                a.asname.as_ref().map(ToString::to_string),
            )
        })
        .collect()
}

/// `(module name without dots, level, absolute)` of a `from` import; the
/// level is `None` for an absolute import, as astroid has it.
fn from_import(stmt: &Stmt) -> Option<(String, Option<u32>)> {
    match stmt {
        Stmt::ImportFrom(s) => Some((
            s.module
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            (s.level > 0).then_some(s.level),
        )),
        _ => None,
    }
}

fn outside(
    ctx: &Ctx<'_>,
    import: &Import<'_>,
    allowed: &BTreeSet<&str>,
    out: &mut Vec<Diagnostic>,
) {
    let modname = from_import(import.stmt).map(|(module, _)| module);
    let listed: Vec<String> = names(import.stmt)
        .into_iter()
        .map(|(name, _)| match &modname {
            Some(module) => format!("{module}.{name}"),
            None => name,
        })
        .filter(|name| !allowed.contains(name.as_str()))
        .collect();
    if !listed.is_empty() {
        out.push(ctx.report(
            OUTSIDE,
            import.stmt,
            format!("Import outside toplevel ({})", listed.join(", ")),
        ));
    }
}

/// The `preferred-modules` couples, in order, a repeated module keeping its
/// first place.
fn preferred(ctx: &Ctx<'_>) -> Vec<(String, String)> {
    let mut couples: Vec<(String, String)> = Vec::new();
    for item in ctx
        .option("preferred-modules")
        .unwrap_or_default()
        .split(',')
    {
        let Some((module, better)) = item.trim().split_once(':') else {
            continue;
        };
        match couples.iter_mut().find(|(m, _)| m == module) {
            Some(couple) => better.clone_into(&mut couple.1),
            None => couples.push((module.to_owned(), better.to_owned())),
        }
    }
    couples
}

fn preferred_module(
    ctx: &Ctx<'_>,
    import: &Import<'_>,
    couples: &[(String, String)],
    out: &mut Vec<Diagnostic>,
) {
    if couples.is_empty() {
        return;
    }
    let lists: Vec<Vec<String>> = match from_import(import.stmt) {
        // One check for the whole statement, over every `module.name`.
        Some((module, _)) => vec![
            names(import.stmt)
                .into_iter()
                .map(|(name, _)| format!("{module}.{name}"))
                .collect(),
        ],
        None => names(import.stmt)
            .into_iter()
            .map(|(name, _)| vec![name])
            .collect(),
    };
    for compare in lists {
        let hit = couples.iter().find(|(key, _)| {
            compare.iter().any(|module| {
                // Pylint also takes a key that is a substring of the root.
                key == module
                    || module
                        .split('.')
                        .next()
                        .unwrap_or_default()
                        .contains(key.as_str())
            })
        });
        if let Some((key, better)) = hit {
            out.push(ctx.report(
                PREFERRED,
                import.stmt,
                format!(
                    "Prefer importing {} instead of {}",
                    py_repr(better),
                    py_repr(key)
                ),
            ));
        }
    }
}

/// `W0404` for a name that one `from` statement imports twice.
fn same_line(ctx: &Ctx<'_>, node: &Import<'_>, out: &mut Vec<Diagnostic>) {
    if from_import(node.stmt).is_none() {
        return;
    }
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (name, _) in names(node.stmt) {
        match counts.iter_mut().find(|(seen, _)| *seen == name) {
            Some(entry) => entry.1 += 1,
            None => counts.push((name, 1)),
        }
    }
    for (name, count) in counts {
        if count > 1 {
            out.push(ctx.report(
                REIMPORTED,
                node.stmt,
                format!("Reimport {} (imported line {})", py_repr(&name), node.line),
            ));
        }
    }
}

/// `W0404` and `W0416`: the first import of the same name that the
/// statement's frame, or the module, holds before it.
fn reimport(
    ctx: &Ctx<'_>,
    walk: &Walk<'_>,
    node: &Import<'_>,
    enabled: &BTreeSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let base = from_import(node.stmt);
    let level = base.as_ref().and_then(|(_, level)| *level);
    let mut contexts = vec![(node.frame, level)];
    if node.frame != 0 {
        contexts.push((0, None));
    }
    for (frame, level) in contexts {
        for (name, alias) in names(node.stmt) {
            let fullname = match &base {
                Some((module, _)) if !module.is_empty() => format!("{module}.{name}"),
                _ => name.clone(),
            };
            // Only the first import that matches counts, even when it is
            // exclusive with the statement.
            let found = walk.imports.iter().find_map(|first| {
                if std::ptr::eq(first, node) || first.frame != frame || !first.direct {
                    return None;
                }
                if first.frame == node.frame && first.line > node.line {
                    return None;
                }
                let kind = matches_first(first, &fullname, &name, alias.as_deref(), level)?;
                Some((first, kind))
            });
            let Some((first, shadowed)) =
                found.filter(|(first, _)| !exclusive(&first.path, &node.path))
            else {
                continue;
            };
            let (id, text, shown) = if shadowed {
                (SHADOWED, "Shadowed", alias.clone().unwrap_or_default())
            } else {
                (REIMPORTED, "Reimport", name.clone())
            };
            if enabled.contains(id) {
                out.push(ctx.report(
                    id,
                    node.stmt,
                    format!("{text} {} (imported line {})", py_repr(&shown), first.line),
                ));
            }
        }
    }
}

/// Whether `first` imports `fullname` (`Some(false)`) or an unaliased name
/// that `alias` shadows (`Some(true)`), as `_get_first_import` tests.
fn matches_first(
    first: &Import<'_>,
    fullname: &str,
    name: &str,
    alias: Option<&str>,
    level: Option<u32>,
) -> Option<bool> {
    match from_import(first.stmt) {
        None => {
            let listed = names(first.stmt);
            if listed.iter().any(|(imported, _)| imported == fullname) {
                return Some(false);
            }
            listed
                .iter()
                .any(|(imported, imported_alias)| {
                    imported_alias.is_none() && Some(imported.as_str()) == alias
                })
                .then_some(true)
        }
        Some((module, first_level)) => {
            if level != first_level {
                return None;
            }
            for (imported, imported_alias) in names(first.stmt) {
                if fullname == format!("{module}.{imported}")
                    || (name != "*"
                        && name == imported
                        && alias.is_none()
                        && imported_alias.is_none())
                {
                    return Some(false);
                }
                if imported_alias.is_none() && Some(imported.as_str()) == alias {
                    return Some(true);
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    const IDS: &[&str] = &["W0404", "W0416", "C0415", "W0407"];

    #[test]
    fn reimports_shadows_and_toplevel() {
        let source = include_str!("fixtures/imports.py");
        let expected = include_str!("fixtures/imports.expected").trim_end();
        assert_eq!(render(source, &[], IDS), expected);
    }

    #[test]
    fn preferred_modules_and_allowed_levels() {
        let source = include_str!("fixtures/imports_options.py");
        let expected = include_str!("fixtures/imports_options.expected").trim_end();
        let options = [
            (
                "preferred-modules",
                "json:orjson,urllib.request:httpx,unittest:pytest,dup:a,dup:b",
            ),
            ("allow-any-import-level", "json,os.path,typing.cast,re"),
        ];
        assert_eq!(render(source, &options, IDS), expected);
    }
}
