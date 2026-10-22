//! Scopes and bindings of one file, and the queries the contract's
//! `semantic` interface answers from them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, ExprContext as Ctx, HasNodeIndex, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ferrolint_core::LineIndex;

use crate::ast::Flat;
use crate::ast::FlatNode;
use crate::wit::{Binding, BindingKind, Range, Scope, ScopeKind};

/// The Python minor version builtins are taken from.
const PYTHON_MINOR: u8 = 14;
/// Builtin bindings get ids from here up, one per builtin name.
const BUILTIN_BASE: u32 = 0x8000_0000;
const UNSET: u32 = u32::MAX;

/// The semantic model of one file.
pub struct Model {
    scopes: Vec<ScopeData>,
    bindings: Vec<BindingData>,
    /// The scope of each node, by node id.
    node_scope: Vec<u32>,
    /// The binding each binding site creates, by node id.
    node_binding: HashMap<u32, u32>,
    /// Name loads inside deferred annotations: they see a scope's last binding.
    deferred: HashSet<u32>,
}

struct ScopeData {
    kind: ScopeKind,
    name: Option<String>,
    node: u32,
    parent: Option<u32>,
    /// Where a comprehension scope ends; 0 for any other scope.
    end: u32,
    /// The bindings of each name, in source order.
    names: BTreeMap<String, Vec<u32>>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
}

struct BindingData {
    name: String,
    kind: BindingKind,
    scope: u32,
    node: Option<u32>,
    range: Range,
    origin: Option<String>,
    /// Where the binding takes effect: a use before it sees an older one.
    effective: u32,
}

/// The answer to a resolution: a binding of the file, or a builtin.
#[derive(Clone, Copy)]
enum Resolved {
    Binding(u32),
    Builtin(u32),
}

impl Model {
    pub fn build(
        module: &ast::ModModule,
        nodes: &[FlatNode],
        source: &str,
        index: &LineIndex,
        py_version: Option<(u32, u32)>,
    ) -> Self {
        // PEP 649 (3.14) and PEP 563 (`from __future__ import annotations`)
        // evaluate annotations after the module body ran.
        let deferred = py_version.is_some_and(|version| version >= (3, 14))
            || module.body.iter().any(|stmt| {
                matches!(stmt, Stmt::ImportFrom(import)
                    if import.level == 0
                        && import.module.as_ref().is_some_and(|m| m.as_str() == "__future__")
                        && import.names.iter().any(|alias| &alias.name == "annotations"))
            });
        let mut builder = Builder {
            source,
            index,
            model: Model {
                scopes: Vec::new(),
                bindings: Vec::new(),
                node_scope: vec![UNSET; nodes.len()],
                node_binding: HashMap::new(),
                deferred: HashSet::new(),
            },
            current: 0,
            store_kind: BindingKind::Assignment,
            statement_end: 0,
            deferred_annotations: deferred,
            in_annotation: false,
        };
        builder.push_scope(ScopeKind::Module, None, 0);
        builder.model.node_scope[0] = 0;
        builder.visit_body(&module.body);
        let mut model = builder.model;
        for (id, node) in nodes.iter().enumerate() {
            if model.node_scope[id] == UNSET {
                model.node_scope[id] = node
                    .parent
                    .map_or(0, |parent| model.node_scope[parent as usize]);
            }
        }
        model
    }

    fn scope_of_node(&self, node: u32) -> Option<u32> {
        self.node_scope.get(node as usize).copied()
    }

    pub fn scope(&self, node: u32) -> Option<Scope> {
        let id = self.scope_of_node(node)?;
        let scope = &self.scopes[id as usize];
        Some(Scope {
            id,
            kind: scope.kind,
            name: scope.name.clone(),
            node: scope.node,
            parent: scope.parent,
        })
    }

    pub fn resolve(&self, flat: &Flat, node: u32) -> Option<Binding> {
        if let Some(&binding) = self.node_binding.get(&node) {
            return Some(self.binding(binding));
        }
        let target = flat.nodes.get(node as usize)?;
        let name = flat.string(target.name)?;
        let scope = self.scope_of_node(node)?;
        let offset = if self.deferred.contains(&node) {
            UNSET
        } else {
            target.range.start.offset
        };
        self.resolve_name(name, scope, offset)
            .map(|resolved| self.output(resolved))
    }

