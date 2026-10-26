//! The rules on the attributes a class's methods give its instances:
//! `W0201` attribute-defined-outside-init, `E0202` method-hidden, `E0203`
//! access-member-before-definition and `R0902` too-many-instance-attributes.
//!
//! Pylint's instance attributes are the `x.name` targets whose `x` infers to
//! an instance of the class, minus those a `__slots__` leaves out. shortcut:
//! here `x` is the first parameter of a method of the class (or of a function
//! or lambda nested in it that does not rebind it), a name whose nearest
//! binding is that parameter, or a call of a class of the file, of `cls`,
//! of `self._Nested` or of `__new__` on it or on `object`
//! (`object.__new__(self.__class__)`); a branch that rebinds the name is not
//! read, nor is a class reached through a list, a field or a call to another
//! module. Upgrade with a value inference. The ancestors come from
//! [`super::hierarchy`]: a class without an MRO is not checked by the rules
//! that look at its ancestors.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use regex::Regex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ExceptHandler, Expr, ExprContext, ExprName, Parameters, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};

use super::classes::{Env, Local, bases, is_classmethod, is_static, locals, methods, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::misc::{csv, regex};
use super::slots::{names, slots_values};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const OUTSIDE_INIT: &str = "W0201";
const HIDDEN: &str = "E0202";
const BEFORE_DEFINITION: &str = "E0203";
const TOO_MANY: &str = "R0902";

/// What every class has from `object` or astroid, so that `local_attr` finds
/// it: `object`'s attributes but `__doc__`, and the ones astroid adds.
const OBJECT_ATTRS: [&str; 27] = [
    "__annotations__",
    "__class__",
    "__delattr__",
    "__dict__",
    "__dir__",
    "__eq__",
    "__format__",
    "__ge__",
    "__getattribute__",
    "__getstate__",
    "__gt__",
    "__hash__",
    "__init__",
    "__init_subclass__",
    "__le__",
    "__lt__",
    "__module__",
    "__ne__",
    "__new__",
    "__qualname__",
    "__reduce__",
    "__reduce_ex__",
    "__repr__",
    "__setattr__",
    "__sizeof__",
    "__str__",
    "__subclasshook__",
];

/// What astroid finds in the `BaseException` of a running interpreter.
const EXCEPTION_ATTRS: [&str; 8] = [
    "__cause__",
    "__context__",
    "__setstate__",
    "__suppress_context__",
    "__traceback__",
    "add_note",
    "args",
    "with_traceback",
];

/// How a site uses the attribute.
#[derive(Clone, Copy, PartialEq)]
enum Use {
    Assign,
    /// `self.x += 1`, which reads it too.
    Aug,
    /// `self.x: int`, which gives it no value.
    Bare,
    Load,
    /// `setattr(self, "x", value)`.
    Setattr,
}

/// A `try` block around a site: its statement's address and which block.
#[derive(Clone, Copy, PartialEq)]
enum Block {
    Body,
    /// A handler, and whether it catches what Pylint asks about.
    Handler(usize, bool),
    Else,
    Finally,
}

/// One use of `self.name` in a method.
#[derive(Clone)]
struct Site<'c> {
    name: &'c str,
    range: TextRange,
    use_: Use,
    /// The address of the innermost function or lambda around the site.
    frame: usize,
    /// That function, `None` for a lambda.
    func: Option<&'c StmtFunctionDef>,
    /// The method the site is in; `None` for code outside the class.
    method: Option<&'c StmtFunctionDef>,
    /// Whether the method binds the class (`cls.x`) rather than an instance.
    on_class: bool,
    /// Whether the site is the callee of a call, as `self.m()`.
    call: bool,
    tries: Vec<(usize, Block)>,
}

impl Site<'_> {
    fn stores(&self) -> bool {
        matches!(self.use_, Use::Assign | Use::Aug | Use::Bare)
    }

    fn is_instance_attr(&self) -> bool {
        self.stores() && !self.on_class
    }

    fn reads(&self) -> bool {
        matches!(self.use_, Use::Load | Use::Aug)
    }
}

/// Every use of the first parameter's attributes in the methods of `class`.
fn sites(class: &StmtClassDef) -> Vec<Site<'_>> {
    let mut out = Vec::new();
    for method in methods(&class.body) {
        if is_static(method) {
            continue;
        }
        let p = &method.parameters;
        let Some(first) = p.posonlyargs.first().or(p.args.first()) else {
            continue;
        };
        let mut walk = Sites {
            param: first.parameter.name.as_str(),
            method,
            on_class: is_classmethod(method),
            frames: vec![(std::ptr::from_ref(method).addr(), Some(method))],
            tries: Vec::new(),
            store: Use::Assign,
            callee: None,
            out: &mut out,
        };
        walk.visit_body(&method.body);
    }
    out
}

