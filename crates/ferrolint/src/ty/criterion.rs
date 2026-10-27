//! Pylint's criterion over ty's types. ty reports far more than Pylint
//! (513 unsubscriptable values against 11 on one codebase), so a rule asks
//! this module, not ty, whether a value fails.

use ty_python_semantic::Db;
use ty_python_semantic::types::Type;

/// What a `None` member of a union counts for; each rule picks its own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoneMember {
    /// Pylint skips it: `x[0]` with `x: list | None` is not reported.
    Ignore,
    /// It is a member like any other, and fails the test.
    Count,
}

/// Whether the value fails `test`, as Pylint would report it.
///
/// An unknown or divergent type is silent. A union fails only when every
/// member does, once a `None` is set aside under [`NoneMember::Ignore`]; a
/// union that was only `None` is silent. `test` sees the members that are
/// neither dynamic, nor `None`, nor a union.
pub fn fails<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    none: NoneMember,
    test: &dyn Fn(Type<'db>) -> bool,
) -> bool {
    match ty {
        Type::Dynamic(_) | Type::Divergent(_) => false,
        Type::Union(union) => {
            let members: Vec<_> = union
                .elements(db)
                .iter()
                .copied()
                .filter(|member| none == NoneMember::Count || !member.is_none(db))
                .collect();
            !members.is_empty() && members.iter().all(|&m| fails(db, m, none, test))
        }
        _ => (none == NoneMember::Count || !ty.is_none(db)) && test(ty),
    }
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

    // One expression statement per parameter, in order; the test treats
    // `int`, `str` and `None` as the values that fail.
    const SOURCE: &str = "\
def f(only_none: None, list_or_none: list | None, int_or_none: int | None,
      int_or_str: int | str, int_or_list: int | list, unknown, missing: Missing):
    only_none
    list_or_none
    int_or_none
    int_or_str
    int_or_list
    unknown
    missing
";

    fn verdicts(none: NoneMember) -> Vec<bool> {
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
        let Stmt::FunctionDef(function) = &module.syntax().body[0] else {
            unreachable!("the source starts with a function");
        };
        let test = |ty: Type<'_>| {
            matches!(
                ty.display(&db, &env).to_string().as_str(),
                "int" | "str" | "None"
            )
        };
        function
            .body
            .iter()
            .map(|stmt| {
                let Stmt::Expr(statement) = stmt else {
                    unreachable!("the body is expression statements");
                };
                let ty = ExprRef::from(&*statement.value)
                    .inferred_type(&model)
                    .unwrap();
                fails(&db, ty, none, &test)
            })
            .collect()
    }

    #[test]
    fn none_is_skipped_when_ignored() {
        // only None, list|None, int|None, int|str, int|list, unknown, unresolved
        assert_eq!(
            verdicts(NoneMember::Ignore),
            [false, false, true, true, false, false, false]
        );
    }

    #[test]
    fn none_is_a_member_when_counted() {
        assert_eq!(
            verdicts(NoneMember::Count),
            [true, false, true, true, false, false, false]
        );
    }
}
