//! An expression as astroid's `as_string()` prints it, for the messages that
//! quote one. Parentheses follow astroid's precedence table, so a node
//! prints as Pylint prints it whatever the source spacing and quoting.

use std::fmt::Write;

use ruff_python_ast::{BoolOp, Comprehension, Expr, ExprNumberLiteral, Number, Operator, UnaryOp};
use ruff_text_size::Ranged;

use super::classes::const_repr;

/// shortcut: a lambda, an f-string, a t-string and a complex number with an
/// exponent print as their source text or in Rust's form; upgrade when a
/// message needs one.
pub fn as_string(source: &str, expr: &Expr) -> String {
    Printer { source }.show(expr)
}

/// As [`as_string`] for a node whose parent is a statement, where astroid
/// leaves a `yield` unparenthesized.
pub fn as_string_in_statement(source: &str, expr: &Expr) -> String {
    let text = as_string(source, expr);
    match expr {
        Expr::Yield(_) | Expr::YieldFrom(_) => text[1..text.len() - 1].to_owned(),
        _ => text,
    }
}

/// Python's `repr` of a float: Rust's shortest digits, with the exponent
/// written `e+16` or `e-07`.
fn float_repr(value: f64) -> String {
    let text = format!("{value:?}");
    match text.split_once('e') {
        Some((mantissa, exponent)) => {
            let (sign, digits) = exponent
                .strip_prefix('-')
                .map_or(("+", exponent), |digits| ("-", digits));
            format!("{mantissa}e{sign}{digits:0>2}")
        }
        None => text,
    }
}

struct Printer<'s> {
    source: &'s str,
}

/// astroid's `OP_PRECEDENCE`; every other node binds tightest.
fn precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Lambda(_) => 0,
        Expr::If(_) => 1,
        Expr::BoolOp(b) => 2 + u8::from(b.op == BoolOp::And),
        Expr::UnaryOp(u) if u.op == UnaryOp::Not => 4,
        Expr::Named(_) => 5,
        Expr::Compare(_) => 6,
        Expr::BinOp(b) => match b.op {
            Operator::BitOr => 7,
            Operator::BitXor => 8,
            Operator::BitAnd => 9,
            Operator::LShift | Operator::RShift => 10,
            Operator::Add | Operator::Sub => 11,
            Operator::Pow => 14,
            _ => 12,
        },
        Expr::UnaryOp(_) => 13,
        Expr::Await(_) => 15,
        _ => 22,
    }
}

/// Everything is left associative except `**` and a conditional expression.
fn left_associative(expr: &Expr) -> bool {
    !matches!(expr, Expr::If(_)) && !matches!(expr, Expr::BinOp(b) if b.op == Operator::Pow)
}