struct Sites<'c, 'o> {
    param: &'c str,
    method: &'c StmtFunctionDef,
    on_class: bool,
    frames: Vec<(usize, Option<&'c StmtFunctionDef>)>,
    tries: Vec<(usize, Block)>,
    /// What a store target means in the statement being walked.
    store: Use,
    /// The address of the callee of the call being walked.
    callee: Option<usize>,
    out: &'o mut Vec<Site<'c>>,
}

impl<'c> Sites<'c, '_> {
    fn push(&mut self, name: &'c str, range: TextRange, use_: Use, call: bool) {
        let (frame, func) = *self.frames.last().unwrap_or(&(0, None));
        self.out.push(Site {
            name,
            range,
            use_,
            frame,
            func,
            method: Some(self.method),
            on_class: self.on_class,
            call,
            tries: self.tries.clone(),
        });
    }

    fn block(&mut self, key: (usize, Block), body: &'c [Stmt]) {
        self.tries.push(key);
        self.visit_body(body);
        self.tries.pop();
    }
}

impl<'c> Visitor<'c> for Sites<'c, '_> {
    fn visit_stmt(&mut self, stmt: &'c Stmt) {
        match stmt {
            Stmt::FunctionDef(func) => {
                // A parameter or a local of the same name is not the method's `self`.
                if !func.parameters.includes(self.param)
                    && !locals(&func.body).iter().any(|(n, _)| n == self.param)
                {
                    self.frames
                        .push((std::ptr::from_ref(func).addr(), Some(func)));
                    self.visit_body(&func.body);
                    self.frames.pop();
                }
            }
            Stmt::Try(t) => {
                let id = std::ptr::from_ref(t).addr();
                self.block((id, Block::Body), &t.body);
                for (i, handler) in t.handlers.iter().enumerate() {
                    let ExceptHandler::ExceptHandler(h) = handler;
                    // astroid's `catch` of `AttributeError`, `Exception` and `BaseException`.
                    let catches = h.type_.as_deref().is_none_or(|kind| {
                        let mut names = Names(Vec::new());
                        names.visit_expr(kind);
                        names.0.iter().any(|name| {
                            matches!(
                                name.as_str(),
                                "AttributeError" | "Exception" | "BaseException"
                            )
                        })
                    });
                    self.block((id, Block::Handler(i, catches)), &h.body);
                }
                self.block((id, Block::Else), &t.orelse);
                self.block((id, Block::Finally), &t.finalbody);
            }
            _ => {
                let saved = self.store;
                self.store = match stmt {
                    Stmt::AugAssign(_) => Use::Aug,
                    Stmt::AnnAssign(a) if a.value.is_none() => Use::Bare,
                    _ => Use::Assign,
                };
                visitor::walk_stmt(self, stmt);
                self.store = saved;
            }
        }
    }

