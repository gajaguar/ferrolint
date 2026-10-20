//! `E0239` inherit-non-class, `E0243` invalid-class-object, `W0143`
//! comparison-with-callable and `W1116` isinstance-second-argument-not-valid-type: the rules that ask what an expression stands
//! for, answered for the few expressions whose value needs no type
//! inference: a literal, a function, a class, a module, a call of a class
//! and a name bound once to one of those.
//!
//! shortcut: any other expression (a call of a function, a name bound twice,
//! a name from an import, an attribute of an object) has no answer and the
//! rules stay silent on it; Pylint infers those. Upgrade with inference.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    CmpOp, Expr, ExprAttribute, ExprName, FStringPartRef, HasNodeIndex, Operator, StmtClassDef,
    StmtFunctionDef, UnaryOp,
};
use ruff_python_ast::{Stmt, StmtAnnAssign, StmtAssign};
use ruff_text_size::Ranged;

use super::astring::as_string;
use super::classes::{Env, Local, bases, is_classmethod, is_static, locals, methods, py_repr};
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const INHERIT: &str = "E0239";
const CLASS_OBJECT: &str = "E0243";
const CALLABLE: &str = "W0143";
const ISINSTANCE: &str = "W1116";

/// Builtin classes Python refuses to subclass.
const FINAL_BUILTINS: [&str; 4] = ["bool", "range", "slice", "memoryview"];

/// The builtins that are functions, which astroid builds as `FunctionDef`.
const BUILTIN_FUNCTIONS: [&str; 44] = [
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "breakpoint",
    "callable",
    "chr",
    "compile",
    "delattr",
    "dir",
    "divmod",
    "eval",
    "exec",
    "format",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "hex",
    "id",
    "input",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "locals",
    "max",
    "min",
    "next",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "repr",
    "round",
    "setattr",
    "sorted",
    "sum",
    "vars",
    "__import__",
];

/// How deep a name is followed through the values it was bound to.
const DEPTH: u8 = 8;

/// What an expression infers to, by the astroid node it would be.
#[derive(Clone, Copy, PartialEq)]
enum Value {
    Const,
    List,
    Tuple,
    Dict,
    Set,
    Lambda,
    /// A `FunctionDef`; `counted` is whether W0143 takes it for a bare callable.
    Func {
        is_async: bool,
        counted: bool,
    },
    BoundMethod {
        counted: bool,
    },
    UnboundMethod,
    Module,
    Instance,
    Class,
    /// A parameter: Pylint infers none without a call, so it is no function.
    Param,
}

impl Value {
    /// The astroid class name E0243 prints.
    fn node(self) -> Option<&'static str> {
        Some(match self {
            Value::Const => "Const",
            Value::List => "List",
            Value::Tuple => "Tuple",
            Value::Dict => "Dict",
            Value::Set => "Set",
            Value::Lambda => "Lambda",
            Value::Func { is_async, .. } => {
                if is_async {
                    "AsyncFunctionDef"
                } else {
                    "FunctionDef"
                }
            }
            Value::BoundMethod { .. } => "BoundMethod",
            Value::UnboundMethod => "UnboundMethod",
            Value::Module => "Module",
            Value::Instance => "Instance",
            Value::Class | Value::Param => return None,
        })
    }
}

/// Where an expression is written: the class whose method it is in and the
/// name of that method's receiver.
#[derive(Clone, Copy, Default)]
struct Scope<'a> {
    class: Option<&'a StmtClassDef>,
    receiver: Option<&'a str>,
    /// Whether the receiver is a class (`cls`), not an instance.
    on_class: bool,
}

struct Infer<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: &'e Hierarchy<'a, 'e>,
    /// The value each single name target is assigned, by node id.
    assigned: HashMap<u32, &'a Expr>,
}

struct Assigned<'a, 'm>(&'m mut HashMap<u32, &'a Expr>);

impl<'a> Visitor<'a> for Assigned<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Assign(StmtAssign { targets, value, .. }) => {
                for target in targets {
                    self.add(target, value);
                }
            }
            Stmt::AnnAssign(StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => self.add(target, value),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// The value each single name target of `body` is assigned, by node id.
pub(super) fn assigned_values(body: &[Stmt]) -> HashMap<u32, &Expr> {
    let mut assigned = HashMap::new();
    Assigned(&mut assigned).visit_body(body);
    assigned
}

impl<'a> Assigned<'a, '_> {
    fn add(&mut self, target: &Expr, value: &'a Expr) {
        if let Expr::Name(name) = target
            && let Some(id) = name.node_index().load().as_u32()
        {
            self.0.insert(id, value);
        }
    }
}

