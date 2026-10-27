//! Phase 2 type inference: a best-effort summary of what an expression
//! evaluates to, followed through literals, bindings, calls and
//! annotations, and given up on anything dynamic. An answer is resolved
//! only when it holds on every path: a name bound more than once, a
//! comparison a class can overload or a decorated function is unresolved.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Flat, FlatNode};
use crate::semantic::Model;
use crate::wit::ferrolint::plugin::inference::{Inferred, Origin};
use crate::wit::{BindingKind, ExprContext, NodeKind};

/// How many bindings and calls one answer may follow.
const DEPTH: u8 = 16;

/// Builtins whose name is a class, so calling one makes an instance.
const BUILTIN_TYPES: &[&str] = &[
    "bool",
    "bytearray",
    "bytes",
    "complex",
    "dict",
    "float",
    "frozenset",
    "int",
    "list",
    "memoryview",
    "object",
    "range",
    "set",
    "slice",
    "str",
    "tuple",
    "type",
];

/// The inferred type of the expression at `node`.
pub fn infer(flat: &Flat, model: &Model, source: &str, node: u32) -> Inferred {
    Engine {
        flat,
        model,
        source,
        memo: RefCell::new(HashMap::new()),
    }
    .infer(node, DEPTH)
}

/// What ty infers for the expression at `range`, when the engine above gave up.
/// shortcut: a class, a function, a method or a module only; ty names no more outside its crate.
#[cfg(feature = "ty")]
pub fn from_ty(
    types: &crate::ty::Types,
    path: &std::path::Path,
    source: &str,
    range: ruff_text_size::TextRange,
) -> Option<Inferred> {
    use crate::native::{TyValue, TypeAt};

    let value = types.with_file(path, source, |db, file| {
        crate::ty::values::Values::new(db, file).value(range)
    })??;
    let name = match value {
        TyValue::Class(name) => format!("type[{name}]"),
        TyValue::Function | TyValue::Method => "function".to_owned(),
        TyValue::Module => "module".to_owned(),
        TyValue::Literal => return None,
    };
    Some(known(name, Origin::Unknown))
}

fn unknown() -> Inferred {
    Inferred {
        name: "unknown".into(),
        origin: Origin::Unknown,
        resolved: false,
    }
}

fn known(name: impl Into<String>, origin: Origin) -> Inferred {
    Inferred {
        name: name.into(),
        origin,
        resolved: true,
    }
}

struct Engine<'a> {
    flat: &'a Flat,
    model: &'a Model,
    source: &'a str,
    /// Each expression's answer within one query, so shared operands are
    /// inferred once and a cycle (`x = x + x`) ends unresolved.
    memo: RefCell<HashMap<u32, Inferred>>,
}

