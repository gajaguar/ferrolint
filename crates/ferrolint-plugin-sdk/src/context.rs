//! What a checker sees of the current file, and how it asks the host.

use std::cell::OnceCell;

use crate::bindings::ferrolint::plugin::semantic;
use crate::{Binding, Diagnostic, File, Inferred, Node, NodeKind, Origin, Range, Scope};

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
    /// The file's comments in source order, each as its range and whether
    /// it is alone on its line. A native test that does not care answers
    /// that there are none.
    fn comment_list(&self) -> Vec<(Range, bool)> {
        Vec::new()
    }
    /// The options the configuration gives this plugin alone, sorted by
    /// key. A native test that does not care answers that there are none.
    fn plugin_options(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// The names the `from m import *` at the alias `node` binds, or none
    /// when the host cannot tell. A native test that does not care answers
    /// that it cannot.
    fn star_names(&self, _node: u32) -> Option<Vec<String>> {
        None
    }
    /// The parameter names of the method that the function at `method`
    /// overrides, or none when the host cannot tell. A native test that does
    /// not care answers that it cannot.
    fn overridden_parameters(&self, _method: u32) -> Option<Vec<String>> {
        None
    }
}

/// A comment of the file: its range and its text, `#` included.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Comment<'a> {
    pub range: Range,
    pub text: &'a str,
    /// True when only whitespace precedes the comment on its line.
    pub own_line: bool,
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

    fn comment_list(&self) -> Vec<(Range, bool)> {
        crate::bindings::ferrolint::plugin::comments::comment_list()
            .into_iter()
            .map(|comment| (comment.range, comment.own_line))
            .collect()
    }

    fn plugin_options(&self) -> Vec<(String, String)> {
        crate::bindings::ferrolint::plugin::settings::plugin_options()
    }

    fn star_names(&self, node: u32) -> Option<Vec<String>> {
        crate::bindings::ferrolint::plugin::modules::star_names(node)
    }

    fn overridden_parameters(&self, method: u32) -> Option<Vec<String>> {
        crate::bindings::ferrolint::plugin::modules::overridden_parameters(method)
    }
}

/// The current file, its nodes, the host's queries and the diagnostics
/// reported so far.
pub struct Context<'a> {
    file: &'a File,
    nodes: &'a [Node<'a>],
    semantic: &'a dyn Semantic,
    diagnostics: Vec<Diagnostic>,
    // Asked of the host on first use, so a plugin that never reads them
    // never calls the query.
    comments: OnceCell<Vec<Comment<'a>>>,
    plugin_options: OnceCell<Vec<(String, String)>>,
}

impl<'a> Context<'a> {
    pub fn new(file: &'a File, nodes: &'a [Node<'a>], semantic: &'a dyn Semantic) -> Self {
        Self {
            file,
            nodes,
            semantic,
            diagnostics: Vec::new(),
            comments: OnceCell::new(),
            plugin_options: OnceCell::new(),
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

    /// The configuration's option `key` as written, kebab-case like
    /// Pylint's (`max-args`); none when the configuration does not set it.
    pub fn option(&self, key: &str) -> Option<&str> {
        let found = self.file.options.iter().find(|(name, _)| name == key);
        found.map(|(_, value)| value.as_str())
    }

    /// The option `key` the plugin's own `[[plugins]]` entry sets in its
    /// `options` table, as written; none when it does not. Unlike
    /// [`option`](Self::option), no other plugin sees it.
    pub fn plugin_option(&self, key: &str) -> Option<&str> {
        let options = self
            .plugin_options
            .get_or_init(|| self.semantic.plugin_options());
        let found = options.iter().find(|(name, _)| name == key);
        found.map(|(_, value)| value.as_str())
    }

    /// The file's comments, in source order, each with its text.
    pub fn comments(&self) -> &[Comment<'a>] {
        self.comments.get_or_init(|| {
            let source = self.file.source.as_str();
            self.semantic
                .comment_list()
                .into_iter()
                .filter_map(|(range, own_line)| {
                    let text =
                        source.get(range.start.offset as usize..range.end.offset as usize)?;
                    Some(Comment {
                        range,
                        text,
                        own_line,
                    })
                })
                .collect()
        })
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

    /// The names the `from m import *` at the alias `node` binds, or none
    /// when the host cannot tell (see the contract's `modules` interface).
    pub fn star_names(&self, node: &Node<'_>) -> Option<Vec<String>> {
        self.semantic.star_names(node.id)
    }

    /// The parameter names of the method that the function at `method`
    /// overrides, position-only ones left out, or none when there is none or
    /// the host cannot tell (see the contract's `modules` interface).
    pub fn overridden_parameters(&self, method: &Node<'_>) -> Option<Vec<String>> {
        self.semantic.overridden_parameters(method.id)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The host's answers for a file, as a native test supplies them.
    struct Fake;

    impl Semantic for Fake {
        fn scope_of(&self, _node: u32) -> Scope {
            unreachable!()
        }
        fn visible_bindings(&self, _node: u32) -> Vec<Binding> {
            Vec::new()
        }
        fn resolve(&self, _node: u32) -> Option<Binding> {
            None
        }
        fn comment_list(&self) -> Vec<(Range, bool)> {
            // `# one` at offsets 2..7 (trailing) and `# two` at 10..15
            // (alone on its line); a third range past the source is dropped.
            let at = |offset| crate::Position {
                offset,
                line: 1,
                column: offset + 1,
            };
            [(2, 7, false), (10, 15, true), (90, 95, true)]
                .map(|(start, end, own_line)| {
                    let range = Range {
                        start: at(start),
                        end: at(end),
                    };
                    (range, own_line)
                })
                .to_vec()
        }
        fn plugin_options(&self) -> Vec<(String, String)> {
            vec![("markers".into(), "a,b".into())]
        }
    }

    fn file() -> File {
        File {
            path: "a.py".into(),
            module: "a".into(),
            source: "x # one\ny # two\n".into(),
            options: vec![("shared".into(), "1".into())],
        }
    }

    #[test]
    fn comments_pair_each_range_with_its_text() {
        let (file, semantic) = (file(), Fake);
        let cx = Context::new(&file, &[], &semantic);
        let texts: Vec<_> = cx.comments().iter().map(|comment| comment.text).collect();
        assert_eq!(texts, ["# one", "# two"]);
        let own_lines: Vec<_> = cx.comments().iter().map(|c| c.own_line).collect();
        assert_eq!(own_lines, [false, true]);
    }

    #[test]
    fn plugin_options_are_apart_from_the_shared_options() {
        let (file, semantic) = (file(), Fake);
        let cx = Context::new(&file, &[], &semantic);
        assert_eq!(cx.plugin_option("markers"), Some("a,b"));
        assert_eq!(cx.plugin_option("shared"), None);
        assert_eq!(cx.option("shared"), Some("1"));
        assert_eq!(cx.option("markers"), None);
    }
}
