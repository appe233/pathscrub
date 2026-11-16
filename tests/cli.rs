//! End-to-end tests: run the real binary against a scratch directory tree.

use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_pathscrub");

/// A unique scratch directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pathscrub-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    /// Create a directory and return its path as a string.
    fn dir(&self, name: &str) -> String {
        let p = self.0.join(name);
        fs::create_dir_all(&p).unwrap();
        s(&p)
    }

    fn path(&self, name: &str) -> String {
        s(&self.0.join(name))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn run_with(args: &[&str], f: impl FnOnce(&mut Command)) -> Output {
    let mut c = Command::new(BIN);
    c.args(args);
    f(&mut c);
    c.output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

fn scrub(value: &str) -> String {
    let o = run(&["--value", value]);
    assert!(o.status.success(), "stderr: {}", stderr(&o));
    stdout(&o)
}

#[test]
fn keeps_valid_entries_in_order() {
    let t = Scratch::new();
    let (a, b, c) = (t.dir("a"), t.dir("b"), t.dir("c"));
    assert_eq!(scrub(&format!("{c}:{a}:{b}")), format!("{c}:{a}:{b}\n"));
}

#[test]
fn drops_empty_entries() {
    let t = Scratch::new();
    let a = t.dir("a");
    assert_eq!(scrub(&format!(":{a}::{a}:")), format!("{a}\n"));
}

#[test]
fn drops_relative_entries() {
    let t = Scratch::new();
    let a = t.dir("a");
    let o = run(&["-v", "--value", &format!(".:bin:~/x:$HOME/y:{a}")]);
    assert_eq!(stdout(&o), format!("{a}\n"));
    let err = stderr(&o);
    assert!(err.contains("\"bin\": relative path\n"));
    assert!(err.contains("\"~/x\": relative path (unexpanded"));
    assert!(err.contains("\"$HOME/y\": relative path (unexpanded"));
}

#[test]
fn drops_missing_and_non_directories() {
    let t = Scratch::new();
    let a = t.dir("a");
    let file = t.path("file");
    fs::write(&file, "").unwrap();
    let missing = t.path("missing");
    let o = run(&["-v", "--value", &format!("{missing}:{file}:{a}")]);
    assert_eq!(stdout(&o), format!("{a}\n"));
    assert!(stderr(&o).contains("not found"));
    assert!(stderr(&o).contains("not a directory"));
}

#[test]
fn trailing_slashes_are_duplicates_and_stripped() {
    let t = Scratch::new();
    let a = t.dir("a");
    assert_eq!(scrub(&format!("{a}/:{a}//:{a}")), format!("{a}\n"));
    assert_eq!(scrub("/"), "/\n");
}

#[test]
fn symlinks_are_duplicates_first_wins() {
    let t = Scratch::new();
    let real = t.dir("real");
    let link = t.path("link");
    symlink(&real, &link).unwrap();
    assert_eq!(scrub(&format!("{link}:{real}")), format!("{link}\n"));
    assert_eq!(scrub(&format!("{real}:{link}")), format!("{real}\n"));
}

#[test]
fn canonical_prints_resolved_paths() {
    let t = Scratch::new();
    let real = t.dir("real");
    let link = t.path("link");
    symlink(&real, &link).unwrap();
    let resolved = s(&fs::canonicalize(&real).unwrap());
    let o = run(&["--canonical", "--value", &format!("{link}:{real}")]);
    assert_eq!(stdout(&o), format!("{resolved}\n"));
}

#[test]
fn keep_missing() {
    let t = Scratch::new();
    let a = t.dir("a");
    let m = t.path("missing");
    let o = run(&["--keep-missing", "--value", &format!("{m}:{a}:{m}/:rel")]);
    assert_eq!(stdout(&o), format!("{m}:{a}\n"));
}

#[test]
fn fish_output_one_per_line() {
    let t = Scratch::new();
    let (a, b) = (t.dir("a"), t.dir("b"));
    let o = run(&["--fish", "--value", &format!("{a}:{b}:{a}")]);
    assert_eq!(stdout(&o), format!("{a}\n{b}\n"));
}

#[test]
fn fish_drops_newline_entries() {
    let t = Scratch::new();
    let a = t.dir("a");
    let nl = t.dir("x\ny");
    let o = run(&["--fish", "--value", &format!("{nl}:{a}")]);
    assert_eq!(stdout(&o), format!("{a}\n"));
}

#[test]
fn empty_result() {
    assert_eq!(scrub("rel::"), "\n");
    let o = run(&["--fish", "--value", "rel"]);
    assert_eq!(stdout(&o), "");
}

#[test]
fn colon_in_directory_name_hint() {
    let t = Scratch::new();
    let a = t.dir("a");
    let weird = t.dir("my:tools");
    let o = run(&["-v", "--value", &format!("{weird}:{a}")]);
    assert_eq!(stdout(&o), format!("{a}\n"));
    assert!(stderr(&o).contains(&format!("hint: {weird} is a directory")));
}

#[test]
fn check_exit_codes() {
    let t = Scratch::new();
    let a = t.dir("a");

    let clean = run(&["--check", "--value", &a]);
    assert_eq!(clean.status.code(), Some(0));
    assert!(clean.stdout.is_empty());

    let dirty = run(&["--check", "--value", &format!("{a}:{a}")]);
    assert_eq!(dirty.status.code(), Some(1));
    assert!(dirty.stdout.is_empty());
    assert!(stderr(&dirty).contains("duplicate"));
}

#[test]
fn reads_path_by_default() {
    let t = Scratch::new();
    let a = t.dir("a");
    let o = run_with(&[], |c| {
        c.env("PATH", format!("{a}:{a}"));
    });
    assert_eq!(stdout(&o), format!("{a}\n"));
}

#[test]
fn reads_named_env() {
    let t = Scratch::new();
    let a = t.dir("a");
    let o = run_with(&["--env", "PATHSCRUB_TEST"], |c| {
        c.env("PATHSCRUB_TEST", format!("{a}::{a}"));
    });
    assert_eq!(stdout(&o), format!("{a}\n"));

    let unset = run_with(&["--env", "PATHSCRUB_UNSET"], |c| {
        c.env_remove("PATHSCRUB_UNSET");
    });
    assert_eq!(unset.status.code(), Some(2));
}

#[test]
fn reads_stdin() {
    let t = Scratch::new();
    let a = t.dir("a");
    let mut child = Command::new(BIN)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{a}:{a}").unwrap();
    let o = child.wait_with_output().unwrap();
    assert_eq!(stdout(&o), format!("{a}\n"));
}

#[test]
fn usage_errors() {
    assert_eq!(run(&["--value", "/", "-"]).status.code(), Some(2));
    assert_eq!(run(&["--bogus"]).status.code(), Some(2));
    assert_eq!(run(&["--env"]).status.code(), Some(2));
    assert!(run(&["--help"]).status.success());
}

#[test]
fn verbose_does_not_pollute_stdout() {
    let t = Scratch::new();
    let a = t.dir("a");
    let o = run(&["-v", "--value", &format!("x:{a}:{a}")]);
    assert_eq!(stdout(&o), format!("{a}\n"));
    assert!(!stderr(&o).is_empty());
}