impl Engine<'_> {
    fn node(&self, id: u32) -> Option<&FlatNode> {
        self.flat.nodes.get(id as usize)
    }

    /// The direct children of a node, in source order.
    fn children(&self, id: u32) -> Vec<u32> {
        let Some(node) = self.node(id) else {
            return Vec::new();
        };
        let mut children = Vec::new();
        let mut next = id + 1;
        while next <= node.last {
            children.push(next);
            next = self
                .node(next)
                .map_or(u32::MAX, |child| child.last.saturating_add(1));
        }
        children
    }

    fn text(&self, id: u32) -> &str {
        self.node(id)
            .and_then(|node| {
                self.source
                    .get(node.range.start.offset as usize..node.range.end.offset as usize)
            })
            .unwrap_or("")
    }

    fn infer(&self, id: u32, depth: u8) -> Inferred {
        if let Some(answer) = self.memo.borrow().get(&id) {
            return answer.clone();
        }
        self.memo.borrow_mut().insert(id, unknown());
        let answer = self.compute(id, depth);
        self.memo.borrow_mut().insert(id, answer.clone());
        answer
    }

    fn compute(&self, id: u32, depth: u8) -> Inferred {
        let Some(node) = self.node(id) else {
            return unknown();
        };
        if depth == 0 {
            return unknown();
        }
        let literal = |name: &str| known(name, Origin::Literal);
        match node.kind {
            NodeKind::NumberLiteral => literal(number_type(self.text(id))),
            NodeKind::StringLiteral | NodeKind::FString => literal("str"),
            NodeKind::BytesLiteral => literal("bytes"),
            NodeKind::BooleanLiteral => literal("bool"),
            NodeKind::NoneLiteral => literal("NoneType"),
            NodeKind::EllipsisLiteral => literal("ellipsis"),
            NodeKind::List | NodeKind::ListComp => literal("list"),
            NodeKind::Tuple => literal("tuple"),
            NodeKind::Dict | NodeKind::DictComp => literal("dict"),
            NodeKind::Set | NodeKind::SetComp => literal("set"),
            NodeKind::Generator => literal("generator"),
            NodeKind::Lambda => literal("function"),
            NodeKind::UnaryOp if self.flat.string(node.detail) == Some("not") => literal("bool"),
            NodeKind::BinOp => self.binary(id, depth),
            NodeKind::Named => self
                .children(id)
                .last()
                .map_or_else(unknown, |&value| self.infer(value, depth - 1)),
            NodeKind::Name if node.context == ExprContext::Load => self.name(id, depth),
            NodeKind::Call => self.call(id, depth),
            _ => unknown(),
        }
    }

    /// `+`, `-` and `*` on two `int`s or `float`s, `/` on them (always a
    /// `float`), and `+` on two `str`s, `bytes`, `list`s or `tuple`s.
    /// Anything else is unresolved.
    fn binary(&self, id: u32, depth: u8) -> Inferred {
        let Some(node) = self.node(id) else {
            return unknown();
        };
        let children = self.children(id);
        let [left, right] = children.as_slice() else {
            return unknown();
        };
        let (left, right) = (self.infer(*left, depth - 1), self.infer(*right, depth - 1));
        if !left.resolved || !right.resolved {
            return unknown();
        }
        let operator = self.flat.string(node.detail).unwrap_or_default();
        let numeric = |name: &str| matches!(name, "int" | "float");
        let (a, b) = (left.name.as_str(), right.name.as_str());
        let name = match operator {
            "+" | "-" | "*" if numeric(a) && numeric(b) => {
                if a == "int" && b == "int" {
                    "int"
                } else {
                    "float"
                }
            }
            "/" if numeric(a) && numeric(b) => "float",
            "+" if a == b && matches!(a, "str" | "bytes" | "list" | "tuple") => a,
            _ => return unknown(),
        };
        known(name, Origin::Literal)
    }

    fn name(&self, id: u32, depth: u8) -> Inferred {
        let Some(binding) = self.model.resolve(self.flat, id) else {
            return unknown();
        };
        // A name bound twice in its scope (`try`/`except`, `if`/`else`, a
        // rebinding) may hold either value at a use.
        if binding.kind != BindingKind::Builtin && !self.model.is_sole_binding(binding.id) {
            return unknown();
        }
        match binding.kind {
            BindingKind::Builtin => {
                let name = binding.name;
                if BUILTIN_TYPES.contains(&name.as_str()) {
                    known(format!("type[{name}]"), Origin::Builtin(name))
                } else {
                    known("builtin_function_or_method", Origin::Builtin(name))
                }
            }
            BindingKind::Import => {
                // `import os.path` binds `os`, the top-level package;
                // `import os.path as p` binds `os.path` itself.
                let origin = binding.origin.unwrap_or_default();
                let module = if binding.name == origin
                    || origin.starts_with(&format!("{}.", binding.name))
                {
                    origin.split('.').next().unwrap_or_default().to_owned()
                } else {
                    origin
                };
                known("module", Origin::Import(module))
            }
            BindingKind::FromImport => Inferred {
                // A class, a function or a module: the engine cannot tell.
                name: "unknown".into(),
                origin: Origin::Import(binding.origin.unwrap_or_default()),
                resolved: false,
            },
            BindingKind::ClassDef => match binding.node {
                Some(node) => known(format!("type[{}]", binding.name), Origin::ClassDef(node)),
                None => unknown(),
            },
            BindingKind::FunctionDef => match binding.node {
                Some(node) => known("function", Origin::FunctionDef(node)),
                None => unknown(),
            },
            BindingKind::Assignment | BindingKind::Annotation | BindingKind::NamedExpr => binding
                .node
                .map_or_else(unknown, |target| self.assigned(target, depth)),
            _ => unknown(),
        }
    }

    /// The type an annotation names, when it is a plain name or dotted
    /// name; anything more elaborate is unresolved.
    fn annotation(&self, id: u32) -> Inferred {
        let Some(node) = self.node(id) else {
            return unknown();
        };
        let text = self.text(id).trim_matches(|c| c == '"' || c == '\'');
        let plain = !text.is_empty()
            && text.split('.').all(|part| {
                part.chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && part.chars().all(|c| c.is_alphanumeric() || c == '_')
            });
        if plain
            && matches!(
                node.kind,
                NodeKind::Name | NodeKind::Attribute | NodeKind::StringLiteral
            )
        {
            known(text, Origin::Annotation(id))
        } else {
            unknown()
        }
    }

    /// The type a binding's target got: its annotation if it has one,
    /// else the value of a single-target assignment.
    fn assigned(&self, target: u32, depth: u8) -> Inferred {
        let Some(statement) = self.node(target).and_then(|node| node.parent) else {
            return unknown();
        };
        let Some(kind) = self.node(statement).map(|node| node.kind) else {
            return unknown();
        };
        let children = self.children(statement);
        match (kind, children.as_slice()) {
            (NodeKind::Assign, [first, value]) if *first == target => self.infer(*value, depth - 1),
            // The annotation declares the type, wider than the value may
            // be (`x: Callable | None = None`), so the value never counts.
            (NodeKind::AnnAssign, [first, annotation, ..]) if *first == target => {
                self.annotation(*annotation)
            }
            (NodeKind::Named, [first, value]) if *first == target => self.infer(*value, depth - 1),
            _ => unknown(),
        }
    }

    /// Calling a class makes an instance of it; calling an undecorated,
    /// synchronous function with a plain return annotation returns that
    /// type.
    fn call(&self, id: u32, depth: u8) -> Inferred {
        let Some(&target) = self.children(id).first() else {
            return unknown();
        };
        let called = self.infer(target, depth - 1);
        if !called.resolved {
            return unknown();
        }
        if let Some(class) = called
            .name
            .strip_prefix("type[")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            return known(class, Origin::CallReturn(id));
        }
        if let Origin::FunctionDef(function) = called.origin
            && self.plain_function(function)
        {
            // Decorators come before the parameters, the return
            // annotation right after them.
            let returns = self
                .children(function)
                .into_iter()
                .skip_while(|&child| {
                    self.node(child)
                        .is_none_or(|node| node.kind != NodeKind::Parameters)
                })
                .nth(1)
                .filter(|&child| {
                    self.node(child)
                        .is_some_and(|node| is_expression(node.kind))
                });
            if let Some(returns) = returns {
                let declared = self.annotation(returns);
                if declared.resolved {
                    return known(declared.name, Origin::CallReturn(id));
                }
            }
        }
        unknown()
    }
}

