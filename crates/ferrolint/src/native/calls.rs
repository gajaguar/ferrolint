//! Rules about one call or assignment: `E3701` invalid-field-call, `W1115`
//! non-str-assignment-to-dunder-name, `W1506` bad-thread-instantiation,
//! `W1503` redundant-unittest-assert, `W0715` raising-format-tuple, `W1114`
//! arguments-out-of-order and `W1117` kwarg-superseded-by-positional-arg; and
//! the arity of the same callees: `E1120` no-value-for-parameter, `E1121`
//! too-many-function-args, `E1123` unexpected-keyword-arg, `E1124`
//! redundant-keyword-arg and `E1125` missing-kwoa; and `E1111`
//! assignment-from-no-return and `E1128` assignment-from-none for a function
//! of the file.
//!
//! shortcut: Pylint infers what a call calls. Here a callee is a name bound
//! once to an undecorated function of this file, `self.method` of the
//! enclosing class, or a class of this file with an `__init__` (`W1114` and
//! `W1117`, and the arity rules, which also drop a class whose bases are not
//! all known); a `self` is a `TestCase` when a base is named `...TestCase`
//! (`W1503`). `E1111` and `E1128` take the same callees but a constructor,
//! and not the builtin methods (`lst.sort()`). Upgrade with a call inference.

use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Expr, ExprCall, ParameterWithDefault, Stmt, StmtAssign, StmtClassDef, StmtFunctionDef,
    StmtRaise,
};

use super::abstract_methods::raises_not_implemented;
use super::classes::{Env, Local, const_repr, is_static, locals, py_repr};
use super::flow::own_returns;
use super::misc::csv;
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
                "The result of a call to a function of the file that returns nothing, assigned.",
            ),
            rule(
                NONE_RETURNED,
                "assignment-from-none",
                "The result of a call to a function of the file that returns only `None`, assigned.",
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
            annotated: HashSet::new(),
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
    /// The values of the annotated assignments met, by address.
    annotated: HashSet<usize>,
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
        if self.on(UNITTEST) {
            self.unittest(call);
        }
        if self.on(OUT_OF_ORDER) || self.on(SUPERSEDED) || ARITY.iter().any(|id| self.on(id)) {
            self.arguments(call);
        }
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
        callee(self.env, &self.classes, call)
    }

    /// Whether the arity rules can trust `callee`: Pylint reads the
    /// constructor off the whole class, and a method off the instance.
    fn settled(&self, call: &ExprCall, callee: &Callee<'a>) -> bool {
        settled(self.env, &self.classes, call, callee)
    }

    fn arguments(&mut self, call: &ExprCall) {
        let Some(callee) = self.callee(call) else {
            return;
        };
        let (func, in_class) = (callee.func, callee.kind != "function");
        let args = &call.arguments;
        if args.args.iter().any(|a| matches!(a, Expr::Starred(_)))
            || args.keywords.iter().any(|k| k.arg.is_none())
        {
            return;
        }
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
        if ARITY.iter().any(|id| self.on(id)) && self.settled(call, &callee) {
            self.arity(call, &callee);
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
    fn arity(&mut self, call: &ExprCall, callee: &Callee<'_>) {
        let (p, args) = (&callee.func.parameters, &call.arguments);
        // The bound `self` of a method or constructor is an argument too.
        let positional =
            args.args.len() + usize::from(matches!(callee.kind, "method" | "constructor"));
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
                if p.kwarg.is_none() {
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
        let Some(func) = function_called(self.env, &self.classes, call)
            .filter(|func| func.decorator_list.is_empty() && !func.is_async)
        else {
            return;
        };
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
            return;
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
                self.out.push(ctx.report(
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
            self.out.push(ctx.report(
                NONE_RETURNED,
                assign,
                "Assigning result of a function call, where the function returns None",
            ));
        }
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
                return func.decorator_list.is_empty().then_some(Callee {
                    func,
                    kind: "function",
                    class: None,
                });
            }
            let class = env.class_of(&call.func)?;
            let plain = class.decorator_list.is_empty()
                && class
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

/// Whether the arity rules can trust `callee`: Pylint reads the
/// constructor off the whole class, and a method off the instance.
fn settled<'a>(
    env: &Env<'a>,
    classes: &[(&'a StmtClassDef, bool)],
    call: &ExprCall,
    callee: &Callee<'a>,
) -> bool {
    let name = callee.func.name.as_str();
    match (&*call.func, callee.class) {
        (_, Some(class)) => constructible(env, class, 0),
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

/// Whether the constructor of `class` is its `__init__`: no `__new__`
/// and no base but a builtin class or such a class of the file.
fn constructible(env: &Env<'_>, class: &StmtClassDef, depth: u8) -> bool {
    depth < 16
        && !locals(&class.body).iter().any(|(n, _)| n == "__new__")
        && super::classes::bases(class).iter().all(|base| {
            env.class_of(base).map_or_else(
                || {
                    matches!(base, Expr::Name(n)
                        if env.binding(n).is_some_and(|b| b.kind == BindingKind::Builtin))
                },
                |class| constructible(env, class, depth + 1),
            )
        })
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
    settled(env, classes, call, &callee).then_some(callee.func)
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
struct Stores<'n>(&'n str, bool);

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

/// The method `name` the class binds once in its body, undecorated or static.
fn own_method<'a>(class: &'a StmtClassDef, name: &str) -> Option<&'a StmtFunctionDef> {
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
        .filter(|f| f.decorator_list.is_empty() || is_static(f))
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
    use super::super::classes::render;

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
    fn call_arity() {
        let source = include_str!("fixtures/call_arity.py");
        let expected = include_str!("fixtures/call_arity.expected").trim_end();
        let ids = ["E1120", "E1121", "E1123", "E1124", "E1125"];
        assert_eq!(render(source, &[], &ids), expected);
    }
}
