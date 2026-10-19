//! Host bindings of the `ferrolint:plugin` WIT contract in `wit/`.

wasmtime::component::bindgen!({
    path: "../../wit",
    world: "plugin",
    imports: { default: trappable },
});

pub use exports::ferrolint::plugin::checker::{Ast, Diagnostic, File, PluginInfo};
pub use ferrolint::plugin::comments::Comment;
pub use ferrolint::plugin::semantic::{Binding, BindingKind, Scope, ScopeKind};
pub use ferrolint::plugin::types::{ExprContext, NodeKind, Position, Range, Severity};