impl Engine<'_> {
    /// A function neither `async` (a call returns a coroutine) nor
    /// decorated (a decorator may return anything).
    fn plain_function(&self, function: u32) -> bool {
        let Some(node) = self.node(function) else {
            return false;
        };
        self.flat.string(node.detail) != Some("async")
            && !self.children(function).iter().any(|&child| {
                self.node(child)
                    .is_some_and(|node| node.kind == NodeKind::Decorator)
            })
    }
}

/// The type a number literal's text spells.
fn number_type(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if lower.ends_with('j') {
        "complex"
    } else if lower.starts_with("0x") || lower.starts_with("0o") || lower.starts_with("0b") {
        "int"
    } else if lower.contains(['.', 'e']) {
        "float"
    } else {
        "int"
    }
}

/// Whether a function definition's child of this kind can be its return
/// annotation: an expression, not a decorator, parameter list, type
/// parameter or statement.
fn is_expression(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Name
            | NodeKind::Attribute
            | NodeKind::Subscript
            | NodeKind::StringLiteral
            | NodeKind::NoneLiteral
            | NodeKind::BinOp
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::flatten;
    use ferrolint_core::LineIndex;

    /// What the `nth` name load spelled `name` infers to.
    fn infer_name(source: &str, name: &str, nth: usize) -> (String, bool) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = LineIndex::new(source);
        let flat = flatten(source, &index, parsed.syntax());
        let model = Model::build(parsed.syntax(), &flat.nodes, source, &index, None);
        let node = flat
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.kind == NodeKind::Name
                    && node.context == ExprContext::Load
                    && flat.string(node.name) == Some(name)
            })
            .nth(nth)
            .map(|(id, _)| u32::try_from(id).unwrap())
            .unwrap();
        let inferred = infer(&flat, &model, source, node);
        (inferred.name, inferred.resolved)
    }

    #[test]
    fn literals_and_assignments() {
        let source = "a = 1\nb = 2.5\nc = a + b\nd = [a]\ne = f'{a}'\nf = d == c\ng = not d\nh = a / a\nk = e - e\nprint(a, b, c, d, e, f, g, h, k)\n";
        assert_eq!(infer_name(source, "a", 2), ("int".into(), true));
        assert_eq!(infer_name(source, "b", 1), ("float".into(), true));
        assert_eq!(infer_name(source, "c", 1), ("float".into(), true));
        assert_eq!(infer_name(source, "d", 2), ("list".into(), true));
        assert_eq!(infer_name(source, "e", 2), ("str".into(), true));
        // `__eq__` may return anything; `/` always makes a float; `str`
        // has no `-`.
        assert_eq!(infer_name(source, "f", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "g", 0), ("bool".into(), true));
        assert_eq!(infer_name(source, "h", 0), ("float".into(), true));
        assert_eq!(infer_name(source, "k", 0), ("unknown".into(), false));
    }

    #[test]
    fn classes_calls_and_annotations() {
        let source = "class Widget:\n    pass\n\ndef build() -> Widget:\n    return Widget()\n\n@cache\ndef make() -> Widget:\n    return Widget()\n\nasync def fetch() -> int:\n    return 1\n\nw = Widget()\nb = build()\nm = make()\nf = fetch()\nn = int('3')\n\ndef use(x: str, y):\n    return x, y\n\nprint(w, b, m, f, n, Widget)\n";
        assert_eq!(infer_name(source, "w", 0), ("Widget".into(), true));
        assert_eq!(infer_name(source, "b", 0), ("Widget".into(), true));
        assert_eq!(infer_name(source, "m", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "f", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "n", 0), ("int".into(), true));
        assert_eq!(
            infer_name(source, "Widget", 4),
            ("type[Widget]".into(), true)
        );
        assert_eq!(infer_name(source, "x", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "y", 0), ("unknown".into(), false));
    }

    #[test]
    fn imports_and_dynamic_patterns() {
        let source = "import os.path\nimport json.decoder as jd\nfrom collections import OrderedDict\nimport random\nx = random.choice([1, 'a'])\ny, z = 1, 2\nprint(os, jd, OrderedDict, x, y)\n";
        assert_eq!(infer_name(source, "os", 0), ("module".into(), true));
        assert_eq!(infer_name(source, "jd", 0), ("module".into(), true));
        assert_eq!(
            infer_name(source, "OrderedDict", 0),
            ("unknown".into(), false)
        );
        assert_eq!(infer_name(source, "x", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "y", 0), ("unknown".into(), false));
    }

    #[test]
    fn branches_declarations_and_cycles_stay_unresolved() {
        let source = "try:\n    from _speedups import fast\nexcept ImportError:\n    fast = None\n\ncallback: Callable[[], None] | None = None\n\ndef loop():\n    x = x + x\n    return x\n\nprint(fast, callback)\n";
        assert_eq!(infer_name(source, "fast", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "callback", 0), ("unknown".into(), false));
        assert_eq!(infer_name(source, "x", 2), ("unknown".into(), false));
    }

    #[cfg(feature = "ty")]
    #[test]
    fn ty_answers_what_the_engine_cannot_follow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("use.py");
        let source =
            "from queue import LifoQueue\nimport os\nx = LifoQueue\ny = os.getcwd\nz = os\nn = 1\n";
        std::fs::write(&path, source).unwrap();
        let types = crate::ty::Types::new(dir.path()).unwrap();
        let answer = |name: &str, value: &str| {
            let start = source.find(&format!("{name} = {value}")).unwrap() + name.len() + 3;
            let range = ruff_text_size::TextRange::at(
                u32::try_from(start).unwrap().into(),
                u32::try_from(value.len()).unwrap().into(),
            );
            from_ty(&types, &path, source, range).map(|found| found.name)
        };
        assert_eq!(answer("x", "LifoQueue").as_deref(), Some("type[LifoQueue]"));
        assert_eq!(answer("y", "os.getcwd").as_deref(), Some("function"));
        assert_eq!(answer("z", "os").as_deref(), Some("module"));
        assert_eq!(answer("n", "1"), None);
    }
}
