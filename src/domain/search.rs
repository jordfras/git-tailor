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

// Matching one commit against a search: its message, its changed lines, or the
// paths it touches.

mod case;

use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};

pub use case::SearchCase;

use crate::fragmap::ChangePos;
use crate::{CommitInfo, DiffLineKind, FileDiff};

/// What part of a commit a search looks at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchScope {
    #[default]
    MessageAndDiff,
    Message,
    /// Added and removed lines, as `git log -G` matches them.
    Diff,
    Paths,
}

impl SearchScope {
    const CYCLE: [SearchScope; 4] = [
        SearchScope::MessageAndDiff,
        SearchScope::Message,
        SearchScope::Diff,
        SearchScope::Paths,
    ];

    /// The scope after this one, wrapping around.
    pub fn next(self) -> Self {
        Self::CYCLE[(self.cycle_index() + 1) % Self::CYCLE.len()]
    }

    /// The scope before this one, wrapping around.
    pub fn prev(self) -> Self {
        Self::CYCLE[(self.cycle_index() + Self::CYCLE.len() - 1) % Self::CYCLE.len()]
    }

    fn cycle_index(self) -> usize {
        Self::CYCLE
            .iter()
            .position(|&s| s == self)
            .unwrap_or_default()
    }
}

/// The pattern does not compile as a regex, or for [`SearchScope::Paths`] as a
/// glob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPattern;

/// A compiled search, built once per query and matched against every row.
#[derive(Debug)]
pub struct CommitQuery {
    scope: SearchScope,
    matcher: Matcher,
}

#[derive(Debug)]
enum Matcher {
    Text(Regex),
    Paths(GlobSet),
}

impl CommitQuery {
    pub fn new(
        scope: SearchScope,
        pattern: &str,
        case: SearchCase,
    ) -> Result<Self, InvalidPattern> {
        let matcher = match scope {
            SearchScope::Paths => {
                Matcher::Paths(path_glob(pattern, case.ignores_case_in_glob(pattern))?)
            }
            _ => Matcher::Text(search_regex(pattern, case).map_err(|_| InvalidPattern)?),
        };
        Ok(Self { scope, matcher })
    }

    /// The regex the commit message is matched with, `None` when the scope
    /// leaves the message out.
    pub fn message_regex(&self) -> Option<&Regex> {
        match &self.matcher {
            Matcher::Text(regex) if self.scope != SearchScope::Diff => Some(regex),
            _ => None,
        }
    }

    /// Whether `commit` matches. `files` is its diff; without one only the
    /// message can match.
    pub fn matches(&self, commit: &CommitInfo, files: Option<&[FileDiff]>) -> bool {
        let files = files.unwrap_or_default();
        match &self.matcher {
            Matcher::Text(regex) => {
                // A staged or unstaged row's message is a placeholder of ours.
                let message_matches = self.scope != SearchScope::Diff
                    && !commit.oid.is_synthetic()
                    && commit.message.lines().any(|line| regex.is_match(line));
                message_matches
                    || (self.scope != SearchScope::Message && diff_matches(files, regex))
            }
            Matcher::Paths(globs) => first_matching_change(files, globs).is_some(),
        }
    }

    /// The first change in `files` whose old or new path matches, `None` also
    /// when this is not a path search.
    pub fn first_matching_path(&self, files: &[FileDiff]) -> Option<ChangePos> {
        match &self.matcher {
            Matcher::Paths(globs) => first_matching_change(files, globs),
            Matcher::Text(_) => None,
        }
    }
}

fn diff_matches(files: &[FileDiff], regex: &Regex) -> bool {
    files
        .iter()
        .flat_map(|file| &file.hunks)
        .flat_map(|hunk| &hunk.lines)
        .any(|line| {
            line.kind != DiffLineKind::Context
                && regex.is_match(line.content.trim_end_matches(['\n', '\r']))
        })
}

fn first_matching_change(files: &[FileDiff], globs: &GlobSet) -> Option<ChangePos> {
    files
        .iter()
        .position(|file| {
            [&file.old_path, &file.new_path]
                .into_iter()
                .flatten()
                .any(|path| globs.is_match(Path::new(path)))
        })
        .map(ChangePos)
}

/// `pattern` compiled as `case` says.
pub fn search_regex(pattern: &str, case: SearchCase) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern)
        .case_insensitive(case.ignores_case_in_regex(pattern))
        .build()
}

