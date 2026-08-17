//! Records which build this is.
//!
//! `0.1.0` alone cannot tell two binaries apart, and it needs to: the skill
//! installs a release build while a contributor has a locally built one, both
//! print the same version string, and the symptom is a command that exists in
//! one and not the other with no way to see why. So `--version` carries the
//! commit as well.
//!
//! The commit *date* is used rather than the build date on purpose. A wall clock
//! makes every rebuild of the same source produce a different binary, which
//! defeats reproducible builds and turns "did anything change?" into a question
//! nobody can answer from the artefact.

use std::process::Command;

fn main() {
    // CI building from a tarball has no `.git`, so it passes the value in.
    let build = std::env::var("AMCLI_BUILD")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(commit)
        .unwrap_or_else(|| "unknown build".to_string());
    println!("cargo::rustc-env=AMCLI_BUILD={build}");

    println!("cargo::rerun-if-env-changed=AMCLI_BUILD");
    // Only HEAD matters, and it has to be located rather than guessed: in a git
    // worktree `.git` is a file and the real directory is elsewhere entirely.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo::rerun-if-changed={dir}/HEAD");
    }
}

fn commit() -> Option<String> {
    git(&["show", "-s", "--format=%h %cs"])
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}
