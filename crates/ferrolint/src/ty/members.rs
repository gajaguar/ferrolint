//! `E1101` (`no-member`) for `self` and `cls`, answered by ty.
//!
//! The rule starts from ty's `unresolved-attribute` diagnostic on a read of
//! `self.x` or `cls.x` and keeps it only where Pylint reports it. ty types
//! `self` as a type variable, so the receiver is the enclosing class. The class
//! must be closed: no ancestor defines `__getattr__` or `__getattribute__`,
//! sets a metaclass or has a base ty cannot resolve (Pylint is silent there).
//! Any other receiver is silent: ty types an annotated parameter or attribute by
//! its declaration, where Pylint infers a value, and the two disagree on most
//! of them.

use std::collections::{BTreeSet, HashMap, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use regex::Regex;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{
    ExceptHandler, Expr, ExprAttribute, ExprContext, ExprRef, Stmt, StmtClassDef,
};
use ruff_text_size::{Ranged, TextRange};
use ty_project::{Db as _, ProjectDatabase};
use ty_python_core::{Program, ProgramFile};
use ty_python_semantic::types::Type;
use ty_python_semantic::{
    HasType, ProgramEnvironment, SemanticModel, check_file, type_hierarchy_prepare,
    type_hierarchy_supertypes,
};

use super::Types;
use crate::native::{self, Ctx};

const CODE: &str = "E1101";

pub fn rules() -> Vec<RuleInfo> {
    vec![native::rule(
        CODE,
        "no-member",
        "Accessing a member `self` or `cls` does not have.",
    )]
}

/// Whether the rule is enabled.
pub fn wanted(enabled: &BTreeSet<String>) -> bool {
    enabled.contains(CODE)
}

/// The diagnostics of the rule on the file at `ctx.path`.
pub fn check(types: &Types, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    types
        .with_file(ctx.path, ctx.source, |db, file| scan(db, file, ctx))
        .unwrap_or_default()
}

fn scan(db: &ProjectDatabase, file: ProgramFile<'_>, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let Ok(diagnostics) = check_file(db, file) else {
        return Vec::new();
    };
    let found: HashSet<TextRange> = diagnostics
        .iter()
        .filter(|d| d.id().as_str() == "unresolved-attribute")
        .filter_map(|d| d.primary_span()?.range())
        .collect();
    if found.is_empty() {
        return Vec::new();
    }
    let module = parsed_module(db, file.python_file(db)).load(db);
    let model = SemanticModel::new(db, file);
    let env = model.program_environment();
    let mut scan = Scan {
        ctx,
        found: &found,
        model: &model,
        env: &env,
        closed: Closed {
            db,
            program: db.project().program(db),
            env: &env,
            memo: HashMap::new(),
            parsed: HashMap::new(),
        },
        settings: Settings::new(ctx),
        classes: Vec::new(),
        guarded: 0,
        out: Vec::new(),
    };
    scan.visit_body(module.suite());
    scan.out
}

/// The options Pylint reads for this check.
struct Settings {
    mixins: Option<Regex>,
    generated: Vec<Regex>,
    ignored: Vec<String>,
    hint: bool,
    distance: usize,
    choices: usize,
}

impl Settings {
    fn new(ctx: &Ctx<'_>) -> Self {
        let list = |key: &str, default: &str| -> Vec<String> {
            ctx.option(key)
                .unwrap_or(default)
                .split(',')
                .map(|item| item.trim().to_owned())
                .filter(|item| !item.is_empty())
                .collect()
        };
        let number = |key: &str, default: usize| {
            ctx.option(key)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(default)
        };
        let ignores_mixins = list(
            "ignored-checks-for-mixins",
            "no-member,not-async-context-manager,not-context-manager,attribute-defined-outside-init",
        )
        .iter()
        .any(|name| name == "no-member");
        Self {
            mixins: ignores_mixins
                .then(|| {
                    let pattern = ctx.option("mixin-class-rgx").unwrap_or(".*[Mm]ixin");
                    Regex::new(&format!("^(?:{pattern})")).ok()
                })
                .flatten(),
            generated: list("generated-members", "")
                .iter()
                .filter_map(|pattern| Regex::new(&format!("^(?:{pattern})")).ok())
                .collect(),
            ignored: list(
                "ignored-classes",
                "optparse.Values,thread._local,_thread._local",
            ),
            hint: !matches!(
                ctx.option("missing-member-hint").map(str::trim),
                Some("n" | "no" | "false" | "0")
            ),
            distance: number("missing-member-hint-distance", 1),
            choices: number("missing-member-max-choices", 1),
        }
    }

