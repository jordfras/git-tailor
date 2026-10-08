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

use crate::common;
use crate::common::prelude::*;

fn twenty_lines_with_line_5(line_5: &str) -> String {
    (1..=20)
        .map(|i| match i {
            5 => format!("{line_5}\n"),
            _ => format!("line {i}\n"),
        })
        .collect()
}

/// Moving a commit back across a rename conflicts in the old name, which the
/// paused conflict writes to disk. Finishing must take it back: left behind, it
/// is an untracked file the next operation refuses to overwrite.
#[test]
fn a_move_resolved_across_a_rename_leaves_no_old_name_behind() {
    let test = common::TestRepo::new();
    let base = test.commit_file("foo.txt", &twenty_lines_with_line_5("line 5"), "base");
    test.commit_file("foo.txt", &twenty_lines_with_line_5("edited"), "edit foo");
    test.rename_file("foo.txt", "bar.txt", None, "rename foo to bar");
    let moved = test.commit_file("bar.txt", &twenty_lines_with_line_5("again"), "edit bar");

    let mut git_repo = test.git_repo();
    let mut outcome = git_repo
        .move_commit(&Oid::from(moved), Some(&Oid::from(base)), &Oid::from(moved))
        .unwrap();
    let workdir = test.repo.workdir().unwrap().to_path_buf();
    // Bounded: an unresolved conflict is legitimately reported again.
    for _ in 0..4 {
        let RebaseOutcome::Conflict(state) = outcome else {
            break;
        };
        test.write_file("foo.txt", &twenty_lines_with_line_5("again"));
        let _ = std::fs::remove_file(workdir.join("bar.txt"));
        git_repo
            .auto_stage_resolved_conflicts(&state.conflicting_files)
            .unwrap();
        outcome = git_repo.rebase_continue(&state).unwrap();
    }

    assert_rebase_complete!(outcome);
    assert!(
        !workdir.join("foo.txt").exists(),
        "the old name is not in the result and must not be left on disk"
    );
}
