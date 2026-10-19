//! `W0401` wildcard-import and `W0614` unused-wildcard-import: a `from m
//! import *`, and the names it binds that the file never reads. The module
//! is found like a base class is (see [`Project`]); one that is not found is
//! skipped by `W0614`, and an `__init__.py` by both, as Pylint does.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext, Stmt};

use super::project::{Project, bound};
use super::{Checker, Ctx, rule};

const WILDCARD: &str = "W0401";
const UNUSED: &str = "W0614";

pub struct Wildcard;

impl Checker for Wildcard {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                WILDCARD,
                "wildcard-import",
                "A `from m import *`, unless `allow-wildcard-with-all` and `m` defines `__all__`.",
            ),
            rule(
                UNUSED,
                "unused-wildcard-import",
                "Names that a wildcard import binds and the file never reads.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let (wildcard, unused) = (enabled.contains(WILDCARD), enabled.contains(UNUSED));
        if !(wildcard || unused) || ctx.path.file_stem().is_some_and(|s| s == "__init__") {
            return;
        }
        let mut walk = Walk {
            stars: Vec::new(),
            nested: 0,
            read: BTreeSet::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
        if walk.stars.is_empty() {
            return;
        }
        let project = Project::with_packages(ctx.path, ctx.module);
        let allow_all = ctx.option("allow-wildcard-with-all").is_some_and(|v| {
            matches!(v.trim().to_lowercase().as_str(), "y" | "yes" | "true" | "1")
        });
        // astroid lists a module's names in the order it binds them: its own
        // first, then what each star brings.
        let mut order = Vec::new();
        bound("", &ctx.parsed.syntax().body, &mut Vec::new(), &mut order);
        for (stmt, origin, top) in &walk.stars {
            let module = project.absolute(origin);
            if wildcard && !(allow_all && project.defines_all(&module)) {
                // Pylint names the module as written, without the dots of a relative import.
                let written = origin.trim_start_matches('.');
                out.push(ctx.report(WILDCARD, *stmt, format!("Wildcard import {written}")));
            }
            // Pylint tracks unused names of the module's own scope only.
            if !unused || !*top {
                continue;
            }
            let Some(names) = project.public_names(&module) else {
                continue;
            };
            for name in &names {
                if !order.contains(name) {
                    order.push(name.clone());
                }
            }
            let mut unused: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|name| !walk.read.contains(*name))
                .collect();
            unused.sort_by_key(|name| order.iter().position(|seen| seen == name));
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
    /// Each `from m import *`, its module as written and whether it is in
    /// the scope of the module (not of a function or a class).
    stars: Vec<(&'a Stmt, String, bool)>,
    nested: usize,
    /// The names the file reads, and those its `__all__` lists.
    read: BTreeSet<String>,
}

impl<'a> Visitor<'a> for Walk<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ImportFrom(import) if import.names.iter().any(|a| &a.name == "*") => {
                let dots = ".".repeat(import.level as usize);
                let name = import.module.as_ref().map_or("", |m| m.as_str());
                self.stars
                    .push((stmt, format!("{dots}{name}"), self.nested == 0));
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
        let scope = matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_));
        self.nested += usize::from(scope);
        visitor::walk_stmt(self, stmt);
        self.nested -= usize::from(scope);
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
        run_with(files, main, module, &[], &["W0614"])
    }

    fn run_with(
        files: &[(&str, &str)],
        main: &str,
        module: &str,
        options: &[(&str, &str)],
        ids: &[&str],
    ) -> String {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let source = std::fs::read_to_string(root.path().join(main)).unwrap();
        render_at(&source, &root.path().join(main), module, options, ids)
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

    #[test]
    fn a_wildcard_import_is_reported_unless_all_allows_it() {
        let files = [
            ("pkg/__init__.py", ""),
            ("pkg/plain.py", "a = 1\n"),
            ("pkg/listed.py", "if True:\n    __all__ = ['a']\na = 1\n"),
            (
                "pkg/user.py",
                "from pkg.plain import *\nfrom .listed import *\nfrom . import *\nfrom json import *\n",
            ),
        ];
        let ids = ["W0401"];
        assert_eq!(
            run_with(&files, "pkg/user.py", "pkg.user", &[], &ids),
            "1:W0401:Wildcard import pkg.plain\n\
             2:W0401:Wildcard import listed\n\
             3:W0401:Wildcard import \n\
             4:W0401:Wildcard import json"
        );
        let allowed = [("allow-wildcard-with-all", "yes")];
        assert_eq!(
            run_with(&files, "pkg/user.py", "pkg.user", &allowed, &ids),
            "1:W0401:Wildcard import pkg.plain\n\
             3:W0401:Wildcard import \n\
             4:W0401:Wildcard import json"
        );
    }

    #[test]
    fn a_wildcard_import_in_an_init_module_is_not_reported() {
        let files = [("pkg/m.py", ""), ("pkg/__init__.py", "from .m import *\n")];
        assert_eq!(
            run_with(&files, "pkg/__init__.py", "pkg", &[], &["W0401"]),
            ""
        );
    }

    #[test]
    fn private_names_and_all_do_not_decide_the_bound_names() {
        let files = [
            (
                "pkg/dunder.py",
                "__all__ = ['_priv', 'pub', '__dunder__']\n_priv = 1\npub = 2\nother = 3\n__dunder__ = 4\n",
            ),
            ("pkg/user.py", "from pkg.dunder import *\nprint(_priv)\n"),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) pub and other from wildcard import of pkg.dunder"
        );
    }

    #[test]
    fn a_wildcard_import_in_a_function_or_class_binds_nothing_to_report() {
        let files = [
            ("pkg/m.py", "a = 1\n"),
            (
                "pkg/user.py",
                "def f():\n    from pkg.m import *\n\nclass C:\n    from pkg.m import *\n",
            ),
        ];
        assert_eq!(run(&files, "pkg/user.py", "pkg.user"), "");
        assert_eq!(
            run_with(&files, "pkg/user.py", "pkg.user", &[], &["W0401"]),
            "2:W0401:Wildcard import pkg.m\n5:W0401:Wildcard import pkg.m"
        );
    }

    #[test]
    fn a_name_the_file_binds_again_stays_unused_until_read() {
        let files = [
            ("pkg/m.py", "a = 1\nb = 2\n"),
            ("pkg/m3.py", "from pkg.m import *\nq = 1\nb = 9\n"),
            ("pkg/user.py", "from pkg.m import *\na = 5\nprint(b)\n"),
            ("pkg/user2.py", "from pkg.m import *\nb = 5\n"),
            ("pkg/user3.py", "a = 5\nfrom pkg.m import *\nprint(a)\n"),
            ("pkg/user4.py", "from pkg.m3 import *\n"),
        ];
        let at = |main: &str, module: &str| run(&files, main, module);
        assert_eq!(
            at("pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) a from wildcard import of pkg.m"
        );
        assert_eq!(
            at("pkg/user2.py", "pkg.user2"),
            "1:W0614:Unused import(s) b and a from wildcard import of pkg.m"
        );
        assert_eq!(
            at("pkg/user3.py", "pkg.user3"),
            "2:W0614:Unused import(s) b from wildcard import of pkg.m"
        );
        // The module binds `q` and `b` itself; the names of its star come after.
        assert_eq!(
            at("pkg/user4.py", "pkg.user4"),
            "1:W0614:Unused import(s) q, b and a from wildcard import of pkg.m3"
        );
    }

    #[test]
    fn a_nested_star_is_bound_after_the_names_of_the_module() {
        let files = [
            ("pkg/m.py", "a = 1\nb = 2\n"),
            ("pkg/mid.py", "from pkg.m import *\nc = 1\n"),
            ("pkg/all.py", "from pkg.mid import *\n"),
            ("pkg/some.py", "from pkg.mid import *\nprint(a, c)\n"),
        ];
        assert_eq!(
            run(&files, "pkg/all.py", "pkg.all"),
            "1:W0614:Unused import(s) c, a and b from wildcard import of pkg.mid"
        );
        assert_eq!(
            run(&files, "pkg/some.py", "pkg.some"),
            "1:W0614:Unused import(s) b from wildcard import of pkg.mid"
        );
    }

    #[test]
    fn names_bound_in_compound_statements_count() {
        let files = [
            (
                "pkg/m.py",
                "for i in range(3):\n    pass\nwith open('x') as fh:\n    pass\nwhile False:\n    w = 1\n\
                 try:\n    pass\nexcept ValueError as err:\n    pass\nfinally:\n    cf = 1\n\
                 k = 1\nk += 1\ntype T = int\n",
            ),
            ("pkg/user.py", "from pkg.m import *\n"),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) i, fh, w, err, cf, k and T from wildcard import of pkg.m"
        );
    }
}
