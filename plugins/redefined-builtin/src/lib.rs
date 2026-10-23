//! A port of Pylint's `redefined-builtin` (W0622): a module or function
//! binds a name that shadows a builtin, such as `list = []` or a
//! parameter named `id`. Each name reports once per scope, at its first
//! binding; only a module and a function are checked, as Pylint does, and
//! imports from `redefining-builtins-modules` are left alone.
//!
//! shortcut: a name that a star import binds counts as no module name;
//! upgrade by reading the names the host gives (`modules.star-names`).

use std::collections::HashSet;

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "NAM002";

/// Python's builtins, sorted for a binary search.
const BUILTINS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "Ellipsis",
    "EncodingWarning",
    "EnvironmentError",
    "Exception",
    "ExceptionGroup",
    "False",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "None",
    "NotADirectoryError",
    "NotImplemented",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "PythonFinalizationError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "True",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
    "_IncompleteInputError",
    "__build_class__",
    "__builtins__",
    "__debug__",
    "__doc__",
    "__import__",
    "__loader__",
    "__name__",
    "__package__",
    "__spec__",
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "copyright",
    "credits",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "exit",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "license",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "quit",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];

#[derive(Default)]
struct RedefinedBuiltin {
    /// (scope, name) pairs already reported: Pylint reports a name once
    /// per scope.
    reported: HashSet<(u32, String)>,
}

impl Checker for RedefinedBuiltin {
    fn info() -> Info {
        Info::new("redefined-builtin", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(CODE, "redefined-builtin", "A binding shadows a builtin.")
                .pylint("W0622", "redefined-builtin"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if !binds(node) {
            return;
        }
        let Some(binding) = cx.resolve(node) else {
            return;
        };
        if binding.node != Some(node.id) || BUILTINS.binary_search(&binding.name.as_str()).is_err()
        {
            return;
        }
        // A name that a `global` statement binds belongs to the module.
        let module = cx.nodes().first().map(|root| cx.scope(root).id);
        let kind = if Some(binding.scope) == module {
            ScopeKind::Module
        } else {
            cx.scope(node).kind
        };
        // Pylint's `visit_module` skips `__doc__`; its `visit_functiondef`
        // leaves a name the module binds to `redefined-outer-name`.
        if !matches!(kind, ScopeKind::Module | ScopeKind::Function)
            || (kind == ScopeKind::Module && binding.name == "__doc__")
            || (kind == ScopeKind::Function
                && (module_binds(cx, module, &binding.name)
                    || cx
                        .option("allowed-redefined-builtins")
                        .is_some_and(|names| {
                            names.split(',').any(|name| name.trim() == binding.name)
                        })))
        {
            return;
        }
        // Pylint judges only the first binding of a name in a scope.
        if !self.reported.insert((binding.scope, binding.name.clone())) {
            return;
        }
        if node.kind == NodeKind::Parameter && overridden(cx, node) {
            return;
        }
        // An import from one of these modules keeps the builtin; Pylint
        // reports any other at its statement.
        let mut range = binding.range.unwrap_or(node.range);
        if node.kind == NodeKind::Alias
            && let Some(statement) = cx.parent(node)
        {
            let modules = cx
                .option("redefining-builtins-modules")
                .unwrap_or("six.moves,past.builtins,future.builtins,builtins,io");
            if statement.kind == NodeKind::ImportFrom
                && statement
                    .detail
                    .is_some_and(|source| modules.split(',').any(|name| name.trim() == source))
            {
                return;
            }
            range = statement.range;
        }
        let message = format!("Redefining built-in '{}'", binding.name);
        cx.report(Diagnostic::new(CODE, message, range));
    }
}

/// Whether `node` may bind a name.
fn binds(node: &Node<'_>) -> bool {
    match node.kind {
        NodeKind::Name => node.context == ExprContext::Store,
        NodeKind::FunctionDef
        | NodeKind::ClassDef
        | NodeKind::Parameter
        | NodeKind::Alias
        | NodeKind::ExceptHandler
        | NodeKind::Pattern => true,
        _ => false,
    }
}

/// Pylint's `overridden_method` test: the parameter belongs to a method that
/// an ancestor class defines with a parameter of the same name.
fn overridden(cx: &Context<'_>, parameter: &Node<'_>) -> bool {
    let mut up = cx.ancestors(parameter);
    let (_, Some(method), Some(name)) = (up.next(), up.next(), parameter.name) else {
        return false;
    };
    cx.overridden_parameters(method)
        .is_some_and(|names| names.iter().any(|other| other == name))
}

/// Whether the module `scope` binds `name`, so a function binding it is `W0621`.
fn module_binds(cx: &Context<'_>, module: Option<u32>, name: &str) -> bool {
    // An alias names the module imported, not the name it binds.
    cx.nodes().iter().any(|node| {
        (node.name == Some(name) || node.kind == NodeKind::Alias)
            && binds(node)
            && cx
                .resolve(node)
                .is_some_and(|binding| binding.name == name && Some(binding.scope) == module)
    })
}

export_checker!(RedefinedBuiltin);
