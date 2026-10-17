//! What a checker sees of the current file, and how it asks the host.

use crate::bindings::ferrolint::plugin::semantic;
use crate::{Binding, Diagnostic, File, Inferred, Node, NodeKind, Origin, Scope};

/// The scope and binding queries the host answers about the current file.
/// Plugins use the host's answers; a native test can supply its own.
pub trait Semantic {
    /// The innermost scope holding `node`.
    fn scope_of(&self, node: u32) -> Scope;
    /// The bindings visible at `node`, one per name, builtins left out.
    fn visible_bindings(&self, node: u32) -> Vec<Binding>;
    /// What the name at `node` resolves to.
    fn resolve(&self, node: u32) -> Option<Binding>;
    /// The host's best-effort type of the expression at `node`. A native
    /// test that does not care answers that nothing is known.
    fn infer(&self, _node: u32) -> Inferred {
        Inferred {
            name: "unknown".into(),
            origin: Origin::Unknown,
            resolved: false,
        }
    }
}

/// The host's answers, through the contract's `semantic` import.
pub(crate) struct HostSemantic;

impl Semantic for HostSemantic {
    fn scope_of(&self, node: u32) -> Scope {
        semantic::scope_of(node)
    }

    fn visible_bindings(&self, node: u32) -> Vec<Binding> {
        semantic::visible_bindings(node)
    }

    fn resolve(&self, node: u32) -> Option<Binding> {
        semantic::resolve(node)
    }

    fn infer(&self, node: u32) -> Inferred {
        crate::bindings::ferrolint::plugin::inference::infer(node)
    }
}

/// The current file, its nodes, the host's queries and the diagnostics
/// reported so far.
pub struct Context<'a> {
    file: &'a File,
    nodes: &'a [Node<'a>],
    semantic: &'a dyn Semantic,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Context<'a> {
    pub fn new(file: &'a File, nodes: &'a [Node<'a>], semantic: &'a dyn Semantic) -> Self {
        Self {
            file,
            nodes,
            semantic,
            diagnostics: Vec::new(),
        }
    }

    /// The path as reports show it.
    pub fn path(&self) -> &str {
        &self.file.path
    }

    /// The dotted module name.
    pub fn module(&self) -> &str {
        &self.file.module
    }

    pub fn source(&self) -> &str {
        &self.file.source
    }

    /// Every node, in pre-order; a node's id is its index.
    pub fn nodes(&self) -> &'a [Node<'a>] {
        self.nodes
    }

    pub fn node(&self, id: u32) -> Option<&'a Node<'a>> {
        self.nodes.get(id as usize)
    }

    pub fn parent(&self, node: &Node<'_>) -> Option<&'a Node<'a>> {
        node.parent.and_then(|id| self.node(id))
    }

    /// The node's ancestors, nearest first.
    pub fn ancestors(&self, node: &Node<'_>) -> impl Iterator<Item = &'a Node<'a>> + use<'a> {
        let nodes = self.nodes;
        std::iter::successors(node.parent, move |&id| nodes[id as usize].parent)
            .map(move |id| &nodes[id as usize])
    }

    /// The node's direct children, in source order.
    pub fn children(&self, node: &Node<'_>) -> impl Iterator<Item = &'a Node<'a>> + use<'a> {
        let nodes = self.nodes;
        let (first, last) = (node.id + 1, node.last);
        let mut next = first;
        std::iter::from_fn(move || {
            if next > last {
                return None;
            }
            let child = &nodes[next as usize];
            next = child.last + 1;
            Some(child)
        })
    }

    /// The node's source text.
    pub fn text(&self, node: &Node<'_>) -> &'a str {
        let range = node.range;
        self.file
            .source
            .get(range.start.offset as usize..range.end.offset as usize)
            .unwrap_or("")
    }

    pub fn scope(&self, node: &Node<'_>) -> Scope {
        self.semantic.scope_of(node.id)
    }

    pub fn visible_bindings(&self, node: &Node<'_>) -> Vec<Binding> {
        self.semantic.visible_bindings(node.id)
    }

    pub fn resolve(&self, node: &Node<'_>) -> Option<Binding> {
        self.semantic.resolve(node.id)
    }

    /// The host's best-effort type of the expression at `node`; check
    /// `resolved` before trusting it.
    pub fn infer(&self, node: &Node<'_>) -> Inferred {
        self.semantic.infer(node.id)
    }

    /// Whether the node is a statement.
    pub fn is_statement(node: &Node<'_>) -> bool {
        use NodeKind as K;
        matches!(
            node.kind,
            K::FunctionDef
                | K::ClassDef
                | K::Return
                | K::Delete
                | K::Assign
                | K::AugAssign
                | K::AnnAssign
                | K::TypeAlias
                | K::For
                | K::While
                | K::If
                | K::With
                | K::Match
                | K::Raise
                | K::Try
                | K::Assert
                | K::Import
                | K::ImportFrom
                | K::Global
                | K::Nonlocal
                | K::Expr
                | K::Pass
                | K::Break
                | K::Continue
        )
    }

    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}
