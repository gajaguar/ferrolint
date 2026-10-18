//! `match` statements: `E1901` bare-name-capture-pattern, `E1902`
//! invalid-match-args-definition, `E1903` too-many-positional-sub-patterns,
//! `E1904` multiple-class-sub-patterns, `R1905` match-class-bind-self and
//! `R1906` match-class-positional-attributes.
//!
//! shortcut: Pylint infers the class of a class pattern. Here it is a
//! builtin class, or a class of this file bound once, whose `__match_args__`
//! is a tuple of strings written in its body or in a base of this file;
//! a class imported from another module is not known. Upgrade with a class
//! inference.

use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Pattern, PatternMatchClass, Stmt, StmtClassDef, StmtMatch};
use ruff_text_size::Ranged;

use super::classes::{Env, bases};
use super::{Checker, Ctx, rule};

const BARE_NAME: &str = "E1901";
const MATCH_ARGS: &str = "E1902";
const TOO_MANY: &str = "E1903";
const DUPLICATE: &str = "E1904";
const BIND_SELF: &str = "R1905";
const POSITIONAL: &str = "R1906";

/// The builtin classes that a class pattern matches as itself.
const SELF_NAMES: [&str; 11] = [
    "bool",
    "bytearray",
    "bytes",
    "dict",
    "float",
    "frozenset",
    "int",
    "list",
    "set",
    "str",
    "tuple",
];

pub struct MatchPatterns;

impl Checker for MatchPatterns {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                BARE_NAME,
                "bare-name-capture-pattern",
                "A name capture in a `case` that has `case`s after it.",
            ),
            rule(
                MATCH_ARGS,
                "invalid-match-args-definition",
                "`__match_args__` that is not a tuple of strings.",
            ),
            rule(
                TOO_MANY,
                "too-many-positional-sub-patterns",
                "A class pattern with more positional sub-patterns than `__match_args__`.",
            ),
            rule(
                DUPLICATE,
                "multiple-class-sub-patterns",
                "A class pattern with two sub-patterns for one attribute.",
            ),
            rule(
                BIND_SELF,
                "match-class-bind-self",
                "`case int(x)` where `case int() as x` is meant.",
            ),
            rule(
                POSITIONAL,
                "match-class-positional-attributes",
                "Positional sub-patterns where keyword ones are clearer.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            BARE_NAME, MATCH_ARGS, TOO_MANY, DUPLICATE, BIND_SELF, POSITIONAL,
        ];
        if !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            in_class: false,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// What a class says about its `__match_args__`.
enum MatchArgs {
    Missing,
    Invalid,
    Found(Vec<String>),
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    /// Whether the statement is in a class body, not in a method of it.
    in_class: bool,
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    /// The `__match_args__` of a class, from the first class of this file
    /// that defines one.
    fn match_args(&self, class: &'a StmtClassDef, depth: u8) -> MatchArgs {
        for stmt in &class.body {
            let Stmt::Assign(assign) = stmt else { continue };
            if assign
                .targets
                .iter()
                .any(|t| matches!(t, Expr::Name(n) if n.id.as_str() == "__match_args__"))
            {
                return strings(&assign.value).map_or(MatchArgs::Invalid, MatchArgs::Found);
            }
        }
        if depth >= 16 {
            return MatchArgs::Missing;
        }
        bases(class)
            .iter()
            .filter_map(|base| self.env.class_of(base))
            .map(|base| self.match_args(base, depth + 1))
            .find(|found| !matches!(found, MatchArgs::Missing))
            .unwrap_or(MatchArgs::Missing)
    }