    fn visit_expr(&mut self, expr: &'c Expr) {
        match expr {
            Expr::Attribute(a) if matches!(&*a.value, Expr::Name(n) if n.id.as_str() == self.param) =>
            {
                let call = self.callee == Some(std::ptr::from_ref(expr).addr());
                match a.ctx {
                    ExprContext::Store => self.push(a.attr.as_str(), a.range(), self.store, false),
                    ExprContext::Load => self.push(a.attr.as_str(), a.range(), Use::Load, call),
                    _ => {}
                }
            }
            Expr::Call(c) => {
                // Pylint reads `setattr` in the method itself, not in a nested function.
                if let (Expr::Name(f), [Expr::Name(target), Expr::StringLiteral(name), _, ..]) =
                    (&*c.func, &c.arguments.args[..])
                    && f.id.as_str() == "setattr"
                    && target.id.as_str() == self.param
                    && !self.on_class
                    && self.frames.len() == 1
                {
                    self.push(name.value.to_str(), c.range(), Use::Setattr, false);
                }
                let saved = self.callee;
                self.callee = Some(std::ptr::from_ref(&*c.func).addr());
                self.visit_expr(&c.func);
                self.callee = saved;
                self.visit_arguments(&c.arguments);
            }
            Expr::Lambda(l) => {
                if !l
                    .parameters
                    .as_ref()
                    .is_some_and(|p| p.includes(self.param))
                {
                    self.frames.push((std::ptr::from_ref(l).addr(), None));
                    self.visit_expr(&l.body);
                    self.frames.pop();
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// The sites outside a class that give its instances an attribute, by the
/// class's address: `x.name = value` where `x` is bound once in its scope
/// to a call of a class of this file, `x = Model()`, or of a name bound
/// once to such a class or to a call of a function that returns one.
struct Clients<'a, 'e> {
    env: &'e Env<'a>,
    /// The value of each name bound by a plain `name = value`, by node id.
    values: HashMap<u32, &'a Expr>,
    frames: Vec<(usize, Option<&'a StmtFunctionDef>)>,
    classes: Vec<&'a StmtClassDef>,
    /// The first parameter of each method being walked (by node id), with its
    /// class and whether it is a classmethod (the parameter is the class, not
    /// an instance).
    cls: Vec<(Option<u32>, &'a StmtClassDef, bool)>,
    store: Use,
    out: HashMap<usize, Vec<Site<'a>>>,
}

/// The value of every `name = value` in a module.
struct Values<'a>(HashMap<u32, &'a Expr>);

impl<'a> Visitor<'a> for Values<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let bound = match stmt {
            Stmt::Assign(a) => match &a.targets[..] {
                [Expr::Name(name)] => Some((name, &*a.value)),
                _ => None,
            },
            Stmt::AnnAssign(a) => match (&*a.target, a.value.as_deref()) {
                (Expr::Name(name), Some(value)) => Some((name, value)),
                _ => None,
            },
            _ => None,
        };
        if let Some((name, value)) = bound
            && let Some(id) = ruff_python_ast::HasNodeIndex::node_index(name)
                .load()
                .as_u32()
        {
            self.0.insert(id, value);
        }
        visitor::walk_stmt(self, stmt);
    }
}

impl<'a> Clients<'a, '_> {
    fn bound(&self, expr: &Expr) -> Option<&'a Expr> {
        let Expr::Name(name) = expr else { return None };
        let binding = self.env.binding(name)?;
        (binding.kind == BindingKind::Assignment)
            .then(|| self.values.get(&binding.node?).copied())
            .flatten()
    }

    /// The class of this file that `expr` evaluates to.
    fn class(&self, expr: &Expr, depth: u8) -> Option<&'a StmtClassDef> {
        if depth > 8 {
            return None;
        }
        match expr {
            Expr::Name(name) => {
                if let Some((class, true)) = self.param(name) {
                    return Some(class);
                }
            }
            // `self.__class__`
            Expr::Attribute(a) if a.attr.as_str() == "__class__" => {
                if let Expr::Name(name) = &*a.value
                    && let Some((class, false)) = self.param(name)
                {
                    return Some(class);
                }
            }
            // `self._result` or `cls._result`, a class the method's class
            // binds once in its body. shortcut: not one it inherits or an
            // instance attribute shadows; upgrade with a value inference.
            Expr::Attribute(a) => {
                if let Expr::Name(name) = &*a.value
                    && let Some((class, _)) = self.param(name)
                {
                    let attr = a.attr.as_str();
                    if locals(&class.body)
                        .iter()
                        .filter(|(n, _)| n == attr)
                        .count()
                        == 1
                    {
                        return class.body.iter().find_map(|stmt| match stmt {
                            Stmt::ClassDef(c) if c.name.as_str() == attr => Some(c),
                            _ => None,
                        });
                    }
                }
            }
            _ => {}
        }
        if let Some(class) = self.env.class_of(expr) {
            return Some(class);
        }
        if let Some(value) = self.bound(expr) {
            return self.class(value, depth + 1);
        }
        let Expr::Call(call) = expr else { return None };
        let Expr::Name(callee) = &*call.func else {
            return None;
        };
        let func = self.env.func_of(callee)?;
        let mut returns = Returns(Vec::new());
        returns.visit_body(&func.body);
        let mut classes = returns
            .0
            .iter()
            .map(|value| self.class(value, depth + 1).map(std::ptr::from_ref));
        let first = classes.next()??;
        classes
            .all(|class| class == Some(first))
            .then(|| self.class(returns.0[0], depth + 1))
            .flatten()
    }

    /// The class of this file whose instance `expr` is.
    fn instance(&self, expr: &Expr, depth: u8) -> Option<&'a StmtClassDef> {
        if depth > 8 {
            return None;
        }
        match expr {
            Expr::Call(call) => {
                // `object.__new__(C)` makes an instance of `C`, and so does
                // `C.__new__(...)` unless `C` has a `__new__` of its own.
                if let Expr::Attribute(f) = &*call.func
                    && f.attr.as_str() == "__new__"
                {
                    if self.env.qualified(&f.value).is_some_and(|q| q == "object") {
                        return self.class(call.arguments.args.first()?, depth + 1);
                    }
                    let class = self.class(&f.value, depth + 1)?;
                    return (!self.env.defines(class, "__new__", 0)).then_some(class);
                }
                self.class(&call.func, depth + 1)
            }
            // An alias of the method's own `self`.
            Expr::Name(name) if depth > 0 => match self.param(name) {
                Some((class, false)) => Some(class),
                _ => self.instance(self.bound(expr)?, depth + 1),
            },
            _ => self.instance(self.bound(expr)?, depth + 1),
        }
    }

    /// The class of the method whose first parameter `name` is, and whether
    /// that parameter is the class itself.
    fn param(&self, name: &ExprName) -> Option<(&'a StmtClassDef, bool)> {
        let binding = self.env.binding(name)?;
        let node = binding
            .node
            .filter(|_| binding.kind == BindingKind::Parameter)?;
        self.cls
            .iter()
            .find(|(p, ..)| *p == Some(node))
            .map(|&(_, class, is_cls)| (class, is_cls))
    }
}

/// The values a function returns, nested functions aside.
struct Returns<'a>(Vec<&'a Expr>);

impl<'a> Visitor<'a> for Returns<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Return(r) => self.0.extend(r.value.as_deref()),
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