impl Printer<'_> {
    /// `child` in parentheses when `parent` needs them to keep the meaning.
    fn wrap(&self, parent: &Expr, child: &Expr, is_left: bool) -> String {
        let (outer, inner) = (precedence(parent), precedence(child));
        let wrapped = outer > inner || (outer == inner && is_left != left_associative(parent));
        let text = match (parent, child) {
            // astroid parenthesizes a walrus unless its parent is a comparison
            (Expr::Compare(_), Expr::Named(n)) => {
                format!("{} := {}", self.show(&n.target), self.show(&n.value))
            }
            _ => self.show(child),
        };
        if wrapped { format!("({text})") } else { text }
    }

    fn list(&self, items: &[Expr]) -> String {
        items
            .iter()
            .map(|e| self.show(e))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn generators(&self, generators: &[Comprehension]) -> String {
        generators
            .iter()
            .map(|g| {
                let ifs = g
                    .ifs
                    .iter()
                    .map(|c| format!(" if {}", self.show(c)))
                    .collect::<Vec<_>>()
                    .concat();
                format!(
                    "{}for {} in {}{ifs}",
                    if g.is_async { "async " } else { "" },
                    self.show(&g.target),
                    self.show(&g.iter)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Displays and comprehensions.
    fn container(&self, expr: &Expr) -> Option<String> {
        Some(match expr {
            Expr::Dict(d) => {
                let items: Vec<String> = d
                    .items
                    .iter()
                    .map(|item| match &item.key {
                        Some(key) => format!("{}: {}", self.show(key), self.show(&item.value)),
                        None => format!("**{}", self.show(&item.value)),
                    })
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            Expr::Set(s) => format!("{{{}}}", self.list(&s.elts)),
            Expr::List(l) => format!("[{}]", self.list(&l.elts)),
            Expr::Tuple(t) if t.elts.len() == 1 => format!("({}, )", self.show(&t.elts[0])),
            Expr::Tuple(t) => format!("({})", self.list(&t.elts)),
            Expr::ListComp(c) => {
                format!("[{} {}]", self.show(&c.elt), self.generators(&c.generators))
            }
            Expr::SetComp(c) => format!(
                "{{{} {}}}",
                self.show(&c.elt),
                self.generators(&c.generators)
            ),
            Expr::Generator(c) => {
                format!("({} {})", self.show(&c.elt), self.generators(&c.generators))
            }
            Expr::DictComp(c) => {
                let value = self.show(&c.value);
                let entry = match &c.key {
                    Some(key) => format!("{}: {value}", self.show(key)),
                    None => format!("**{value}"),
                };
                format!("{{{entry} {}}}", self.generators(&c.generators))
            }
            _ => return None,
        })
    }

    fn show(&self, expr: &Expr) -> String {
        match expr {
            Expr::BoolOp(b) => b
                .values
                .iter()
                .map(|v| self.wrap(expr, v, true))
                .collect::<Vec<_>>()
                .join(&format!(" {} ", b.op.as_str())),
            Expr::Named(n) => format!("({} := {})", self.show(&n.target), self.show(&n.value)),
            Expr::BinOp(b) => {
                let (left, right) = (
                    self.wrap(expr, &b.left, true),
                    self.wrap(expr, &b.right, false),
                );
                if b.op == Operator::Pow {
                    format!("{left}**{right}")
                } else {
                    format!("{left} {} {right}", b.op.as_str())
                }
            }
            Expr::UnaryOp(u) => {
                let op = if u.op == UnaryOp::Not {
                    "not "
                } else {
                    u.op.as_str()
                };
                format!("{op}{}", self.wrap(expr, &u.operand, true))
            }
            Expr::If(i) => format!(
                "{} if {} else {}",
                self.wrap(expr, &i.body, true),
                self.wrap(expr, &i.test, true),
                self.wrap(expr, &i.orelse, false)
            ),
            Expr::Await(a) => format!("await {}", self.show(&a.value)),
            // astroid parenthesizes a yield unless it is a statement
            Expr::Yield(y) => match &y.value {
                Some(value) => format!("(yield {})", self.show(value)),
                None => "(yield)".to_owned(),
            },
            Expr::YieldFrom(y) => format!("(yield from {})", self.show(&y.value)),
            Expr::Compare(c) => {
                let mut out = self.wrap(expr, &c.operands[0], true);
                for (op, operand) in c.ops.iter().zip(&c.operands[1..]) {
                    let _ = write!(out, " {} {}", op.as_str(), self.wrap(expr, operand, false));
                }
                out
            }
            Expr::Call(c) => {
                let args = c.arguments.args.iter().map(|a| self.show(a));
                let keywords = c.arguments.keywords.iter().map(|k| match &k.arg {
                    Some(name) => format!("{name}={}", self.show(&k.value)),
                    None => format!("**{}", self.show(&k.value)),
                });
                let all: Vec<String> = args.chain(keywords).collect();
                format!("{}({})", self.wrap(expr, &c.func, true), all.join(", "))
            }
            Expr::Attribute(a) => {
                let left = self.wrap(expr, &a.value, true);
                if !left.is_empty() && left.bytes().all(|b| b.is_ascii_digit()) {
                    format!("({left}).{}", a.attr)
                } else {
                    format!("{left}.{}", a.attr)
                }
            }
            Expr::Subscript(s) => {
                let index = self.show(&s.slice);
                // a tuple index prints without its parentheses
                let index = match &*s.slice {
                    Expr::Tuple(t) if !t.elts.is_empty() => index[1..index.len() - 1].to_owned(),
                    _ => index,
                };
                format!("{}[{index}]", self.wrap(expr, &s.value, true))
            }
            Expr::Slice(s) => {
                let part =
                    |e: &Option<Box<Expr>>| e.as_ref().map_or_else(String::new, |e| self.show(e));
                let (lower, upper, step) = (part(&s.lower), part(&s.upper), part(&s.step));
                if step.is_empty() {
                    format!("{lower}:{upper}")
                } else {
                    format!("{lower}:{upper}:{step}")
                }
            }
            Expr::Starred(s) => format!("*{}", self.show(&s.value)),
            Expr::Name(n) => n.id.to_string(),
            Expr::NumberLiteral(ExprNumberLiteral {
                value: Number::Float(value),
                ..
            }) => float_repr(*value),
            other => self.container(other).unwrap_or_else(|| {
                const_repr(other, "...").unwrap_or_else(|| self.source[other.range()].to_owned())
            }),
        }
    }
}