/// The elements of a tuple or list display.
fn elements(expr: &Expr) -> Option<&[Expr]> {
    match expr {
        Expr::Tuple(t) => Some(&t.elts),
        Expr::List(l) => Some(&l.elts),
        _ => None,
    }
}

/// Whether some statement under `class` assigns the attribute `name`.
fn stores(class: &StmtClassDef, name: &str) -> bool {
    struct Find<'n>(&'n str, bool);
    impl Visitor<'_> for Find<'_> {
        fn visit_expr(&mut self, expr: &Expr) {
            if let Expr::Attribute(a) = expr
                && a.attr.as_str() == self.0
                && !matches!(a.ctx, ruff_python_ast::ExprContext::Load)
            {
                self.1 = true;
            }
            visitor::walk_expr(self, expr);
        }
    }
    let mut find = Find(name, false);
    find.visit_body(&class.body);
    find.1
}

impl<'a> Infer<'a, '_> {
    fn value(&self, expr: &'a Expr, scope: Scope<'a>, depth: u8) -> Option<Value> {
        if depth == 0 {
            return None;
        }
        Some(match expr {
            Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_) => Value::Const,
            Expr::List(_) => Value::List,
            Expr::Tuple(_) => Value::Tuple,
            Expr::Dict(_) => Value::Dict,
            Expr::Set(_) => Value::Set,
            // An f-string without interpolation is a constant to Pylint.
            Expr::FString(f)
                if f.value.iter().all(|part| match part {
                    FStringPartRef::Literal(_) => true,
                    FStringPartRef::FString(s) => s.elements.interpolations().next().is_none(),
                }) =>
            {
                Value::Const
            }
            Expr::Lambda(_) => Value::Lambda,
            Expr::UnaryOp(u)
                if matches!(u.op, UnaryOp::USub | UnaryOp::UAdd | UnaryOp::Invert)
                    && matches!(&*u.operand, Expr::NumberLiteral(_)) =>
            {
                Value::Const
            }
            Expr::BinOp(b)
                if [&*b.left, &*b.right].iter().all(|side| {
                    matches!(side, Expr::NumberLiteral(_) | Expr::StringLiteral(_))
                }) =>
            {
                Value::Const
            }
            Expr::Name(name) => return self.name(expr, name, scope, depth),
            Expr::Attribute(attribute) => return self.attribute(expr, attribute, scope),
            Expr::Call(call) => {
                // A call of a class that derives from `object` alone makes an
                // instance of it, unless a metaclass or a decorator steps in.
                let class = self.env.class_of(&call.func)?;
                let plain = class.decorator_list.is_empty()
                    && class
                        .arguments
                        .as_ref()
                        .is_none_or(|arguments| arguments.keywords.is_empty())
                    && self.env.plain(class, 0);
                return plain.then_some(Value::Instance);
            }
            _ => return None,
        })
    }

    fn name(
        &self,
        expr: &'a Expr,
        name: &'a ExprName,
        scope: Scope<'a>,
        depth: u8,
    ) -> Option<Value> {
        let binding = self.env.binding(name)?;
        let sole = self.env.ctx.model.is_sole_binding(binding.id);
        match binding.kind {
            BindingKind::Builtin => {
                let builtin = binding.name.as_str();
                if BUILTIN_FUNCTIONS.contains(&builtin) {
                    Some(Value::Func {
                        is_async: false,
                        counted: true,
                    })
                } else if builtin == "NotImplemented" || builtin == "Ellipsis" {
                    Some(Value::Const)
                } else if builtin.starts_with(|c: char| c.is_ascii_uppercase())
                    || matches!(
                        builtin,
                        "bool"
                            | "bytearray"
                            | "bytes"
                            | "classmethod"
                            | "complex"
                            | "dict"
                            | "enumerate"
                            | "filter"
                            | "float"
                            | "frozenset"
                            | "int"
                            | "list"
                            | "map"
                            | "memoryview"
                            | "object"
                            | "property"
                            | "range"
                            | "reversed"
                            | "set"
                            | "slice"
                            | "staticmethod"
                            | "str"
                            | "super"
                            | "tuple"
                            | "type"
                            | "zip"
                    )
                {
                    Some(Value::Class)
                } else {
                    None
                }
            }
            BindingKind::Parameter => Some(Value::Param),
            BindingKind::FunctionDef if sole => {
                let func = self.env.func_of(name)?;
                Some(Value::Func {
                    is_async: func.is_async,
                    counted: self.counted(func, None),
                })
            }
            BindingKind::ClassDef if sole => Some(Value::Class),
            BindingKind::Import | BindingKind::FromImport => self.module(expr),
            BindingKind::Assignment | BindingKind::Annotation if sole => {
                let value = self.assigned.get(&binding.node?)?;
                self.value(value, scope, depth - 1)
            }
            _ => None,
        }
    }

    /// The expression a name stands for when it is bound once to an
    /// assignment, followed through other such names.
    fn target(&self, mut expr: &'a Expr) -> &'a Expr {
        for _ in 0..DEPTH {
            let Expr::Name(name) = expr else { break };
            let Some(binding) = self.env.binding(name) else {
                break;
            };
            let assigned = matches!(
                binding.kind,
                BindingKind::Assignment | BindingKind::Annotation
            ) && self.env.ctx.model.is_sole_binding(binding.id);
            match binding
                .node
                .and_then(|id| self.assigned.get(&id))
                .filter(|_| assigned)
            {
                Some(value) => expr = value,
                None => break,
            }
        }
        expr
    }

    /// Whether `expr` is certainly no class, tuple of classes or union of
    /// classes, the second argument `isinstance` takes (Pylint's
    /// `_is_invalid_isinstance_type`).
    fn invalid_type(&self, expr: &'a Expr, scope: Scope<'a>) -> bool {
        // A union held by a name is inferred only when both sides are classes.
        if let Expr::BinOp(b) = expr
            && b.op == Operator::BitOr
        {
            return [&*b.left, &*b.right].into_iter().any(|side| {
                !matches!(side, Expr::NoneLiteral(_)) && self.invalid_type(side, scope)
            });
        }
        match self.target(expr) {
            Expr::Tuple(t) => t.elts.iter().any(|elt| self.invalid_type(elt, scope)),
            Expr::BinOp(_) => false,
            other => self
                .value(other, scope, DEPTH)
                .is_some_and(|value| !matches!(value, Value::Class | Value::Param)),
        }
    }

    /// A module the project or the environment holds.
    fn module(&self, expr: &Expr) -> Option<Value> {
        let qualified = self.hierarchy.project.absolute(&self.env.qualified(expr)?);
        self.hierarchy
            .project
            .has_module(&qualified)
            .then_some(Value::Module)
    }

    fn attribute(
        &self,
        expr: &Expr,
        attribute: &'a ExprAttribute,
        scope: Scope<'a>,
    ) -> Option<Value> {
        let Expr::Name(base) = &*attribute.value else {
            return None;
        };
        let name = attribute.attr.as_str();
        let binding = self.env.binding(base)?;
        if binding.kind == BindingKind::Parameter && scope.receiver == Some(base.id.as_str()) {
            return self.member(scope.class?, name, !scope.on_class);
        }
        if let Some(class) = self.env.class_of(&attribute.value) {
            return self.member(class, name, false);
        }
        self.module(expr)
    }

    /// The method `name` of `class` as `instance.name` or `Class.name`
    /// reads it, when the class and its ancestors define it once, undisguised.
    fn member(&self, class: &'a StmtClassDef, name: &str, instance: bool) -> Option<Value> {
        let chain = self
            .hierarchy
            .mro(&Class::Local(class))
            .unwrap_or_else(|| vec![Class::Local(class)]);
        // An instance attribute of that name competes with the method.
        if instance
            && chain
                .iter()
                .any(|c| c.def().is_some_and(|def| stores(def, name)))
        {
            return None;
        }
        for owner in &chain {
            let Some(def) = owner.def() else { continue };
            let found: Vec<Local> = locals(&def.body)
                .into_iter()
                .filter(|(local, _)| local == name)
                .map(|(_, kind)| kind)
                .collect();
            if found.is_empty() {
                continue;
            }
            if found != [Local::Function] {
                return None;
            }
            let func = methods(&def.body)
                .into_iter()
                .find(|func| func.name.as_str() == name)?;
            let counted = self.counted(func, Some(owner));
            return match func.decorator_list.len() {
                0 if instance => Some(Value::BoundMethod { counted }),
                0 => Some(Value::UnboundMethod),
                1 if is_static(func) => Some(Value::Func {
                    is_async: func.is_async,
                    counted,
                }),
                1 if is_classmethod(func) => Some(Value::BoundMethod { counted }),
                _ => None,
            };
        }
        None
    }

    /// Whether W0143 counts `func` as a bare callable: not one that raises
    /// at its top level, nor a `typing._SpecialForm`.
    fn counted(&self, func: &StmtFunctionDef, owner: Option<&Class<'a>>) -> bool {
        !func.body.iter().any(|stmt| matches!(stmt, Stmt::Raise(_)))
            && !func.decorator_list.iter().any(|decorator| {
                let expr = match &decorator.expression {
                    Expr::Call(call) => &*call.func,
                    other => other,
                };
                let qualified = match owner {
                    Some(owner) => self.hierarchy.qualify(owner, expr),
                    None => self.env.qualified(expr),
                };
                qualified.as_deref() == Some("typing._SpecialForm")
            })
    }
}