impl<'a> Visitor<'a> for Clients<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            self.classes.push(class);
            visitor::walk_stmt(self, stmt);
            self.classes.pop();
            return;
        }
        if let Stmt::FunctionDef(func) = stmt {
            let p = &func.parameters;
            let cls = self
                .classes
                .last()
                .filter(|c| {
                    !is_static(func) && methods(&c.body).iter().any(|m| std::ptr::eq(*m, func))
                })
                .zip(p.posonlyargs.first().or(p.args.first()))
                .map(|(class, first)| {
                    (
                        ruff_python_ast::HasNodeIndex::node_index(&first.parameter)
                            .load()
                            .as_u32(),
                        *class,
                        is_classmethod(func),
                    )
                });
            self.cls.extend(cls);
            self.frames
                .push((std::ptr::from_ref(func).addr(), Some(func)));
            visitor::walk_stmt(self, stmt);
            self.frames.pop();
            if cls.is_some() {
                self.cls.pop();
            }
            return;
        }
        let saved = self.store;
        self.store = match stmt {
            Stmt::AugAssign(_) => Use::Aug,
            Stmt::AnnAssign(a) if a.value.is_none() => Use::Bare,
            _ => Use::Assign,
        };
        visitor::walk_stmt(self, stmt);
        self.store = saved;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Lambda(l) = expr {
            self.frames.push((std::ptr::from_ref(l).addr(), None));
            visitor::walk_expr(self, expr);
            self.frames.pop();
            return;
        }
        if let Expr::Attribute(a) = expr
            && a.ctx == ExprContext::Store
            && matches!(&*a.value, Expr::Name(_))
            && let Some(class) = self.instance(&a.value, 0)
        {
            let (frame, func) = *self.frames.last().unwrap_or(&(0, None));
            self.out
                .entry(std::ptr::from_ref(class).addr())
                .or_default()
                .push(Site {
                    name: a.attr.as_str(),
                    range: a.range(),
                    use_: self.store,
                    frame,
                    func,
                    method: None,
                    on_class: false,
                    call: false,
                    tries: Vec::new(),
                });
        }
        visitor::walk_expr(self, expr);
    }
}

/// The names in an expression.
struct Names(Vec<String>);

impl Visitor<'_> for Names {
    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Name(name) = expr {
            self.0.push(name.id.to_string());
        }
        visitor::walk_expr(self, expr);
    }
}

/// astroid's `are_exclusive` with Pylint's exceptions: one site in a `try`
/// body and the other in a handler that catches them, or one in its `else`
/// and the other in a handler.
fn exclusive(a: &Site<'_>, b: &Site<'_>) -> bool {
    let Some(k) = (0..a.tries.len().min(b.tries.len())).find(|&k| a.tries[k] != b.tries[k]) else {
        return false;
    };
    let ((ta, ba), (tb, bb)) = (a.tries[k], b.tries[k]);
    ta == tb
        && matches!(
            (ba, bb),
            (Block::Body, Block::Handler(_, true))
                | (Block::Handler(_, true), Block::Body)
                | (Block::Else, Block::Handler(..))
                | (Block::Handler(..), Block::Else)
        )
}

/// The fields a dataclass gives its instances: the annotated names of its
/// body that are not `ClassVar`, `InitVar` or the `KW_ONLY` sentinel.
fn dataclass_fields(class: &StmtClassDef) -> Vec<&str> {
    class
        .body
        .iter()
        .filter_map(|stmt| {
            let Stmt::AnnAssign(a) = stmt else {
                return None;
            };
            let Expr::Name(name) = &*a.target else {
                return None;
            };
            let head = match &*a.annotation {
                Expr::Subscript(s) => &*s.value,
                other => other,
            };
            let last = match head {
                Expr::Name(n) => n.id.as_str(),
                Expr::Attribute(a) => a.attr.as_str(),
                _ => "",
            };
            (!matches!(last, "ClassVar" | "InitVar" | "KW_ONLY")).then(|| name.id.as_str())
        })
        .collect()
}

/// The `name = property(fget, fset)` setters of a class body.
fn classic_setters(class: &StmtClassDef) -> Vec<&str> {
    let mut out = Vec::new();
    for stmt in &class.body {
        let Stmt::Assign(assign) = stmt else { continue };
        let Expr::Call(call) = &*assign.value else {
            continue;
        };
        if !matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "property") {
            continue;
        }
        let fset = call
            .arguments
            .find_keyword("fset")
            .map(|k| &k.value)
            .or(call.arguments.args.get(1));
        if let Some(Expr::Name(name)) = fset {
            out.push(name.id.as_str());
        }
    }
    out
}

fn is_setter(func: &StmtFunctionDef) -> bool {
    func.decorator_list
        .iter()
        .any(|d| matches!(&d.expression, Expr::Attribute(a) if a.attr.as_str() == "setter"))
}

/// Pylint's `is_attr_private`: two leading underscores, at most one trailing.
pub(super) fn is_private(name: &str) -> bool {
    let end = name.strip_suffix('_').unwrap_or(name);
    name.starts_with("__") && end.len() > 2 && !end.ends_with('_')
}