    /// Whether `name` is bound in the function, lambda, class or module that
    /// holds `node` (a comprehension does not count), or in the module.
    pub fn is_frame_or_module_local(&self, node: u32, name: &str) -> bool {
        let mut id = self.scope_of_node(node);
        while let Some(scope) = id.map(|id| &self.scopes[id as usize]) {
            if !matches!(scope.kind, ScopeKind::Comprehension | ScopeKind::TypeParams) {
                return scope.names.contains_key(name) || self.scopes[0].names.contains_key(name);
            }
            id = scope.parent;
        }
        false
    }

    /// Whether `binding` is the only binding of its name in its scope.
    pub fn is_sole_binding(&self, binding: u32) -> bool {
        let Some(data) = self.bindings.get(binding as usize) else {
            return false;
        };
        self.scopes[data.scope as usize]
            .names
            .get(&data.name)
            .is_some_and(|ids| ids.len() == 1)
    }

    /// The bindings that `binding`'s scope holds for its name, in source order.
    pub fn same_name(&self, binding: u32) -> Vec<Binding> {
        let Some(data) = self.bindings.get(binding as usize) else {
            return Vec::new();
        };
        self.scopes[data.scope as usize]
            .names
            .get(&data.name)
            .into_iter()
            .flatten()
            .map(|&id| self.binding(id))
            .collect()
    }

    pub fn visible_bindings(&self, flat: &Flat, node: u32) -> Vec<Binding> {
        let (Some(scope), Some(target)) = (self.scope_of_node(node), flat.nodes.get(node as usize))
        else {
            return Vec::new();
        };
        let offset = if self.deferred.contains(&node) {
            UNSET
        } else {
            target.range.start.offset
        };
        let mut seen = HashSet::new();
        let mut visible = Vec::new();
        let mut current = Some(scope);
        while let Some(id) = current {
            let data = &self.scopes[id as usize];
            if id == scope || data.kind != ScopeKind::Class {
                for name in data.names.keys() {
                    if seen.insert(name.as_str())
                        && let Some(Resolved::Binding(binding)) =
                            self.resolve_name(name, scope, offset)
                    {
                        visible.push(self.binding(binding));
                    }
                }
            }
            current = data.parent;
        }
        visible.sort_by(|a, b| a.name.cmp(&b.name));
        visible
    }

    /// Python's lookup for `name` used at `offset` inside `scope`.
    fn resolve_name(&self, name: &str, scope: u32, mut offset: u32) -> Option<Resolved> {
        let mut current = scope;
        let mut deferred = false;
        loop {
            let data = &self.scopes[current as usize];
            // A comprehension evaluates its element after its `for` and `if`
            // clauses, so a use inside it follows all of its bindings.
            offset = offset.max(data.end);
            if data.globals.contains(name) && current != 0 {
                return self
                    .pick(0, name, offset, true, false)
                    .or_else(|| builtin(name));
            }
            let skip = data.nonlocals.contains(name)
                || (current != scope && data.kind == ScopeKind::Class);
            if !skip
                && let Some(binding) = self.pick(current, name, offset, deferred, current == scope)
            {
                return Some(binding);
            }
            if matches!(data.kind, ScopeKind::Function | ScopeKind::Lambda) {
                deferred = true;
            }
            match data.parent {
                Some(parent) => current = parent,
                None => return builtin(name),
            }
        }
    }

    /// The binding of `name` in `scope` a use at `offset` sees. A deferred
    /// use (from inside a nested function) sees the scope's last binding.
    fn pick(
        &self,
        scope: u32,
        name: &str,
        offset: u32,
        deferred: bool,
        own: bool,
    ) -> Option<Resolved> {
        let data = &self.scopes[scope as usize];
        let candidates: Vec<u32> = data
            .names
            .get(name)?
            .iter()
            .copied()
            .filter(|&id| {
                !matches!(
                    self.bindings[id as usize].kind,
                    BindingKind::Global | BindingKind::Nonlocal
                )
            })
            .collect();
        // A bare annotation (`x: int`) gives the name no value, so a use
        // sees the binding before it; it still makes a function's name local.
        let valued = |id: &u32| self.bindings[*id as usize].kind != BindingKind::Annotation;
        if deferred {
            let last = candidates.iter().copied().rfind(valued);
            return last
                .or_else(|| candidates.last().copied())
                .map(Resolved::Binding);
        }
        let before = candidates
            .iter()
            .copied()
            .filter(valued)
            .rfind(|&id| self.bindings[id as usize].effective <= offset);
        if before.is_some() {
            return before.map(Resolved::Binding);
        }
        // A function's names are local wherever they are bound in it.
        let local_scope = matches!(
            data.kind,
            ScopeKind::Function | ScopeKind::Lambda | ScopeKind::Comprehension
        );
        (own && local_scope)
            .then(|| candidates.first().copied())
            .flatten()
            .map(Resolved::Binding)
    }

