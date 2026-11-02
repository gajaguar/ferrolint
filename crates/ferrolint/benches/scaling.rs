//! How a run's time grows with the number of files: a cold run (no cache)
//! and a warm one (cache filled), over trees of 50 to 400 files. A cold run
//! should grow linearly; a bend upward is a quadratic step somewhere.

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

const SIZES: [u64; 4] = [50, 100, 200, 400];

fn module(index: u64) -> String {
    let mut source = String::from("import os\n\n");
    for n in 0..20 {
        let _ = write!(
            source,
            "def f{index}_{n}(a, b):\n    value = a + b\n    return value\n\n"
        );
    }
    source
}

fn tree(files: u64) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("temporary directory: {err}"));
    for index in 0..files {
        std::fs::write(dir.path().join(format!("m{index}.py")), module(index))
            .unwrap_or_else(|err| panic!("source file: {err}"));
    }
    dir
}

fn run(dir: &Path, extra: &[&str]) {
    // Findings make the exit code non-zero; only the time matters here.
    Command::new(env!("CARGO_BIN_EXE_ferrolint"))
        .arg(".")
        .args(extra)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|err| panic!("the ferrolint binary: {err}"));
}

fn scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("run");
    group.sample_size(10);
    for files in SIZES {
        let dir = tree(files);
        group.throughput(Throughput::Elements(files));
        group.bench_with_input(BenchmarkId::new("cold", files), &files, |b, _| {
            b.iter(|| run(dir.path(), &["--no-cache"]));
        });
        run(dir.path(), &[]);
        group.bench_with_input(BenchmarkId::new("warm", files), &files, |b, _| {
            b.iter(|| run(dir.path(), &[]));
        });
    }
    group.finish();
}

criterion_group!(benches, scaling);
criterion_main!(benches);