/// Whether `method`, of `owner`, calls the function `name` on `self` or on a
/// local, as `self.name()`. Pylint asks that the callee infer to a bound
/// method of that name: a call on `self.helper` or on another parameter does
/// not, and `initialize = _initialize` in `owner` binds `_initialize`, not
/// `initialize`. shortcut: any local counts as an instance (`inst = new(cls);
/// inst.name()`), and an alias is read in `owner` alone; upgrade with a value
/// inference.
fn calls(owner: &StmtClassDef, method: &StmtFunctionDef, name: &str) -> bool {
    struct Find<'n>(&'n str, &'n str, &'n StmtClassDef, &'n Parameters, bool);
    impl Visitor<'_> for Find<'_> {
        fn visit_expr(&mut self, expr: &Expr) {
            if let Expr::Call(c) = expr
                && let Expr::Attribute(a) = &*c.func
                && matches!(&*a.value, Expr::Name(n)
                    if n.id.as_str() == self.1 || !self.3.includes(n.id.as_str()))
                && alias(self.2, a.attr.as_str()) == self.0
            {
                self.4 = true;
            }
            visitor::walk_expr(self, expr);
        }
    }
    let p = &method.parameters;
    let Some(first) = p.posonlyargs.first().or(p.args.first()) else {
        return false;
    };
    let mut find = Find(name, first.parameter.name.as_str(), owner, p, false);
    find.visit_body(&method.body);
    find.4
}

/// What `attr` names in `class` when its body binds it as `attr = other`.
fn alias<'a>(class: &'a StmtClassDef, attr: &'a str) -> &'a str {
    class
        .body
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Assign(a) => match (&a.targets[..], &*a.value) {
                ([Expr::Name(target)], Expr::Name(value)) if target.id.as_str() == attr => {
                    Some(value.id.as_str())
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap_or(attr)
}

pub struct InstanceAttrs;

impl Checker for InstanceAttrs {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                OUTSIDE_INIT,
                "attribute-defined-outside-init",
                "An instance attribute first assigned outside the methods of `defining-attr-methods`.",
            ),
            rule(
                HIDDEN,
                "method-hidden",
                "A method that an instance attribute of the class or an ancestor hides.",
            ),
            rule(
                BEFORE_DEFINITION,
                "access-member-before-definition",
                "An instance attribute read in a method before the method assigns it.",
            ),
            rule(
                TOO_MANY,
                "too-many-instance-attributes",
                "A class with more instance attributes than `max-attributes`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![OUTSIDE_INIT, HIDDEN, BEFORE_DEFINITION, TOO_MANY]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let env = Env::new(ctx);
        let csv = |key: &str, default: &str| csv(ctx, key, default);
        let mixins = csv(
            "ignored-checks-for-mixins",
            "no-member,not-async-context-manager,not-context-manager,attribute-defined-outside-init",
        )
        .iter()
        .any(|name| name == "attribute-defined-outside-init")
        .then(|| {
            let pattern = ctx.option("mixin-class-rgx").unwrap_or(".*[Mm]ixin");
            regex(&format!("^(?:{pattern})"))
        })
        .flatten();
        let mut walk = Walk {
            env: &env,
            hierarchy: Hierarchy::new(&env),
            clients: clients(&env),
            enabled,
            defining: csv(
                "defining-attr-methods",
                "__init__,__new__,setUp,asyncSetUp,__post_init__",
            ),
            mixins,
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// The sites outside the classes of the file that give their instances an attribute,
/// by the class's address.
fn clients<'a>(env: &Env<'a>) -> HashMap<usize, Vec<Site<'a>>> {
    let body = &env.ctx.parsed.syntax().body;
    let mut values = Values(HashMap::new());
    values.visit_body(body);
    let mut clients = Clients {
        env,
        values: values.0,
        frames: Vec::new(),
        classes: Vec::new(),
        cls: Vec::new(),
        store: Use::Assign,
        out: HashMap::new(),
    };
    clients.visit_body(body);
    clients.out
}

/// The attributes that code outside `class` gives its instances (`x = Cls()` then
/// `x.name = value`), each with its first site, in the order astroid records them.
pub(super) fn outside_attrs<'a>(env: &Env<'a>) -> HashMap<usize, Vec<(&'a str, TextRange)>> {
    clients(env)
        .into_iter()
        .map(|(class, sites)| {
            let attrs = sites
                .iter()
                .filter(|s| s.is_instance_attr())
                .map(|s| (s.name, s.range))
                .collect();
            (class, attrs)
        })
        .collect()
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: Hierarchy<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    defining: Vec<String>,
    mixins: Option<Regex>,
    clients: HashMap<usize, Vec<Site<'a>>>,
    out: &'e mut Vec<Diagnostic>,
}

/// What the classes of an MRO define, read once per class.
struct Ancestry<'m> {
    /// The names the classes bind in their bodies or as `cls.x`: astroid's
    /// `local_attr`.
    names: BTreeSet<String>,
    /// The sites of each class but the first, in MRO order.
    sites: Vec<Vec<Site<'m>>>,
    /// The methods of `defining-attr-methods` of each class, from that
    /// class on: the ones Pylint looks a defining method up in.
    defining: Vec<Vec<(&'m StmtClassDef, &'m StmtFunctionDef)>>,
}