    /// Whether Pylint is silent on `attr` of an instance of `class`.
    fn silent(&self, class: &str, attr: &str) -> bool {
        self.mixins.as_ref().is_some_and(|rgx| rgx.is_match(class))
            || self.generated.iter().any(|rgx| rgx.is_match(attr))
            || self
                .ignored
                .iter()
                .any(|name| name.rsplit('.').next() == Some(class))
    }
}

/// Whether a class, its ancestors and their bases are all known to ty and
/// plain: no `__getattr__`, `__getattribute__` or metaclass, and every base
/// resolved. `object` counts as plain: typeshed gives it `__getattribute__`.
struct Closed<'a, 'db> {
    db: &'db ProjectDatabase,
    program: Program<'db>,
    env: &'a ProgramEnvironment<'db>,
    memo: HashMap<(File, TextRange), bool>,
    parsed: HashMap<File, Option<ruff_db::parsed::ParsedModuleRef>>,
}

impl<'db> Closed<'_, 'db> {
    /// The body facts of the class named at `name` in `file`: whether it is
    /// plain, and how many bases it writes.
    fn facts(&mut self, file: File, name: TextRange) -> Option<(bool, usize)> {
        let db = self.db;
        let parsed = self.parsed.entry(file).or_insert_with(|| {
            let pf = self.program.program_file(db, file);
            Some(parsed_module(db, pf.python_file(db)).load(db))
        });
        let module = parsed.as_ref()?;
        module.suite().iter().find_map(|stmt| match stmt {
            Stmt::ClassDef(class) if class.name.range() == name => {
                let hooks = class.body.iter().any(|stmt| {
                    matches!(stmt, Stmt::FunctionDef(f)
                        if matches!(f.name.as_str(), "__getattr__" | "__getattribute__"))
                });
                let (bases, metaclass) = class.arguments.as_ref().map_or((0, false), |args| {
                    (
                        args.args.len(),
                        args.keywords
                            .iter()
                            .any(|k| k.arg.as_ref().is_some_and(|n| n.as_str() == "metaclass")),
                    )
                });
                Some((!hooks && !metaclass, bases))
            }
            _ => None,
        })
    }

    fn plain(&mut self, ty: Type<'db>) -> bool {
        let Some(class) = type_hierarchy_prepare(self.db, self.env, ty) else {
            return false;
        };
        if class.name.as_str() == "object" {
            return true;
        }
        let file = class.file.file(self.db);
        let key = (file, class.selection_range);
        if let Some(&known) = self.memo.get(&key) {
            return known;
        }
        // A provisional answer stops a cycle.
        self.memo.insert(key, true);
        let verdict = self.verdict(ty, file, class.selection_range);
        self.memo.insert(key, verdict);
        verdict
    }

    fn verdict(&mut self, ty: Type<'db>, file: File, name: TextRange) -> bool {
        let Some((plain, bases)) = self.facts(file, name) else {
            return false;
        };
        let supers = type_hierarchy_supertypes(self.db, self.env, ty);
        if !plain || supers.len() < bases {
            return false;
        }
        supers.iter().all(|base| {
            let base_file = base.file.file(self.db);
            self.class_type(base_file, base.selection_range)
                .is_some_and(|base_ty| self.plain(base_ty))
        })
    }

    /// The class named at `name` in `file`, as a type.
    fn class_type(&self, file: File, name: TextRange) -> Option<Type<'db>> {
        let pf = self.program.program_file(self.db, file);
        let module = parsed_module(self.db, pf.python_file(self.db)).load(self.db);
        let model = SemanticModel::new(self.db, pf);
        module.suite().iter().find_map(|stmt| match stmt {
            Stmt::ClassDef(class) if class.name.range() == name => class.inferred_type(&model),
            _ => None,
        })
    }
}

