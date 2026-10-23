//! `W0401` wildcard-import and `W0614` unused-wildcard-import: a `from m
//! import *`, and the names it binds that the file never reads. The module
//! is found like a base class is (see [`Project`]); one that is not found is
//! skipped by `W0614`, and an `__init__.py` by both, as Pylint does.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, ExprContext, HasNodeIndex, Stmt};
use ruff_text_size::{Ranged, TextSize};

use super::project::Project;
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
            ctx,
            stars: Vec::new(),
            imports: Vec::new(),
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
        // astroid lists a module's names in the order it binds them.
        let order = project.bound_names(&ctx.parsed.syntax().body);
        // Pylint hands a name to the first statement that binds it; a later star skips it.
        let mut claimed = BTreeSet::new();
        for (stmt, origin, top) in &walk.stars {
            let module = project.absolute(origin);
            // Pylint names the module as written, without the dots of a relative import.
            let written = origin.trim_start_matches('.');
            if wildcard && !(allow_all && project.defines_all(&module)) {
                out.push(ctx.report(WILDCARD, *stmt, format!("Wildcard import {written}")));
            }
            // Pylint tracks unused names of the module's own scope only.
            if !unused || !*top {
                continue;
            }
            let Some(names) = project.public_names(&module) else {
                continue;
            };
            claimed.extend(
                (walk.imports.iter())
                    .filter(|(at, _, bound)| *at < stmt.start() && !walk.read.contains(bound))
                    .map(|(_, name, _)| name.clone()),
            );
            let mut unused: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|name| !walk.read.contains(*name) && !claimed.contains(*name))
                .collect();
            claimed.extend(names.iter().cloned());
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
                format!("Unused import(s) {joined} from wildcard import of {written}"),
            ));
        }
    }
}

struct Walk<'a> {
    ctx: &'a Ctx<'a>,
    /// Each `from m import *`, its module as written and whether it is in
    /// the scope of the module (not of a function or a class).
    stars: Vec<(&'a Stmt, String, bool)>,
    /// Each name an import of the module's scope binds, as `(offset, imported name, bound name)`.
    imports: Vec<(TextSize, String, String)>,
    nested: usize,
    /// The names the file reads, and those its `__all__` lists.
    read: BTreeSet<String>,
}

