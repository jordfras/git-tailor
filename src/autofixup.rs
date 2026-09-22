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

//! Pure planning logic for bulk autofixup (mirrors `git rebase --autosquash`):
//! matching `fixup!`/`squash!`-prefixed commits to the earlier commit their
//! summary names. No git access — operates on already-loaded `CommitInfo`.

use crate::CommitInfo;
use crate::Oid;
use crate::app::SquashMode;
use bstr::{BStr, BString};

/// One `fixup!`/`squash!` commit matched to the target it will be squashed into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofixupPair {
    pub source_oid: Oid,
    pub target_oid: Oid,
    pub source_summary: String,
    pub target_summary: String,
    /// Full commit message (summary + body) of the source/target, needed to
    /// build the non-interactive squash message; the confirmation dialog
    /// only shows the summaries.
    pub source_message: String,
    pub target_message: String,
    pub mode: SquashMode,
}

/// One target commit and every `fixup!`/`squash!` commit that will be folded
/// into it, for display grouped by target. Purely a view over `AutofixupPair`
/// — execution still proceeds pair by pair (see `plan_autofixup`'s docs on
/// why re-matching by summary, not by this grouping, drives the batch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofixupGroup {
    pub target_oid: Oid,
    pub target_summary: String,
    pub target_message: String,
    /// Oldest-first, same as `plan_autofixup`'s overall order.
    pub sources: Vec<AutofixupPair>,
}

/// Group `pairs` by target, preserving the order each target first appears
/// in and each group's internal (oldest-first) order.
pub fn group_by_target(pairs: &[AutofixupPair]) -> Vec<AutofixupGroup> {
    let mut groups: Vec<AutofixupGroup> = Vec::new();
    for pair in pairs {
        if let Some(group) = groups
            .iter_mut()
            .find(|g| g.target_summary == pair.target_summary)
        {
            group.sources.push(pair.clone());
        } else {
            groups.push(AutofixupGroup {
                target_oid: pair.target_oid.clone(),
                target_summary: pair.target_summary.clone(),
                target_message: pair.target_message.clone(),
                sources: vec![pair.clone()],
            });
        }
    }
    groups
}

const COMMENT_PREFIX: &str = "# ";

/// The message without its trailing newlines, which the template supplies.
fn trim_trailing_newlines(message: &BStr) -> &[u8] {
    let bytes: &[u8] = message.as_ref();
    let end = bytes.iter().rposition(|&b| b != b'\n').map_or(0, |i| i + 1);
    &bytes[..end]
}

/// Build the text shown in `$EDITOR` when the user edits a target group's
/// final message: `target_message`, live and editable, followed by each
/// source's message commented out — mirroring `git rebase
/// --autosquash`'s own combination template. Left untouched, the commented
/// sources contribute nothing, so a no-op edit is the same as not editing at
/// all (matches `fixup!`'s already-silent default).
pub fn edit_template(target_message: &BStr, sources: &[(SquashMode, BString)]) -> BString {
    let mut text = BString::from(trim_trailing_newlines(target_message));
    text.push(b'\n');
    for (mode, source_message) in sources {
        text.push(b'\n');
        text.extend_from_slice(COMMENT_PREFIX.as_bytes());
        text.extend_from_slice(match mode {
            SquashMode::Fixup => b"The message below is from a fixup! commit being folded in:",
            SquashMode::Squash => b"The message below is from a squash! commit being folded in:",
        });
        text.push(b'\n');
        text.extend_from_slice(COMMENT_PREFIX.as_bytes());
        text.push(b'\n');
        for line in source_message.split(|&b| b == b'\n') {
            text.extend_from_slice(COMMENT_PREFIX.as_bytes());
            text.extend_from_slice(line.strip_suffix(b"\r").unwrap_or(line));
            text.push(b'\n');
        }
    }
    text
}

