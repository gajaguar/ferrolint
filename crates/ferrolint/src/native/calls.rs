//! Rules about one call or assignment: `E3701` invalid-field-call, `W1115`
//! non-str-assignment-to-dunder-name, `W1506` bad-thread-instantiation,
//! `W1503` redundant-unittest-assert, `W0715` raising-format-tuple, `W1114`
//! arguments-out-of-order and `W1117` kwarg-superseded-by-positional-arg; and
//! the arity of the same callees: `E1120` no-value-for-parameter, `E1121`
//! too-many-function-args, `E1123` unexpected-keyword-arg, `E1124`
//! redundant-keyword-arg and `E1125` missing-kwoa; and `E1111`
//! assignment-from-no-return and `E1128` assignment-from-none for a function
//! of the file or of another module of the project.
//!
//! shortcut: Pylint infers what a call calls. Here a callee is a name bound
//! once to an undecorated function of this file, `self.method` of the
//! enclosing class, or a class of this file with an `__init__` (`W1114` and
//! `W1117`, and the arity rules, which also drop a class whose bases are not
//! all known), or an undecorated function another module of the project
//! defines once (under an `if` or `try` too) or imports once from one that
//! does, imported by a name bound once; a `self` is a `TestCase` when a base is named `...TestCase`
//! (`W1503`). `E1111` and `E1128` take the same callees but a constructor,
//! and not the builtin methods (`lst.sort()`). Upgrade with a call inference.

use std::cell::OnceCell;
use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Arguments, Expr, ExprCall, ParameterWithDefault, Stmt, StmtAssign, StmtClassDef,
    StmtFunctionDef, StmtRaise,
};

use super::abstract_methods::raises_not_implemented;
use super::classes::{Env, Local, bases, const_repr, is_classmethod, is_static, locals, py_repr};
use super::flow::own_returns;
use super::hierarchy::{Class, Hierarchy};
use super::misc::csv;
use super::project::{ClassRef, FunctionRef, Project};
use super::statements::is_generator;
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const FIELD: &str = "E3701";
const DUNDER_NAME: &str = "W1115";
const THREAD: &str = "W1506";
const UNITTEST: &str = "W1503";
const FORMAT_TUPLE: &str = "W0715";
const OUT_OF_ORDER: &str = "W1114";
const SUPERSEDED: &str = "W1117";
const MISSING: &str = "E1120";
const TOO_MANY: &str = "E1121";
const UNEXPECTED: &str = "E1123";
const REDUNDANT: &str = "E1124";
const MISSING_KWOA: &str = "E1125";
const NO_RETURN: &str = "E1111";
const NONE_RETURNED: &str = "E1128";
/// The `ctypes` types that astroid's ctypes brain redefines with `__init__(self, value)`.
const CTYPES_VALUE: [&str; 26] = [
    "c_bool",
    "c_byte",
    "c_char",
    "c_double",
    "c_float",
    "c_int",
    "c_int16",
    "c_int32",
    "c_int64",
    "c_int8",
    "c_long",
    "c_longdouble",
    "c_longlong",
    "c_short",
    "c_size_t",
    "c_ssize_t",
    "c_ubyte",
    "c_uint",
    "c_uint16",
    "c_uint32",
    "c_uint64",
    "c_uint8",
    "c_ulong",
    "c_ulonglong",
    "c_ushort",
    "c_wchar",
];
/// The `ctypes.wintypes` names that are plain aliases of those types (`DWORD = c_ulong`).
// shortcut: a fixed list of the aliases, not read from `wintypes.py`; upgrade with inference.
const WINTYPES_VALUE: [&str; 17] = [
    "BOOL",
    "BOOLEAN",
    "BYTE",
    "CHAR",
    "DOUBLE",
    "DWORD",
    "FLOAT",
    "INT",
    "LONG",
    "SHORT",
    "UINT",
    "ULONG",
    "USHORT",
    "WCHAR",
    "WORD",
    "LARGE_INTEGER",
    "ULARGE_INTEGER",
];
const ARITY: [&str; 5] = [MISSING, TOO_MANY, UNEXPECTED, REDUNDANT, MISSING_KWOA];

/// Whether `name` is a `ctypes` type (or a `wintypes` alias of one) built from a `value`.
fn ctypes_value(name: &str) -> bool {
    match name.strip_prefix("ctypes.") {
        Some(rest) => match rest.strip_prefix("wintypes.") {
            Some(alias) => WINTYPES_VALUE.contains(&alias),
            None => CTYPES_VALUE.contains(&rest),
        },
        None => false,
    }
}

pub struct Calls;

