//! `R0901` too-many-ancestors: a class with more ancestors than
//! `max-parents`. Pylint walks `ancestors(recurs=False)` from the class,
//! skipping the stdlib classes it lists and the `ignored-parents` (and what
//! they inherit). The parents come from [`super::hierarchy`]'s sources; a base
//! the project cannot read adds nothing, as an uninferable base does for
//! Pylint, so the count can fall short but never exceeds Pylint's.

use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;

use super::classes::{Env, bases};
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};

const TOO_MANY: &str = "R0901";

/// Pylint's `STDLIB_CLASSES_IGNORE_ANCESTOR`, as it spells it (`frozenset`
/// is misspelled there, so it counts).
const IGNORED: &[&str] = &[
    "builtins.object",
    "builtins.tuple",
    "builtins.dict",
    "builtins.list",
    "builtins.set",
    "collections.ChainMap",
    "collections.Counter",
    "collections.OrderedDict",
    "collections.UserDict",
    "collections.UserList",
    "collections.UserString",
    "collections.defaultdict",
    "collections.deque",
    "collections.namedtuple",
    "_collections_abc.Awaitable",
    "_collections_abc.Coroutine",
    "_collections_abc.AsyncIterable",
    "_collections_abc.AsyncIterator",
    "_collections_abc.AsyncGenerator",
    "_collections_abc.Hashable",
    "_collections_abc.Iterable",
    "_collections_abc.Iterator",
    "_collections_abc.Generator",
    "_collections_abc.Reversible",
    "_collections_abc.Sized",
    "_collections_abc.Container",
    "_collections_abc.Callable",
    "_collections_abc.Collection",
    "_collections_abc.Set",
    "_collections_abc.MutableSet",
    "_collections_abc.Mapping",
    "_collections_abc.MutableMapping",
    "_collections_abc.MappingView",
    "_collections_abc.KeysView",
    "_collections_abc.ItemsView",
    "_collections_abc.ValuesView",
    "_collections_abc.Sequence",
    "_collections_abc.MutableSequence",
    "_collections_abc.ByteString",
    "_collections_abc.Buffer",
    "typing.Tuple",
    "typing.List",
    "typing.Dict",
    "typing.Set",
    "typing.FrozenSet",
    "typing.Deque",
    "typing.DefaultDict",
    "typing.OrderedDict",
    "typing.Counter",
    "typing.ChainMap",
    "typing.Awaitable",
    "typing.Coroutine",
    "typing.AsyncIterable",
    "typing.AsyncIterator",
    "typing.AsyncGenerator",
    "typing.Iterable",
    "typing.Iterator",
    "typing.Generator",
    "typing.Reversible",
    "typing.Container",
    "typing.Collection",
    "typing.AbstractSet",
    "typing.MutableSet",
    "typing.Mapping",
    "typing.MutableMapping",
    "typing.Sequence",
    "typing.MutableSequence",
    "typing.ByteString",
    "typing.MappingView",
    "typing.KeysView",
    "typing.ItemsView",
    "typing.ValuesView",
    "typing.ContextManager",
    "typing.AsyncContextManager",
    "typing.Hashable",
    "typing.Sized",
    "typing.NamedTuple",
    "typing.TypedDict",
    "typing_extensions.TypedDict",
];

/// The builtin classes with no ancestor but `object`.
const LEAVES: &[&str] = &[
    "int",
    "str",
    "bytes",
    "bytearray",
    "float",
    "complex",
    "type",
    "property",
    "frozenset",
    "memoryview",
    "range",
    "slice",
    "staticmethod",
    "classmethod",
    "super",
];

