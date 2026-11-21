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

//! The note saying which base `gt` fell back to, as the binary prints it.

#[allow(dead_code)]
mod common;

/// A wrong guess can be why there is nothing to show, so the note matters most
/// exactly when gt exits without drawing anything.
#[test]
fn the_fallback_base_is_named_when_there_is_nothing_to_show() {
    let test = common::TestRepo::new();
    // On main, so main is both the fallback base and HEAD.
    test.commit_file("a.txt", "a\n", "only");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gt"))
        .current_dir(test.repo.workdir().unwrap())
        .output()
        .expect("failed to run gt");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("No commits to display"), "{stderr}");
    assert!(
        stderr.contains("origin/HEAD is not set to a branch, so the base is main"),
        "{stderr}"
    );
}