impl Checker for Calls {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                FIELD,
                "invalid-field-call",
                "`dataclasses.field()` outside the value of a dataclass field.",
            ),
            rule(
                DUNDER_NAME,
                "non-str-assignment-to-dunder-name",
                "A value that is not a string assigned to `__name__`.",
            ),
            rule(
                THREAD,
                "bad-thread-instantiation",
                "`threading.Thread` built without its `target`.",
            ),
            rule(
                UNITTEST,
                "redundant-unittest-assert",
                "`assertTrue` or `assertEqual` over constants.",
            ),
            rule(
                FORMAT_TUPLE,
                "raising-format-tuple",
                "An exception built from a format string and arguments, not formatted.",
            ),
            rule(
                OUT_OF_ORDER,
                "arguments-out-of-order",
                "Positional arguments named like the parameters, in another order.",
            ),
            rule(
                SUPERSEDED,
                "kwarg-superseded-by-positional-arg",
                "A keyword argument named like a positional-only parameter of a function with `**kwargs`.",
            ),
            rule(
                NO_RETURN,
                "assignment-from-no-return",
                "The result of a call to a function of the project that returns nothing, assigned.",
            ),
            rule(
                NONE_RETURNED,
                "assignment-from-none",
                "The result of a call to a function of the project that returns only `None`, assigned.",
            ),
            rule(
                MISSING,
                "no-value-for-parameter",
                "A call that leaves a parameter without a value.",
            ),
            rule(
                TOO_MANY,
                "too-many-function-args",
                "A call with more positional arguments than the function takes.",
            ),
            rule(
                UNEXPECTED,
                "unexpected-keyword-arg",
                "A call with a keyword that is no parameter of the function.",
            ),
            rule(
                REDUNDANT,
                "redundant-keyword-arg",
                "A call that gives a parameter a value by position and by keyword.",
            ),
            rule(
                MISSING_KWOA,
                "missing-kwoa",
                "A call that leaves a keyword-only parameter without a value.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            FIELD,
            DUNDER_NAME,
            THREAD,
            UNITTEST,
            FORMAT_TUPLE,
            OUT_OF_ORDER,
            SUPERSEDED,
            NO_RETURN,
            NONE_RETURNED,
        ]
        .into_iter()
        .chain(ARITY);
        if !ours.into_iter().any(|id| enabled.contains(id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            frames: Vec::new(),
            classes: Vec::new(),
            defs: Vec::new(),
            annotated: HashSet::new(),
            project: OnceCell::new(),
            hierarchy: OnceCell::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// What encloses the code under the walk, for `field()`.
enum Frame {
    /// A class body; `true` for a dataclass.
    Class(bool),
    /// The arguments of a call; `true` for `make_dataclass`.
    Call(bool),
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    frames: Vec<Frame>,
    /// The enclosing classes, with whether each derives from a `TestCase`.
    classes: Vec<(&'a StmtClassDef, bool)>,
    /// The enclosing functions.
    defs: Vec<&'a StmtFunctionDef>,
    /// The values of the annotated assignments met, by address.
    annotated: HashSet<usize>,
    /// The project around the file, read when a call builds a class of another module.
    project: OnceCell<Project>,
    /// The class hierarchy around the file, read when a method comes from a base of another module.
    hierarchy: OnceCell<Hierarchy<'a, 'e>>,
}

fn address(expr: &Expr) -> usize {
    std::ptr::from_ref(expr) as usize
}

fn name_of(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(n) => Some(n.id.as_str()),
        _ => None,
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    /// Whether `expr` names `target` (`dataclasses.field`) by an import.
    fn is(&self, expr: &Expr, target: &str) -> bool {
        self.env.qualified(expr).as_deref() == Some(target)
    }

    fn is_dataclass(class: &StmtClassDef) -> bool {
        class.decorator_list.iter().any(|decorator| {
            let expr = match &decorator.expression {
                Expr::Call(call) => &*call.func,
                other => other,
            };
            match expr {
                Expr::Name(n) => n.id.as_str() == "dataclass",
                Expr::Attribute(a) => a.attr.as_str() == "dataclass",
                _ => false,
            }
        })
    }

    fn call(&mut self, expr: &'a Expr, call: &'a ExprCall) {
        let ctx = self.env.ctx;
        if self.on(FIELD) && self.is(&call.func, "dataclasses.field") {
            let used = match self.frames.last() {
                Some(Frame::Call(make_dataclass)) => (!make_dataclass).then_some(
                    "it should be used within a dataclass or the make_dataclass() function.",
                ),
                Some(Frame::Class(true)) if self.annotated.contains(&address(expr)) => None,
                Some(Frame::Class(true)) => {
                    Some("it should be the value of an assignment within a dataclass.")
                }
                _ => Some("it should be used within a dataclass or the make_dataclass() function."),
            };
            if let Some(why) = used {
                self.out
                    .push(ctx.report(FIELD, call, format!("Invalid usage of field(), {why}")));
            }
        }
        if self.on(THREAD) && self.is(&call.func, "threading.Thread") {
            let target = call
                .arguments
                .keywords
                .iter()
                .any(|k| k.arg.as_ref().is_some_and(|a| a == "target"));
            if !target && call.arguments.args.len() < 2 {
                self.out.push(ctx.report(
                    THREAD,
                    call,
                    "threading.Thread needs the target function",
                ));
            }
        }
        // Pylint reads `c_int()` off the brain's constructor, which requires `value`.
        if self.on(MISSING)
            && call.arguments.args.is_empty()
            && call
                .arguments
                .keywords
                .iter()
                .all(|k| k.arg.as_ref().is_some_and(|a| a != "value"))
            && self
                .env
                .qualified(&call.func)
                .is_some_and(|name| ctypes_value(&name))
        {
            self.out.push(ctx.report(
                MISSING,
                call,
                "No value for argument 'value' in constructor call",
            ));
        }
        if self.on(MISSING) && self.object_new(call) {
            self.out.push(ctx.report(
                MISSING,
                call,
                "No value for argument 'cls' in classmethod call",
            ));
        }
        if self.on(UNITTEST) {
            self.unittest(call);
        }
        if self.on(OUT_OF_ORDER) || self.on(SUPERSEDED) || ARITY.iter().any(|id| self.on(id)) {
            self.arguments(call);
        }
        if self.on(MISSING) || self.on(TOO_MANY) {
            self.descriptor(call);
        }
        if self.on(OUT_OF_ORDER) || self.on(SUPERSEDED) || ARITY.iter().any(|id| self.on(id)) {
            self.imported(call);
        }
    }

    /// `f.__get__(a, b)` of a function of the file: astroid gives `__get__` the
    /// parameters of `f`, defaulted or not, and `type`.
    // shortcut: positional arguments only; upgrade with a keyword match.
    fn descriptor(&mut self, call: &ExprCall) {
        let (Expr::Attribute(method), args) = (&*call.func, &call.arguments) else {
            return;
        };
        let Expr::Name(name) = &*method.value else {
            return;
        };
        if method.attr.as_str() != "__get__"
            || !args.keywords.is_empty()
            || args.args.iter().any(|a| matches!(a, Expr::Starred(_)))
        {
            return;
        }
        let Some(func) = self.env.func_of(name) else {
            return;
        };
        let p = &func.parameters;
        let wanted: Vec<&str> = (p.posonlyargs.iter().chain(&p.args))
            .map(named)
            .chain(["type"])
            .collect();
        let ctx = self.env.ctx;
        let kind = if is_static(func) {
            "staticmethod"
        } else if is_classmethod(func) {
            "classmethod"
        } else {
            "function"
        };
        if args.args.len() > wanted.len() && self.on(TOO_MANY) {
            self.out.push(ctx.report(
                TOO_MANY,
                call,
                format!("Too many positional arguments for {kind} call"),
            ));
        }
        if self.on(MISSING) {
            for missing in wanted.iter().skip(args.args.len()) {
                self.out.push(ctx.report(
                    MISSING,
                    call,
                    format!("No value for argument {} in {kind} call", py_repr(missing)),
                ));
            }
        }
    }

    /// A class of another module of the project built by a call, which has its own
    /// `__init__` and no `__new__` along its bases.
    // shortcut: the project's own modules, not the installed packages; upgrade with their sources.
    fn imported(&mut self, call: &ExprCall) {
        if self.callee(call).is_some() {
            return;
        }
        let Some(qualified) = self.env.qualified(&call.func).filter(|q| q.contains('.')) else {
            return;
        };
        let project = self.project.get_or_init(|| {
            Project::new(self.env.ctx.path, self.env.ctx.module, self.env.ctx.roots)
        });
        if let Some(function) = self.imported_function(project, &call.func, &qualified) {
            if let Some(func) = function.func() {
                let callee = Callee {
                    func,
                    kind: "function",
                    class: None,
                };
                self.compare(call, &callee, true);
            }
            return;
        }
        if !ARITY.iter().any(|id| self.on(id)) {
            return;
        }
        let Some(class) = project.class(&project.absolute(&qualified)) else {
            return;
        };
        let Some(callee) = class
            .class()
            .and_then(constructor)
            .filter(|_| class.is_sole() && buildable(project, &class, 0))
        else {
            return;
        };
        let Some(count) = self.positional(&call.arguments) else {
            return;
        };
        self.arity(call, &callee, count);
    }

    /// The function of another module of the project that `qualified` names, when the
    /// file binds the imported name (`mod` of `mod.f`, or `f`) once, by an import.
    fn imported_function(
        &self,
        project: &Project,
        func: &Expr,
        qualified: &str,
    ) -> Option<FunctionRef> {
        imported_function(self.env, project, func, qualified)
    }

    /// The number of positional arguments of a call, a `*name` of a list or tuple
    /// literal counting as its elements; `None` when a `**` or another `*` hides them.
    fn positional(&self, args: &Arguments) -> Option<usize> {
        if args.keywords.iter().any(|k| k.arg.is_none()) {
            return None;
        }
        args.args.iter().try_fold(0, |count, arg| match arg {
            Expr::Starred(s) => self.env.literal_len(&s.value).map(|n| count + n),
            _ => Some(count + 1),
        })
    }

    /// `cls.method(...)` in a method of a metaclass: astroid infers `cls` as the class
    /// itself, so a method of the metaclass is read unbound, its first parameter included.
    // shortcut: a metaclass deriving from `type` or `abc.ABCMeta` through classes of the file.
    fn metaclass_call(&self, call: &ExprCall) -> Option<Callee<'a>> {
        let Expr::Attribute(attribute) = &*call.func else {
            return None;
        };
        let Expr::Name(receiver) = &*attribute.value else {
            return None;
        };
        let (class, _) = self.classes.last()?;
        let method = self.defs.last()?;
        let direct = super::classes::methods(&class.body)
            .into_iter()
            .any(|m| std::ptr::eq(m, *method));
        let func = own_method(class, attribute.attr.as_str())?;
        (direct
            && !is_static(method)
            && receiver.id.as_str() == named(method.parameters.args.first()?)
            && self
                .env
                .binding(receiver)
                .is_some_and(|b| b.kind == BindingKind::Parameter)
            && bases(class).iter().any(|base| {
                self.env
                    .base_is(base, &|name| matches!(name, "type" | "abc.ABCMeta"))
            }))
        .then_some(Callee {
            func,
            kind: if is_static(func) {
                "staticmethod"
            } else {
                "unbound method"
            },
            class: None,
        })
    }

    /// `self.__new__(X)` in a class: astroid reads the `__new__` of an instance as
    /// `(self, cls)` whatever the bases are, so the one argument leaves `cls` without a value.
    fn object_new(&self, call: &ExprCall) -> bool {
        let Expr::Attribute(method) = &*call.func else {
            return false;
        };
        let Expr::Name(receiver) = &*method.value else {
            return false;
        };
        let (positional, keywords) = (call.arguments.args.len(), call.arguments.keywords.len());
        method.attr.as_str() == "__new__"
            && (positional, keywords) == (1, 0)
            && !matches!(call.arguments.args[0], Expr::Starred(_))
            && receiver.id.as_str() == "self"
            && !self.classes.is_empty()
            && self
                .env
                .binding(receiver)
                .is_some_and(|b| b.kind == BindingKind::Parameter)
    }

    fn unittest(&mut self, call: &ExprCall) {
        let Expr::Attribute(method) = &*call.func else {
            return;
        };
        let Expr::Name(receiver) = &*method.value else {
            return;
        };
        let is_self = receiver.id.as_str() == "self"
            && self
                .env
                .binding(receiver)
                .is_some_and(|b| b.kind == BindingKind::Parameter);
        if !is_self || !self.classes.last().is_some_and(|(_, testcase)| *testcase) {
            return;
        }
        let args = &call.arguments.args;
        let shown = match (method.attr.as_str(), args.as_slice()) {
            ("assertTrue" | "assertFalse", [first, ..]) => const_repr(first, "Ellipsis"),
            ("assertEqual" | "assertNotEqual", [first, second, ..]) => {
                match (
                    const_repr(first, "Ellipsis"),
                    const_repr(second, "Ellipsis"),
                ) {
                    (Some(a), Some(b)) => Some(format!("({a}, {b})")),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(shown) = shown {
            self.out.push(self.env.ctx.report(
                UNITTEST,
                call,
                format!(
                    "Redundant use of {} with constant value {shown}",
                    method.attr
                ),
            ));
        }
    }

    /// The function a call calls, with how Pylint names the call.
    fn callee(&self, call: &ExprCall) -> Option<Callee<'a>> {
        (self.metaclass_call(call))
            .or_else(|| callee(self.env, &self.classes, call))
            .or_else(|| self.cls_constructor(call))
    }

    /// `cls(...)` in a classmethod, which builds the class that defines it.
    fn cls_constructor(&self, call: &ExprCall) -> Option<Callee<'a>> {
        let Expr::Name(name) = &*call.func else {
            return None;
        };
        let (class, _) = self.classes.last()?;
        let method = self.defs.last()?;
        let first = method.parameters.args.first()?;
        let direct = super::classes::methods(&class.body)
            .into_iter()
            .any(|m| std::ptr::eq(m, *method));
        (direct
            && is_classmethod(method)
            && name.id.as_str() == named(first)
            && self
                .env
                .binding(name)
                .is_some_and(|b| b.kind == BindingKind::Parameter))
        .then(|| constructor(class))
        .flatten()
    }

    /// Whether the arity rules can trust `callee`: Pylint reads the
    /// constructor off the whole class, and a method off the instance.
    fn settled(&self, call: &ExprCall, callee: &Callee<'a>) -> bool {
        let project = || {
            self.project.get_or_init(|| {
                Project::new(self.env.ctx.path, self.env.ctx.module, self.env.ctx.roots)
            })
        };
        settled(self.env, &self.classes, call, callee, Some(&project))
    }

    fn arguments(&mut self, call: &ExprCall) {
        let Some(callee) = self.callee(call) else {
            return;
        };
        let settled = ARITY.iter().any(|id| self.on(id)) && self.settled(call, &callee);
        self.compare(call, &callee, settled);
    }

    /// The arity and argument rules of a call to `callee`; `settled` when the arity rules may speak.
    fn compare(&mut self, call: &ExprCall, callee: &Callee<'_>, settled: bool) {
        let (func, in_class) = (callee.func, callee.kind != "function");
        let args = &call.arguments;
        let Some(count) = self.positional(args) else {
            return;
        };
        let p = &func.parameters;
        let mut params: Vec<&str> = p
            .posonlyargs
            .iter()
            .chain(&p.args)
            .map(|a| a.parameter.name.as_str())
            .collect();
        if params.iter().collect::<HashSet<_>>().len() != params.len() {
            return;
        }
        if settled {
            self.arity(call, callee, count);
        }
        if self.on(OUT_OF_ORDER) {
            if in_class && params.first() == Some(&"self") {
                params.remove(0);
            }
            let positional: Option<Vec<&str>> = args.args.iter().map(name_of).collect();
            let keyword: Option<Vec<&str>> =
                args.keywords.iter().map(|k| name_of(&k.value)).collect();
            if let (Some(positional), Some(keyword)) = (positional, keyword) {
                let given: HashSet<&str> = positional.iter().chain(&keyword).copied().collect();
                let wanted: HashSet<&str> = params.iter().copied().collect();
                if given == wanted && positional[..] != params[..positional.len().min(params.len())]
                {
                    self.out.push(self.env.ctx.report(
                        OUT_OF_ORDER,
                        call,
                        "Positional arguments appear to be out of order",
                    ));
                }
            }
        }
        if self.on(SUPERSEDED)
            && let Some(kwarg) = &p.kwarg
        {
            // Pylint keys the keywords by name, so a repeated one counts once.
            let mut seen = HashSet::new();
            for keyword in args
                .keywords
                .iter()
                .filter_map(|k| k.arg.as_ref())
                .filter(|k| seen.insert(k.as_str()))
            {
                if p.posonlyargs
                    .iter()
                    .any(|a| a.parameter.name.as_str() == keyword.as_str())
                {
                    self.out.push(self.env.ctx.report(
                        SUPERSEDED,
                        call,
                        format!(
                            "{} will be included in {} since a positional-only parameter with this name already exists",
                            py_repr(keyword.as_str()),
                            py_repr(&format!("**{}", kwarg.name))
                        ),
                    ));
                }
            }
        }
    }

    /// Pylint's match of the arguments of a call against the parameters.
    fn arity(&mut self, call: &ExprCall, callee: &Callee<'_>, count: usize) {
        let (p, args) = (&callee.func.parameters, &call.arguments);
        // The bound `self` of a method or constructor is an argument too.
        let positional = count + usize::from(matches!(callee.kind, "method" | "constructor"));
        let params: Vec<_> = p.posonlyargs.iter().chain(&p.args).collect();
        let mut given = vec![false; params.len()];
        let mut kw_given = vec![false; p.kwonlyargs.len()];
        let at = format!("in {} call", callee.kind);
        let ctx = self.env.ctx;
        let on = |id: &str| self.enabled.contains(id);
        let mut found = Vec::new();
        let mut say = |id: &'static str, text: String| {
            if on(id) {
                found.push(ctx.report(id, call, text));
            }
        };
        if positional > params.len() && p.vararg.is_none() {
            say(
                TOO_MANY,
                format!("Too many positional arguments for {} call", callee.kind),
            );
        }
        given.iter_mut().take(positional).for_each(|g| *g = true);
        for keyword in args.keywords.iter().filter_map(|k| k.arg.as_ref()) {
            let name = keyword.as_str();
            let shown = py_repr(name);
            if p.kwarg.is_some() && p.posonlyargs.iter().any(|a| named(a) == name) {
                continue; // W1117's case
            }
            let slot = if let Some(i) = params.iter().position(|a| named(a) == name) {
                &mut given[i]
            } else if let Some(i) = p.kwonlyargs.iter().position(|a| named(a) == name) {
                &mut kw_given[i]
            } else {
                // Pylint lets a decorator of a function or constructor take the keyword.
                // shortcut: any decorator, not the signature of what it returns; upgrade with inference.
                let decorators = callee.func.decorator_list.len();
                let wrapped =
                    callee.kind != "method" && decorators > usize::from(is_static(callee.func));
                if p.kwarg.is_none() && !wrapped {
                    say(
                        UNEXPECTED,
                        format!("Unexpected keyword argument {shown} {at}"),
                    );
                }
                continue;
            };
            if *slot {
                say(
                    REDUNDANT,
                    format!("Argument {shown} passed by position and keyword {at}"),
                );
            }
            *slot = true;
        }
        for a in params
            .iter()
            .zip(&given)
            .filter_map(|(a, g)| (a.default.is_none() && !g).then_some(a))
        {
            say(
                MISSING,
                format!("No value for argument {} {at}", py_repr(named(a))),
            );
        }
        for a in p
            .kwonlyargs
            .iter()
            .zip(&kw_given)
            .filter_map(|(a, g)| (a.default.is_none() && !g).then_some(a))
        {
            say(
                MISSING_KWOA,
                format!(
                    "Missing mandatory keyword argument {} {at}",
                    py_repr(named(a))
                ),
            );
        }
        self.out.extend(found);
    }

    /// `x = f()` where `f` returns nothing (`E1111`) or only `None` (`E1128`).
    fn assignment(&mut self, assign: &StmtAssign) {
        let Expr::Call(call) = &*assign.value else {
            return;
        };
        // Pylint skips a callee of a module that is not fully defined, as a stub is.
        if self.env.ctx.is_stub() {
            return;
        }
        let found = match function_called(self.env, &self.classes, call) {
            Some(func) => self.judge(assign, call, func),
            None => self
                .inherited(assign, call)
                .or_else(|| self.judge_imported(assign, call)),
        };
        self.out.extend(found);
    }

    /// `x = f()` where `f` is a function of another module of the project, imported by a name.
    fn judge_imported(&self, assign: &StmtAssign, call: &ExprCall) -> Option<Diagnostic> {
        if self.callee(call).is_some() {
            return None;
        }
        let qualified = self.env.qualified(&call.func).filter(|q| q.contains('.'))?;
        let project = self.project.get_or_init(|| {
            Project::new(self.env.ctx.path, self.env.ctx.module, self.env.ctx.roots)
        });
        let function = self.imported_function(project, &call.func, &qualified)?;
        self.judge(assign, call, function.func()?)
    }

    /// `x = self.f()` or `x = super().f()` where an ancestor of another
    /// module defines `f`, the first of the MRO after the class to bind it.
    ///
    /// shortcut: only a class whose MRO the project reads in full, with no
    /// builtin base; upgrade with a call inference.
    fn inherited(&self, assign: &StmtAssign, call: &ExprCall) -> Option<Diagnostic> {
        let Expr::Attribute(attribute) = &*call.func else {
            return None;
        };
        let (class, _) = *self.classes.last()?;
        let method = *self.defs.last()?;
        let first = named(method.parameters.args.first()?);
        let direct = super::classes::methods(&class.body)
            .into_iter()
            .any(|m| std::ptr::eq(m, method));
        let name = attribute.attr.as_str();
        let on_self = match &*attribute.value {
            Expr::Name(receiver) => {
                let is_self = receiver.id.as_str() == first
                    && self
                        .env
                        .binding(receiver)
                        .is_some_and(|b| b.kind == BindingKind::Parameter);
                if !is_self {
                    return None;
                }
                true
            }
            Expr::Call(sup) => {
                let builtin = matches!(&*sup.func, Expr::Name(n) if n.id.as_str() == "super"
                    && self.env.binding(n).is_some_and(|b| b.kind == BindingKind::Builtin));
                if !(builtin && sup.arguments.is_empty()) {
                    return None;
                }
                false
            }
            _ => return None,
        };
        if !direct || is_static(method) || is_classmethod(method) {
            return None;
        }
        if locals(&class.body).iter().any(|(n, _)| n == name) {
            return None;
        }
        let hierarchy = self.hierarchy.get_or_init(|| Hierarchy::new(self.env));
        let mro = hierarchy.mro(&Class::Local(class))?;
        if mro.iter().any(|c| hierarchy.has_builtin_base(c)) {
            return None;
        }
        // An attribute stored as `self.f = ...` hides the method.
        if on_self {
            for ancestor in &mro {
                let mut stores = Stores(name, false);
                stores.visit_body(&ancestor.def()?.body);
                if stores.1 {
                    return None;
                }
            }
        }
        let owner = mro.iter().skip(1).find(|c| {
            c.def()
                .is_some_and(|def| locals(&def.body).iter().any(|(n, _)| n == name))
        })?;
        if matches!(owner, Class::Remote(remote) if remote.is_stub()) {
            return None;
        }
        self.judge(assign, call, own_method(owner.def()?, name)?)
    }

    /// The diagnostic for `assign`, a call of `func` whose result is assigned.
    fn judge(
        &self,
        assign: &StmtAssign,
        call: &ExprCall,
        func: &StmtFunctionDef,
    ) -> Option<Diagnostic> {
        if !func.decorator_list.is_empty() || func.is_async {
            return None;
        }
        let returns = own_returns(&func.body);
        // astroid keeps the docstring out of the body
        let docstring =
            matches!(func.body.first(), Some(Stmt::Expr(e)) if e.value.is_string_literal_expr());
        let body = &func.body[usize::from(docstring)..];
        let raises = |stmt: Option<&Stmt>| matches!(stmt, Some(Stmt::Raise(_)));
        if (body.len() == 1 && raises(body.first()))
            || (raises(body.last()) && returns.is_empty())
            || is_generator(func)
            || raises_not_implemented(func)
        {
            return None;
        }
        let ctx = self.env.ctx;
        if returns.is_empty() {
            if self.on(NO_RETURN) {
                let name = match &*call.func {
                    Expr::Attribute(attribute) => attribute.attr.as_str(),
                    other => name_of(other).unwrap_or_default(),
                };
                let hint = csv(
                    ctx,
                    "known-side-effects-only-functions",
                    "reverse:reversed,sort:sorted",
                )
                .iter()
                .filter_map(|pair| pair.split_once(':'))
                .find(|(function, _)| *function == name)
                .map_or_else(String::new, |(_, suggestion)| {
                    format!(", did you mean to use '{suggestion}(...)' instead?")
                });
                return Some(ctx.report(
                    NO_RETURN,
                    assign,
                    format!("Assigning result of a function call, but '{name}' returns None{hint}"),
                ));
            }
        } else if self.on(NONE_RETURNED)
            && returns.iter().all(|r| {
                r.value
                    .as_deref()
                    .is_none_or(|v| matches!(v, Expr::NoneLiteral(_)))
            })
        {
            return Some(ctx.report(
                NONE_RETURNED,
                assign,
                "Assigning result of a function call, where the function returns None",
            ));
        }
        None
    }

    fn dunder_name(&mut self, assign: &StmtAssign) {
        let Some(Expr::Attribute(target)) = assign.targets.first() else {
            return;
        };
        let known_not_str = matches!(
            &*assign.value,
            Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::Lambda(_)
                | Expr::List(_)
                | Expr::Tuple(_)
                | Expr::Set(_)
                | Expr::Dict(_)
        );
        if target.attr.as_str() == "__name__" && known_not_str {
            self.out.push(self.env.ctx.report(
                DUNDER_NAME,
                assign,
                "Non-string value assigned to __name__",
            ));
        }
    }

    fn raise(&mut self, raise: &StmtRaise) {
        if let Some(Expr::Call(call)) = raise.exc.as_deref()
            && let [Expr::StringLiteral(message), _, ..] = &call.arguments.args[..]
        {
            let text = message.value.to_str();
            if text.contains('%') || (text.contains('{') && text.contains('}')) {
                self.out.push(self.env.ctx.report(
                    FORMAT_TUPLE,
                    raise,
                    "Exception arguments suggest string formatting might be intended",
                ));
            }
        }
    }
}

/// The function a call calls, with how Pylint names the call.
pub(super) fn callee<'a>(
    env: &Env<'a>,
    classes: &[(&'a StmtClassDef, bool)],
    call: &ExprCall,
) -> Option<Callee<'a>> {
    match &*call.func {
        Expr::Name(name) => {
            if let Some(func) = env.func_of(name) {
                return (!rebinds(func)).then_some(Callee {
                    func,
                    kind: "function",
                    class: None,
                });
            }
            constructor(env.class_of(&call.func)?)
        }
        Expr::Attribute(attribute) => {
            let Expr::Name(receiver) = &*attribute.value else {
                return None;
            };
            let is_self = receiver.id.as_str() == "self"
                && env
                    .binding(receiver)
                    .is_some_and(|b| b.kind == BindingKind::Parameter);
            let (class, _) = classes.last()?;
            let func = own_method(class, attribute.attr.as_str()).filter(|_| is_self)?;
            Some(Callee {
                func,
                kind: if is_static(func) {
                    "staticmethod"
                } else {
                    "method"
                },
                class: None,
            })
        }
        _ => None,
    }
}

/// The `__init__` that building `class` calls.
fn constructor(class: &StmtClassDef) -> Option<Callee<'_>> {
    // A class decorator does not change the class Pylint infers, only a metaclass does.
    let plain = class
        .arguments
        .as_ref()
        .is_none_or(|a| a.keywords.is_empty());
    let init = own_method(class, "__init__")?;
    (plain && own_method(class, "__new__").is_none()).then_some(Callee {
        func: init,
        kind: "constructor",
        class: Some(class),
    })
}

/// Whether the arity rules can trust `callee`: Pylint reads the
/// constructor off the whole class, and a method off the instance.
fn settled<'a>(
    env: &Env<'a>,
    classes: &[(&'a StmtClassDef, bool)],
    call: &ExprCall,
    callee: &Callee<'a>,
    project: Option<&dyn Fn() -> &'a Project>,
) -> bool {
    let name = callee.func.name.as_str();
    match (&*call.func, callee.class) {
        (_, Some(class)) => constructible(env, class, 0, project),
        // A function of a class body is no name inside its methods.
        (Expr::Name(_), None) => !classes.iter().any(|(class, _)| {
            super::classes::methods(&class.body)
                .into_iter()
                .any(|m| std::ptr::eq(m, callee.func))
        }),
        // Pylint does not judge `self.__init__`, and an attribute
        // assigned to `self.name` hides the method.
        _ => {
            name != "__init__"
                && classes.last().is_some_and(|(class, _)| {
                    let mut stores = Stores(name, false);
                    stores.visit_body(&class.body);
                    !stores.1
                })
        }
    }
}

/// Whether the constructor of the project's `class` is its `__init__`: no `__new__`
/// along its bases, which are all `object`, a builtin or such a class.
fn buildable(project: &Project, class: &ClassRef, depth: u8) -> bool {
    let Some(def) = class.class() else {
        return false;
    };
    let found = class.bases(project);
    depth < 16
        && !locals(&def.body).iter().any(|(n, _)| n == "__new__")
        && found.len() == bases(def).len()
        && found.iter().all(|base| match base {
            Ok(base) => buildable(project, base, depth + 1),
            Err(name) => !name.contains('.'),
        })
}

/// Whether `base` names a class of another module of the project that `buildable` accepts.
fn imported_base(env: &Env<'_>, base: &Expr, project: &Project) -> bool {
    env.qualified(base)
        .filter(|q| q.contains('.'))
        .and_then(|q| project.class(&project.absolute(&q)))
        .is_some_and(|class| class.is_sole() && buildable(project, &class, 0))
}

/// Whether the constructor of `class` is its `__init__`: no `__new__`
/// and no base but a builtin class or such a class of the file.
fn constructible<'p>(
    env: &Env<'_>,
    class: &StmtClassDef,
    depth: u8,
    project: Option<&dyn Fn() -> &'p Project>,
) -> bool {
    depth < 16
        && !locals(&class.body).iter().any(|(n, _)| n == "__new__")
        && super::classes::bases(class).iter().all(|base| {
            env.class_of(base).map_or_else(
                || {
                    matches!(base, Expr::Name(n)
                        if env.binding(n).is_some_and(|b| b.kind == BindingKind::Builtin))
                        || project.is_some_and(|project| imported_base(env, base, project()))
                },
                |class| constructible(env, class, depth + 1, project),
            )
        })
}

/// The function of another module of the project that `qualified` names, when the file
/// binds the imported name (`mod` of `mod.f`, or `f`) once, by an import.
pub(super) fn imported_function(
    env: &Env<'_>,
    project: &Project,
    func: &Expr,
    qualified: &str,
) -> Option<FunctionRef> {
    let mut root = func;
    while let Expr::Attribute(attribute) = root {
        root = &attribute.value;
    }
    let Expr::Name(name) = root else {
        return None;
    };
    let binding = env.binding(name)?;
    (matches!(binding.kind, BindingKind::Import | BindingKind::FromImport)
        && env.ctx.model.is_sole_binding(binding.id))
    .then(|| project.function(&project.absolute(qualified)))
    .flatten()
}

/// The function or method of the file that a call calls, which Pylint
/// resolves too: no constructor, and no name a class body or a `self.name = ...`
/// would change.
pub(super) fn function_called<'a>(
    env: &Env<'a>,
    classes: &[(&'a StmtClassDef, bool)],
    call: &ExprCall,
) -> Option<&'a StmtFunctionDef> {
    let callee = callee(env, classes, call).filter(|callee| callee.class.is_none())?;
    // The result of a decorated function is not the one of its body (static: as before).
    (settled(env, classes, call, &callee, None)
        && (callee.func.decorator_list.is_empty() || callee.kind == "staticmethod"))
        .then_some(callee.func)
}

/// Whether a decorator makes Pylint read the function as a classmethod or a
/// property, not by its signature. Any other decorator leaves the signature as written.
// shortcut: a property by the name of its decorator (`...property`, `setter`), not inferred.
fn rebinds(func: &StmtFunctionDef) -> bool {
    func.decorator_list.iter().any(|d| {
        let expr = match &d.expression {
            Expr::Call(call) => &*call.func,
            other => other,
        };
        let name = match expr {
            Expr::Name(n) => n.id.as_str(),
            Expr::Attribute(a) => a.attr.as_str(),
            _ => return false,
        };
        name.ends_with("property")
            || matches!(name, "classmethod" | "setter" | "getter" | "deleter")
    })
}

fn named(a: &ParameterWithDefault) -> &str {
    a.parameter.name.as_str()
}

/// A function a call calls: `kind` is how Pylint names the call, and
/// `class` the class a constructor call builds.
pub(super) struct Callee<'a> {
    pub(super) func: &'a StmtFunctionDef,
    kind: &'static str,
    pub(super) class: Option<&'a StmtClassDef>,
}

