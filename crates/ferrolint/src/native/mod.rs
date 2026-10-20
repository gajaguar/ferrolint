//! Pylint's rules that the host runs itself, over the same syntax tree and
//! scope model the plugins query. A native rule's code is its Pylint
//! msg-id, so `W0134` and `return-in-finally` both name it.

#![allow(dead_code)] // shortcut: the context fields wait for the rules that read them; drop this with the first model-based rule

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, LineIndex, RuleInfo, Severity};
use ruff_python_ast::ModModule;
use ruff_python_parser::Parsed;
use ruff_text_size::{Ranged, TextRange};

use crate::ast::Flat;
use crate::semantic::Model;

mod abstract_methods;
mod ancestors;
mod astring;
mod bad_super;
mod booleaness;
mod calls;
mod class_bases;
mod class_members;
mod classes;
mod code_style;
mod conditions;
mod contextmgr;
mod deprecated;
mod django;
mod duplicate_args;
mod exceptions;
mod extensions;
mod finally;
mod flow;
mod formats;
mod hierarchy;
mod imports;
mod imports_scan;
mod inferred;
mod instance_attrs;
mod logging;
mod matchpat;
mod members;
mod misc;
mod names;
mod overrides;
mod project;
pub use project::{Project, environment};
pub mod project_phase;
mod scopes;
mod slots;
mod special;
mod statements;
mod strings;
mod text;
mod unassigned;
mod unpacking;
mod versions;
mod wildcard;

/// What a native rule sees of one file that parses.
pub struct Ctx<'a> {
    pub source: &'a str,
    pub index: &'a LineIndex,
    pub parsed: &'a Parsed<ModModule>,
    pub flat: &'a Flat,
    pub model: &'a Model,
    pub options: &'a [(String, String)],
    pub py_version: Option<(u32, u32)>,
    /// The dotted name Python imports the file under.
    pub module: &'a str,
    /// The file, to find the other modules of its project.
    pub path: &'a std::path::Path,
}

impl Ctx<'_> {
    /// The option `key`, as the configuration spells it.
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

/// A family of native rules that share one walk of the tree.
pub trait Checker: Sync {
    fn rules(&self) -> Vec<RuleInfo>;

    /// Report on `ctx` the rules of `enabled` this checker owns.
    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>);
}

const CHECKERS: &[&dyn Checker] = &[
    &abstract_methods::AbstractMethods,
    &ancestors::Ancestors,
    &bad_super::BadSuper,
    &booleaness::Booleaness,
    &calls::Calls,
    &class_bases::ClassBases,
    &classes::Classes,
    &code_style::CodeStyle,
    &conditions::Conditions,
    &contextmgr::ContextManagers,
    &deprecated::Deprecated,
    &django::Django,
    &duplicate_args::DuplicateArgs,
    &exceptions::Exceptions,
    &extensions::Extensions,
    &finally::Finally,
    &flow::Flow,
    &imports::Imports,
    &class_members::ClassMembers,
    &inferred::Inferred,
    &instance_attrs::InstanceAttrs,
    &logging::Logging,
    &matchpat::MatchPatterns,
    &members::Members,
    &misc::Misc,
    &names::Naming,
    &overrides::Overrides,
    &scopes::Scopes,
    &slots::Slots,
    &special::Special,
    &statements::Statements,
    &strings::Strings,
    &text::Text,
    &unpacking::Unpacking,
    &versions::Versions,
    &wildcard::Wildcard,
];

/// The native rules Pylint ships disabled: `select` or an `enable` option
/// turns them on.
pub const OFF_BY_DEFAULT: &[&str] = &["C1804", "C1805", "R6105"];

/// Every native rule.
pub fn rules() -> Vec<RuleInfo> {
    CHECKERS
        .iter()
        .flat_map(|checker| checker.rules())
        .chain(project_phase::rules())
        .collect()
}

/// Whether any native rule is enabled, so a file needs the model at all.
pub fn wanted(enabled: &BTreeSet<String>) -> bool {
    CHECKERS.iter().any(|checker| {
        checker
            .rules()
            .iter()
            .any(|rule| enabled.contains(&rule.code))
    })
}

/// The diagnostics of the enabled native rules on one file.
pub fn check(ctx: &Ctx<'_>, enabled: &BTreeSet<String>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for checker in CHECKERS {
        checker.check(ctx, enabled, &mut out);
    }
    out
}

/// A rule named by its Pylint msg-id and symbol; the severity follows the
/// message's category: E error, W warning, C and R info.
pub fn rule(id: &str, symbol: &str, summary: &str) -> RuleInfo {
    let severity = match id.as_bytes().first() {
        Some(b'E' | b'F') => Severity::Error,
        Some(b'W') => Severity::Warning,
        _ => Severity::Info,
    };
    RuleInfo {
        code: id.into(),
        name: symbol.into(),
        summary: summary.into(),
        severity,
        pylint_id: None,
        pylint_symbol: Some(symbol.into()),
        plugin: None,
    }
}

impl Ctx<'_> {
    /// A diagnostic of the rule `id` over `node`'s range.
    pub fn report(&self, id: &str, node: &impl Ranged, message: impl Into<String>) -> Diagnostic {
        self.report_at(id, node.range(), message)
    }

    pub fn report_at(&self, id: &str, range: TextRange, message: impl Into<String>) -> Diagnostic {
        let severity = rule(id, "", "").severity;
        Diagnostic {
            code: id.into(),
            message: message.into(),
            range: self
                .index
                .range(self.source, range.start().to_u32(), range.end().to_u32()),
            severity,
            fix: None,
        }
    }
}

/// Run every native rule on `source`, for the rules' unit tests: the
/// `(line, code)` pairs, in report order.
#[cfg(test)]
pub fn lint(source: &str) -> Vec<(u32, String)> {
    lint_with(source, &[])
}

/// As [`lint`], with configuration `options`.
#[cfg(test)]
pub fn lint_with(source: &str, options: &[(&str, &str)]) -> Vec<(u32, String)> {
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let index = LineIndex::new(source);
    let flat = crate::ast::flatten(source, &index, parsed.syntax());
    let model = Model::build(parsed.syntax(), &flat.nodes, source, &index, None);
    let options: Vec<(String, String)> = options
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    let ctx = Ctx {
        source,
        index: &index,
        parsed: &parsed,
        flat: &flat,
        model: &model,
        options: &options,
        py_version: None,
        module: "mod",
        path: std::path::Path::new("mod.py"),
    };
    let enabled = rules().into_iter().map(|rule| rule.code).collect();
    check(&ctx, &enabled)
        .into_iter()
        .map(|diagnostic| (diagnostic.range.start.line, diagnostic.code))
        .collect()
}