struct Scan<'a, 'db> {
    ctx: &'a Ctx<'a>,
    found: &'a HashSet<TextRange>,
    model: &'a SemanticModel<'db>,
    env: &'a ProgramEnvironment<'db>,
    closed: Closed<'a, 'db>,
    settings: Settings,
    /// The classes enclosing the node, innermost last, with the names they
    /// assign on `self` when they declare `__slots__`.
    classes: Vec<(Option<Type<'db>>, HashSet<String>)>,
    /// Inside a `try` whose handler would catch an `AttributeError`.
    guarded: usize,
    out: Vec<Diagnostic>,
}

impl Scan<'_, '_> {
    fn attribute(&mut self, expr: &Expr, attr: &ExprAttribute) {
        let Expr::Name(receiver) = &*attr.value else {
            return;
        };
        let owner = match receiver.id.as_str() {
            "self" => "Instance of",
            "cls" => "Class",
            _ => return,
        };
        if attr.ctx == ExprContext::Store
            || self.guarded > 0
            || !(self.found.contains(&expr.range()) || self.found.contains(&attr.attr.range()))
            // `self` is `Self`, a type variable; `cls` is `type[Self]`.
            || !matches!(
                ExprRef::from(&*attr.value).inferred_type(self.model),
                Some(Type::TypeVar(_) | Type::SubclassOf(_))
            )
        {
            return;
        }
        let Some((Some(class_ty), assigned)) = self.classes.last() else {
            return;
        };
        let class_ty = *class_ty;
        if assigned.contains(attr.attr.as_str()) {
            return;
        }
        let Some(class) = type_hierarchy_prepare(self.closed.db, self.env, class_ty) else {
            return;
        };
        let name = class.name.as_str();
        if self.settings.silent(name, attr.attr.as_str()) || !self.closed.plain(class_ty) {
            return;
        }
        let hint = self.hint(attr);
        self.out.push(self.ctx.report_at(
            CODE,
            expr.range(),
            format!(
                "{owner} '{name}' has no '{}' member{hint}",
                attr.attr.as_str()
            ),
        ));
    }

    /// Pylint's `; maybe 'x'?`.
    fn hint(&self, attr: &ExprAttribute) -> String {
        if !self.settings.hint {
            return String::new();
        }
        let names = self
            .model
            .attribute_completions(attr)
            .into_iter()
            .map(|completion| completion.name.to_string());
        similar(
            attr.attr.as_str(),
            names,
            self.settings.distance,
            self.settings.choices,
        )
    }
}

/// The names the class defines where ty does not look: `self.x = ...` in a class with
/// `__slots__` (ty ignores them where the slots omit them) and `cls.x = ...` in `__new__` or
/// `__init_subclass__` (ty reads only a classmethod). Pylint reads both as members.
fn slotted(class: &StmtClassDef) -> HashSet<String> {
    let declares = class.body.iter().any(|stmt| {
        matches!(stmt, Stmt::Assign(a) if a.targets.iter().any(
            |t| matches!(t, Expr::Name(n) if n.id.as_str() == "__slots__")))
    });
    let mut names = HashSet::new();
    if declares {
        let mut slots = Assigned::new("self");
        slots.visit_body(&class.body);
        names.extend(slots.names);
    }
    for stmt in &class.body {
        if let Stmt::FunctionDef(func) = stmt
            && matches!(func.name.as_str(), "__new__" | "__init_subclass__")
            && let Some(first) = func.parameters.args.first()
        {
            let mut stored = Assigned::new(first.parameter.name.as_str());
            stored.visit_body(&func.body);
            names.extend(stored.names);
        }
    }
    names
}

/// The attributes assigned on one receiver name.
struct Assigned<'r> {
    receiver: &'r str,
    names: HashSet<String>,
}

