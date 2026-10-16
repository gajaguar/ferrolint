//! Helpers the acceptance tests share: a scratch project directory and a
//! way to run the built binary inside it.

// Each acceptance test uses some of these helpers, and a helper that
// panics is how a test fails, as `clippy.toml` allows inside `#[test]`.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use tempfile::TempDir;

/// A Python file with one bare `# noqa`, which the host reports as FL002.
pub const BARE_NOQA: &str = "x = 1  # noqa\n";
/// A Python file that does not parse, which the host reports as FL003.
pub const SYNTAX_ERROR: &str = "def broken(:\n    pass\n";
/// A Python file with no findings.
pub const CLEAN: &str = "def greet(name):\n    return f\"hello {name}\"\n";

/// The lowercase hex SHA-256 of `bytes`, as a plugin entry pins it.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    use std::fmt::Write;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            write!(out, "{byte:02x}").unwrap();
            out
        })
}

/// The workspace's plugin packages the tests load.
const PLUGIN_PACKAGES: &[&str] = &[
    "ferrolint-test-probe",
    "ferrolint-test-sandbox",
    "ferrolint-test-runaway",
];

/// The directory holding the plugins built for `wasm32-wasip2`, building
/// them once per test process. A separate target directory keeps this
/// build from waiting on the lock of the build running the tests.
fn plugin_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = workspace.join("target/test-plugins");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut command = Command::new(cargo);
        command
            .args([
                "build",
                "--locked",
                "--release",
                "--target",
                "wasm32-wasip2",
            ])
            .arg("--target-dir")
            .arg(&target)
            .current_dir(&workspace);
        for package in PLUGIN_PACKAGES {
            command.args(["-p", package]);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "building the test plugins failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        target.join("wasm32-wasip2/release")
    })
}

/// The built component of the plugin package `name`.
pub fn plugin_bytes(name: &str) -> Vec<u8> {
    let file = format!("{}.wasm", name.replace('-', "_"));
    fs::read(plugin_dir().join(file)).unwrap()
}

/// The outcome of one run of the binary.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// The report lines that are findings, not the summary.
    pub fn finding_lines(&self) -> Vec<&str> {
        let lines: Vec<&str> = self.stdout.lines().collect();
        lines[..lines.len().saturating_sub(1)].to_vec()
    }

    pub fn summary(&self) -> &str {
        self.stdout.lines().last().unwrap_or_default()
    }
}

/// A scratch directory a test lays files out in.
pub struct Project {
    dir: TempDir,
}

impl Project {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".home")).unwrap();
        Self { dir }
    }

    /// A project that is also a git repository, so `.gitignore` applies.
    pub fn git() -> Self {
        let project = Self::new();
        project.git_cmd(&["init", "-q", "-b", "main"]);
        project.write(".gitignore", ".home/\n");
        project
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.path().join(relative)
    }

    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> &Self {
        let path = self.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        self
    }

    /// Copy the built plugin `package` into the project as `file` and
    /// return its `[[plugins]]` table.
    pub fn plugin(&self, package: &str, file: &str) -> String {
        let bytes = plugin_bytes(package);
        let digest = sha256(&bytes);
        self.write(file, bytes);
        format!("[[plugins]]\npath = \"{file}\"\nsha256 = \"{digest}\"\n")
    }

    pub fn remove(&self, relative: &str) -> &Self {
        fs::remove_file(self.join(relative)).unwrap();
        self
    }

    /// Run git in the project with a fixed identity and no user config.
    pub fn git_cmd(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=ferrolint",
                "-c",
                "user.email=ferrolint@example.com",
            ])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(self.path())
            .env("HOME", self.join(".home"))
            .env("XDG_CONFIG_HOME", self.join(".home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    /// Commit everything in the work tree.
    pub fn commit(&self, message: &str) {
        self.git_cmd(&["add", "-A"]);
        self.git_cmd(&["commit", "-q", "-m", message]);
    }

    /// Run the linter in the project directory.
    pub fn run(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_ferrolint"))
            .args(args)
            .current_dir(self.path())
            .env("HOME", self.join(".home"))
            .env("XDG_CONFIG_HOME", self.join(".home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        Run {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
        }
    }
}