/// Whether a class body assigns an attribute called `.0`.
pub(super) struct Stores<'n>(pub(super) &'n str, pub(super) bool);

impl Visitor<'_> for Stores<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Attribute(a) = expr
            && a.attr.as_str() == self.0
            && a.ctx.is_store()
        {
            self.1 = true;
        }
        visitor::walk_expr(self, expr);
    }
}

/// The method `name` the class binds once in its body, read by its signature.
pub(super) fn own_method<'a>(class: &'a StmtClassDef, name: &str) -> Option<&'a StmtFunctionDef> {
    let bound: Vec<Local> = locals(&class.body)
        .into_iter()
        .filter(|(n, _)| n == name)
        .map(|(_, k)| k)
        .collect();
    if bound != [Local::Function] {
        return None;
    }
    super::classes::methods(&class.body)
        .into_iter()
        .find(|f| f.name.as_str() == name)
        .filter(|f| !rebinds(f))
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
                let testcase = class
                    .arguments
                    .iter()
                    .flat_map(|a| &a.args)
                    .any(|base| self.env.base_is(base, &|name| name.ends_with("TestCase")));
                self.frames.push(Frame::Class(Self::is_dataclass(class)));
                self.classes.push((class, testcase));
                self.visit_body(&class.body);
                self.classes.pop();
                self.frames.pop();
            }
            Stmt::FunctionDef(func) => {
                self.defs.push(func);
                visitor::walk_stmt(self, stmt);
                self.defs.pop();
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    self.annotated.insert(address(value));
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Assign(assign) => {
                if self.on(DUNDER_NAME) {
                    self.dunder_name(assign);
                }
                if self.on(NO_RETURN) || self.on(NONE_RETURNED) {
                    self.assignment(assign);
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Raise(raise) => {
                if self.on(FORMAT_TUPLE) {
                    self.raise(raise);
                }
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            self.call(expr, call);
            let make_dataclass = matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "make_dataclass")
                && self.is(&call.func, "dataclasses.make_dataclass");
            self.frames.push(Frame::Call(make_dataclass));
            visitor::walk_expr(self, expr);
            self.frames.pop();
        } else {
            visitor::walk_expr(self, expr);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::classes::{render, render_at};

    #[test]
    fn single_call_and_assignment_rules() {
        let source = include_str!("fixtures/single_call_and_assignment_rules.py");
        let expected =
            include_str!("fixtures/single_call_and_assignment_rules.expected").trim_end();
        assert_eq!(
            render(source, &[], &["W1115", "W1506", "W1503", "W0715", "E3701"]),
            expected
        );
    }

    #[test]
    fn call_arguments() {
        let source = include_str!("fixtures/call_arguments.py");
        let expected = include_str!("fixtures/call_arguments.expected").trim_end();
        assert_eq!(render(source, &[], &["W1114", "W1117"]), expected);
    }

    #[test]
    fn assignment_from_no_return() {
        let source = include_str!("fixtures/assignment_from_no_return.py");
        let expected = include_str!("fixtures/assignment_from_no_return.expected").trim_end();
        assert_eq!(render(source, &[], &["E1111", "E1128"]), expected);
    }

    #[test]
    fn a_stub_is_not_fully_defined() {
        let source = "def f():\n    return None\n\n\nx = f()\n";
        let at = |name| render_at(source, Path::new(name), "mod", &[], &["E1128"]);
        assert_eq!(
            at("mod.py"),
            "5:E1128:Assigning result of a function call, where the function returns None"
        );
        assert_eq!(at("mod.pyi"), "");
    }

    #[test]
    fn side_effects_hints_follow_the_option() {
        let source = "def put():\n    pass\n\n\ndef sort():\n    pass\n\n\nx = put()\ny = sort()\n";
        let options = [("known-side-effects-only-functions", "put:putting")];
        assert_eq!(
            render(source, &options, &["E1111"]),
            "9:E1111:Assigning result of a function call, but 'put' returns None, did you mean to use 'putting(...)' instead?\n\
             10:E1111:Assigning result of a function call, but 'sort' returns None"
        );
    }

    #[test]
    fn a_method_of_a_base_in_another_module() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("__init__.py"), "").unwrap();
        std::fs::write(
            package.join("base.py"),
            "class Base:\n    def put(self):\n        \"\"\"Put.\"\"\"\n        ...\n\n    def get(self):\n        return 1\n\n    def set(self):\n        return None\n",
        )
        .unwrap();
        let source = "from pkg.base import Base\n\n\nclass Own(Base):\n    def run(self):\n        a = self.put()\n        b = super().put()\n        c = self.get()\n        d = self.set()\n        return a, b, c, d\n\n\nclass Dict(dict, Base):\n    def run(self):\n        return self.put()\n\n\nclass Hidden(Base):\n    def __init__(self):\n        self.put = len\n\n    def run(self):\n        x = self.put()\n        return x\n";
        let ids = ["E1111", "E1128"];
        let found = render_at(source, &package.join("use.py"), "pkg.use", &[], &ids);
        assert_eq!(
            found,
            "6:E1111:Assigning result of a function call, but 'put' returns None\n7:E1111:Assigning result of a function call, but 'put' returns None\n9:E1128:Assigning result of a function call, where the function returns None",
            "{found}"
        );
    }

    #[test]
    fn a_function_of_another_module_that_returns_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("__init__.py"), "").unwrap();
        std::fs::write(
            package.join("funcs.py"),
            "def check(a):\n    if not a:\n        raise ValueError\n\n\ndef quiet(a):\n    if not a:\n        return\n    print(a)\n\n\ndef value(a):\n    return a\n",
        )
        .unwrap();
        let source = "from pkg import funcs\nfrom pkg.funcs import check, quiet, value\n\na = check(1)\nb = quiet(1)\nc = value(1)\nd = funcs.check(1)\n";
        let found = render_at(
            source,
            &package.join("use.py"),
            "pkg.use",
            &[],
            &["E1111", "E1128"],
        );
        assert_eq!(
            found,
            "4:E1111:Assigning result of a function call, but 'check' returns None\n5:E1128:Assigning result of a function call, where the function returns None\n7:E1111:Assigning result of a function call, but 'check' returns None",
            "{found}"
        );
    }

    #[test]
    fn call_arity() {
        let source = include_str!("fixtures/call_arity.py");
        let expected = include_str!("fixtures/call_arity.expected").trim_end();
        let ids = ["E1120", "E1121", "E1123", "E1124", "E1125"];
        assert_eq!(render(source, &[], &ids), expected);
    }

    #[test]
    fn constructors_of_other_modules() {
        use super::super::classes::render_at;

        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("__init__.py"), "").unwrap();
        std::fs::write(
            package.join("base.py"),
            "class Base:\n    def __init__(self, a, b):\n        pass\n\n\nclass Own(Base):\n    def __init__(self, a):\n        pass\n\n\nclass Newer:\n    def __new__(cls, *args):\n        return object.__new__(cls)\n\n    def __init__(self, a):\n        pass\n",
        )
        .unwrap();
        let source = "from pkg import base\nfrom pkg.base import Base, Newer, Own\n\nBase(1)\nNewer()\nbase.Own()\nlst = []\nOwn(*lst)\nOwn(1)\n\n\nclass Child(Base):\n    def __init__(self, c):\n        pass\n\n\nclass Odd(Newer):\n    def __init__(self, c):\n        pass\n\n\nChild()\nOdd()\n";
        let ids = ["E1120", "E1121"];
        let found = render_at(source, &package.join("use.py"), "pkg.use", &[], &ids);
        assert_eq!(
            found,
            "4:E1120:No value for argument 'b' in constructor call\n6:E1120:No value for argument 'a' in constructor call\n8:E1120:No value for argument 'a' in constructor call\n22:E1120:No value for argument 'c' in constructor call",
            "{found}"
        );
    }

    #[test]
    fn functions_of_other_modules() {
        use super::super::classes::render_at;

        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(
            package.join("__init__.py"),
            "from .funcs import pair as relayed\n",
        )
        .unwrap();
        std::fs::write(
            package.join("funcs.py"),
            "import functools\n\n\ndef helper(x):\n    return x\n\n\ndef pair(a, b):\n    return a\n\n\n@functools.cache\ndef wrapped(x):\n    return x\n\n\ndef twice(x):\n    return x\n\n\ndef twice(x, y):\n    return x\n\n\nif functools:\n    def cond(x):\n        return x\n\n\ntry:\n    def guarded(x):\n        return x\nexcept ImportError:\n    pass\n",
        )
        .unwrap();
        let source = "import pkg.funcs\nimport pkg.funcs as alias\nfrom pkg import funcs, relayed\nfrom pkg.funcs import cond, guarded, helper, pair, twice, wrapped\nfrom pkg.funcs import helper as h2\n\nfuncs.helper(1, 2)\nalias.helper()\nhelper(1, 2)\nh2(y=1)\npair(b, a)\nwrapped(1, 2)\ntwice(1)\ncond(1, 2)\nfuncs.helper(1)\nrelayed(1)\nguarded()\n\n\ndef rebound(helper):\n    return helper(1, 2)\n";
        let ids = ["E1120", "E1121", "E1123", "W1114"];
        let found = render_at(source, &package.join("use.py"), "pkg.use", &[], &ids);
        assert_eq!(
            found,
            "7:E1121:Too many positional arguments for function call\n8:E1120:No value for argument 'x' in function call\n9:E1121:Too many positional arguments for function call\n10:E1120:No value for argument 'x' in function call\n10:E1123:Unexpected keyword argument 'y' in function call\n11:W1114:Positional arguments appear to be out of order\n14:E1121:Too many positional arguments for function call\n16:E1120:No value for argument 'b' in function call\n17:E1120:No value for argument 'x' in function call",
            "{found}"
        );
    }
}
