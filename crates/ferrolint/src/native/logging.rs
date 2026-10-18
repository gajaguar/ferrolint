//! `E1200` logging-unsupported-format and `E1201` logging-format-truncated,
//! on the calls of the `logging-modules` and of the loggers they make.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt};
use ruff_text_size::Ranged;

use super::formats::{PercentError, parse_method, parse_percent, repr};
use super::{Checker, Ctx, rule};

const UNSUPPORTED: &str = "E1200";
const TRUNCATED: &str = "E1201";

/// The logging methods whose first argument is the format.
const METHODS: &[&str] = &[
    "critical",
    "debug",
    "error",
    "exception",
    "fatal",
    "info",
    "warn",
    "warning",
];

pub struct Logging;

impl Checker for Logging {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                UNSUPPORTED,
                "logging-unsupported-format",
                "A logging format string with a `%` conversion character that does not exist.",
            ),
            rule(
                TRUNCATED,
                "logging-format-truncated",
                "A logging format string that ends inside a conversion.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(UNSUPPORTED) && !enabled.contains(TRUNCATED) {
            return;
        }
        let modules: Vec<&str> = ctx
            .option("logging-modules")
            .unwrap_or("logging")
            .split(',')
            .map(str::trim)
            .filter(|module| !module.is_empty())
            .collect();
        let mut scan = Scan {
            ctx,
            modules,
            names: BTreeSet::new(),
            made: Vec::new(),
        };
        scan.visit_body(&ctx.parsed.syntax().body);
        // shortcut: a logger is a name assigned from `<logging>.getLogger(...)` anywhere in the
        // file, where Pylint infers the class; upgrade with the inference query
        let loggers = scan
            .made
            .iter()
            .filter(|(_, module)| scan.names.contains(module))
            .map(|(target, _)| target.clone())
            .collect();
        let mut walk = Walk {
            ctx,
            out,
            names: scan.names,
            loggers,
            new_style: ctx.option("logging-format-style") == Some("new"),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// The names the logging modules are bound to, and the `target = module.getLogger()`
/// assignments.
struct Scan<'a, 'c> {
    ctx: &'a Ctx<'c>,
    modules: Vec<&'a str>,
    names: BTreeSet<String>,
    made: Vec<(String, String)>,
}

impl Scan<'_, '_> {
    fn made_by(&mut self, target: &Expr, value: &Expr) {
        if let Expr::Call(call) = value
            && let Expr::Attribute(attribute) = &*call.func
            && let Expr::Name(module) = &*attribute.value
            && matches!(attribute.attr.as_str(), "getLogger" | "Logger")
        {
            let target = self.ctx.source[target.range()].to_owned();
            self.made.push((target, module.id.to_string()));
        }
    }
}

impl<'a> Visitor<'a> for Scan<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    if self.modules.contains(&alias.name.as_str()) {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        self.names.insert(name.to_string());
                    }
                }
            }
            // `logging-modules=pkg.log` makes `from pkg import log` a logging name
            Stmt::ImportFrom(import) => {
                let module = import.module.as_ref().map_or("", |module| module.as_str());
                for alias in &import.names {
                    if self
                        .modules
                        .contains(&format!("{module}.{}", alias.name).as_str())
                    {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        self.names.insert(name.to_string());
                    }
                }
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.made_by(target, &assign.value);
                }
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    self.made_by(&assign.target, value);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

struct Walk<'a, 'c> {
    ctx: &'a Ctx<'c>,
    out: &'a mut Vec<Diagnostic>,
    names: BTreeSet<String>,
    loggers: BTreeSet<String>,
    new_style: bool,
}

impl Walk<'_, '_> {
    fn is_logger(&self, receiver: &Expr) -> bool {
        match receiver {
            Expr::Name(name) if self.names.contains(name.id.as_str()) => true,
            Expr::Call(call) => matches!(&*call.func, Expr::Attribute(attribute)
                if matches!(&*attribute.value, Expr::Name(module) if self.names.contains(module.id.as_str()))
                    && attribute.attr.as_str() == "getLogger"),
            other => self.loggers.contains(&self.ctx.source[other.range()]),
        }
    }

    fn call(&mut self, call: &ExprCall) {
        let Expr::Attribute(attribute) = &*call.func else {
            return;
        };
        if !self.is_logger(&attribute.value) {
            return;
        }
        let arguments = &call.arguments;
        let method = attribute.attr.as_str();
        if arguments.args.iter().any(Expr::is_starred_expr)
            || arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            return;
        }
        let position = match method {
            "log" if arguments.args.len() >= 2 => 1,
            m if METHODS.contains(&m) && !arguments.args.is_empty() => 0,
            _ => return,
        };
        let text = match &arguments.args[position] {
            Expr::StringLiteral(string) => string.value.to_str().to_owned(),
            Expr::BytesLiteral(bytes) => {
                let all: Vec<u8> = bytes
                    .value
                    .iter()
                    .flat_map(|part| part.value.iter().copied())
                    .collect();
                String::from_utf8_lossy(&all).into_owned()
            }
            _ => return,
        };
        let supplied = arguments.args.len() - position - 1;
        if self.new_style {
            if parse_method(&text).is_err() {
                self.truncated(call);
            }
            return;
        }
        match parse_percent(&text) {
            Err(PercentError::Incomplete) => self.truncated(call),
            // without arguments the logging module never formats the message
            Err(PercentError::Unsupported(index)) if supplied > 0 => {
                let found = text.chars().nth(index).unwrap_or_default();
                self.out.push(self.ctx.report(
                    UNSUPPORTED,
                    call,
                    format!(
                        "Unsupported logging format character {} ({:#x}) at index {index}",
                        repr(&found.to_string()),
                        u32::from(found)
                    ),
                ));
            }
            _ => {}
        }
    }

    fn truncated(&mut self, call: &ExprCall) {
        self.out.push(self.ctx.report(
            TRUNCATED,
            call,
            "Logging format string ends in middle of conversion specifier",
        ));
    }
}

impl<'a> Visitor<'a> for Walk<'_, '_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            self.call(call);
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::{lint, lint_with};

    const SOURCE: &str = "\
import logging
import logging as lg
log = logging.getLogger(__name__)


class A:
    def __init__(self):
        self.logger = lg.getLogger()

    def run(self):
        self.logger.info('%y', 1)
        self.logger.info('%y')
        self.logger.error('100%', 1)
        self.logger.log(10, 'ab%', 1)


logging.warning('%s %z', 1)
log.debug('%(a)s %', 1)
log.info('%s', 1)
other.info('%y', 1)
logging.info(b'%y', 1)
logging.info('{} {', 1)
logging.info('%y', *args)
";

    #[test]
    fn unsupported_and_truncated() {
        let found: Vec<_> = lint(SOURCE)
            .into_iter()
            .filter(|(_, code)| code.starts_with("E12"))
            .collect();
        let rows = [
            (11, "E1200"),
            (13, "E1201"),
            (14, "E1201"),
            (17, "E1200"),
            (18, "E1201"),
            (21, "E1200"),
        ];
        let expected: Vec<_> = rows
            .iter()
            .map(|(line, code)| (*line, (*code).to_owned()))
            .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn new_style_and_modules() {
        let source =
            "import pkglog\npkglog.info('{ {', 1)\nimport logging\nlogging.info('{ {', 1)\n";
        let options = [
            ("logging-modules", "pkglog"),
            ("logging-format-style", "new"),
        ];
        assert_eq!(lint_with(source, &options), [(2, "E1201".to_owned())]);
    }
}