impl<'m> Ancestry<'m> {
    fn new(mro: &'m [Class<'_>], defining: &[String]) -> Self {
        let defs: Vec<&StmtClassDef> = mro.iter().filter_map(Class::def).collect();
        let mut names = BTreeSet::new();
        let mut all = Vec::new();
        for (i, def) in defs.iter().enumerate() {
            let found = sites(def);
            names.extend(locals(&def.body).into_iter().map(|(name, _)| name));
            names.extend(
                found
                    .iter()
                    .filter(|s| s.on_class && s.stores())
                    .map(|s| s.name.to_owned()),
            );
            if i > 0 {
                all.push(found);
            }
        }
        let defining = (0..defs.len())
            .map(|i| {
                defs[i..]
                    .iter()
                    .flat_map(|def| methods(&def.body).into_iter().map(move |f| (*def, f)))
                    .filter(|(_, f)| defining.iter().any(|name| name == f.name.as_str()))
                    .collect()
            })
            .collect();
        Self {
            names,
            sites: all,
            defining,
        }
    }

    /// Whether an ancestor gives instances `name`.
    fn instance_attr(&self, name: &str) -> bool {
        self.sites
            .iter()
            .flatten()
            .any(|s| s.is_instance_attr() && s.name == name)
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn line(&self, range: TextRange) -> u32 {
        self.env.ctx.index.line(range.start().to_u32())
    }

    fn defining(&self, func: Option<&StmtFunctionDef>) -> bool {
        func.is_some_and(|f| self.defining.iter().any(|name| name == f.name.as_str()))
    }

    /// astroid's `ClassDef.slots`: the slot names of `class` and its ancestors
    /// when every one of them has plain `__slots__`; `None` when one has not,
    /// or when none names a slot. shortcut: a builtin base is not in the MRO,
    /// where Pylint finds it without slots and lets any attribute through.
    fn slots(&self, class: &Class<'a>) -> Option<BTreeSet<String>> {
        let mut out = BTreeSet::new();
        for found in self.hierarchy.mro(class)? {
            for value in slots_values(found.def()?)? {
                let items = match value {
                    Expr::StringLiteral(s) => vec![s.value.to_str()],
                    _ => names(value)?,
                };
                out.extend(items.into_iter().map(str::to_owned));
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn class(&mut self, class: &'a StmtClassDef) {
        let mut own = sites(class);
        own.extend(
            self.clients
                .get(&std::ptr::from_ref(class).addr())
                .into_iter()
                .flatten()
                .cloned(),
        );
        let mro = self.hierarchy.mro(&Class::Local(class));
        let mut ancestry = mro.as_deref().map(|mro| Ancestry::new(mro, &self.defining));
        // astroid's `_can_assign_attr`: a `self.x = ...` outside the slots adds nothing.
        let keep = |site: &Site<'_>, slots: &Option<BTreeSet<String>>| {
            !site.is_instance_attr() || slots.as_ref().is_none_or(|s| s.contains(site.name))
        };
        let slots = self.slots(&Class::Local(class));
        own.retain(|s| keep(s, &slots));
        // shortcut: only the attributes every builtin exception has, and only through
        // bases of this file; `errno` or `msg` of a subclass, upgrade with the builtin MRO.
        if let Some(ancestry) = &mut ancestry
            && bases(class).iter().any(|b| {
                self.env
                    .base_is(b, &|name| super::flow::builtin_exception(name).is_some())
            })
        {
            ancestry.names.extend(EXCEPTION_ATTRS.map(String::from));
        }
        if let (Some(mro), Some(ancestry)) = (&mro, &mut ancestry) {
            let ancestors = mro.iter().filter(|c| c.def().is_some()).skip(1);
            for (found, sites) in ancestors.zip(&mut ancestry.sites) {
                let slots = self.slots(found);
                sites.retain(|s| keep(s, &slots));
            }
        }
        if self.on(TOO_MANY) {
            self.too_many(class, &own);
        }
        if self.on(HIDDEN)
            && let (Some(mro), Some(ancestry)) = (&mro, &ancestry)
        {
            for method in methods(&class.body) {
                self.hidden(class, mro, ancestry, &own, method);
            }
        }
        if self
            .mixins
            .as_ref()
            .is_some_and(|rx| rx.is_match(class.name.as_str()))
        {
            return;
        }
        let metaclass = bases(class)
            .iter()
            .any(|base| self.env.base_is(base, &|name| name == "type"));
        if self.on(BEFORE_DEFINITION)
            && !metaclass
            && let Some(ancestry) = &ancestry
        {
            self.before_definition(class, ancestry, &own);
        }
        if self.on(OUTSIDE_INIT) {
            self.outside_init(class, ancestry.as_ref(), &own, metaclass);
        }
    }

    fn too_many(&mut self, class: &StmtClassDef, own: &[Site<'_>]) {
        let max = self
            .env
            .ctx
            .option("max-attributes")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(7);
        let mut names: BTreeSet<&str> = own
            .iter()
            .filter(|s| s.is_instance_attr())
            .map(|s| s.name)
            .collect();
        if self.is_dataclass(class) {
            names.extend(dataclass_fields(class));
        }
        if names.len() > max {
            self.out.push(self.env.ctx.report_at(
                TOO_MANY,
                class.name.range(),
                format!("Too many instance attributes ({}/{max})", names.len()),
            ));
        }
    }

    /// astroid's `is_decorated_with_dataclass`: a `dataclass` of the
    /// standard library, `pydantic` or `marshmallow_dataclass`, or a decorator
    /// it cannot read named `dataclass`.
    fn is_dataclass(&self, class: &StmtClassDef) -> bool {
        class.decorator_list.iter().any(|decorator| {
            let expr = match &decorator.expression {
                Expr::Call(call) => &*call.func,
                other => other,
            };
            match self.env.qualified(expr) {
                Some(name) => matches!(
                    name.as_str(),
                    "dataclasses.dataclass"
                        | "pydantic.dataclasses.dataclass"
                        | "marshmallow_dataclass.dataclass"
                ),
                None => match expr {
                    Expr::Name(n) => n.id.as_str() == "dataclass",
                    Expr::Attribute(a) => a.attr.as_str() == "dataclass",
                    _ => false,
                },
            }
        })
    }

    /// Pylint's decorators that keep a method from being hidden: a property,
    /// a setter, a cached property, and a class with `__get__` and
    /// `__set__`. shortcut: a decorator that is a function counts as one
    /// that returns no such class, where Pylint infers what it returns;
    /// upgrade with a value inference.
    fn shields(&self, class: &'a StmtClassDef, method: &StmtFunctionDef) -> bool {
        method.decorator_list.iter().any(|decorator| {
            let expr = &decorator.expression;
            if let Expr::Attribute(a) = expr
                && matches!(a.attr.as_str(), "getter" | "setter" | "deleter")
            {
                return true;
            }
            if matches!(expr, Expr::Name(n) if n.id.as_str() == "property")
                || self
                    .env
                    .qualified(expr)
                    .is_some_and(|name| name == "functools.cached_property")
            {
                return true;
            }
            let Some(found) = self.hierarchy.resolve(&Class::Local(class), expr) else {
                return false;
            };
            self.hierarchy.mro(&found).is_none_or(|mro| {
                let names = Ancestry::new(&mro, &[]).names;
                names.contains("__get__") && names.contains("__set__")
            })
        })
    }

    fn hidden(
        &mut self,
        class: &'a StmtClassDef,
        mro: &[Class<'a>],
        ancestry: &Ancestry<'_>,
        own: &[Site<'_>],
        method: &StmtFunctionDef,
    ) {
        let name = method.name.as_str();
        if name == "__init__" || self.shields(class, method) {
            return;
        }
        // astroid's `instance_attr`: the class's own sites, then its ancestors'.
        let first = std::iter::once(own)
            .chain(ancestry.sites.iter().map(Vec::as_slice))
            .zip(mro)
            .find_map(|(sites, owner)| {
                sites
                    .iter()
                    .find(|s| s.is_instance_attr() && s.name == name)
                    .map(|s| (owner, s))
            });
        // Only an attribute assigned in a method itself hides one.
        let Some((owner, site)) = first else { return };
        if !site
            .method
            .is_some_and(|m| site.func.is_some_and(|f| std::ptr::eq(f, m)))
            || site.on_class
        {
            return;
        }
        if is_private(name) && ancestry.instance_attr(name) {
            return;
        }
        let ancestor_method = mro[1..].iter().filter_map(Class::def).any(|def| {
            locals(&def.body)
                .iter()
                .any(|(local, kind)| local == name && *kind == Local::Function)
        });
        if ancestor_method {
            return;
        }
        let message = format!(
            "An attribute defined in {} line {} hides this method",
            self.hierarchy.module(owner),
            self.hierarchy.line(owner, site.range.start())
        );
        self.out
            .push(self.env.ctx.report_at(HIDDEN, method.name.range(), message));
    }

    fn before_definition(
        &mut self,
        class: &StmtClassDef,
        ancestry: &Ancestry<'_>,
        own: &[Site<'_>],
    ) {
        let mut names: Vec<&str> = Vec::new();
        for site in own.iter().filter(|s| s.reads()) {
            if !names.contains(&site.name) {
                names.push(site.name);
            }
        }
        for name in names {
            if OBJECT_ATTRS.contains(&name)
                || ancestry.names.contains(name)
                || ancestry.instance_attr(name)
            {
                continue;
            }
            let defs: Vec<&Site<'_>> = own
                .iter()
                .filter(|s| s.is_instance_attr() && s.name == name && s.use_ == Use::Assign)
                .collect();
            let Some(first) = defs.first() else { continue };
            // Several assignments in one function count as the first one.
            let defs: Vec<&Site<'_>> = defs
                .iter()
                .enumerate()
                .filter(|(i, s)| *i == 0 || s.frame != first.frame)
                .map(|(_, s)| *s)
                .collect();
            let [def] = defs[..] else { continue };
            let line = self.line(def.range);
            let accessed: Vec<&Site<'_>> =
                own.iter().filter(|s| s.reads() && s.name == name).collect();
            let mut nodes = accessed.clone();
            if def.func.is_some_and(|f| f.name.as_str() == "__init__") {
                // A method called before the assignment does not see it either.
                for site in &accessed {
                    let direct = class.body.iter().find_map(|stmt| match stmt {
                        Stmt::FunctionDef(f) if site.func.is_some_and(|g| std::ptr::eq(f, g)) => {
                            Some(f)
                        }
                        _ => None,
                    });
                    if let Some(method) = direct {
                        nodes.extend(
                            own.iter()
                                .filter(|s| s.reads() && s.call && s.name == method.name.as_str()),
                        );
                    }
                }
            }
            let mut seen = BTreeSet::new();
            for node in nodes {
                if seen.insert(node.range.start())
                    && node.frame == def.frame
                    && self.line(node.range) < line
                    && !exclusive(node, def)
                {
                    self.out.push(self.env.ctx.report_at(
                        BEFORE_DEFINITION,
                        node.range,
                        format!(
                            "Access to member {} before its definition line {line}",
                            py_repr(name)
                        ),
                    ));
                }
            }
        }
    }

    fn outside_init(
        &mut self,
        class: &StmtClassDef,
        ancestry: Option<&Ancestry<'_>>,
        own: &[Site<'_>],
        metaclass: bool,
    ) {
        let setters = classic_setters(class);
        let counts = |s: &Site<'_>| {
            (!s.on_class && matches!(s.use_, Use::Assign | Use::Bare))
                || (s.use_ == Use::Setattr && !metaclass)
        };
        let mut names: Vec<&str> = Vec::new();
        for site in own.iter().filter(|s| s.is_instance_attr() || counts(s)) {
            if !names.contains(&site.name) {
                names.push(site.name);
            }
        }
        for name in names {
            if OBJECT_ATTRS.contains(&name) {
                continue;
            }
            // `self.x += 1` alone is left to the typing rules, as in Pylint.
            let nodes: Vec<&Site<'_>> =
                own.iter().filter(|s| s.name == name && counts(s)).collect();
            let excused = |s: &&Site<'_>| {
                self.defining(s.func)
                    || s.func.is_some_and(|f| {
                        is_setter(f)
                        || (setters.contains(&f.name.as_str())
                            && class.body.iter().any(
                                |stmt| matches!(stmt, Stmt::FunctionDef(g) if std::ptr::eq(g, f)),
                            ))
                    })
            };
            if nodes.is_empty() || nodes.iter().any(excused) {
                continue;
            }
            let Some(ancestry) = ancestry else { return };
            if ancestry.names.contains(name) || self.in_parents(ancestry, name) {
                continue;
            }
            let called = |s: &&Site<'_>| {
                s.func.is_some_and(|f| {
                    ancestry.defining[0]
                        .iter()
                        .any(|(owner, method)| calls(owner, method, f.name.as_str()))
                })
            };
            if nodes.iter().any(called) {
                continue;
            }
            for node in nodes {
                self.out.push(self.env.ctx.report_at(
                    OUTSIDE_INIT,
                    node.range,
                    format!("Attribute {} defined outside __init__", py_repr(name)),
                ));
            }
        }
    }

    /// Pylint's `_defined_in_parent_init` and `_parent_setattr_names`: an
    /// ancestor assigns `name`, or sets it with `setattr`, in a defining
    /// method or in a method that one calls.
    fn in_parents(&self, ancestry: &Ancestry<'_>, name: &str) -> bool {
        ancestry.sites.iter().enumerate().any(|(i, sites)| {
            let defining = &ancestry.defining[i + 1];
            sites.iter().any(|s| {
                s.name == name
                    && ((s.is_instance_attr()
                        && (self.defining(s.func)
                            || s.func.is_some_and(|f| {
                                defining.iter().any(|(o, m)| calls(o, m, f.name.as_str()))
                            })))
                        || (s.use_ == Use::Setattr && self.defining(s.method)))
            })
        })
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
    fn instance_attributes() {
        let source = include_str!("fixtures/instance_attributes.py");
        let expected = include_str!("fixtures/instance_attributes.expected").trim_end();
        assert_eq!(
            render(source, &[], &["W0201", "E0202", "E0203", "R0902"]),
            expected
        );
    }

    #[test]
    fn instance_attribute_slots() {
        let source = include_str!("fixtures/instance_attribute_slots.py");
        let expected = include_str!("fixtures/instance_attribute_slots.expected").trim_end();
        assert_eq!(
            render(
                source,
                &[("max-attributes", "1")],
                &["W0201", "E0202", "E0203", "R0902"]
            ),
            expected
        );
    }

    #[test]
    fn instance_attribute_external() {
        let source = include_str!("fixtures/instance_attribute_external.py");
        let expected = include_str!("fixtures/instance_attribute_external.expected").trim_end();
        assert_eq!(
            render(
                source,
                &[("max-attributes", "1")],
                &["W0201", "E0202", "E0203", "R0902"]
            ),
            expected
        );
    }

    #[test]
    fn instance_attribute_inference() {
        let source = include_str!("fixtures/instance_attribute_inference.py");
        let expected = include_str!("fixtures/instance_attribute_inference.expected").trim_end();
        assert_eq!(render(source, &[], &["W0201"]), expected);
    }

    #[test]
    fn instance_attribute_options() {
        let source = include_str!("fixtures/instance_attribute_options.py");
        let expected = include_str!("fixtures/instance_attribute_options.expected").trim_end();
        assert_eq!(
            render(
                source,
                &[
                    ("defining-attr-methods", "__init__,setup"),
                    ("max-attributes", "2"),
                    ("mixin-class-rgx", ".*Helper"),
                ],
                &["W0201", "E0202", "E0203", "R0902"]
            ),
            expected
        );
    }
}