/// Each builtin exception with its bases.
const EXCEPTIONS: &[(&str, &[&str])] = &[
    ("BaseException", &[]),
    ("Exception", &["BaseException"]),
    ("GeneratorExit", &["BaseException"]),
    ("KeyboardInterrupt", &["BaseException"]),
    ("SystemExit", &["BaseException"]),
    ("BaseExceptionGroup", &["BaseException"]),
    ("ExceptionGroup", &["BaseExceptionGroup", "Exception"]),
    ("ArithmeticError", &["Exception"]),
    ("FloatingPointError", &["ArithmeticError"]),
    ("OverflowError", &["ArithmeticError"]),
    ("ZeroDivisionError", &["ArithmeticError"]),
    ("AssertionError", &["Exception"]),
    ("AttributeError", &["Exception"]),
    ("BufferError", &["Exception"]),
    ("EOFError", &["Exception"]),
    ("ImportError", &["Exception"]),
    ("ModuleNotFoundError", &["ImportError"]),
    ("LookupError", &["Exception"]),
    ("IndexError", &["LookupError"]),
    ("KeyError", &["LookupError"]),
    ("MemoryError", &["Exception"]),
    ("NameError", &["Exception"]),
    ("UnboundLocalError", &["NameError"]),
    ("OSError", &["Exception"]),
    ("BlockingIOError", &["OSError"]),
    ("ChildProcessError", &["OSError"]),
    ("ConnectionError", &["OSError"]),
    ("BrokenPipeError", &["ConnectionError"]),
    ("ConnectionAbortedError", &["ConnectionError"]),
    ("ConnectionRefusedError", &["ConnectionError"]),
    ("ConnectionResetError", &["ConnectionError"]),
    ("FileExistsError", &["OSError"]),
    ("FileNotFoundError", &["OSError"]),
    ("InterruptedError", &["OSError"]),
    ("IsADirectoryError", &["OSError"]),
    ("NotADirectoryError", &["OSError"]),
    ("PermissionError", &["OSError"]),
    ("ProcessLookupError", &["OSError"]),
    ("TimeoutError", &["OSError"]),
    ("ReferenceError", &["Exception"]),
    ("RuntimeError", &["Exception"]),
    ("NotImplementedError", &["RuntimeError"]),
    ("RecursionError", &["RuntimeError"]),
    ("PythonFinalizationError", &["RuntimeError"]),
    ("StopAsyncIteration", &["Exception"]),
    ("StopIteration", &["Exception"]),
    ("SyntaxError", &["Exception"]),
    ("IndentationError", &["SyntaxError"]),
    ("TabError", &["IndentationError"]),
    ("SystemError", &["Exception"]),
    ("TypeError", &["Exception"]),
    ("ValueError", &["Exception"]),
    ("UnicodeError", &["ValueError"]),
    ("UnicodeDecodeError", &["UnicodeError"]),
    ("UnicodeEncodeError", &["UnicodeError"]),
    ("UnicodeTranslateError", &["UnicodeError"]),
    ("Warning", &["Exception"]),
    ("BytesWarning", &["Warning"]),
    ("DeprecationWarning", &["Warning"]),
    ("EncodingWarning", &["Warning"]),
    ("FutureWarning", &["Warning"]),
    ("ImportWarning", &["Warning"]),
    ("PendingDeprecationWarning", &["Warning"]),
    ("ResourceWarning", &["Warning"]),
    ("RuntimeWarning", &["Warning"]),
    ("SyntaxWarning", &["Warning"]),
    ("UnicodeWarning", &["Warning"]),
    ("UserWarning", &["Warning"]),
];

/// A parent of a class: one the project reads, or a builtin or typing
/// name it only knows by its dotted name.
enum Parent<'a> {
    Class(Class<'a>),
    Named(String),
}

pub struct Ancestors;