    fn output(&self, resolved: Resolved) -> Binding {
        match resolved {
            Resolved::Binding(id) => self.binding(id),
            Resolved::Builtin(id) => Binding {
                id,
                name: builtins()[(id - BUILTIN_BASE) as usize].to_owned(),
                kind: BindingKind::Builtin,
                scope: 0,
                node: None,
                range: None,
                origin: None,
            },
        }
    }

    fn binding(&self, id: u32) -> Binding {
        let data = &self.bindings[id as usize];
        Binding {
            id,
            name: data.name.clone(),
            kind: data.kind,
            scope: data.scope,
            node: data.node,
            range: data.node.map(|_| data.range),
            origin: data.origin.clone(),
        }
    }
}

fn builtins() -> &'static [&'static str] {
    static BUILTINS: OnceLock<Vec<&'static str>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let mut names: Vec<&'static str> =
            ruff_python_stdlib::builtins::python_builtins(PYTHON_MINOR, false).collect();
        names.sort_unstable();
        names.dedup();
        names
    })
}

fn builtin(name: &str) -> Option<Resolved> {
    let index = builtins().binary_search(&name).ok()?;
    Some(Resolved::Builtin(BUILTIN_BASE + u32::try_from(index).ok()?))
}

fn position(position: ferrolint_core::Position) -> crate::wit::Position {
    crate::wit::Position {
        offset: position.offset,
        line: position.line,
        column: position.column,
    }
}

fn id_of(node: &impl HasNodeIndex) -> Option<u32> {
    node.node_index().load().as_u32()
}

struct Builder<'s> {
    source: &'s str,
    index: &'s LineIndex,
    model: Model,
    current: u32,
    store_kind: BindingKind,
    statement_end: u32,
    deferred_annotations: bool,
    in_annotation: bool,
}