/// Strip `#`-prefixed comment lines and trim surrounding blank lines — mirrors
/// git's own `commit.cleanup=strip` handling of the combination template
/// above, so leaving the commented-out sources untouched discards them.
pub fn strip_comment_lines(text: &[u8]) -> BString {
    // Bytes throughout: a commit message is bytes to git, and the editor hands
    // back whatever the user typed. Decoding to `String` first would replace
    // anything that is not UTF-8 with U+FFFD — silently rewriting their text.
    let kept: Vec<&[u8]> = text
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.starts_with(b"#"))
        .collect();
    kept.join(&b'\n').trim_ascii().into()
}

/// Match every `fixup!`/`squash!`-prefixed commit in `commits` (oldest-first,
/// as returned by `list_commits`) to the nearest earlier commit whose summary
/// its prefix names. Commits with no resolvable target are omitted — they are
/// left in place by the caller. Pairs are returned oldest-fixup-first, so
/// applying them in order naturally stacks multiple fixups aimed at the same
/// target (each squash keeps the target's summary, so later matches still
/// resolve correctly against the rewritten commit).
pub fn plan_autofixup(commits: &[CommitInfo]) -> Vec<AutofixupPair> {
    let mut pairs = Vec::new();
    for (i, commit) in commits.iter().enumerate() {
        let Some((mode, target_text)) = [SquashMode::Fixup, SquashMode::Squash]
            .into_iter()
            .find_map(|mode| {
                commit
                    .summary
                    .strip_prefix(mode.prefix())
                    .map(|text| (mode, text))
            })
        else {
            continue;
        };

        // Nearest preceding commit wins, in case of duplicate summaries.
        let Some(target) = commits[..i].iter().rev().find(|c| c.summary == target_text) else {
            continue;
        };

        pairs.push(AutofixupPair {
            source_oid: commit.oid.expect_real_oid(),
            target_oid: target.oid.expect_real_oid(),
            source_summary: commit.summary.clone(),
            target_summary: target.summary.clone(),
            source_message: commit.message.clone(),
            target_message: target.message.clone(),
            mode,
        });
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualOid;
    use bstr::ByteSlice;

    fn commit(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::new(oid.repeat(40))),
            summary: summary.to_string(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: summary.to_string(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    #[test]
    fn matches_a_fixup_to_its_target() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Add parser")];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_summary, "fixup! Add parser");
        assert_eq!(pairs[0].target_summary, "Add parser");
        assert_eq!(pairs[0].mode, SquashMode::Fixup);
    }

    #[test]
    fn matches_a_squash_to_its_target() {
        let commits = vec![commit("a", "Add parser"), commit("b", "squash! Add parser")];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].mode, SquashMode::Squash);
    }

    #[test]
    fn a_fixup_with_no_matching_target_is_skipped() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Nope")];
        assert_eq!(plan_autofixup(&commits), vec![]);
    }

    #[test]
    fn multiple_fixups_for_the_same_target_stack_in_branch_order() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "Unrelated"),
            commit("d", "fixup! Add parser"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].source_summary, "fixup! Add parser");
        assert_eq!(pairs[1].source_summary, "fixup! Add parser");
        // Both target the same original commit, oldest fixup first.
        assert_eq!(pairs[0].target_oid, pairs[1].target_oid);
    }

    #[test]
    fn mixed_fixup_and_squash_prefixes() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "Add lexer"),
            commit("c", "fixup! Add parser"),
            commit("d", "squash! Add lexer"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].mode, SquashMode::Fixup);
        assert_eq!(pairs[1].mode, SquashMode::Squash);
    }

    #[test]
    fn nearest_preceding_match_wins_on_duplicate_summaries() {
        let commits = vec![
            commit("a", "Tweak"),
            commit("b", "Tweak"),
            commit("c", "fixup! Tweak"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, commits[1].oid.expect_real_oid());
    }

    #[test]
    fn group_by_target_stacks_multiple_sources_under_one_group() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "Add lexer"),
            commit("d", "fixup! Add parser"),
            commit("e", "squash! Add lexer"),
        ];
        let pairs = plan_autofixup(&commits);
        let groups = group_by_target(&pairs);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].target_summary, "Add parser");
        assert_eq!(groups[0].sources.len(), 2);
        assert_eq!(groups[1].target_summary, "Add lexer");
        assert_eq!(groups[1].sources.len(), 1);
    }

    /// The template for `group`, built the way `dispatch::autofixup::edit_seed`
    /// builds it.
    fn template_for(group: &AutofixupGroup) -> BString {
        let sources: Vec<(SquashMode, BString)> = group
            .sources
            .iter()
            .map(|pair| (pair.mode, BString::from(pair.source_message.clone())))
            .collect();
        edit_template(group.target_message.as_str().into(), &sources)
    }

    /// A commit message ends in a newline, which is a terminator and not an
    /// empty last line. Treating it as one puts a bare "# " under every folded
    /// source in the editor.
    #[test]
    fn edit_template_does_not_comment_a_line_past_the_end_of_a_message() {
        let target = BString::from("Add parser\n");
        let sources = vec![(SquashMode::Fixup, BString::from("fixup! Add parser\n"))];

        let template = edit_template(target.as_bstr(), &sources);

        assert_eq!(
            template,
            concat!(
                "Add parser\n",
                "\n",
                "# The message below is from a fixup! commit being folded in:\n",
                "# \n",
                "# fixup! Add parser\n",
            )
        );
    }

    #[test]
    fn edit_template_comments_out_every_source() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Add parser")];
        let pairs = plan_autofixup(&commits);
        let group = &group_by_target(&pairs)[0];

        let template = template_for(group);
        assert!(template.starts_with(b"Add parser\n"));
        for line in template
            .split(|&b| b == b'\n')
            .skip(1)
            .filter(|l| !l.is_empty())
        {
            assert!(
                line.starts_with(b"#"),
                "expected every non-blank line after the target message to be commented: {:?}",
                line.as_bstr()
            );
        }
    }

    #[test]
    fn strip_comment_lines_on_an_untouched_template_yields_just_the_target_message() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "fixup! Add parser"),
        ];
        let pairs = plan_autofixup(&commits);
        let group = &group_by_target(&pairs)[0];

        let template = template_for(group);
        assert_eq!(strip_comment_lines(&template), b"Add parser");
    }

    #[test]
    fn strip_comment_lines_on_an_all_commented_template_yields_empty() {
        // If the user deletes the live target line too (or comments it out),
        // the result is empty — main.rs relies on exactly this to know the
        // edit should clear any existing override rather than store a blank
        // message.
        let text = "# Add parser\n# fixup! Add parser";
        assert_eq!(strip_comment_lines(text.as_bytes()), b"");
    }

    #[test]
    fn strip_comment_lines_keeps_uncommented_additions() {
        let text = "Add parser\n\n# comment\nExtra detail the user typed\n# more comment";
        assert_eq!(
            strip_comment_lines(text.as_bytes()),
            b"Add parser\n\nExtra detail the user typed"
        );
    }

    /// Bytes in, bytes out. Routing this through `String` would replace
    /// anything that is not UTF-8 with U+FFFD, silently rewriting a message the
    /// user typed — which is what git stores verbatim.
    #[test]
    fn strip_comment_lines_keeps_bytes_that_are_not_utf8() {
        // Latin-1 "Fix för åäö handling": valid git, invalid UTF-8.
        let text: &[u8] = b"Fix f\xf6r \xe5\xe4\xf6 handling\n# a comment\n";
        assert_eq!(
            strip_comment_lines(text),
            b"Fix f\xf6r \xe5\xe4\xf6 handling".to_vec()
        );
    }

    #[test]
    fn strip_comment_lines_preserves_internal_blank_lines_in_a_multi_paragraph_message() {
        let text = "Summary\n\nBody paragraph one.\n\nBody paragraph two.";
        assert_eq!(strip_comment_lines(text.as_bytes()), text.as_bytes());
    }
}