impl Walk<'_> {
    fn is_local(&self, name: &ast::ExprName) -> bool {
        HasNodeIndex::node_index(name)
            .load()
            .as_u32()
            .and_then(|id| self.ctx.model.resolve(self.ctx.flat, id))
            .is_some_and(|binding| binding.scope != 0)
    }
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
            Stmt::Import(ast::StmtImport { names, .. })
            | Stmt::ImportFrom(ast::StmtImportFrom { names, .. })
                if self.nested == 0 =>
            {
                for alias in names {
                    let bound = alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default(),
                        |a| a.as_str(),
                    );
                    self.imports
                        .push((stmt.start(), alias.name.to_string(), bound.to_owned()));
                }
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
            // a name bound in a function, class or comprehension is read there, not from the module
            && !self.is_local(name)
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
                "from pkg.m import *\nfrom .listed import *\nfrom pkg.gone import *\nfrom nowhere import *\n\nprint(a, x)\n",
            ),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) os, b, f and K from wildcard import of pkg.m\n\
             2:W0614:Unused import(s) y and z from wildcard import of listed"
        );
    }

    #[test]
    fn a_local_variable_is_not_a_use_of_the_name() {
        let files = [
            ("pkg/m.py", "cost = 1\nother = 2\nthird = 3\n"),
            (
                "pkg/user.py",
                "from pkg.m import *\n\ndef f(cost):\n    return cost\n\ndef g():\n    other = 0\n    return other\n\n\
                 print([third for third in range(3)])\n",
            ),
            (
                "pkg/global_.py",
                "from pkg.m import *\n\ndef f():\n    global cost\n    cost = 0\n\nprint(other, third)\n",
            ),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) cost, other and third from wildcard import of pkg.m"
        );
        // a `global` write binds in the module but reads nothing
        assert_eq!(
            run(&files, "pkg/global_.py", "pkg.global_"),
            "1:W0614:Unused import(s) cost from wildcard import of pkg.m"
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
                "from pkg.plain import *\nfrom .listed import *\nfrom . import *\nfrom nowhere import *\n",
            ),
        ];
        let ids = ["W0401"];
        assert_eq!(
            run_with(&files, "pkg/user.py", "pkg.user", &[], &ids),
            "1:W0401:Wildcard import pkg.plain\n\
             2:W0401:Wildcard import listed\n\
             3:W0401:Wildcard import \n\
             4:W0401:Wildcard import nowhere"
        );
        let allowed = [("allow-wildcard-with-all", "yes")];
        assert_eq!(
            run_with(&files, "pkg/user.py", "pkg.user", &allowed, &ids),
            "1:W0401:Wildcard import pkg.plain\n\
             3:W0401:Wildcard import \n\
             4:W0401:Wildcard import nowhere"
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
    fn a_dotted_import_binds_only_its_first_component() {
        let files = [
            ("a/__init__.py", "x = 1\ny = 2\na = 3\n"),
            ("a/b.py", "p = 1\nq = 2\nx = 3\n"),
            ("c1.py", "import a.b\nfrom a.b import *\nprint(a)\n"),
            ("c2.py", "import a.b\nfrom a import *\nprint(x)\n"),
            ("c3.py", "from a import *\nimport a.b\nprint(a)\n"),
            ("c5.py", "import a.b as ab\nfrom a import *\nprint(ab)\n"),
            (
                "c8.py",
                "import a\nfrom a import *\nfrom a.b import *\nprint(q)\n",
            ),
        ];
        let at = |main: &str| run(&files, &format!("{main}.py"), main);
        // `import a.b` binds `a`, which the module `a` defines too; the import came first.
        assert_eq!(
            at("c1"),
            "2:W0614:Unused import(s) p, q and x from wildcard import of a.b"
        );
        assert_eq!(
            at("c2"),
            "2:W0614:Unused import(s) a and y from wildcard import of a"
        );
        assert_eq!(
            at("c3"),
            "1:W0614:Unused import(s) x and y from wildcard import of a"
        );
        // `import a.b as ab` binds only `ab`, so the star still lists `a`.
        assert_eq!(
            at("c5"),
            "2:W0614:Unused import(s) x, y and a from wildcard import of a"
        );
        assert_eq!(
            at("c8"),
            "2:W0614:Unused import(s) x and y from wildcard import of a\n\
             3:W0614:Unused import(s) p from wildcard import of a.b"
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

    #[test]
    fn only_the_first_statement_that_binds_a_name_lists_it() {
        let files = [
            ("pkg/m1.py", "a = 1\nb = 2\n"),
            ("pkg/m2.py", "b = 3\nc = 4\n"),
            ("pkg/two.py", "from pkg.m1 import *\nfrom pkg.m2 import *\n"),
            (
                "pkg/twice.py",
                "from pkg.m1 import *\nfrom pkg.m1 import *\n",
            ),
            (
                "pkg/named.py",
                "from pkg.m1 import a\nfrom pkg.m1 import *\n",
            ),
            (
                "pkg/read.py",
                "from pkg.m1 import a as x\nfrom pkg.m1 import *\nprint(x)\n",
            ),
        ];
        let at = |main: &str, module: &str| run(&files, main, module);
        assert_eq!(
            at("pkg/two.py", "pkg.two"),
            "1:W0614:Unused import(s) a and b from wildcard import of pkg.m1\n\
             2:W0614:Unused import(s) c from wildcard import of pkg.m2"
        );
        assert_eq!(
            at("pkg/twice.py", "pkg.twice"),
            "1:W0614:Unused import(s) a and b from wildcard import of pkg.m1"
        );
        assert_eq!(
            at("pkg/named.py", "pkg.named"),
            "2:W0614:Unused import(s) b from wildcard import of pkg.m1"
        );
        // `x` is read, so the star still brings `a`.
        assert_eq!(
            at("pkg/read.py", "pkg.read"),
            "2:W0614:Unused import(s) a and b from wildcard import of pkg.m1"
        );
    }

    #[test]
    fn the_message_names_the_module_as_written() {
        let files = [
            ("pkg/__init__.py", ""),
            ("pkg/events.py", "class Event: ...\n"),
            ("pkg/user.py", "from .events import *\n"),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) Event from wildcard import of events"
        );
    }

    #[test]
    fn match_captures_walruses_and_declared_globals_are_bound() {
        let files = [
            (
                "pkg/m.py",
                "import os\n\n\ndef f():\n    global from_global, from_import\n    from_global = 1\n    from os import path as from_import\n    local = 1\n\n\n\
                 def g():\n    global declared_only\n\n\n\
                 match os.name:\n    case [first, *rest] as whole:\n        pass\n    case {'k': v, **extra}:\n        pass\n\n\
                 if (walrus := 1):\n    pass\ncomp = [(inner := n) for n in range(3)]\nfor i in (w := range(3)):\n    pass\nlam = lambda: (hidden := 1)\n\n\n\
                 class K:\n    inside = (never := 1)\n\n\ndef fn(a=(d := 1)): ...\n",
            ),
            ("pkg/user.py", "from pkg.m import *\n"),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) os, from_global, f, g, first, rest, whole, v, extra, \
             walrus, comp, inner, i, w, lam, K, d, fn and from_import from wildcard import of pkg.m"
        );
    }

    #[test]
    fn future_imports_are_not_names_of_the_module() {
        let files = [
            (
                "pkg/m.py",
                "from __future__ import annotations\nimport os\n",
            ),
            ("pkg/user.py", "from pkg.m import *\n"),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) os from wildcard import of pkg.m"
        );
    }

    #[test]
    fn the_names_of_from_imports_come_after_the_others() {
        let files = [
            (
                "pkg/m.py",
                "import os\n\n\ndef f(): ...\n\n\nfrom json import loads\n\nVALUE = 1\nimport sys\n",
            ),
            ("pkg/user.py", "from pkg.m import *\n"),
        ];
        assert_eq!(
            run(&files, "pkg/user.py", "pkg.user"),
            "1:W0614:Unused import(s) os, f, VALUE, sys and loads from wildcard import of pkg.m"
        );
    }

    #[test]
    fn a_c_module_lists_its_public_names() {
        let files = [
            ("pkg/__init__.py", ""),
            ("pkg/c.py", "from atexit import *\nz = 1\n"),
            ("pkg/direct.py", "from atexit import *\nunregister()\n"),
            ("pkg/chained.py", "from pkg.c import *\n"),
            ("shadow/atexit.py", "mine = 1\n"),
            ("shadow/user.py", "from atexit import *\n"),
        ];
        let at = |main: &str, module: &str| run(&files, main, module);
        assert_eq!(
            at("pkg/direct.py", "pkg.direct"),
            "1:W0614:Unused import(s) register from wildcard import of atexit"
        );
        assert_eq!(
            at("pkg/chained.py", "pkg.chained"),
            "1:W0614:Unused import(s) z, register and unregister from wildcard import of pkg.c"
        );
        // A file of the project hides the C module of the same name.
        assert_eq!(
            at("shadow/user.py", "shadow.user"),
            "1:W0614:Unused import(s) mine from wildcard import of atexit"
        );
    }
}