impl Checker for Ancestors {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![rule(
            TOO_MANY,
            "too-many-ancestors",
            "A class with more ancestors than `max-parents`.",
        )]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(TOO_MANY) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            hierarchy: Hierarchy::new(&env),
            max: ctx
                .option("max-parents")
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(7),
            ignored: ctx
                .option("ignored-parents")
                .map(|value| {
                    value
                        .split(',')
                        .map(|name| name.trim().to_owned())
                        .filter(|name| !name.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: Hierarchy<'a, 'e>,
    max: usize,
    ignored: HashSet<String>,
    out: &'e mut Vec<Diagnostic>,
}

/// `name` as Pylint's `qname` spells it.
fn normalize(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("collections.abc.") {
        format!("_collections_abc.{rest}")
    } else if let Some(rest) = name.strip_prefix("typing_extensions.Protocol") {
        format!("typing.Protocol{rest}")
    } else if name.contains('.') {
        name.to_owned()
    } else {
        format!("builtins.{name}")
    }
}

impl<'a> Walk<'a, '_> {
    fn skipped(&self, name: &str) -> bool {
        IGNORED.contains(&name) || self.ignored.contains(name)
    }

    fn qname(&self, class: &Class<'a>) -> String {
        match (class, class.def()) {
            (Class::Local(_), Some(def)) => {
                format!("{}.{}", self.hierarchy.module(class), def.name)
            }
            _ => class.key(),
        }
    }

    /// The direct parents of `class`; one the project cannot read is left out.
    fn parents(&self, class: &Class<'a>) -> Vec<Parent<'a>> {
        match class {
            Class::Local(def) => bases(def)
                .iter()
                .filter_map(|base| {
                    let base = match base {
                        Expr::Subscript(s) => &*s.value,
                        other => other,
                    };
                    let named = self.env.qualified(base).map(|q| normalize(&q));
                    if named.as_deref().is_some_and(|q| self.skipped(q)) {
                        return None;
                    }
                    match self.hierarchy.resolve(class, base) {
                        Some(found) => Some(Parent::Class(found)),
                        None => named.map(Parent::Named),
                    }
                })
                .collect(),
            Class::Remote(remote) => remote
                .bases(&self.hierarchy.project)
                .into_iter()
                .map(|base| match base {
                    Ok(found) => Parent::Class(Class::Remote(found)),
                    Err(name) => Parent::Named(normalize(&name)),
                })
                .collect(),
        }
    }

    /// The parents of a name only known by its dotted name, or `None` for
    /// a name the tables lack.
    fn named_parents(name: &str) -> Option<Vec<Parent<'a>>> {
        let named =
            |names: &[&str]| Some(names.iter().map(|n| Parent::Named(normalize(n))).collect());
        match name.strip_prefix("builtins.") {
            Some("bool") => named(&["int"]),
            Some(builtin) if LEAVES.contains(&builtin) => named(&[]),
            Some(builtin) => EXCEPTIONS
                .iter()
                .find(|(n, _)| *n == builtin)
                .and_then(|(_, parents)| named(parents)),
            None => match name {
                "abc.ABC" | "typing.Generic" | "_typing.Generic" | "enum.Enum" => named(&[]),
                "typing.Protocol" => named(&["typing.Generic"]),
                "enum.Flag" => named(&["enum.Enum"]),
                _ => None,
            },
        }
    }

    fn count(&self, class: &'a StmtClassDef) -> usize {
        let mut seen = HashSet::new();
        let mut explore = self.parents(&Class::Local(class));
        while let Some(parent) = explore.pop() {
            let name = match &parent {
                Parent::Class(found) => self.qname(found),
                Parent::Named(name) => name.clone(),
            };
            // astroid gives a class that derives from `Enum` or `Flag` an
            // extra `NoneType` ancestor.
            if name == "enum.Enum" || name == "enum.Flag" {
                seen.insert("builtins.NoneType".to_owned());
            }
            if self.skipped(&name) {
                continue;
            }
            match parent {
                Parent::Class(found) => {
                    if seen.insert(found.key()) {
                        explore.extend(self.parents(&found));
                    }
                }
                Parent::Named(name) => {
                    if let Some(parents) = Self::named_parents(&name)
                        && seen.insert(name)
                    {
                        explore.extend(parents);
                    }
                }
            }
        }
        seen.len()
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        let count = self.count(class);
        if count > self.max {
            let diagnostic = self.env.ctx.report_at(
                TOO_MANY,
                class.name.range(),
                format!("Too many ancestors ({count}/{})", self.max),
            );
            self.out.push(diagnostic);
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            self.class(class);
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn too_many_ancestors() {
        let source = include_str!("fixtures/too_many_ancestors.py");
        let expected = include_str!("fixtures/too_many_ancestors.expected").trim_end();
        assert_eq!(
            render(source, &[("max-parents", "0")], &["R0901"]),
            expected
        );
    }

    #[test]
    fn ignored_parents() {
        let source = "class A:\n    pass\n\n\nclass B(A):\n    pass\n\n\nclass C(B):\n    pass\n";
        let options = [("max-parents", "0"), ("ignored-parents", "x.Y, mod.A")];
        assert_eq!(
            render(source, &options, &["R0901"]),
            "9:R0901:Too many ancestors (1/0)"
        );
    }
}
