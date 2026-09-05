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

//! Integration tests for the configurable diff context lines (T166): a larger
//! context merges hunks whose context regions overlap, and a smaller one splits
//! them apart — behavior git computes for us when we re-diff with the new
//! context.

#[allow(dead_code)]
mod common;

use common::TestRepo;
use git_tailor::app::{AppAction, AppState, KeyCommand};
use git_tailor::{Oid, repo::RepoRead, views};

/// Build a 30-line file, then change two lines far enough apart (line 5 and line
/// 20) that they are separate hunks at small context but merge at large context.
fn repo_with_two_distant_changes() -> (TestRepo, Oid) {
    let test = TestRepo::new();
    let base: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    test.commit_file("f.txt", &base, "base");

    let changed: String = (1..=30)
        .map(|n| match n {
            5 => "line 5 CHANGED\n".to_string(),
            20 => "line 20 CHANGED\n".to_string(),
            _ => format!("line {n}\n"),
        })
        .collect();
    let oid = test.commit_file("f.txt", &changed, "change two distant regions");
    (test, Oid::from(oid))
}

#[test]
fn small_context_keeps_two_hunks() {
    let (test, head) = repo_with_two_distant_changes();
    let diff = test.git_repo().commit_diff(&head, 3).unwrap();
    let file = &diff.files[0];
    assert_eq!(
        file.hunks.len(),
        2,
        "expected two separate hunks at context 3"
    );
}

#[test]
fn large_context_merges_into_one_hunk() {
    let (test, head) = repo_with_two_distant_changes();
    let diff = test.git_repo().commit_diff(&head, 10).unwrap();
    let file = &diff.files[0];
    assert_eq!(
        file.hunks.len(),
        1,
        "expected the hunks to merge into one at context 10"
    );
}

#[test]
fn context_does_not_change_the_files_touched() {
    let (test, head) = repo_with_two_distant_changes();
    let tight = test.git_repo().commit_diff(&head, 3).unwrap();
    let wide = test.git_repo().commit_diff(&head, 10).unwrap();
    let paths = |d: &git_tailor::CommitDiff| {
        d.files
            .iter()
            .map(|f| f.new_path.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&tight), paths(&wide));
}

/// Opening the detail view and changing the context width are the two moments
/// the diff is read; everything in between is redraw. Drive both through the
/// key handlers so the wiring cannot rot into a view that shows a diff computed
/// at the old context.
#[test]
fn opening_the_detail_view_and_changing_context_reload_the_diff() {
    let (test, head) = repo_with_two_distant_changes();
    let repo = test.git_repo();

    let mut app = AppState::new();
    app.list.commits = vec![repo.commit_diff(&head, 3).unwrap().commit];
    app.list.selection_index = 0;

    let action = views::commit_list::handle_key(KeyCommand::ToggleDetail, &mut app);
    assert!(
        matches!(action, AppAction::LoadDetailDiff),
        "opening the detail view must ask for the diff"
    );
    views::commit_detail::load_diff(&repo, &mut app);
    assert_eq!(hunk_count(&app), 2, "two separate hunks at context 3");

    for _ in 0..7 {
        let action = views::commit_detail::handle_key(KeyCommand::IncreaseContext, &mut app);
        assert!(
            matches!(action, AppAction::LoadDetailDiff),
            "a context change must ask for the diff again"
        );
        views::commit_detail::load_diff(&repo, &mut app);
    }

    assert_eq!(app.detail.context_lines.0, 10);
    assert_eq!(hunk_count(&app), 1, "the hunks merge at context 10");
}

fn hunk_count(app: &AppState) -> usize {
    app.detail.diff.as_ref().unwrap().files[0].hunks.len()
}