impl Builder<'_> {
    fn note_deferred(&mut self, name: &ast::ExprName) {
        if self.in_annotation
            && let Some(id) = id_of(name)
        {
            self.model.deferred.insert(id);
        }
    }

    /// Visit an annotation; when annotations are deferred its names see
    /// the whole scope.
    fn visit_annotation(&mut self, annotation: &Expr) {
        let outer = std::mem::replace(&mut self.in_annotation, self.deferred_annotations);
        self.visit_expr(annotation);
        self.in_annotation = outer;
    }

    fn push_scope(&mut self, kind: ScopeKind, name: Option<String>, node: u32) -> u32 {
        let id = u32::try_from(self.model.scopes.len()).unwrap_or(UNSET);
        let parent = (!self.model.scopes.is_empty()).then_some(self.current);
        self.model.scopes.push(ScopeData {
            kind,
            name,
            node,
            parent,
            end: 0,
            names: BTreeMap::new(),
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
        });
        self.current = id;
        id
    }

    fn pop_scope(&mut self) {
        if let Some(parent) = self.model.scopes[self.current as usize].parent {
            self.current = parent;
        }
    }

    fn mark(&mut self, node: &impl HasNodeIndex) {
        if let Some(id) = id_of(node)
            && let Some(slot) = self.model.node_scope.get_mut(id as usize)
        {
            *slot = self.current;
        }
    }

    /// The scope a new binding of `name` lands in, following `global` and
    /// `nonlocal` declarations.
    fn target_scope(&self, name: &str, kind: BindingKind) -> u32 {
        let mut scope = self.current;
        if kind == BindingKind::NamedExpr {
            while self.model.scopes[scope as usize].kind == ScopeKind::Comprehension {
                scope = self.model.scopes[scope as usize].parent.unwrap_or(0);
            }
        }
        let data = &self.model.scopes[scope as usize];
        if matches!(kind, BindingKind::Global | BindingKind::Nonlocal) {
            return scope;
        }
        if data.globals.contains(name) {
            return 0;
        }
        if data.nonlocals.contains(name) {
            let mut outer = data.parent;
            while let Some(id) = outer {
                let candidate = &self.model.scopes[id as usize];
                if matches!(candidate.kind, ScopeKind::Function | ScopeKind::Lambda) {
                    return id;
                }
                outer = candidate.parent;
            }
        }
        scope
    }

    fn bind(
        &mut self,
        name: &str,
        kind: BindingKind,
        node: Option<u32>,
        range: TextRange,
        origin: Option<String>,
        effective: u32,
    ) {
        let scope = self.target_scope(name, kind);
        let id = u32::try_from(self.model.bindings.len()).unwrap_or(UNSET);
        let range = self
            .index
            .range(self.source, range.start().to_u32(), range.end().to_u32());
        self.model.bindings.push(BindingData {
            name: name.to_owned(),
            kind,
            scope,
            node,
            range: Range {
                start: position(range.start),
                end: position(range.end),
            },
            origin,
            effective,
        });
        self.model.scopes[scope as usize]
            .names
            .entry(name.to_owned())
            .or_default()
            .push(id);
        if let Some(node) = node {
            self.model.node_binding.entry(node).or_insert(id);
        }
    }

    fn with_store_kind(&mut self, kind: BindingKind, visit: impl FnOnce(&mut Self)) {
        let previous = std::mem::replace(&mut self.store_kind, kind);
        visit(self);
        self.store_kind = previous;
    }

    fn bind_parameters(&mut self, parameters: &ast::Parameters) {
        self.mark(parameters);
        for parameter in parameters {
            let parameter = parameter.as_parameter();
            self.mark(parameter);
            let range = parameter.name.range();
            self.bind(
                &parameter.name,
                BindingKind::Parameter,
                id_of(parameter),
                range,
                None,
                range.end().to_u32(),
            );
        }
    }

    /// Visit the parts of a parameter list the enclosing scope evaluates.
    fn visit_parameter_values(&mut self, parameters: &ast::Parameters) {
        for parameter in parameters {
            if let Some(default) = parameter.default() {
                self.visit_expr(default);
            }
            if let Some(annotation) = parameter.annotation() {
                self.visit_annotation(annotation);
            }
        }
    }

    fn visit_type_params_in_scope(
        &mut self,
        type_params: Option<&ast::TypeParams>,
        node: u32,
    ) -> bool {
        let Some(type_params) = type_params else {
            return false;
        };
        self.push_scope(ScopeKind::TypeParams, None, node);
        for param in &type_params.type_params {
            self.mark(param);
            let name = param.name();
            let range = name.range();
            self.bind(
                name,
                BindingKind::TypeParam,
                id_of(param),
                range,
                None,
                range.end().to_u32(),
            );
            visitor::walk_type_param(self, param);
        }
        true
    }

    fn visit_comprehension_scope(
        &mut self,
        node: u32,
        end: u32,
        generators: &[ast::Comprehension],
        elements: &[&Expr],
    ) {
        let Some((first, _)) = generators.split_first() else {
            return;
        };
        self.visit_expr(&first.iter);
        let scope = self.push_scope(ScopeKind::Comprehension, None, node);
        self.model.scopes[scope as usize].end = end;
        for (index, generator) in generators.iter().enumerate() {
            self.mark(generator);
            self.with_store_kind(BindingKind::ComprehensionVar, |this| {
                this.visit_expr(&generator.target);
            });
            if index > 0 {
                self.visit_expr(&generator.iter);
            }
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        for element in elements {
            self.visit_expr(element);
        }
        self.pop_scope();
    }

    fn visit_function(&mut self, def: &ast::StmtFunctionDef) {
        let id = id_of(def).unwrap_or(0);
        for decorator in &def.decorator_list {
            self.visit_decorator(decorator);
        }
        let range = def.name.range();
        self.bind(
            &def.name,
            BindingKind::FunctionDef,
            Some(id),
            range,
            None,
            range.end().to_u32(),
        );
        let typed = self.visit_type_params_in_scope(def.type_params.as_deref(), id);
        self.visit_parameter_values(&def.parameters);
        if let Some(returns) = &def.returns {
            self.visit_annotation(returns);
        }
        self.push_scope(ScopeKind::Function, Some(def.name.to_string()), id);
        self.bind_parameters(&def.parameters);
        self.visit_body(&def.body);
        self.pop_scope();
        if typed {
            self.pop_scope();
        }
    }

    fn visit_class(&mut self, def: &ast::StmtClassDef) {
        let id = id_of(def).unwrap_or(0);
        for decorator in &def.decorator_list {
            self.visit_decorator(decorator);
        }
        let range = def.name.range();
        self.bind(
            &def.name,
            BindingKind::ClassDef,
            Some(id),
            range,
            None,
            range.end().to_u32(),
        );
        // PEP 695 type parameters are visible in the bases and the body.
        let typed = self.visit_type_params_in_scope(def.type_params.as_deref(), id);
        if let Some(arguments) = &def.arguments {
            self.visit_arguments(arguments);
        }
        self.push_scope(ScopeKind::Class, Some(def.name.to_string()), id);
        self.visit_body(&def.body);
        self.pop_scope();
        if typed {
            self.pop_scope();
        }
    }

    fn visit_import(&mut self, aliases: &[ast::Alias], module: Option<&str>) {
        for alias in aliases {
            self.mark(alias);
            let node = id_of(alias);
            let end = alias.range().end().to_u32();
            match module {
                None => {
                    let dotted = alias.name.as_str();
                    let (name, range) = match &alias.asname {
                        Some(asname) => (asname.as_str(), asname.range()),
                        None => (
                            dotted.split('.').next().unwrap_or(dotted),
                            alias.name.range(),
                        ),
                    };
                    self.bind(
                        name,
                        BindingKind::Import,
                        node,
                        range,
                        Some(dotted.to_owned()),
                        end,
                    );
                }
                Some(module) if alias.name.as_str() == "*" => {
                    self.bind(
                        "*",
                        BindingKind::StarImport,
                        node,
                        alias.range(),
                        Some(module.to_owned()),
                        end,
                    );
                }
                Some(module) => {
                    let (name, range) = match &alias.asname {
                        Some(asname) => (asname.as_str(), asname.range()),
                        None => (alias.name.as_str(), alias.name.range()),
                    };
                    let separator = if module.ends_with('.') || module.is_empty() {
                        ""
                    } else {
                        "."
                    };
                    let origin = format!("{module}{separator}{}", alias.name);
                    self.bind(
                        name,
                        BindingKind::FromImport,
                        node,
                        range,
                        Some(origin),
                        end,
                    );
                }
            }
        }
    }

    fn declare(&mut self, names: &[ast::Identifier], kind: BindingKind) {
        for name in names {
            let scope = &mut self.model.scopes[self.current as usize];
            if kind == BindingKind::Global {
                scope.globals.insert(name.to_string());
            } else {
                scope.nonlocals.insert(name.to_string());
            }
            let range = name.range();
            self.bind(name, kind, id_of(name), range, None, range.end().to_u32());
        }
    }
}

