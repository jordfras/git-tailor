// Copyright 2026 Thomas Johannesson
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Integration tests for `gt completions` and the dynamic-completion engine.
//! These invoke the built `gt` binary directly.

use std::process::Command;

/// Path to the compiled `gt` binary under test.
fn gt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gt"))
}

#[test]
fn completions_prints_a_registration_script() {
    let out = gt()
        .args(["completions", "--shell", "bash"])
        .output()
        .expect("failed to run gt");
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The dynamic bash stub registers a completion function for the `gt` binary.
    assert!(
        stdout.contains("_clap_complete_gt") && stdout.contains("gt"),
        "unexpected script:\n{stdout}"
    );
}

#[test]
fn completions_install_writes_the_user_local_file() {
    let home = tempfile::tempdir().unwrap();
    let out = gt()
        .args(["completions", "--shell", "fish", "--install"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run gt");
    assert!(out.status.success(), "exit: {:?}", out.status);

    let installed = home.path().join(".config/fish/completions/gt.fish");
    assert!(
        installed.exists(),
        "expected {} to exist",
        installed.display()
    );
    assert!(
        !std::fs::read(&installed).unwrap().is_empty(),
        "installed script is empty"
    );
}

#[test]
fn completions_run_outside_a_git_repository() {
    // The subcommand must not require (or open) a repo. Run it from a temp dir
    // that is not a git repository.
    let cwd = tempfile::tempdir().unwrap();
    let out = gt()
        .args(["completions", "--shell", "zsh"])
        .current_dir(cwd.path())
        .output()
        .expect("failed to run gt");
    assert!(out.status.success(), "exit: {:?}", out.status);
    assert!(!out.stdout.is_empty(), "expected a script on stdout");
}

#[test]
fn dynamic_completion_offers_theme_enum_values() {
    // Mimic the shell asking to complete the value of `--theme` (the word at
    // index 2 in `gt --theme <cursor>`), the way the installed stub does.
    let out = gt()
        .args(["--", "gt", "--theme", ""])
        .env("COMPLETE", "bash")
        .env("_CLAP_COMPLETE_INDEX", "2")
        .output()
        .expect("failed to run gt");
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    for variant in ["highlight", "plain", "classic"] {
        assert!(
            stdout.contains(variant),
            "missing `{variant}` in candidates:\n{stdout}"
        );
    }
}

#[test]
fn help_lists_the_completions_subcommand() {
    let out = gt().arg("--help").output().expect("failed to run gt");
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("completions"),
        "help does not mention the completions subcommand:\n{stdout}"
    );
}
