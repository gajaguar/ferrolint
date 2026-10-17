//! `--diff-base`: the files that changed between the merge base of a ref
//! and the working tree, asked of git.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

/// The absolute paths of every file added, copied, modified or renamed
/// since the merge base of `reference` and `HEAD`, plus untracked files git
/// does not ignore.
pub fn changed_files(cwd: &Path, reference: &str) -> Result<BTreeSet<PathBuf>> {
    let fail = |what: &str| Error::usage(format!("--diff-base {reference}: {what}"));
    let root = git(cwd, &["rev-parse", "--show-toplevel"])
        .map_err(|err| fail(&format!("not inside a git repository ({err})")))?;
    let root = PathBuf::from(root.trim_end());
    let commit = git(
        cwd,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .map_err(|_| fail("does not resolve to a commit"))?;
    let base = git(cwd, &["merge-base", commit.trim_end(), "HEAD"])
        .map_err(|err| fail(&format!("has no merge base with HEAD ({err})")))?;
    let changed = git(
        &root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
            base.trim_end(),
        ],
    )
    .map_err(|err| fail(&err))?;
    let untracked = git(&root, &["ls-files", "--others", "--exclude-standard", "-z"])
        .map_err(|err| fail(&err))?;
    Ok(changed
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|path| !path.is_empty())
        .map(|path| root.join(path).components().collect())
        .collect())
}

/// Run git in `dir` and return its standard output, or its error output.
fn git(dir: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let output = Command::new("git")
        .args(["-c", "core.quotePath=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|err| format!("cannot run git: {err}"))?;
    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|err| err.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
