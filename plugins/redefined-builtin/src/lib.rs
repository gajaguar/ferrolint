//! A port of Pylint's `redefined-builtin` (W0622): a module or function
//! binds a name that shadows a builtin, such as `list = []` or a
//! parameter named `id`.

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
    "__import__",
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
struct RedefinedBuiltin;

impl Checker for RedefinedBuiltin {
    fn info() -> Info {
        Info::new("redefined-builtin", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(CODE, "redefined-builtin", "A binding shadows a builtin.")
                .pylint("W0622", "redefined-builtin"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        let binds = match node.kind {
            NodeKind::Name => node.context == ExprContext::Store,
            NodeKind::FunctionDef
            | NodeKind::ClassDef
            | NodeKind::Parameter
            | NodeKind::Alias
            | NodeKind::ExceptHandler => true,
            _ => false,
        };
        if !binds {
            return;
        }
        let Some(binding) = cx.resolve(node) else {
            return;
        };
        if binding.node != Some(node.id) || BUILTINS.binary_search(&binding.name.as_str()).is_err()
        {
            return;
        }
        if cx.scope(node).kind == ScopeKind::Class {
            return;
        }
        let message = format!("Redefining built-in '{}'", binding.name);
        let range = binding.range.unwrap_or(node.range);
        cx.report(Diagnostic::new(CODE, message, range));
    }
}

export_checker!(RedefinedBuiltin);
