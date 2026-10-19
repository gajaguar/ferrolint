//! `W0614` unused-wildcard-import: the names a `from m import *` binds
//! that the file never reads. The module is found like a base class is
//! (see [`Project`]); one that is not found is skipped, and so is an
//! `__init__.py`, as Pylint does.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext, Stmt};

use super::project::Project;
use super::{Checker, Ctx, rule};

const UNUSED: &str = "W0614";

pub struct Wildcard;

impl Checker for Wildcard {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![rule(
            UNUSED,
            "unused-wildcard-import",
            "Names that a wildcard import binds and the file never reads.",
        )]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(UNUSED) || ctx.path.file_stem().is_some_and(|s| s == "__init__") {
            return;
        }
        let mut walk = Walk {
            stars: Vec::new(),
            read: BTreeSet::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
        if walk.stars.is_empty() {
            return;
        }
        let project = Project::with_packages(ctx.path, ctx.module);
        for (stmt, origin) in &walk.stars {
            let module = project.absolute(origin);
            let Some(names) = project.public_names(&module) else {
                continue;
            };
            let unused: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|name| !walk.read.contains(*name))
                .collect();
            let Some((last, rest)) = unused.split_last() else {
                continue;
            };
            let joined = if rest.is_empty() {
                (*last).to_owned()
            } else {
                format!("{} and {last}", rest.join(", "))
            };
            out.push(ctx.report(
                UNUSED,
                *stmt,
                format!("Unused import(s) {joined} from wildcard import of {module}"),
            ));
        }
    }
}

struct Walk<'a> {
    /// Each `from m import *` and its module as written.
    stars: Vec<(&'a Stmt, String)>,
    /// The names the file reads, and those its `__all__` lists.
    read: BTreeSet<String>,
}

impl<'a> Visitor<'a> for Walk<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ImportFrom(import) if import.names.iter().any(|a| &a.name == "*") => {
                let dots = ".".repeat(import.level as usize);
                let name = import.module.as_ref().map_or("", |m| m.as_str());
                self.stars.push((stmt, format!("{dots}{name}")));
            }
            Stmt::Assign(assign)
                if assign
                    .targets
                    .iter()
                    .any(|t| matches!(t, Expr::Name(n) if n.id.as_str() == "__all__")) =>
            {
                // shortcut: only a literal list or tuple of strings counts; upgrade with inference.
                let items = match &*assign.value {
                    Expr::List(l) => &l.elts,
                    Expr::Tuple(t) => &t.elts,
                    _ => return,
                };
                self.read.extend(items.iter().filter_map(string));
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(name) = expr
            && name.ctx != ExprContext::Store
        {
            self.read.insert(name.id.to_string());
        }
        visitor::walk_expr(self, expr);
    }
}

fn string(expr: &Expr) -> Option<String> {
    expr.as_string_literal_expr()
        .map(|s| s.value.to_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::classes::render_at;

    fn run(files: &[(&str, &str)], main: &str, module: &str) -> String {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let source = std::fs::read_to_string(root.path().join(main)).unwrap();
        render_at(&source, &root.path().join(main), module, &[], &["W0614"])
    }

    #[test]
    fn reports_the_names_never_read() {
        let files = [
            ("pkg/__init__.py", ""),
            (
                "pkg/m.py",
                "import os\na = 1\nb = 2\n_c = 3\n\ndef f(): ...\n\nclass K: ...\n",
            ),
            (
                "pkg/listed.py",
                "__all__ = ['x', 'y']\nx = 1\ny = 2\nz = 3\n",
            ),
            (
                "pkg/user.py",
                "from pkg.m import *\nfrom .listed import *\nfrom pkg.gone import *\nfrom json import *\n\nprint(a, x)\n",
            ),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) os, b, f and K from wildcard import of pkg.m\n\
             2:W0614:Unused import(s) y and z from wildcard import of pkg.listed"
        );
    }

    #[test]
    fn an_init_module_is_skipped() {
        let files = [
            ("pkg/m.py", "a = 1\n"),
            ("pkg/__init__.py", "from pkg.m import *\n"),
        ];
        assert_eq!(run(&files, "pkg/__init__.py", "pkg"), "");
    }
}