pub struct Inferred;

impl Checker for Inferred {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                INHERIT,
                "inherit-non-class",
                "A class that inherits from something that is not a class.",
            ),
            rule(
                CLASS_OBJECT,
                "invalid-class-object",
                "An object that is not a class assigned to `__class__`.",
            ),
            rule(
                CALLABLE,
                "comparison-with-callable",
                "A function compared with a value, probably without its parentheses.",
            ),
            rule(
                ISINSTANCE,
                "isinstance-second-argument-not-valid-type",
                "The second argument of `isinstance` is certainly not a class.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if ![INHERIT, CLASS_OBJECT, CALLABLE, ISINSTANCE]
            .iter()
            .any(|id| enabled.contains(*id))
        {
            return;
        }
        let env = Env::new(ctx);
        let hierarchy = Hierarchy::new(&env);
        let mut assigned = HashMap::new();
        Assigned(&mut assigned).visit_body(&ctx.parsed.syntax().body);
        let infer = Infer {
            env: &env,
            hierarchy: &hierarchy,
            assigned,
        };
        let mut walk = Walk {
            infer: &infer,
            enabled,
            out,
            scope: Scope::default(),
            class_level: None,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e, 'o> {
    infer: &'e Infer<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    out: &'o mut Vec<Diagnostic>,
    scope: Scope<'a>,
    /// The class whose body is being walked, outside any function of it.
    class_level: Option<&'a StmtClassDef>,
}

impl<'a> Walk<'a, '_, '_> {
    fn ctx(&self) -> &'a Ctx<'a> {
        self.infer.env.ctx
    }

    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn inherit(&mut self, class: &'a StmtClassDef) {
        for base in bases(class) {
            let final_builtin = matches!(base, Expr::Name(name)
            if self.infer.env.binding(name).is_some_and(|b| {
                b.kind == BindingKind::Builtin && FINAL_BUILTINS.contains(&b.name.as_str())
            }));
            let value = self.infer.value(base, self.scope, DEPTH);
            if final_builtin || value.is_some_and(|value| value.node().is_some()) {
                let shown = py_repr(&as_string(self.ctx().source, base));
                self.out.push(self.ctx().report_at(
                    INHERIT,
                    class.name.range(),
                    format!("Inheriting {shown}, which is not a class."),
                ));
            }
        }
    }

    /// `target.__class__ = value`, also as one element of a tuple target.
    fn class_object(&mut self, targets: &'a [Expr], value: &'a Expr) {
        for target in targets {
            match target {
                Expr::Attribute(a) if a.attr.as_str() == "__class__" => {
                    self.class_value(target, value);
                }
                Expr::Tuple(_) | Expr::List(_) => {
                    let (Some(elts), Some(values)) = (elements(target), elements(value)) else {
                        continue;
                    };
                    // Pylint reads the value of the last `__class__` target for each.
                    let is_class = |e: &&Expr| matches!(e, Expr::Attribute(a) if a.attr.as_str() == "__class__");
                    let Some(index) = elts.iter().rposition(|e| is_class(&e)) else {
                        continue;
                    };
                    let Some(assigned) = values.get(index) else {
                        continue;
                    };
                    for element in elts.iter().filter(is_class) {
                        self.class_value(element, assigned);
                    }
                }
                _ => {}
            }
        }
    }

    fn class_value(&mut self, target: &Expr, value: &'a Expr) {
        if let Some(node) = self
            .infer
            .value(value, self.scope, DEPTH)
            .and_then(Value::node)
        {
            self.out.push(self.ctx().report(
                CLASS_OBJECT,
                target,
                format!(
                    "Invalid assignment to '__class__'. Should be a class definition but got a '{node}'"
                ),
            ));
        }
    }

    /// Whether `expr` is a bare callable (`Some(true)`), certainly is not
    /// (`Some(false)`), or cannot be told.
    fn callable(&self, expr: &'a Expr) -> Option<bool> {
        match expr {
            Expr::FString(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_)
            | Expr::Compare(_)
            | Expr::BinOp(_) => Some(false),
            Expr::UnaryOp(u) if u.op == UnaryOp::Not => Some(false),
            _ => match self.infer.value(expr, self.scope, DEPTH)? {
                Value::Func { counted, .. } | Value::BoundMethod { counted } => Some(counted),
                _ => Some(false),
            },
        }
    }

    fn comparison(&mut self, compare: &'a ruff_python_ast::ExprCompare) {
        if !matches!(
            compare.ops[0],
            CmpOp::Eq | CmpOp::NotEq | CmpOp::Lt | CmpOp::Gt | CmpOp::LtE | CmpOp::GtE
        ) {
            return;
        }
        let sides = [&compare.operands[0], &compare.operands[1]].map(|side| self.callable(side));
        if matches!(sides, [Some(true), Some(false)] | [Some(false), Some(true)]) {
            self.out.push(self.ctx().report(
                CALLABLE,
                compare,
                "Comparing against a callable, did you omit the parenthesis?",
            ));
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let saved = (self.scope, self.class_level);
        match stmt {
            Stmt::ClassDef(class) => {
                if self.on(INHERIT) {
                    self.inherit(class);
                }
                self.class_level = Some(class);
                self.scope = Scope::default();
            }
            Stmt::FunctionDef(func) => {
                if let Some(class) = self.class_level {
                    let first = func
                        .parameters
                        .posonlyargs
                        .iter()
                        .chain(&func.parameters.args)
                        .next()
                        .map(|p| p.parameter.name.as_str());
                    self.scope = Scope {
                        class: Some(class),
                        receiver: first.filter(|_| !is_static(func)),
                        on_class: is_classmethod(func),
                    };
                }
                self.class_level = None;
            }
            Stmt::Assign(assign) if self.on(CLASS_OBJECT) => {
                self.class_object(&assign.targets, &assign.value);
            }
            Stmt::AugAssign(assign) if self.on(CLASS_OBJECT) => {
                self.class_object(std::slice::from_ref(&*assign.target), &assign.value);
            }
            Stmt::AnnAssign(assign) if self.on(CLASS_OBJECT) => {
                if let Some(value) = &assign.value {
                    self.class_object(std::slice::from_ref(&*assign.target), value);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
        (self.scope, self.class_level) = saved;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Compare(compare) = expr
            && self.on(CALLABLE)
        {
            self.comparison(compare);
        }
        if let Expr::Call(call) = expr
            && self.on(ISINSTANCE)
            && let Expr::Name(callee) = &*call.func
            && self
                .infer
                .env
                .binding(callee)
                .is_some_and(|b| b.kind == BindingKind::Builtin && b.name.as_str() == "isinstance")
            && !call.arguments.args.iter().any(Expr::is_starred_expr)
            && let Some(second) = call.arguments.args.get(1)
            && self.infer.invalid_type(second, self.scope)
        {
            self.out.push(self.ctx().report(
                ISINSTANCE,
                call,
                "Second argument of isinstance is not a type",
            ));
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn inherit_non_class() {
        let source = include_str!("fixtures/inherit_non_class.py");
        let expected = include_str!("fixtures/inherit_non_class.expected").trim_end();
        assert_eq!(render(source, &[], &["E0239"]), expected);
    }

    #[test]
    fn invalid_class_object() {
        let source = include_str!("fixtures/invalid_class_object.py");
        let expected = include_str!("fixtures/invalid_class_object.expected").trim_end();
        assert_eq!(render(source, &[], &["E0243"]), expected);
    }

    #[test]
    fn isinstance_second_argument() {
        let source = include_str!("fixtures/isinstance_second_argument.py");
        let expected = include_str!("fixtures/isinstance_second_argument.expected").trim_end();
        assert_eq!(render(source, &[], &["W1116"]), expected);
    }

    #[test]
    fn comparison_with_callable() {
        let source = include_str!("fixtures/comparison_with_callable.py");
        let expected = include_str!("fixtures/comparison_with_callable.expected").trim_end();
        assert_eq!(render(source, &[], &["W0143"]), expected);
    }
}