impl<'a> Visitor<'a> for Builder<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        self.mark(stmt);
        let outer_end = std::mem::replace(&mut self.statement_end, stmt.range().end().to_u32());
        match stmt {
            Stmt::FunctionDef(def) => self.visit_function(def),
            Stmt::ClassDef(def) => self.visit_class(def),
            Stmt::Import(import) => self.visit_import(&import.names, None),
            Stmt::ImportFrom(import) => {
                let module = format!(
                    "{}{}",
                    ".".repeat(import.level as usize),
                    import.module.as_ref().map_or("", |module| module.as_str())
                );
                self.visit_import(&import.names, Some(&module));
            }
            Stmt::TypeAlias(alias) => {
                self.visit_expr(&alias.name);
                let id = id_of(alias).unwrap_or(0);
                let typed = self.visit_type_params_in_scope(alias.type_params.as_deref(), id);
                self.visit_expr(&alias.value);
                if typed {
                    self.pop_scope();
                }
            }
            Stmt::Global(global) => self.declare(&global.names, BindingKind::Global),
            Stmt::Nonlocal(nonlocal) => self.declare(&nonlocal.names, BindingKind::Nonlocal),
            Stmt::AnnAssign(assign) => {
                self.visit_annotation(&assign.annotation);
                if let Some(value) = &assign.value {
                    self.visit_expr(value);
                }
                let kind = if assign.value.is_some() {
                    BindingKind::Assignment
                } else {
                    BindingKind::Annotation
                };
                self.with_store_kind(kind, |this| this.visit_expr(&assign.target));
            }
            Stmt::For(stmt) => {
                self.visit_expr(&stmt.iter);
                self.statement_end = stmt.target.range().end().to_u32();
                self.with_store_kind(BindingKind::LoopVar, |this| this.visit_expr(&stmt.target));
                self.visit_body(&stmt.body);
                self.visit_body(&stmt.orelse);
            }
            Stmt::With(stmt) => {
                for item in &stmt.items {
                    self.mark(item);
                    self.visit_expr(&item.context_expr);
                    if let Some(vars) = &item.optional_vars {
                        self.statement_end = vars.range().end().to_u32();
                        self.with_store_kind(BindingKind::WithItem, |this| this.visit_expr(vars));
                    }
                }
                self.visit_body(&stmt.body);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
        self.statement_end = outer_end;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        self.mark(expr);
        match expr {
            Expr::Name(name) => match name.ctx {
                Ctx::Store => {
                    // A comprehension variable exists from its own target on,
                    // not from the end of the statement around it.
                    let effective = if self.store_kind == BindingKind::ComprehensionVar {
                        name.range.end().to_u32()
                    } else {
                        self.statement_end.max(name.range.end().to_u32())
                    };
                    self.bind(
                        &name.id,
                        self.store_kind,
                        id_of(name),
                        name.range,
                        None,
                        effective,
                    );
                }
                Ctx::Del => {
                    let end = name.range.end().to_u32();
                    self.bind(
                        &name.id,
                        BindingKind::Deletion,
                        id_of(name),
                        name.range,
                        None,
                        end,
                    );
                }
                Ctx::Load | Ctx::Invalid => self.note_deferred(name),
            },
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = named.target.as_ref() {
                    self.mark(target);
                    let end = named.range.end().to_u32();
                    self.bind(
                        &target.id,
                        BindingKind::NamedExpr,
                        id_of(target),
                        target.range,
                        None,
                        end,
                    );
                }
            }
            Expr::Lambda(lambda) => {
                let id = id_of(lambda).unwrap_or(0);
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameter_values(parameters);
                }
                self.push_scope(ScopeKind::Lambda, None, id);
                if let Some(parameters) = &lambda.parameters {
                    self.bind_parameters(parameters);
                }
                self.visit_expr(&lambda.body);
                self.pop_scope();
            }
            Expr::ListComp(comp) => {
                self.visit_comprehension_scope(
                    id_of(comp).unwrap_or(0),
                    comp.range.end().to_u32(),
                    &comp.generators,
                    &[&comp.elt],
                );
            }
            Expr::SetComp(comp) => {
                self.visit_comprehension_scope(
                    id_of(comp).unwrap_or(0),
                    comp.range.end().to_u32(),
                    &comp.generators,
                    &[&comp.elt],
                );
            }
            Expr::Generator(comp) => {
                self.visit_comprehension_scope(
                    id_of(comp).unwrap_or(0),
                    comp.range.end().to_u32(),
                    &comp.generators,
                    &[&comp.elt],
                );
            }
            Expr::DictComp(comp) => {
                let elements: Vec<&Expr> = comp
                    .key
                    .as_deref()
                    .into_iter()
                    .chain([comp.value.as_ref()])
                    .collect();
                self.visit_comprehension_scope(
                    id_of(comp).unwrap_or(0),
                    comp.range.end().to_u32(),
                    &comp.generators,
                    &elements,
                );
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ast::ExceptHandler) {
        let ast::ExceptHandler::ExceptHandler(inner) = handler;
        self.mark(inner);
        if let Some(type_) = &inner.type_ {
            self.visit_expr(type_);
        }
        if let Some(name) = &inner.name {
            let range = name.range();
            self.bind(
                name,
                BindingKind::ExceptHandler,
                id_of(inner),
                range,
                None,
                range.end().to_u32(),
            );
        }
        self.visit_body(&inner.body);
    }

    fn visit_pattern(&mut self, pattern: &'a ast::Pattern) {
        self.mark(pattern);
        let name = match pattern {
            ast::Pattern::MatchAs(pattern) => pattern.name.as_ref(),
            ast::Pattern::MatchStar(pattern) => pattern.name.as_ref(),
            ast::Pattern::MatchMapping(pattern) => pattern.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            let range = name.range();
            self.bind(
                name,
                BindingKind::MatchCapture,
                id_of(pattern),
                range,
                None,
                range.end().to_u32(),
            );
        }
        visitor::walk_pattern(self, pattern);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::flatten;
    use ferrolint_core::LineIndex;

    struct Fixture {
        source: &'static str,
        flat: Flat,
        model: Model,
    }

    fn fixture(source: &'static str) -> Fixture {
        fixture_for(source, None)
    }

    fn fixture_for(source: &'static str, py_version: Option<(u32, u32)>) -> Fixture {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = LineIndex::new(source);
        let flat = flatten(source, &index, parsed.syntax());
        let model = Model::build(parsed.syntax(), &flat.nodes, source, &index, py_version);
        Fixture {
            source,
            flat,
            model,
        }
    }

    impl Fixture {
        /// The name load nodes spelled `name`, in source order.
        fn loads(&self, name: &str) -> Vec<u32> {
            self.flat
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| {
                    node.kind == crate::wit::NodeKind::Name
                        && self.flat.string(node.name) == Some(name)
                        && node.context == crate::wit::ExprContext::Load
                })
                .map(|(id, _)| u32::try_from(id).unwrap())
                .collect()
        }

        /// What the `nth` load of `name` resolves to: kind and line.
        fn resolve(&self, name: &str, nth: usize) -> Option<(BindingKind, u32)> {
            let node = self.loads(name)[nth];
            self.model.resolve(&self.flat, node).map(|binding| {
                (
                    binding.kind,
                    binding.range.map_or(0, |range| range.start.line),
                )
            })
        }
    }

    #[test]
    fn locals_shadow_globals_and_imports_resolve_to_their_origin() {
        let f = fixture(
            "counter = 0\n\ndef local():\n    counter = 1\n    return counter\n\ndef outer():\n    return counter\n\ndef importer():\n    import json.decoder as jd\n    return jd\n",
        );
        assert_eq!(f.resolve("counter", 0), Some((BindingKind::Assignment, 4)));
        assert_eq!(f.resolve("counter", 1), Some((BindingKind::Assignment, 1)));
        let jd = f.model.resolve(&f.flat, f.loads("jd")[0]).unwrap();
        assert_eq!(jd.kind, BindingKind::Import);
        assert_eq!(jd.origin.as_deref(), Some("json.decoder"));
        let _ = f.source;
    }

    #[test]
    fn class_scopes_are_invisible_to_nested_functions() {
        let f =
            fixture("x = 1\nclass C:\n    x = 2\n    y = x\n    def m(self):\n        return x\n");
        assert_eq!(f.resolve("x", 0), Some((BindingKind::Assignment, 3)));
        assert_eq!(f.resolve("x", 1), Some((BindingKind::Assignment, 1)));
    }

    #[test]
    fn global_declarations_redirect_bindings() {
        let f = fixture("def f():\n    global g\n    g = 1\n\ndef h():\n    return g\n");
        let binding = f.model.resolve(&f.flat, f.loads("g")[0]).unwrap();
        assert_eq!(binding.kind, BindingKind::Assignment);
        assert_eq!(binding.scope, 0);
    }

    #[test]
    fn a_right_hand_side_sees_the_previous_binding() {
        let f = fixture("x = 1\nx = x + 1\nprint(x)\n");
        assert_eq!(f.resolve("x", 0), Some((BindingKind::Assignment, 1)));
        assert_eq!(f.resolve("x", 1), Some((BindingKind::Assignment, 2)));
        let print = f.model.resolve(&f.flat, f.loads("print")[0]).unwrap();
        assert_eq!(print.kind, BindingKind::Builtin);
    }

    #[test]
    fn comprehension_variables_stay_inside() {
        let f = fixture("x = 0\nys = [x for x in range(3)]\nz = x\n");
        assert_eq!(f.resolve("x", 0), Some((BindingKind::ComprehensionVar, 2)));
        assert_eq!(f.resolve("x", 1), Some((BindingKind::Assignment, 1)));
    }

    #[test]
    fn a_comprehension_element_sees_a_walrus_in_its_condition() {
        let f = fixture("xs = []\nvs = {v for r in xs if (v := r)}\n");
        assert_eq!(f.resolve("v", 0), Some((BindingKind::NamedExpr, 2)));
    }

    #[test]
    fn a_nested_comprehension_sees_the_outer_variable() {
        let f = fixture("y = z = []\nws = [[i] + [i for _ in y] for i, v in enumerate(z)]\n");
        assert_eq!(f.resolve("i", 0), Some((BindingKind::ComprehensionVar, 2)));
        assert_eq!(f.resolve("i", 1), Some((BindingKind::ComprehensionVar, 2)));
    }

    #[test]
    fn a_comprehension_inside_a_container_sees_the_outer_variable() {
        let f = fixture("d = []\nx = {'r': [[i for _ in d] for i in d]}\n");
        assert_eq!(f.resolve("i", 0), Some((BindingKind::ComprehensionVar, 2)));
    }

    #[test]
    fn a_use_before_the_comprehension_binding_stays_undefined() {
        let f = fixture("xs = []\nu = v\nws = [v for r in xs if (v := r)]\n");
        assert_eq!(f.resolve("v", 0), None);
        assert_eq!(f.resolve("v", 1), Some((BindingKind::NamedExpr, 3)));
    }

    #[test]
    fn decorated_definitions_report_the_name_position() {
        let f =
            fixture("import functools\n\n@functools.cache\ndef handler():\n    return handler\n");
        let binding = f.model.resolve(&f.flat, f.loads("handler")[0]).unwrap();
        let start = binding.range.unwrap().start;
        assert_eq!((start.line, start.column), (4, 5));
    }

    #[test]
    fn type_parameters_reach_the_class_body_and_aliases() {
        let f = fixture(
            "class Box[T]:\n    item: T\n    def get(self) -> T: ...\n\ntype Pairs[K] = dict[K, int]\n",
        );
        assert_eq!(f.resolve("T", 0), Some((BindingKind::TypeParam, 1)));
        assert_eq!(f.resolve("T", 1), Some((BindingKind::TypeParam, 1)));
        assert_eq!(f.resolve("K", 0), Some((BindingKind::TypeParam, 5)));
    }

    const LATER: &str = "def f(a: Later) -> Later: ...\nclass C:\n    x: Later\n    def m(self) -> Later: ...\nclass Later: ...\nv = Later\n";

    #[test]
    fn deferred_annotations_see_later_definitions() {
        let future = fixture(Box::leak(
            format!("from __future__ import annotations\n{LATER}").into_boxed_str(),
        ));
        let py314 = fixture_for(LATER, Some((3, 14)));
        for f in [future, py314] {
            for nth in 0..4 {
                assert_eq!(
                    f.resolve("Later", nth).map(|b| b.0),
                    Some(BindingKind::ClassDef)
                );
            }
        }
    }

    #[test]
    fn eager_annotations_and_plain_uses_keep_their_order() {
        for f in [fixture(LATER), fixture_for(LATER, Some((3, 13)))] {
            for nth in 0..4 {
                assert_eq!(f.resolve("Later", nth), None);
            }
        }
        // A use outside an annotation still sees only earlier bindings.
        let f = fixture_for("print(Later)\nclass Later: ...\n", Some((3, 14)));
        assert_eq!(f.resolve("Later", 0), None);
    }

    #[test]
    fn a_bare_annotation_does_not_hide_the_value() {
        let f = fixture("x = 1\nx: int\nprint(x)\n");
        assert_eq!(f.resolve("x", 0), Some((BindingKind::Assignment, 1)));
    }

    #[test]
    fn visible_bindings_leave_out_shadowed_names() {
        let f = fixture("a = 1\nb = 2\ndef f(a):\n    return a + b\n");
        let node = f.loads("b")[0];
        let visible = f.model.visible_bindings(&f.flat, node);
        let summary: Vec<(String, BindingKind)> = visible
            .into_iter()
            .map(|binding| (binding.name, binding.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("a".into(), BindingKind::Parameter),
                ("b".into(), BindingKind::Assignment),
                ("f".into(), BindingKind::FunctionDef),
            ]
        );
    }
}
