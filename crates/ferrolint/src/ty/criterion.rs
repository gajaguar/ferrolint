//! Pylint's criterion over ty's types. ty reports far more than Pylint
//! (513 unsubscriptable values against 11 on one codebase), so a rule keeps a
//! diagnostic only when the operand has a single, concrete type: Pylint's
//! `safe_infer` is silent on a union, on what it cannot infer and on what it
//! only knows by declaration.

use ty_python_semantic::types::{KnownClass, Type};
use ty_python_semantic::{Db, ProgramEnvironment};

/// Whether `ty` is one concrete type: not a union, an intersection, a
/// dynamic or recursive type, a type variable, a special form such as
/// `Annotated[...]`, nor `object`, which stands for "anything".
pub fn single<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> bool {
    !matches!(
        ty,
        Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Recursive(_)
            | Type::RecursiveVar(_)
            | Type::Never
            | Type::Union(_)
            | Type::Intersection(_)
            | Type::EnumComplement(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeVar(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::TypeAlias(_)
    ) && ty != KnownClass::Object.to_instance(db, env)
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast::{ExprRef, Stmt};
    use ty_project::Db as _;
    use ty_python_semantic::{HasType, SemanticModel};

    use super::*;
    use crate::ty::Types;

    // One expression statement per name, in order.
    const SOURCE: &str = "\
from typing import Annotated
Alias = Annotated[int, 1]


def f[T](only_none: None, plain: int, int_or_none: int | None, int_or_str: int | str,
         unknown, missing: Missing, anything: object, var: T):
    only_none
    plain
    int_or_none
    int_or_str
    unknown
    missing
    anything
    var
    Alias
";

    #[test]
    fn only_a_concrete_single_type_counts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.py"), SOURCE).unwrap();
        let db = Types::new(dir.path()).unwrap().database();
        let path = ruff_db::system::SystemPathBuf::from_path_buf(
            std::path::absolute(dir.path().join("f.py")).unwrap(),
        )
        .unwrap();
        let file = system_path_to_file(&db, &path).unwrap();
        let program_file = db.project().program(&db).program_file(&db, file);
        let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
        let model = SemanticModel::new(&db, program_file);
        let env = model.program_environment();
        let Stmt::FunctionDef(function) = &module.syntax().body[2] else {
            unreachable!("the source ends with a function");
        };
        let verdicts: Vec<bool> = function
            .body
            .iter()
            .map(|stmt| {
                let Stmt::Expr(statement) = stmt else {
                    unreachable!("the body is expression statements");
                };
                let ty = ExprRef::from(&*statement.value)
                    .inferred_type(&model)
                    .unwrap();
                single(&db, &env, ty)
            })
            .collect();
        // None, int, int|None, int|str, unknown, unresolved, object, T, Alias
        assert_eq!(
            verdicts,
            [true, true, false, false, false, false, false, false, false]
        );
    }
}
