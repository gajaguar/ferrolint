//! What ty infers for the expression at a range, for the native rules and the
//! plugins that give up on a value they cannot follow.
//!
//! Outside its crate ty names little of a type: the variant, the name of a
//! class (`type_hierarchy_prepare`) and its display. So an answer is the kind
//! of value and, for a class, its name; never the text of a display.

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{Expr, ExprRef};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::ProgramFile;
use ty_python_semantic::types::Type;
use ty_python_semantic::{Db, HasType, ProgramEnvironment, SemanticModel, type_hierarchy_prepare};

use super::criterion;
use crate::native::{TyValue, TypeAt};

/// The values of one file.
pub struct Values<'db> {
    db: &'db dyn Db,
    model: SemanticModel<'db>,
    env: ProgramEnvironment<'db>,
    module: ParsedModuleRef,
}

impl<'db> Values<'db> {
    pub fn new(db: &'db dyn Db, file: ProgramFile<'db>) -> Self {
        let model = SemanticModel::new(db, file);
        let env = model.program_environment();
        let module = parsed_module(db, file.python_file(db)).load(db);
        Self {
            db,
            model,
            env,
            module,
        }
    }
}

impl<'db> Values<'db> {
    /// The type of the expression at `range`, when it is one concrete type.
    fn single_type(&self, range: TextRange) -> Option<Type<'db>> {
        let mut find = Find {
            target: range,
            found: None,
        };
        find.visit_body(self.module.suite());
        let ty = ExprRef::from(find.found?).inferred_type(&self.model)?;
        criterion::single(self.db, &self.env, ty).then_some(ty)
    }
}

impl TypeAt for Values<'_> {
    fn local_class(&self, range: TextRange) -> Option<TextRange> {
        let ty = self.single_type(range)?;
        if !matches!(ty, Type::ClassLiteral(_) | Type::SubclassOf(_)) {
            return None;
        }
        let class = type_hierarchy_prepare(self.db, &self.env, ty)?;
        (class.file.file(self.db) == self.model.file()).then_some(class.selection_range)
    }

    fn value(&self, range: TextRange) -> Option<TyValue> {
        let ty = self.single_type(range)?;
        match ty {
            // `self.Task` is `type[Task]`, the class reached through an instance.
            Type::ClassLiteral(_) | Type::SubclassOf(_) => {
                type_hierarchy_prepare(self.db, &self.env, ty)
                    .map(|class| TyValue::Class(class.name.to_string()))
            }
            Type::FunctionLiteral(_) => Some(TyValue::Function),
            Type::BoundMethod(_) => Some(TyValue::Method),
            Type::ModuleLiteral(_) => Some(TyValue::Module),
            Type::LiteralValue(_) => Some(TyValue::Literal),
            _ => None,
        }
    }
}

/// The expression that spans exactly `target`.
// shortcut: walks the module for each query; index the expressions by range if a rule asks a lot.
struct Find<'a> {
    target: TextRange,
    found: Option<&'a Expr>,
}

impl<'a> SourceOrderVisitor<'a> for Find<'a> {
    fn enter_node(
        &mut self,
        node: ruff_python_ast::AnyNodeRef<'a>,
    ) -> source_order::TraversalSignal {
        if self.found.is_some() || !node.range().contains_range(self.target) {
            return source_order::TraversalSignal::Skip;
        }
        source_order::TraversalSignal::Traverse
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.range() == self.target {
            self.found.get_or_insert(expr);
        } else {
            source_order::walk_expr(self, expr);
        }
    }
}