impl<'r> Assigned<'r> {
    fn new(receiver: &'r str) -> Self {
        Self {
            receiver,
            names: HashSet::new(),
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for Assigned<'_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Attribute(attr) = expr
            && attr.ctx == ExprContext::Store
            && matches!(&*attr.value, Expr::Name(n) if n.id.as_str() == self.receiver)
        {
            self.names.insert(attr.attr.to_string());
        }
        source_order::walk_expr(self, expr);
    }
}

/// Whether a handler of `try` would catch an `AttributeError`.
fn catches(handlers: &[ExceptHandler]) -> bool {
    let named = |expr: &Expr| {
        matches!(expr, Expr::Name(n)
            if matches!(n.id.as_str(), "AttributeError" | "Exception" | "BaseException"))
    };
    handlers.iter().any(|handler| {
        let ExceptHandler::ExceptHandler(handler) = handler;
        match handler.type_.as_deref() {
            None => true,
            Some(Expr::Tuple(tuple)) => tuple.elts.iter().any(named),
            Some(expr) => named(expr),
        }
    })
}

impl<'a> SourceOrderVisitor<'a> for Scan<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Try(node) if catches(&node.handlers) => {
                self.guarded += 1;
                self.visit_body(&node.body);
                self.guarded -= 1;
                for handler in &node.handlers {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    self.visit_body(&handler.body);
                }
                self.visit_body(&node.orelse);
                self.visit_body(&node.finalbody);
            }
            Stmt::ClassDef(class) => {
                self.classes
                    .push((class.inferred_type(self.model), slotted(class)));
                source_order::walk_stmt(self, stmt);
                self.classes.pop();
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Attribute(attr) = expr {
            self.attribute(expr, attr);
        }
        source_order::walk_expr(self, expr);
    }
}

/// Pylint's hint: the names within `distance` edits of `attr`, at most
/// `choices` of them, the closest first, spelled `; maybe 'a'?` or
/// `; maybe one of 'a' or 'b'?`.
fn similar(
    attr: &str,
    names: impl IntoIterator<Item = String>,
    distance: usize,
    choices: usize,
) -> String {
    let mut near: Vec<(usize, String)> = names
        .into_iter()
        .filter(|name| name != attr)
        .map(|name| (edits(attr, &name), name))
        .filter(|(edits, _)| *edits <= distance)
        .collect();
    near.sort();
    near.dedup();
    near.truncate(choices);
    let mut names: Vec<String> = near
        .into_iter()
        .map(|(_, name)| format!("'{name}'"))
        .collect();
    names.sort();
    match names.split_last() {
        None => String::new(),
        Some((only, [])) => format!("; maybe {only}?"),
        Some((last, rest)) => format!("; maybe one of {} or {last}?", rest.join(", ")),
    }
}

/// The Levenshtein distance.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = std::mem::replace(&mut row[0], i + 1);
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (previous + usize::from(ca != *cb))
                .min(above + 1)
                .min(row[j] + 1);
            previous = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|&item| item.to_owned()).collect()
    }

    #[test]
    fn the_distance_is_levenshtein() {
        assert_eq!(edits("kitten", "sitting"), 3);
        assert_eq!(edits("name", "nam"), 1);
        assert_eq!(edits("", "ab"), 2);
    }

    #[test]
    fn the_hint_follows_pylint() {
        let known = names(&["name", "names", "value"]);
        assert_eq!(similar("nme", known.clone(), 1, 1), "; maybe 'name'?");
        assert_eq!(
            similar("nam", known.clone(), 2, 2),
            "; maybe one of 'name' or 'names'?"
        );
        assert_eq!(similar("nam", known.clone(), 1, 1), "; maybe 'name'?");
        assert_eq!(similar("other", known, 1, 1), "");
    }
}