/// Compile `pattern` the way `.gitignore` reads one: without a `/` it matches
/// at any depth, with one it is anchored at the repository root, and a trailing
/// `/` restricts it to directories. A pattern naming a directory matches every
/// path beneath it.
fn path_glob(pattern: &str, ignore_case: bool) -> Result<GlobSet, InvalidPattern> {
    let (pattern, directory_only) = match pattern.strip_suffix('/') {
        Some(rest) => (rest, true),
        None => (pattern, false),
    };
    let rooted = match pattern.strip_prefix('/') {
        Some(rest) => rest.to_string(),
        None if pattern.contains('/') => pattern.to_string(),
        None => format!("**/{pattern}"),
    };
    let mut globs = vec![format!("{rooted}/**")];
    if !directory_only {
        globs.push(rooted);
    }
    let mut set = GlobSetBuilder::new();
    for glob in globs {
        set.add(
            GlobBuilder::new(&glob)
                .literal_separator(true)
                .case_insensitive(ignore_case)
                .build()
                .map_err(|_| InvalidPattern)?,
        );
    }
    set.build().map_err(|_| InvalidPattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeltaStatus, DiffLine, Hunk, Oid, VirtualOid};

    fn commit(message: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::from(format!("{:040x}", 1).as_str())),
            summary: message.lines().next().unwrap_or_default().to_string(),
            summary_key: message.lines().next().unwrap_or_default().into(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: message.to_string(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    fn line(kind: DiffLineKind, content: &str) -> DiffLine {
        DiffLine {
            kind,
            content: format!("{content}\n"),
        }
    }

    fn hunk(lines: Vec<DiffLine>) -> Hunk {
        Hunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            lines,
        }
    }

    fn file(old: Option<&str>, new: Option<&str>, hunks: Vec<Hunk>) -> FileDiff {
        FileDiff {
            old_path: old.map(Into::into),
            new_path: new.map(Into::into),
            status: DeltaStatus::Modified,
            is_binary: false,
            hunks,
        }
    }

    fn modified(path: &str) -> FileDiff {
        file(Some(path), Some(path), vec![])
    }

    fn query(scope: SearchScope, pattern: &str) -> CommitQuery {
        CommitQuery::new(scope, pattern, SearchCase::Sensitive).unwrap()
    }

    fn matches(scope: SearchScope, pattern: &str, c: &CommitInfo, files: &[FileDiff]) -> bool {
        query(scope, pattern).matches(c, Some(files))
    }

    fn path_matches(pattern: &str, path: &str) -> bool {
        matches(SearchScope::Paths, pattern, &commit("m"), &[modified(path)])
    }

    #[test]
    fn message_is_matched_line_by_line() {
        let c = commit("Summary\n\nfix: the body line");
        assert!(matches(SearchScope::Message, "^fix", &c, &[]));
        assert!(matches(SearchScope::Message, "^Summary$", &c, &[]));
        assert!(!matches(SearchScope::Message, "^the", &c, &[]));
    }

    #[test]
    fn a_staged_or_unstaged_row_matches_by_its_diff_only() {
        let files = [file(
            Some("a.txt"),
            Some("a.txt"),
            vec![hunk(vec![line(DiffLineKind::Addition, "in diff")])],
        )];
        let staged = CommitInfo {
            oid: VirtualOid::Staged,
            ..commit("Staged changes")
        };
        assert!(!matches(SearchScope::Message, "changes", &staged, &files));
        assert!(matches(
            SearchScope::MessageAndDiff,
            "diff",
            &staged,
            &files
        ));
    }

    #[test]
    fn diff_matches_added_and_removed_lines_but_not_context() {
        let diff_matches = |kind| {
            let files = [file(
                Some("a.txt"),
                Some("a.txt"),
                vec![hunk(vec![line(kind, "needle")])],
            )];
            matches(SearchScope::Diff, "needle", &commit("m"), &files)
        };
        assert!(diff_matches(DiffLineKind::Deletion));
        assert!(diff_matches(DiffLineKind::Addition));
        assert!(!diff_matches(DiffLineKind::Context));
    }

    #[test]
    fn diff_line_end_anchors_before_the_newline() {
        let files = [file(
            Some("a.txt"),
            Some("a.txt"),
            vec![hunk(vec![line(DiffLineKind::Addition, "ends with foo")])],
        )];
        assert!(matches(SearchScope::Diff, "foo$", &commit("m"), &files));
    }

    #[test]
    fn diff_scope_ignores_the_message_and_message_scope_the_diff() {
        let files = [file(
            Some("a.txt"),
            Some("a.txt"),
            vec![hunk(vec![line(DiffLineKind::Addition, "in diff")])],
        )];
        let c = commit("in message");
        assert!(!matches(SearchScope::Diff, "message", &c, &files));
        assert!(!matches(SearchScope::Message, "diff", &c, &files));
    }

    #[test]
    fn message_and_diff_is_the_union() {
        let files = [file(
            Some("a.txt"),
            Some("a.txt"),
            vec![hunk(vec![line(DiffLineKind::Addition, "word")])],
        )];
        assert!(matches(
            SearchScope::MessageAndDiff,
            "word",
            &commit("a word"),
            &[]
        ));
        assert!(matches(
            SearchScope::MessageAndDiff,
            "word",
            &commit("m"),
            &files
        ));
    }

    #[test]
    fn scopes_cycle_both_ways() {
        let mut scope = SearchScope::default();
        let mut seen = vec![];
        for _ in 0..4 {
            seen.push(scope);
            scope = scope.next();
        }
        assert_eq!(
            seen,
            [
                SearchScope::MessageAndDiff,
                SearchScope::Message,
                SearchScope::Diff,
                SearchScope::Paths
            ]
        );
        assert_eq!(scope, SearchScope::MessageAndDiff);
        assert_eq!(SearchScope::MessageAndDiff.prev(), SearchScope::Paths);
        assert_eq!(SearchScope::Paths.prev(), SearchScope::Diff);
    }

    #[test]
    fn only_scopes_with_the_message_have_a_message_regex() {
        let has_regex = |scope| {
            CommitQuery::new(scope, "x", SearchCase::Sensitive)
                .unwrap()
                .message_regex()
                .is_some()
        };
        assert!(has_regex(SearchScope::MessageAndDiff));
        assert!(has_regex(SearchScope::Message));
        assert!(!has_regex(SearchScope::Diff));
        assert!(!has_regex(SearchScope::Paths));
    }

    #[test]
    fn a_commit_without_a_diff_matches_on_its_message() {
        let query =
            CommitQuery::new(SearchScope::MessageAndDiff, "word", SearchCase::Sensitive).unwrap();
        assert!(query.matches(&commit("a word"), None));
        assert!(!query.matches(&commit("other"), None));
    }

    #[test]
    fn glob_without_slash_matches_at_any_depth() {
        assert!(path_matches("*.rs", "main.rs"));
        assert!(path_matches("*.rs", "src/views/list.rs"));
        assert!(!path_matches("*.rs", "src/main.rsx"));
    }

    #[test]
    fn glob_with_slash_is_anchored_at_the_root() {
        assert!(path_matches("src/*.rs", "src/main.rs"));
        assert!(!path_matches("src/*.rs", "src/views/list.rs"));
        assert!(!path_matches("src/*.rs", "lib/src/main.rs"));
        assert!(path_matches("/a.txt", "a.txt"));
        assert!(!path_matches("/a.txt", "sub/a.txt"));
    }

    #[test]
    fn glob_naming_a_directory_matches_everything_beneath_it() {
        assert!(path_matches("views", "src/views/list.rs"));
        assert!(path_matches("views/", "src/views/list.rs"));
        assert!(!path_matches("views/", "src/views"));
        assert!(path_matches("src/**", "src/views/list.rs"));
    }

    #[test]
    fn glob_matches_a_rename_by_either_path_and_a_deletion_by_its_old_one() {
        let files = [
            modified("other.txt"),
            file(Some("old.txt"), Some("new.txt"), vec![]),
            file(Some("gone.txt"), None, vec![]),
        ];
        let first = |p| query(SearchScope::Paths, p).first_matching_path(&files);
        assert_eq!(first("old.txt"), Some(ChangePos(1)));
        assert_eq!(first("new.txt"), Some(ChangePos(1)));
        assert_eq!(first("gone.txt"), Some(ChangePos(2)));
    }

    #[test]
    fn paths_scope_ignores_the_message() {
        assert!(!matches(SearchScope::Paths, "word", &commit("word"), &[]));
    }

    #[test]
    fn invalid_patterns_are_reported() {
        assert_eq!(
            CommitQuery::new(SearchScope::Diff, "(", SearchCase::Sensitive).unwrap_err(),
            InvalidPattern
        );
        assert_eq!(
            CommitQuery::new(SearchScope::Paths, "a[", SearchCase::Sensitive).unwrap_err(),
            InvalidPattern
        );
    }
}