    fn class_pattern(&mut self, node: &'a PatternMatchClass) {
        let ctx = self.env.ctx;
        let patterns = &node.arguments.patterns;
        let given = patterns.len();
        let mut seen: HashSet<String> = HashSet::new();
        let mut dups: HashSet<String> = HashSet::new();
        // a builtin class matches as itself
        let builtin = self
            .env
            .qualified(&node.cls)
            .filter(|name| SELF_NAMES.contains(&name.as_str()));
        let class = self.env.class_of(&node.cls);
        if let ([Pattern::MatchAs(only)], Some(_)) = (&patterns[..], &builtin)
            && only.pattern.is_none()
            && let Some(name) = &only.name
            && self.on(BIND_SELF)
        {
            self.out.push(ctx.report(
                BIND_SELF,
                only,
                format!(
                    "Use '{}() as {name}' instead",
                    &ctx.source[node.cls.range()]
                ),
            ));
        }
        let args = if given == 0 {
            None
        } else if let Some(class) = class {
            match self.match_args(class, 0) {
                MatchArgs::Found(args) => Some(args),
                _ => None,
            }
        } else {
            builtin.as_ref().map(|_| vec!["<self>".to_owned()])
        };
        if let Some(args) = args {
            if given > args.len() {
                if self.on(TOO_MANY) {
                    self.out.push(ctx.report(
                        TOO_MANY,
                        node,
                        format!(
                            "{} expects {} positional sub-patterns (given {given})",
                            &ctx.source[node.cls.range()],
                            args.len()
                        ),
                    ));
                }
                return;
            }
            let tuple_base = class.is_some_and(|c| {
                bases(c)
                    .iter()
                    .any(|b| matches!(b, Expr::Name(n) if n.id.as_str() == "tuple"))
            });
            if builtin.is_none() && !tuple_base && self.on(POSITIONAL) {
                let attributes: Vec<String> =
                    args[..given].iter().map(|a| format!("'{a}'")).collect();
                self.out.push(ctx.report(
                    POSITIONAL,
                    node,
                    format!(
                        "Use keyword attributes instead of positional ones ({})",
                        attributes.join(", ")
                    ),
                ));
            }
            for name in &args[..given] {
                self.duplicate(name, node, &mut seen, &mut dups);
            }
        }
        for keyword in &node.arguments.keywords {
            self.duplicate(keyword.attr.as_str(), node, &mut seen, &mut dups);
        }
    }

    fn duplicate(
        &mut self,
        name: &str,
        node: &PatternMatchClass,
        seen: &mut HashSet<String>,
        dups: &mut HashSet<String>,
    ) {
        if seen.contains(name) && !dups.contains(name) {
            dups.insert(name.to_owned());
            if self.on(DUPLICATE) {
                self.out.push(self.env.ctx.report(
                    DUPLICATE,
                    node,
                    format!("Multiple sub-patterns for attribute {name}"),
                ));
            }
        } else {
            seen.insert(name.to_owned());
        }
    }

    fn bare_names(&mut self, node: &StmtMatch) {
        let count = node.cases.len();
        for (i, case) in node.cases.iter().enumerate() {
            if let Pattern::MatchAs(p) = &case.pattern
                && p.pattern.is_none()
                && let Some(name) = &p.name
                && case.guard.is_none()
                && i + 1 < count
            {
                self.out.push(self.env.ctx.report(
                    BARE_NAME,
                    &case.pattern,
                    format!("The name capture `case {name}` makes the remaining patterns unreachable. Use a dotted name (for example an enum) to fix this."),
                ));
            }
        }
    }
}

/// The strings of a tuple of string literals.
fn strings(value: &Expr) -> Option<Vec<String>> {
    let Expr::Tuple(tuple) = value else {
        return None;
    };
    tuple
        .elts
        .iter()
        .map(|e| match e {
            Expr::StringLiteral(s) => Some(s.value.to_str().to_owned()),
            _ => None,
        })
        .collect()
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(_) | Stmt::FunctionDef(_) => {
                let before =
                    std::mem::replace(&mut self.in_class, matches!(stmt, Stmt::ClassDef(_)));
                visitor::walk_stmt(self, stmt);
                self.in_class = before;
            }
            Stmt::Assign(assign) if self.in_class && self.on(MATCH_ARGS) => {
                if assign
                    .targets
                    .iter()
                    .any(|t| matches!(t, Expr::Name(n) if n.id.as_str() == "__match_args__"))
                    && strings(&assign.value).is_none()
                {
                    self.out.push(self.env.ctx.report(
                        MATCH_ARGS,
                        &*assign.value,
                        "`__match_args__` must be a tuple of strings.",
                    ));
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Match(node) => {
                if self.on(BARE_NAME) {
                    self.bare_names(node);
                }
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if let Pattern::MatchClass(node) = pattern {
            self.class_pattern(node);
        }
        visitor::walk_pattern(self, pattern);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;
    use crate::native::flow::BATCH_C;

    #[test]
    fn match_statements() {
        let source = include_str!("fixtures/match_patterns.py");
        let expected = include_str!("fixtures/match_patterns.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }
}
